use std::sync::{Arc, Mutex};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    process::{Child, Command},
    sync::mpsc,
    time::Duration,
};
use anyhow::{Result, bail};
use crate::app::{AppEvent, CertInfo, VpnState};

// ─── Privilege Detection ──────────────────────────────────────────────────────
#[derive(Debug, Clone, PartialEq)]
pub enum PrivilegeMethod {
    AlreadyRoot,
    SudoNoPassword,
    Pkexec,
    SudoWithPassword,
    Unavailable,
}

impl PrivilegeMethod {
    pub fn label(&self) -> &str {
        match self {
            PrivilegeMethod::AlreadyRoot => "root (langsung)",
            PrivilegeMethod::SudoNoPassword => "sudo NOPASSWD",
            PrivilegeMethod::Pkexec => "pkexec (polkit)",
            PrivilegeMethod::SudoWithPassword => "sudo -S (dengan password)",
            PrivilegeMethod::Unavailable => "tidak tersedia",
        }
    }
}

pub async fn detect_privilege(tx: &mpsc::UnboundedSender<AppEvent>) -> PrivilegeMethod {
    if get_uid() == 0 {
        let _ = tx.send(AppEvent::LogLine("[PRIV] Berjalan sebagai root ✔".into()));
        return PrivilegeMethod::AlreadyRoot;
    }

    let sudo_nopass = Command::new("sudo")
        .args(["-n", "true"])
        .output()
        .await
        .map(|o| o.status.success())
        .unwrap_or(false);

    if sudo_nopass {
        let _ = tx.send(AppEvent::LogLine("[PRIV] sudo NOPASSWD tersedia ✔".into()));
        return PrivilegeMethod::SudoNoPassword;
    }

    let has_sudo = Command::new("which")
        .arg("sudo")
        .output()
        .await
        .map(|o| o.status.success())
        .unwrap_or(false);

    if has_sudo {
        let _ = tx.send(AppEvent::LogLine("[PRIV] sudo tersedia (butuh password)".into()));
        return PrivilegeMethod::SudoWithPassword;
    }

    let _ = tx.send(AppEvent::LogLine("[PRIV] ✖ Tidak ada metode privilege.".into()));
    PrivilegeMethod::Unavailable
}

unsafe extern "C" {
    fn getuid() -> u32;
}

fn get_uid() -> u32 {
    unsafe { getuid() }
}

// ─── Build Command ────────────────────────────────────────────────────────────
fn build_command(
    host: &str,
    port: u16,
    username: &str,
    trusted_cert: Option<&str>,
    method: &PrivilegeMethod,
) -> Command {
    let mut vpn_args: Vec<String> = vec![
        format!("{}:{}", host, port),
        "-u".into(),
        username.into(),
    ];

    if let Some(hash) = trusted_cert {
        vpn_args.push("--trusted-cert".into());
        vpn_args.push(hash.into());
    }

    let mut cmd = match method {
        PrivilegeMethod::AlreadyRoot => Command::new("openfortivpn"),
        PrivilegeMethod::SudoNoPassword => {
            let mut c = Command::new("sudo");
            c.arg("-n");
            c.arg("openfortivpn");
            c
        }
        PrivilegeMethod::SudoWithPassword => {
            let mut c = Command::new("sudo");
            c.arg("-S");
            c.arg("openfortivpn");
            c
        }
        PrivilegeMethod::Pkexec | PrivilegeMethod::Unavailable => {
            let mut c = Command::new("sudo");
            c.arg("-n");
            c.arg("openfortivpn");
            c
        }
    };

    cmd.args(&vpn_args)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(false);

    cmd
}

// ─── Connect ─────────────────────────────────────────────────────────────────
pub async fn connect(
    host: &str,
    port: u16,
    username: &str,
    password: &str,
    sudo_password: Option<String>,
    trusted_cert: Option<String>,
    event_tx: mpsc::UnboundedSender<AppEvent>,
    pid_store: Arc<Mutex<Option<u32>>>,
    waiting_for_input_flag: Arc<Mutex<bool>>,
) -> Result<()> {
    if host.is_empty() || username.is_empty() || password.is_empty() {
        bail!("Host, username, dan password tidak boleh kosong");
    }

    let _ = event_tx.send(AppEvent::LogLine(format!(
        "[VPN] Menghubungkan ke {}:{} sebagai {}{}",
        host, port, username,
        if trusted_cert.is_some() { " (cert trusted)" } else { "" }
    )));

    let method = detect_privilege(&event_tx).await;

    if method == PrivilegeMethod::Unavailable {
        bail!("openfortivpn memerlukan root. Setup sudoers terlebih dahulu.");
    }

    let _ = event_tx.send(AppEvent::LogLine(format!("[PRIV] Metode: {}", method.label())));

    let mut cmd = build_command(host, port, username, trusted_cert.as_deref(), &method);
    let mut child: Child = cmd.spawn()?;

    if let Some(pid) = child.id() {
        *pid_store.lock().unwrap() = Some(pid);
        let _ = event_tx.send(AppEvent::LogLine(format!("[VPN] PID: {}", pid)));
    }

    let mut stdin = match child.stdin.take() {
        Some(stdin) => stdin,
        None => bail!("Tidak bisa mengambil stdin child process"),
    };

    if method == PrivilegeMethod::SudoWithPassword {
        let sp = sudo_password.as_deref().unwrap_or("");
        if sp.is_empty() {
            bail!("Sudo password diperlukan untuk metode sudo -S");
        }
        stdin.write_all(format!("{}\n", sp).as_bytes()).await?;
        stdin.flush().await?;
        tokio::time::sleep(Duration::from_millis(500)).await;
    }

    stdin.write_all(format!("{}\n", password).as_bytes()).await?;
    stdin.flush().await?;
    
    let stdin_arc = Arc::new(Mutex::new(Some(stdin)));

    let stdout = child.stdout.take().expect("stdout");
    let stderr = child.stderr.take().expect("stderr");

    let cert_buf: Arc<Mutex<CertBuffer>> = Arc::new(Mutex::new(CertBuffer::default()));
    let token_requested_flag = Arc::new(Mutex::new(false));
    let gateway_connected = Arc::new(Mutex::new(false));

    let tx1 = event_tx.clone();
    let cert_buf1 = cert_buf.clone();
    let flag1 = waiting_for_input_flag.clone();
    let token_flag1 = token_requested_flag.clone();
    let gateway_flag1 = gateway_connected.clone();
    let stdin1 = stdin_arc.clone();

    tokio::spawn(async move {
        read_stream(stdout, tx1, false, cert_buf1, flag1, token_flag1, gateway_flag1, stdin1).await;
    });

    let tx2 = event_tx.clone();
    let cert_buf2 = cert_buf.clone();
    let flag2 = waiting_for_input_flag.clone();
    let token_flag2 = token_requested_flag.clone();
    let gateway_flag2 = gateway_connected.clone();
    let stdin2 = stdin_arc.clone();

    tokio::spawn(async move {
        read_stream(stderr, tx2, true, cert_buf2, flag2, token_flag2, gateway_flag2, stdin2).await;
    });

    let tx_waiter = event_tx.clone();
    let pid_store_waiter = pid_store.clone();
    let cert_buf_waiter = cert_buf.clone();
    let flag_waiter = waiting_for_input_flag.clone();

    tokio::spawn(async move {
        wait_for_process(child, tx_waiter, pid_store_waiter, cert_buf_waiter, flag_waiter).await;
    });

    Ok(())
}

async fn read_stream(
    mut stream: impl tokio::io::AsyncRead + Send + Unpin + 'static,
    tx: mpsc::UnboundedSender<AppEvent>,
    _is_stderr: bool,
    cert_buf: Arc<Mutex<CertBuffer>>,
    waiting_flag: Arc<Mutex<bool>>,
    token_requested: Arc<Mutex<bool>>,
    gateway_connected: Arc<Mutex<bool>>,
    _stdin: Arc<Mutex<Option<tokio::process::ChildStdin>>>,
) {
    let mut lines = BufReader::new(stream).lines();
    
    while let Ok(Some(line)) = lines.next_line().await {
        let line_lower = line.to_lowercase();
        
        {
            let mut buf = cert_buf.lock().unwrap();
            buf.feed(&line);
        }
        
        if line_lower.contains("connected to gateway") && !*gateway_connected.lock().unwrap() {
            *gateway_connected.lock().unwrap() = true;
            let _ = tx.send(AppEvent::LogLine(format!("[VPN] {}", line.trim())));
            
            if !*token_requested.lock().unwrap() {
                *token_requested.lock().unwrap() = true;
                let _ = tx.send(AppEvent::LogLine("[VPN] 🔐 Menunggu token OTP...".into()));
                let _ = tx.send(AppEvent::NeedToken);
                let _ = tx.send(AppEvent::StateChanged(VpnState::WaitingToken));
                *waiting_flag.lock().unwrap() = true;
            }
            continue;
        }
        
        if line_lower.contains("tunnel is up") {
            let _ = tx.send(AppEvent::LogLine(format!("[VPN] ✅ {}", line.trim())));
            let _ = tx.send(AppEvent::StateChanged(VpnState::Connected));
            *waiting_flag.lock().unwrap() = false;
            continue;
        }
        
        if line_lower.contains("password:") && line_lower.contains("vpn account") {
            continue;
        }
        
        if !line.trim().is_empty() && !line_lower.contains("two-factor") {
            let prefix = if line_lower.contains("error") { "[ERR] " }
                else if line_lower.contains("warn") { "[WARN] " }
                else { "[VPN] " };
            let _ = tx.send(AppEvent::LogLine(format!("{}{}", prefix, line.trim())));
        }
    }
}

async fn wait_for_process(
    mut child: Child,
    tx: mpsc::UnboundedSender<AppEvent>,
    pid_store: Arc<Mutex<Option<u32>>>,
    cert_buf: Arc<Mutex<CertBuffer>>,
    waiting_flag: Arc<Mutex<bool>>,
) {
    let status = child.wait().await;
    *pid_store.lock().unwrap() = None;

    let cert_info = cert_buf.lock().unwrap().try_emit();
    if let Some(info) = cert_info {
        let _ = tx.send(AppEvent::LogLine(format!("[CERT] Untrusted: CN={}", info.subject_cn)));
        let _ = tx.send(AppEvent::CertError(info));
        return;
    }

    let was_waiting = *waiting_flag.lock().unwrap();

    match status {
        Ok(exit) => {
            if was_waiting {
                let _ = tx.send(AppEvent::LogLine("[VPN] ⚠️ Koneksi terputus saat menunggu token".into()));
                *waiting_flag.lock().unwrap() = false;
                let _ = tx.send(AppEvent::StateChanged(VpnState::Disconnected));
            } else if exit.success() {
                let _ = tx.send(AppEvent::LogLine("[VPN] Koneksi ditutup".into()));
                let _ = tx.send(AppEvent::StateChanged(VpnState::Disconnected));
            } else {
                let code = exit.code().unwrap_or(-1);
                let _ = tx.send(AppEvent::LogLine(format!("[VPN] Exit code: {}", code)));
                let _ = tx.send(AppEvent::StateChanged(VpnState::Disconnected));
            }
        }
        Err(e) => {
            let _ = tx.send(AppEvent::LogLine(format!("[VPN] Error: {}", e)));
            let _ = tx.send(AppEvent::StateChanged(VpnState::Disconnected));
        }
    }
}

// ─── Send OTP Token ──────────────────────────────────────────────────────────
pub async fn send_token(
    token: &str,
    pid_store: Arc<Mutex<Option<u32>>>,
    event_tx: mpsc::UnboundedSender<AppEvent>,
) -> Result<()> {
    let pid = pid_store.lock().unwrap().clone();
    if let Some(pid) = pid {
        let _ = event_tx.send(AppEvent::LogLine(format!("[TOKEN] Mengirim token ke PID {}...", pid)));
        
        let token_line = format!("{}\n", token);
        
        // Try multiple methods with sudo
        let methods = vec![
            format!("echo '{}' | sudo tee /proc/{}/fd/0 > /dev/null 2>&1", token, pid),
            format!("printf '{}' | sudo tee /proc/{}/fd/0 > /dev/null 2>&1", token, pid),
            format!("sudo sh -c \"echo '{}' > /proc/{}/fd/0\" 2>/dev/null", token, pid),
        ];
        
        for method in methods {
            let output = Command::new("sh")
                .arg("-c")
                .arg(&method)
                .output()
                .await;
                
            if let Ok(o) = output {
                if o.status.success() {
                    let _ = event_tx.send(AppEvent::LogLine("[TOKEN] ✅ Token berhasil dikirim".into()));
                    return Ok(());
                }
            }
        }
        
        // Last resort: write to temp file
        let temp_file = "/tmp/fortivpn_token.txt";
        let _ = tokio::fs::write(temp_file, token_line.as_bytes()).await;
        let output = Command::new("sh")
            .arg("-c")
            .arg(format!("sudo cat {} > /proc/{}/fd/0 2>/dev/null", temp_file, pid))
            .output()
            .await;
        let _ = tokio::fs::remove_file(temp_file).await;
        
        if let Ok(o) = output {
            if o.status.success() {
                let _ = event_tx.send(AppEvent::LogLine("[TOKEN] ✅ Token dikirim via file".into()));
                return Ok(());
            }
        }
        
        Err(anyhow::anyhow!("Gagal mengirim token"))
    } else {
        Err(anyhow::anyhow!("Tidak ada proses VPN aktif"))
    }
}

// ─── Cert Error Parser ────────────────────────────────────────────────────────
#[derive(Debug, Default)]
struct CertBuffer {
    collecting: bool,
    hash: String,
    subject_cn: String,
    subject_org: String,
    issuer_cn: String,
    raw_lines: Vec<String>,
    emitted: bool,
    in_subject: bool,
    in_issuer: bool,
}

impl CertBuffer {
    fn try_emit(&mut self) -> Option<CertInfo> {
        if self.emitted || self.hash.is_empty() {
            return None;
        }
        self.emitted = true;
        Some(CertInfo {
            hash: self.hash.clone(),
            subject_cn: self.subject_cn.clone(),
            subject_org: self.subject_org.clone(),
            issuer_cn: self.issuer_cn.clone(),
            raw_lines: self.raw_lines.clone(),
        })
    }

    fn feed(&mut self, line: &str) {
        let lower = line.to_lowercase();
        let trimmed = line.trim();

        if lower.contains("gateway certificate validation failed") {
            self.collecting = true;
            self.raw_lines.push(trimmed.to_string());
            return;
        }

        if !self.collecting {
            return;
        }

        self.raw_lines.push(trimmed.to_string());

        if lower.contains("--trusted-cert") {
            let parts: Vec<&str> = trimmed.split_whitespace().collect();
            if let Some(hash) = parts.last() {
                if hash.len() >= 32 {
                    self.hash = hash.to_string();
                }
            }
            return;
        }

        if lower.contains("subject:") {
            self.in_subject = true;
            self.in_issuer = false;
            return;
        }
        if lower.contains("issuer:") {
            self.in_issuer = true;
            self.in_subject = false;
            return;
        }

        if trimmed.contains('=') {
            let kv: Vec<&str> = trimmed.splitn(2, '=').collect();
            if kv.len() == 2 {
                let key = kv[0].trim().to_uppercase();
                let val = kv[1].trim();
                if self.in_subject {
                    match key.as_str() {
                        "CN" => self.subject_cn = val.to_string(),
                        "O" => self.subject_org = val.to_string(),
                        _ => {}
                    }
                } else if self.in_issuer {
                    if key == "CN" {
                        self.issuer_cn = val.to_string();
                    }
                }
            }
        }

        if lower.contains("closed connection") || lower.contains("could not log out") {
            self.in_subject = false;
            self.in_issuer = false;
        }
    }
}

// ─── Disconnect ──────────────────────────────────────────────────────────────
pub async fn disconnect(
    pid_store: Arc<Mutex<Option<u32>>>,
    event_tx: mpsc::UnboundedSender<AppEvent>,
) -> Result<()> {
    let pid = pid_store.lock().unwrap().clone();
    if let Some(pid) = pid {
        let _ = event_tx.send(AppEvent::LogLine(format!("[VPN] Menghentikan PID {}...", pid)));
        let _ = Command::new("kill").arg("-TERM").arg(pid.to_string()).output().await;
        tokio::time::sleep(Duration::from_secs(1)).await;
        let alive = Command::new("kill").arg("-0").arg(pid.to_string()).output().await
            .map(|o| o.status.success()).unwrap_or(false);
        if alive {
            let _ = Command::new("kill").arg("-KILL").arg(pid.to_string()).output().await;
            let _ = event_tx.send(AppEvent::LogLine("[VPN] Force kill dengan SIGKILL".into()));
        } else {
            let _ = event_tx.send(AppEvent::LogLine("[VPN] Proses berhenti".into()));
        }
        *pid_store.lock().unwrap() = None;
    }
    Ok(())
}