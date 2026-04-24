mod app;
mod config;
mod ui;
mod vpn;

use std::time::Duration;
use anyhow::Result;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use app::{App, AppEvent, Focus, NotifLevel, UiMode, VpnState};
use config::Config;

// ─── Entry Point ─────────────────────────────────────────────────────────────
#[tokio::main]
async fn main() -> Result<()> {
    let debug_enabled = std::env::args().any(|arg| arg == "-d" || arg == "--debug");
    setup_logging(debug_enabled)?;
    let cfg = Config::load().unwrap_or_default();
    let mut app = App::new(debug_enabled);
    
    app.load_profiles(cfg.profiles.clone());
    if let Some(selected) = cfg.selected_profile {
        if let Some(idx) = app.profiles.iter().position(|p| p.name == selected) {
            app.selected_profile_index = idx;
            app.select_profile(idx);
        }
    }
    
    if app.profiles.is_empty() {
        app.ui_mode = UiMode::NewProfile;
        app.focus = Focus::ProfileName;
    }

    enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    terminal.clear()?;

    let result = run_app(&mut terminal, &mut app).await;

    save_all_config(&app).ok();

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen, DisableMouseCapture)?;
    terminal.show_cursor()?;
    if let Err(e) = result { eprintln!("Error: {}", e); }
    Ok(())
}

// ─── Main Loop ────────────────────────────────────────────────────────────────
async fn run_app(
    terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
    app: &mut App,
) -> Result<()> {
    let tick_rate = Duration::from_millis(100);
    loop {
        terminal.draw(|f| ui::render(f, app))?;
        drain_events(app).await;
        if event::poll(tick_rate)? {
            if let Event::Key(key) = event::read()? {
                handle_key(app, key).await?;
            }
        }
        app.tick_notification();
        if app.should_quit { break; }
    }
    Ok(())
}

// ─── Event Drain ─────────────────────────────────────────────────────────────
async fn drain_events(app: &mut App) {
    use tokio::sync::mpsc::error::TryRecvError;
    loop {
        match app.event_rx.try_recv() {
            Ok(event) => match event {
                AppEvent::LogLine(line) => { app.push_log(line); }
                AppEvent::DebugLog(line) => { app.push_debug_log(line); }
                AppEvent::StateChanged(new_state) => {
                    let old = app.vpn_state.clone();
                    app.vpn_state = new_state.clone();
                    match (&old, &new_state) {
                        (_, VpnState::Connected) => {
                            app.notify("✔ VPN Terhubung!", NotifLevel::Success);
                            app.push_log("[APP] ✔ Koneksi VPN berhasil");
                            app.focus = Focus::Disconnect;
                            *app.waiting_for_input_flag.lock().unwrap() = false;
                        }
                        (_, VpnState::Disconnected) => {
                            if !matches!(old, VpnState::Disconnected | VpnState::WaitingCert) {
                                app.notify("VPN Terputus", NotifLevel::Warning);
                                app.push_log("[APP] VPN terputus");
                            }
                            if app.has_modal() {
                                app.focus = Focus::Connect;
                            }
                            *app.waiting_for_input_flag.lock().unwrap() = false;
                        }
                        (_, VpnState::Error(e)) => {
                            app.notify(format!("Error: {}", e), NotifLevel::Error);
                            app.push_log(format!("[APP] Error: {}", e));
                            *app.waiting_for_input_flag.lock().unwrap() = false;
                        }
                        _ => {}
                    }
                }
                AppEvent::NeedToken => {
                    if app.vpn_state == VpnState::WaitingToken { continue; }
                    app.vpn_state = VpnState::WaitingToken;
                    app.focus = Focus::TokenInput;
                    app.token_input = String::new();
                    app.push_log("[APP] ⚡ Token OTP diminta — masukkan token dari email");
                    app.notify("Masukkan token OTP dari email", NotifLevel::Info);
                    *app.waiting_for_input_flag.lock().unwrap() = true;
                }
                AppEvent::CertError(cert_info) => {
                    app.push_log(format!("[CERT] ⚠ Certificate tidak dipercaya: CN={}", cert_info.subject_cn));
                    app.pending_cert = Some(cert_info);
                    app.vpn_state = VpnState::WaitingCert;
                    app.focus = Focus::CertAccept;
                }
                AppEvent::Quit => { app.should_quit = true; }
            },
            Err(TryRecvError::Empty) => break,
            Err(TryRecvError::Disconnected) => break,
        }
    }
}

// ─── Key Handler ─────────────────────────────────────────────────────────────
async fn handle_key(app: &mut App, key: crossterm::event::KeyEvent) -> Result<()> {
    // Help mode - hanya ESC atau F1 untuk keluar
    if app.ui_mode == UiMode::Help {
        if key.code == KeyCode::Esc || key.code == KeyCode::F(1) {
            app.hide_help();
        }
        return Ok(());
    }
    
    // Global F1 untuk help (dari mode apapun)
    if key.code == KeyCode::F(1) {
        app.show_help();
        return Ok(());
    }
    
    // Global quit (Ctrl+C atau Ctrl+Q)
    if matches!((key.modifiers, key.code), (KeyModifiers::CONTROL, KeyCode::Char('c')) | (KeyModifiers::CONTROL, KeyCode::Char('q'))) {
        app.should_quit = true;
        return Ok(());
    }
    
    // Global back to profile list (ESC atau Ctrl+B)
    if key.code == KeyCode::Esc || matches!((key.modifiers, key.code), (KeyModifiers::CONTROL, KeyCode::Char('b'))) {
        app.back_to_profile_list();
        return Ok(());
    }
    
    // Modal dialogs
    if app.vpn_state == VpnState::WaitingToken {
        return handle_token_popup(app, key).await;
    }
    if app.vpn_state == VpnState::WaitingCert {
        return handle_cert_dialog(app, key).await;
    }
    
    // Mode specific handlers
    match app.ui_mode {
        UiMode::ProfileList => handle_profile_list_mode(app, key).await?,
        UiMode::NewProfile | UiMode::EditProfile => handle_profile_form_mode(app, key).await?,
        UiMode::Connect => handle_connect_mode(app, key).await?,
        UiMode::Help => {}
    }
    
    Ok(())
}

// ─── Profile List Mode Handler ───────────────────────────────────────────────
async fn handle_profile_list_mode(app: &mut App, key: crossterm::event::KeyEvent) -> Result<()> {
    match key.code {
        KeyCode::Up => {
            if app.selected_profile_index > 0 {
                app.selected_profile_index -= 1;
                app.select_profile(app.selected_profile_index);
            }
        }
        KeyCode::Down => {
            if app.selected_profile_index + 1 < app.profiles.len() {
                app.selected_profile_index += 1;
                app.select_profile(app.selected_profile_index);
            }
        }
        KeyCode::Enter | KeyCode::F(5) => {
            let profile_name = app.get_current_profile().map(|p| p.name.clone());
            if let Some(name) = profile_name {
                app.ui_mode = UiMode::Connect;
                app.apply_current_profile();
                app.push_log(&format!("[APP] Menggunakan profile: {}", name));
                let _ = do_connect(app).await;
            }
        }
        KeyCode::F(2) | KeyCode::Char('n') | KeyCode::Char('N') => {
            app.ui_mode = UiMode::NewProfile;
            app.profile_name.clear();
            app.profile_host.clear();
            app.profile_port = "443".to_string();
            app.profile_username.clear();
            app.profile_password.clear();
            app.profile_sudo_password.clear();
            app.profile_save_password = false;
            app.profile_use_sudo_password = false;
            app.editing_profile_name = None;
            app.focus = Focus::ProfileName;
            app.push_log("[APP] Mode: New Profile (F2/N)");
        }
        KeyCode::F(3) | KeyCode::Char('e') | KeyCode::Char('E') => {
            if let Some(profile) = app.get_current_profile() {
                let profile_name = profile.name.clone();
                let profile_host = profile.host.clone();
                let profile_port = profile.port.to_string();
                let profile_username = profile.username.clone();
                let profile_password = profile.password.clone();
                let profile_sudo_password = profile.sudo_password.clone();
                let profile_save_password = profile.save_password;
                let profile_use_sudo_password = profile.use_sudo_password;
                
                app.ui_mode = UiMode::EditProfile;
                app.editing_profile_name = Some(profile_name.clone());
                app.profile_name = profile_name.clone();
                app.profile_host = profile_host;
                app.profile_port = profile_port;
                app.profile_username = profile_username;
                app.profile_password = profile_password;
                app.profile_sudo_password = profile_sudo_password;
                app.profile_save_password = profile_save_password;
                app.profile_use_sudo_password = profile_use_sudo_password;
                app.focus = Focus::ProfileName;
                app.push_log(&format!("[APP] Mode: Edit Profile '{}' (F3/E)", &profile_name));
            }
        }
        KeyCode::F(4) | KeyCode::Char('d') | KeyCode::Char('D') => {
            if let Some(profile) = app.get_current_profile() {
                let profile_name = profile.name.clone();
                if app.delete_confirmation.is_some() {
                    if app.delete_confirmation.as_ref() == Some(&profile_name) {
                        let mut cfg = Config::load().unwrap_or_default();
                        cfg.delete_profile(&profile_name);
                        if let Err(e) = cfg.save() {
                            app.notify(format!("Gagal hapus: {}", e), NotifLevel::Error);
                        } else {
                            app.profiles = cfg.profiles;
                            app.selected_profile_index = app.selected_profile_index.min(app.profiles.len().saturating_sub(1));
                            if !app.profiles.is_empty() && app.selected_profile_index < app.profiles.len() {
                                let idx = app.selected_profile_index;
                                let new_profile = app.profiles[idx].clone();
                                app.host = new_profile.host;
                                app.port = new_profile.port;
                                app.username = new_profile.username;
                                app.password = new_profile.password;
                                app.sudo_password = new_profile.sudo_password;
                                app.trusted_cert = new_profile.trusted_cert;
                            }
                            app.notify(format!("Profile '{}' dihapus", &profile_name), NotifLevel::Success);
                            app.push_log(&format!("[APP] Profile '{}' dihapus (F4/D)", &profile_name));
                        }
                        app.delete_confirmation = None;
                    } else {
                        app.delete_confirmation = None;
                    }
                } else {
                    app.delete_confirmation = Some(profile_name.clone());
                    app.notify(format!("Tekan F4 atau D lagi untuk hapus '{}'", &profile_name), NotifLevel::Warning);
                }
            }
        }
        _ => {}
    }
    Ok(())
}

// ─── Profile Form Mode Handler ───────────────────────────────────────────────
async fn handle_profile_form_mode(app: &mut App, key: crossterm::event::KeyEvent) -> Result<()> {
    match key.code {
        KeyCode::Tab => {
            match app.focus {
                Focus::ProfileName => app.focus = Focus::Host,
                Focus::Host => app.focus = Focus::Port,
                Focus::Port => app.focus = Focus::Username,
                Focus::Username => app.focus = Focus::Password,
                Focus::Password => app.focus = Focus::SudoPassword,
                Focus::SudoPassword => app.focus = Focus::SavePassword,
                Focus::SavePassword => app.focus = Focus::UseSudoPassword,
                Focus::UseSudoPassword => app.focus = Focus::ProfileName,
                _ => app.focus = Focus::ProfileName,
            }
        }
        KeyCode::BackTab => {
            match app.focus {
                Focus::ProfileName => app.focus = Focus::UseSudoPassword,
                Focus::Host => app.focus = Focus::ProfileName,
                Focus::Port => app.focus = Focus::Host,
                Focus::Username => app.focus = Focus::Port,
                Focus::Password => app.focus = Focus::Username,
                Focus::SudoPassword => app.focus = Focus::Password,
                Focus::SavePassword => app.focus = Focus::SudoPassword,
                Focus::UseSudoPassword => app.focus = Focus::SavePassword,
                _ => app.focus = Focus::ProfileName,
            }
        }
        KeyCode::Char(' ') => {
            if app.focus == Focus::SavePassword {
                app.profile_save_password = !app.profile_save_password;
                if !app.profile_save_password {
                    app.profile_password.clear();
                }
            } else if app.focus == Focus::UseSudoPassword {
                app.profile_use_sudo_password = !app.profile_use_sudo_password;
                if !app.profile_use_sudo_password {
                    app.profile_sudo_password.clear();
                }
            }
        }
        KeyCode::Enter => {
            save_profile(app).await?;
        }
        KeyCode::Esc => {
            app.ui_mode = UiMode::ProfileList;
            app.focus = Focus::ProfileList;
            app.delete_confirmation = None;
            app.push_log("[APP] Kembali ke daftar profile");
        }
        _ => {
            match app.focus {
                Focus::ProfileName => handle_text_input(&mut app.profile_name, key),
                Focus::Host => handle_text_input(&mut app.profile_host, key),
                Focus::Port => handle_text_input(&mut app.profile_port, key),
                Focus::Username => handle_text_input(&mut app.profile_username, key),
                Focus::Password => handle_text_input(&mut app.profile_password, key),
                Focus::SudoPassword => handle_text_input(&mut app.profile_sudo_password, key),
                _ => {}
            }
        }
    }
    Ok(())
}

// ─── Connect Mode Handler ────────────────────────────────────────────────────
async fn handle_connect_mode(app: &mut App, key: crossterm::event::KeyEvent) -> Result<()> {
    match (key.modifiers, key.code) {
        (KeyModifiers::CONTROL, KeyCode::Char('p')) => {
            app.show_password = !app.show_password;
            return Ok(());
        }
        (KeyModifiers::CONTROL, KeyCode::Char('s')) => {
            save_current_config(app);
            return Ok(());
        }
        (_, KeyCode::Tab) => { app.cycle_focus_forward(); return Ok(()); }
        (KeyModifiers::SHIFT, KeyCode::BackTab) => { app.cycle_focus_backward(); return Ok(()); }
        (_, KeyCode::Up) => {
            if app.focus == Focus::Logs { app.scroll_logs_up(); }
            return Ok(());
        }
        (_, KeyCode::Down) => {
            if app.focus == Focus::Logs { app.scroll_logs_down(); }
            return Ok(());
        }
        (_, KeyCode::PageUp) => { for _ in 0..10 { app.scroll_logs_up(); } return Ok(()); }
        (_, KeyCode::PageDown) => { for _ in 0..10 { app.scroll_logs_down(); } return Ok(()); }
        (_, KeyCode::F(5)) => {
            if !app.logs.is_empty() { app.log_scroll = app.logs.len() - 1; }
            return Ok(());
        }
        _ => {
            // Handle text input
            if app.focus == Focus::Host {
                let mut combined = format!("{}:{}", app.host, app.port);
                handle_text_input(&mut combined, key);
                let (host, port) = parse_host_port(&combined);
                app.host = host;
                app.port = port;
            } else if app.focus == Focus::Username {
                handle_text_input(&mut app.username, key);
            } else if app.focus == Focus::Password {
                handle_text_input(&mut app.password, key);
            } else if app.focus == Focus::SudoPassword {
                handle_text_input(&mut app.sudo_password, key);
            } else if app.focus == Focus::Connect && key.code == KeyCode::Enter {
                do_connect(app).await?;
            } else if app.focus == Focus::Disconnect && key.code == KeyCode::Enter {
                do_disconnect(app).await?;
            }
        }
    }
    Ok(())
}

// ─── Token Popup Handler ──────────────────────────────────────────────────────
async fn handle_token_popup(app: &mut App, key: crossterm::event::KeyEvent) -> Result<()> {
    match key.code {
        KeyCode::Char(c) => {
            app.token_input.push(c);
        }
        KeyCode::Backspace => {
            app.token_input.pop();
        }
        KeyCode::Delete => {
            app.token_input.clear();
        }
        KeyCode::Enter => {
            let token = app.token_input.trim().to_string();
            if token.is_empty() {
                app.notify("Token tidak boleh kosong!", NotifLevel::Error);
                return Ok(());
            }
            
            app.push_log("[TOKEN] Mengirim token OTP...");
            
            let event_tx = app.event_tx.clone();
            let pid_store = app.vpn_pid.clone();
            let waiting_flag = app.waiting_for_input_flag.clone();
            
            tokio::spawn(async move {
                tokio::time::sleep(Duration::from_millis(200)).await;
                match vpn::send_token(&token, pid_store, event_tx.clone()).await {
                    Ok(_) => {
                        let _ = event_tx.send(AppEvent::LogLine("[TOKEN] ✅ Token terkirim".into()));
                    }
                    Err(e) => {
                        let _ = event_tx.send(AppEvent::LogLine(format!("[TOKEN] ❌ Gagal: {}", e)));
                    }
                }
                *waiting_flag.lock().unwrap() = false;
            });
            
            app.token_input.clear();
            app.vpn_state = VpnState::Connecting;
            app.focus = Focus::Disconnect;
        }
        KeyCode::Esc => {
            app.token_input.clear();
            app.push_log("[TOKEN] ❌ Token dibatalkan");
            *app.waiting_for_input_flag.lock().unwrap() = false;
            do_disconnect(app).await?;
        }
        _ => {}
    }
    Ok(())
}

// ─── Cert Dialog Handler ──────────────────────────────────────────────────────
async fn handle_cert_dialog(app: &mut App, key: crossterm::event::KeyEvent) -> Result<()> {
    match key.code {
        KeyCode::Tab | KeyCode::Right | KeyCode::Left => {
            app.focus = match app.focus {
                Focus::CertAccept => Focus::CertDeny,
                _ => Focus::CertAccept,
            };
        }
        KeyCode::Char('y') | KeyCode::Char('Y') => { accept_cert_and_reconnect(app).await?; }
        KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => { deny_cert(app); }
        KeyCode::Enter => {
            if app.focus == Focus::CertAccept {
                accept_cert_and_reconnect(app).await?;
            } else {
                deny_cert(app);
            }
        }
        _ => {}
    }
    Ok(())
}

async fn accept_cert_and_reconnect(app: &mut App) -> Result<()> {
    let cert = match app.pending_cert.take() { Some(c) => c, None => return Ok(()) };
    app.push_log(format!("[CERT] ✔ Certificate diterima: {}", cert.subject_cn));
    app.trusted_cert = Some(cert.hash.clone());
    
    let profile_name = app.get_current_profile().map(|p| p.name.clone());
    if let Some(name) = profile_name {
        app.update_profile_trusted_cert(&name, &cert.hash);
        save_all_config(app).ok();
    }
    
    app.vpn_state = VpnState::Connecting;
    app.focus = Focus::Disconnect;
    app.push_log("[APP] Menghubungkan ulang dengan cert trusted...");

    let host = app.host.clone();
    let port = app.port;
    let username = app.username.clone();
    let password = app.password.clone();
    let trusted_cert = Some(cert.hash);
    let sudo_pwd = if app.sudo_password.is_empty() { None } else { Some(app.sudo_password.clone()) };
    let event_tx = app.event_tx.clone();
    let pid_store = app.vpn_pid.clone();
    let input_flag = app.waiting_for_input_flag.clone();

    tokio::spawn(async move {
        if let Err(e) = vpn::connect(
            &host, port, &username, &password,
            sudo_pwd, trusted_cert,
            event_tx.clone(), pid_store, input_flag,
        ).await {
            let _ = event_tx.send(AppEvent::LogLine(format!("[APP] Gagal connect ulang: {}", e)));
            let _ = event_tx.send(AppEvent::StateChanged(VpnState::Error(e.to_string())));
        }
    });
    Ok(())
}

fn deny_cert(app: &mut App) {
    let cn = app.pending_cert.as_ref().map(|c| c.subject_cn.clone()).unwrap_or_default();
    app.push_log(format!("[CERT] ✖ Certificate ditolak: {}", cn));
    app.pending_cert = None;
    *app.waiting_for_input_flag.lock().unwrap() = false;
    app.vpn_state = VpnState::Disconnected;
    app.focus = Focus::Connect;
    app.notify("Certificate ditolak. Koneksi dibatalkan.", NotifLevel::Warning);
}

// ─── Text Input ───────────────────────────────────────────────────────────────
fn handle_text_input(field: &mut String, key: crossterm::event::KeyEvent) {
    match key.code {
        KeyCode::Char(c) => { field.push(c); }
        KeyCode::Backspace => { field.pop(); }
        KeyCode::Delete => { field.clear(); }
        _ => {}
    }
}

// ─── VPN Actions ─────────────────────────────────────────────────────────────
async fn do_connect(app: &mut App) -> Result<()> {
    if !matches!(app.vpn_state, VpnState::Disconnected | VpnState::Error(_)) {
        app.notify("Sudah terhubung atau sedang menghubungkan", NotifLevel::Warning);
        return Ok(());
    }
    if app.host.is_empty() || app.username.is_empty() || app.password.is_empty() {
        app.notify("Host, Username, dan Password harus diisi", NotifLevel::Error);
        return Ok(());
    }
    
    app.vpn_state = VpnState::Connecting;
    app.push_log(format!("[APP] Menghubungkan ke {}:{}...", app.host, app.port));
    if app.trusted_cert.is_some() {
        app.push_log("[APP] Menggunakan trusted cert yang tersimpan");
    }
    app.notify("Menghubungkan ke VPN...", NotifLevel::Info);

    let host = app.host.clone();
    let port = app.port;
    let username = app.username.clone();
    let password = app.password.clone();
    let trusted_cert = app.trusted_cert.clone();
    let sudo_pwd = if app.sudo_password.is_empty() { None } else { Some(app.sudo_password.clone()) };
    let event_tx = app.event_tx.clone();
    let pid_store = app.vpn_pid.clone();
    let input_flag = app.waiting_for_input_flag.clone();

    tokio::spawn(async move {
        if let Err(e) = vpn::connect(
            &host, port, &username, &password,
            sudo_pwd, trusted_cert,
            event_tx.clone(), pid_store, input_flag,
        ).await {
            let _ = event_tx.send(AppEvent::LogLine(format!("[APP] Gagal: {}", e)));
            let _ = event_tx.send(AppEvent::StateChanged(VpnState::Error(e.to_string())));
        }
    });
    Ok(())
}

async fn do_disconnect(app: &mut App) -> Result<()> {
    if matches!(app.vpn_state, VpnState::Disconnected) {
        app.notify("Tidak ada koneksi aktif", NotifLevel::Info);
        return Ok(());
    }
    
    app.token_input.clear();
    app.pending_cert = None;
    *app.waiting_for_input_flag.lock().unwrap() = false;
    app.vpn_state = VpnState::Disconnecting;
    app.focus = Focus::Connect;
    app.push_log("[APP] Memutuskan koneksi VPN...");
    app.notify("Memutuskan koneksi...", NotifLevel::Warning);

    let event_tx = app.event_tx.clone();
    let pid_store = app.vpn_pid.clone();
    tokio::spawn(async move {
        if let Err(e) = vpn::disconnect(pid_store, event_tx.clone()).await {
            let _ = event_tx.send(AppEvent::LogLine(format!("[APP] Gagal disconnect: {}", e)));
        }
    });
    Ok(())
}

// ─── Profile Save ────────────────────────────────────────────────────────────
async fn save_profile(app: &mut App) -> Result<()> {
    if app.profile_name.trim().is_empty() {
        app.notify("Nama profile tidak boleh kosong!", NotifLevel::Error);
        return Ok(());
    }
    if app.profile_host.trim().is_empty() {
        app.notify("Host tidak boleh kosong!", NotifLevel::Error);
        return Ok(());
    }
    
    let port: u16 = app.profile_port.parse().unwrap_or(443);
    let new_profile = config::VpnProfile {
        name: app.profile_name.clone(),
        host: app.profile_host.clone(),
        port,
        username: app.profile_username.clone(),
        save_password: app.profile_save_password,
        password: if app.profile_save_password { app.profile_password.clone() } else { String::new() },
        trusted_cert: None,
        use_sudo_password: app.profile_use_sudo_password,
        sudo_password: if app.profile_use_sudo_password { app.profile_sudo_password.clone() } else { String::new() },
    };
    
    let mut cfg = Config::load().unwrap_or_default();
    
    let is_edit = app.ui_mode == UiMode::EditProfile;
    let old_name = app.editing_profile_name.clone();
    
    if is_edit {
        if let Some(old_name) = &old_name {
            cfg.delete_profile(old_name);
        }
    }
    
    cfg.add_profile(new_profile.clone());
    
    if let Err(e) = cfg.save() {
        app.notify(format!("Gagal simpan: {}", e), NotifLevel::Error);
        return Ok(());
    }
    
    app.profiles = cfg.profiles;
    app.selected_profile_index = app.profiles.iter().position(|p| p.name == app.profile_name).unwrap_or(0);
    
    if app.selected_profile_index < app.profiles.len() {
        let profile = app.profiles[app.selected_profile_index].clone();
        app.host = profile.host;
        app.port = profile.port;
        app.username = profile.username;
        app.password = profile.password;
        app.sudo_password = profile.sudo_password;
        app.trusted_cert = profile.trusted_cert;
    }
    
    app.ui_mode = UiMode::ProfileList;
    app.focus = Focus::ProfileList;
    app.delete_confirmation = None;
    
    app.notify(format!("Profile '{}' tersimpan!", app.profile_name), NotifLevel::Success);
    app.push_log(&format!("[APP] Profile '{}' tersimpan", app.profile_name));
    Ok(())
}

fn save_current_config(app: &App) {
    let cfg = Config {
        profiles: app.profiles.clone(),
        selected_profile: app.get_current_profile().map(|p| p.name.clone()),
    };
    if let Err(e) = cfg.save() {
        tracing::error!("Gagal simpan config: {}", e);
    } else {
        tracing::info!("Config tersimpan");
    }
}

fn save_all_config(app: &App) -> Result<()> {
    let cfg = Config {
        profiles: app.profiles.clone(),
        selected_profile: app.get_current_profile().map(|p| p.name.clone()),
    };
    cfg.save()?;
    Ok(())
}

// ─── Helpers ─────────────────────────────────────────────────────────────────
fn parse_host_port(host_port: &str) -> (String, u16) {
    if let Some(colon) = host_port.rfind(':') {
        let host = host_port[..colon].to_string();
        let port: u16 = host_port[colon + 1..].parse().unwrap_or(443);
        (host, port)
    } else {
        (host_port.to_string(), 443)
    }
}

// ─── Logging ──────────────────────────────────────────────────────────────────
fn setup_logging(debug_enabled: bool) -> Result<()> {
    if !debug_enabled {
        return Ok(());
    }

    use std::fs::OpenOptions;
    let log_file = std::env::temp_dir().join("openfortivpn-tui.log");
    let file = OpenOptions::new().create(true).append(true).open(&log_file)?;
    let file_layer = tracing_subscriber::fmt::layer()
        .with_writer(std::sync::Mutex::new(file))
        .with_ansi(false);
    tracing_subscriber::registry().with(file_layer).init();
    Ok(())
}
