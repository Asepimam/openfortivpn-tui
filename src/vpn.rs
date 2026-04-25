use crate::app::{AppEvent, CertInfo, VpnState};
use anyhow::{Result, bail};
use std::{
    env,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    process::{Child, Command},
    sync::mpsc,
    time::Duration,
};

// ─── Privilege Detection ──────────────────────────────────────────────────────
#[derive(Debug, Clone, PartialEq)]
pub enum PrivilegeMethod {
    AlreadyRoot,
    SudoNoPassword,
    SudoWithPassword,
    Unavailable,
}

impl PrivilegeMethod {
    pub fn label(&self) -> &str {
        match self {
            PrivilegeMethod::AlreadyRoot => "root (langsung)",
            PrivilegeMethod::SudoNoPassword => "sudo NOPASSWD",
            PrivilegeMethod::SudoWithPassword => "sudo -S (dengan password)",
            PrivilegeMethod::Unavailable => "tidak tersedia",
        }
    }
}

async fn detect_privilege_for_binary(
    binary_path: &str,
    tx: &mpsc::UnboundedSender<AppEvent>,
) -> PrivilegeMethod {
    if get_uid() == 0 {
        let _ = tx.send(AppEvent::DebugLog("[PRIV] Berjalan sebagai root ✔".into()));
        return PrivilegeMethod::AlreadyRoot;
    }

    let sudo_nopass = Command::new("sudo")
        .args(["-n", binary_path, "--help"])
        .output()
        .await
        .map(|o| o.status.success())
        .unwrap_or(false);

    if sudo_nopass {
        let _ = tx.send(AppEvent::DebugLog(format!(
            "[PRIV] sudo NOPASSWD tersedia untuk {} ✔",
            binary_path
        )));
        return PrivilegeMethod::SudoNoPassword;
    }

    let has_sudo = Command::new("sudo")
        .arg("-V")
        .output()
        .await
        .map(|o| o.status.success())
        .unwrap_or(false);

    if has_sudo {
        let _ = tx.send(AppEvent::DebugLog(
            "[PRIV] sudo tersedia (butuh password)".into(),
        ));
        return PrivilegeMethod::SudoWithPassword;
    }

    let _ = tx.send(AppEvent::DebugLog(
        "[PRIV] ✖ Tidak ada metode privilege.".into(),
    ));
    PrivilegeMethod::Unavailable
}

unsafe extern "C" {
    fn getuid() -> u32;
}

fn get_uid() -> u32 {
    unsafe { getuid() }
}

fn find_in_path(binary_name: &str) -> Option<PathBuf> {
    let path_var = env::var_os("PATH")?;

    env::split_paths(&path_var)
        .map(|dir| dir.join(binary_name))
        .find(|candidate| candidate.is_file())
}

fn resolve_openfortivpn_path() -> Option<PathBuf> {
    const COMMON_PATHS: [&str; 2] = ["/usr/bin/openfortivpn", "/usr/sbin/openfortivpn"];

    COMMON_PATHS
        .iter()
        .map(Path::new)
        .find(|candidate| candidate.is_file())
        .map(Path::to_path_buf)
        .or_else(|| find_in_path("openfortivpn"))
}

fn detect_install_hint() -> Option<&'static str> {
    [
        (
            "apt",
            "Install contoh: sudo apt update && sudo apt install openfortivpn",
        ),
        ("dnf", "Install contoh: sudo dnf install openfortivpn"),
        ("pacman", "Install contoh: sudo pacman -S openfortivpn"),
        ("zypper", "Install contoh: sudo zypper install openfortivpn"),
        ("apk", "Install contoh: sudo apk add openfortivpn"),
    ]
    .into_iter()
    .find_map(|(pkg_manager, hint)| find_in_path(pkg_manager).map(|_| hint))
}

fn sudoers_hint(binary_path: &str) -> String {
    format!(
        "Tambahkan via visudo: <username> ALL=(root) NOPASSWD: {}",
        binary_path
    )
}

// ─── Build Command ────────────────────────────────────────────────────────────
fn build_command(
    binary_path: &str,
    host: &str,
    port: u16,
    username: &str,
    trusted_cert: Option<&str>,
    method: &PrivilegeMethod,
) -> Command {
    let mut vpn_args: Vec<String> =
        vec![format!("{}:{}", host, port), "-u".into(), username.into()];

    if let Some(hash) = trusted_cert {
        vpn_args.push("--trusted-cert".into());
        vpn_args.push(hash.into());
    }

    let mut cmd = match method {
        PrivilegeMethod::AlreadyRoot => Command::new(binary_path),
        PrivilegeMethod::SudoNoPassword => {
            let mut c = Command::new("sudo");
            c.arg("-n");
            c.arg(binary_path);
            c
        }
        PrivilegeMethod::SudoWithPassword => {
            let mut c = Command::new("sudo");
            c.arg("-S");
            c.arg(binary_path);
            c
        }
        PrivilegeMethod::Unavailable => {
            let mut c = Command::new("sudo");
            c.arg("-n");
            c.arg(binary_path);
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
#[allow(clippy::too_many_arguments)]
pub async fn connect(
    session_id: u64,
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

    let _ = event_tx.send(AppEvent::LogLine {
        session_id,
        line: format!(
            "[VPN] Menghubungkan ke {}:{} sebagai {}{}",
            host,
            port,
            username,
            if trusted_cert.is_some() {
                " (cert trusted)"
            } else {
                ""
            }
        ),
    });

    if let Some(hint) = detect_install_hint() {
        let _ = event_tx.send(AppEvent::DebugLog(format!(
            "[VPN] Hint install openfortivpn: {}",
            hint
        )));
    }

    let binary_path = resolve_openfortivpn_path().ok_or_else(|| {
        let mut message = String::from(
            "Binary openfortivpn tidak ditemukan. Install openfortivpn terlebih dahulu (contoh path: /usr/bin/openfortivpn atau /usr/sbin/openfortivpn).",
        );
        if let Some(hint) = detect_install_hint() {
            message.push(' ');
            message.push_str(hint);
        }
        anyhow::anyhow!(message)
    })?;

    let binary_path_str = binary_path.to_string_lossy().into_owned();
    let _ = event_tx.send(AppEvent::DebugLog(format!(
        "[VPN] Binary openfortivpn terdeteksi di {}",
        binary_path_str
    )));

    let method = detect_privilege_for_binary(&binary_path_str, &event_tx).await;

    if method == PrivilegeMethod::Unavailable {
        bail!(
            "openfortivpn memerlukan root, tapi akses sudo tidak tersedia. {}",
            sudoers_hint(&binary_path_str)
        );
    }

    let _ = event_tx.send(AppEvent::DebugLog(format!(
        "[PRIV] Metode: {}",
        method.label()
    )));

    let mut cmd = build_command(
        &binary_path_str,
        host,
        port,
        username,
        trusted_cert.as_deref(),
        &method,
    );
    let mut child: Child = cmd.spawn()?;

    if let Some(pid) = child.id() {
        *pid_store.lock().unwrap() = Some(pid);
        let _ = event_tx.send(AppEvent::DebugLog(format!("[VPN] PID: {}", pid)));
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

    stdin
        .write_all(format!("{}\n", password).as_bytes())
        .await?;
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
        read_stream(
            session_id,
            stdout,
            tx1,
            false,
            cert_buf1,
            flag1,
            token_flag1,
            gateway_flag1,
            stdin1,
        )
        .await;
    });

    let tx2 = event_tx.clone();
    let cert_buf2 = cert_buf.clone();
    let flag2 = waiting_for_input_flag.clone();
    let token_flag2 = token_requested_flag.clone();
    let gateway_flag2 = gateway_connected.clone();
    let stdin2 = stdin_arc.clone();

    tokio::spawn(async move {
        read_stream(
            session_id,
            stderr,
            tx2,
            true,
            cert_buf2,
            flag2,
            token_flag2,
            gateway_flag2,
            stdin2,
        )
        .await;
    });

    let tx_waiter = event_tx.clone();
    let pid_store_waiter = pid_store.clone();
    let cert_buf_waiter = cert_buf.clone();
    let flag_waiter = waiting_for_input_flag.clone();

    tokio::spawn(async move {
        wait_for_process(
            session_id,
            child,
            tx_waiter,
            pid_store_waiter,
            cert_buf_waiter,
            flag_waiter,
        )
        .await;
    });

    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn read_stream(
    session_id: u64,
    stream: impl tokio::io::AsyncRead + Send + Unpin + 'static,
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
            let _ = tx.send(AppEvent::DebugLog(format!("[VPN] {}", line.trim())));

            if !*token_requested.lock().unwrap() {
                *token_requested.lock().unwrap() = true;
                let _ = tx.send(AppEvent::LogLine {
                    session_id,
                    line: "[VPN] 🔐 Menunggu token OTP...".into(),
                });
                let _ = tx.send(AppEvent::NeedToken(session_id));
                let _ = tx.send(AppEvent::StateChanged {
                    session_id,
                    state: VpnState::WaitingToken,
                });
                *waiting_flag.lock().unwrap() = true;
            }
            continue;
        }

        if line_lower.contains("tunnel is up") {
            let _ = tx.send(AppEvent::DebugLog(format!("[VPN] ✅ {}", line.trim())));
            let _ = tx.send(AppEvent::StateChanged {
                session_id,
                state: VpnState::Connected,
            });
            *waiting_flag.lock().unwrap() = false;
            continue;
        }

        if line_lower.contains("password:") && line_lower.contains("vpn account") {
            continue;
        }

        if !line.trim().is_empty() && !line_lower.contains("two-factor") {
            let prefix = if line_lower.contains("error") {
                "[ERR] "
            } else if line_lower.contains("warn") {
                "[WARN] "
            } else {
                "[VPN] "
            };
            let _ = tx.send(AppEvent::DebugLog(format!("{}{}", prefix, line.trim())));
        }
    }
}

async fn wait_for_process(
    session_id: u64,
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
        let _ = tx.send(AppEvent::DebugLog(format!(
            "[CERT] Untrusted: CN={}",
            info.subject_cn
        )));
        let _ = tx.send(AppEvent::CertError {
            session_id,
            cert: info,
        });
        return;
    }

    let was_waiting = *waiting_flag.lock().unwrap();

    match status {
        Ok(exit) => {
            if was_waiting {
                let _ = tx.send(AppEvent::DebugLog(
                    "[VPN] ⚠️ Koneksi terputus saat menunggu token".into(),
                ));
                *waiting_flag.lock().unwrap() = false;
                let _ = tx.send(AppEvent::StateChanged {
                    session_id,
                    state: VpnState::Disconnected,
                });
            } else if exit.success() {
                let _ = tx.send(AppEvent::DebugLog("[VPN] Koneksi ditutup".into()));
                let _ = tx.send(AppEvent::StateChanged {
                    session_id,
                    state: VpnState::Disconnected,
                });
            } else {
                let code = exit.code().unwrap_or(-1);
                let _ = tx.send(AppEvent::DebugLog(format!("[VPN] Exit code: {}", code)));
                let _ = tx.send(AppEvent::StateChanged {
                    session_id,
                    state: VpnState::Disconnected,
                });
            }
        }
        Err(e) => {
            let _ = tx.send(AppEvent::DebugLog(format!("[VPN] Error: {}", e)));
            let _ = tx.send(AppEvent::StateChanged {
                session_id,
                state: VpnState::Disconnected,
            });
        }
    }
}

// ─── Send OTP Token ──────────────────────────────────────────────────────────
pub async fn send_token(
    session_id: u64,
    token: &str,
    pid_store: Arc<Mutex<Option<u32>>>,
    event_tx: mpsc::UnboundedSender<AppEvent>,
) -> Result<()> {
    let pid = *pid_store.lock().unwrap();
    if let Some(pid) = pid {
        let _ = event_tx.send(AppEvent::LogLine {
            session_id,
            line: format!("[TOKEN] Mengirim token ke PID {}...", pid),
        });

        let token_line = format!("{}\n", token);

        let methods = vec![
            format!(
                "echo '{}' | sudo tee /proc/{}/fd/0 > /dev/null 2>&1",
                token, pid
            ),
            format!(
                "printf '{}' | sudo tee /proc/{}/fd/0 > /dev/null 2>&1",
                token, pid
            ),
            format!(
                "sudo sh -c \"echo '{}' > /proc/{}/fd/0\" 2>/dev/null",
                token, pid
            ),
        ];

        for method in methods {
            let output = Command::new("sh").arg("-c").arg(&method).output().await;

            if let Ok(o) = output
                && o.status.success()
            {
                let _ = event_tx.send(AppEvent::LogLine {
                    session_id,
                    line: "[TOKEN] ✅ Token berhasil dikirim".into(),
                });
                return Ok(());
            }
        }

        let temp_file = "/tmp/fortivpn_token.txt";
        let _ = tokio::fs::write(temp_file, token_line.as_bytes()).await;
        let output = Command::new("sh")
            .arg("-c")
            .arg(format!(
                "sudo cat {} > /proc/{}/fd/0 2>/dev/null",
                temp_file, pid
            ))
            .output()
            .await;
        let _ = tokio::fs::remove_file(temp_file).await;

        if let Ok(o) = output
            && o.status.success()
        {
            let _ = event_tx.send(AppEvent::LogLine {
                session_id,
                line: "[TOKEN] ✅ Token dikirim via file".into(),
            });
            return Ok(());
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
            if let Some(hash) = parts.last()
                && hash.len() >= 32
            {
                self.hash = hash.to_string();
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
                } else if self.in_issuer && key == "CN" {
                    self.issuer_cn = val.to_string();
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
    session_id: u64,
    pid_store: Arc<Mutex<Option<u32>>>,
    event_tx: mpsc::UnboundedSender<AppEvent>,
) -> Result<()> {
    let pid = *pid_store.lock().unwrap();
    if let Some(pid) = pid {
        let _ = event_tx.send(AppEvent::LogLine {
            session_id,
            line: format!("[VPN] Menghentikan PID {}...", pid),
        });
        let _ = Command::new("kill")
            .arg("-TERM")
            .arg(pid.to_string())
            .output()
            .await;
        tokio::time::sleep(Duration::from_secs(1)).await;
        let alive = Command::new("kill")
            .arg("-0")
            .arg(pid.to_string())
            .output()
            .await
            .map(|o| o.status.success())
            .unwrap_or(false);
        if alive {
            let _ = Command::new("kill")
                .arg("-KILL")
                .arg(pid.to_string())
                .output()
                .await;
            let _ = event_tx.send(AppEvent::LogLine {
                session_id,
                line: "[VPN] Force kill dengan SIGKILL".into(),
            });
        } else {
            let _ = event_tx.send(AppEvent::LogLine {
                session_id,
                line: "[VPN] Proses berhenti".into(),
            });
        }
        *pid_store.lock().unwrap() = None;
    }
    Ok(())
}
