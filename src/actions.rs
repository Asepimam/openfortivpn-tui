use std::time::Duration;

use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent};

use crate::{
    app::{App, AppEvent, Focus, NotifLevel, UiMode, VpnState},
    config::{Config, VpnProfile},
    vpn,
};

pub async fn connect(app: &mut App) -> Result<()> {
    if !matches!(app.vpn_state, VpnState::Disconnected | VpnState::Error(_)) {
        app.notify("Sudah terhubung atau sedang menghubungkan", NotifLevel::Warning);
        return Ok(());
    }
    if !app.connection.is_ready_for_connect() {
        app.notify("Host, Username, dan Password harus diisi", NotifLevel::Error);
        return Ok(());
    }

    app.vpn_state = VpnState::Connecting;
    app.push_log(format!(
        "[APP] Menghubungkan ke {}:{}...",
        app.connection.host, app.connection.port
    ));
    if app.connection.trusted_cert.is_some() {
        app.push_log("[APP] Menggunakan trusted cert yang tersimpan");
    }
    app.notify("Menghubungkan ke VPN...", NotifLevel::Info);

    let host = app.connection.host.clone();
    let port = app.connection.port;
    let username = app.connection.username.clone();
    let password = app.connection.password.clone();
    let trusted_cert = app.connection.trusted_cert.clone();
    let sudo_pwd = app.connection.sudo_password_option();
    let event_tx = app.event_tx.clone();
    let pid_store = app.runtime.vpn_pid.clone();
    let input_flag = app.runtime.waiting_for_input_flag.clone();

    tokio::spawn(async move {
        if let Err(e) = vpn::connect(
            &host,
            port,
            &username,
            &password,
            sudo_pwd,
            trusted_cert,
            event_tx.clone(),
            pid_store,
            input_flag,
        )
        .await
        {
            let _ = event_tx.send(AppEvent::LogLine(format!("[APP] Gagal: {}", e)));
            let _ = event_tx.send(AppEvent::StateChanged(VpnState::Error(e.to_string())));
        }
    });

    Ok(())
}

pub async fn disconnect(app: &mut App) -> Result<()> {
    if matches!(app.vpn_state, VpnState::Disconnected) {
        app.notify("Tidak ada koneksi aktif", NotifLevel::Info);
        return Ok(());
    }

    app.connection.token_input.clear();
    app.pending_cert = None;
    *app.runtime.waiting_for_input_flag.lock().unwrap() = false;
    app.vpn_state = VpnState::Disconnecting;
    app.focus = Focus::Connect;
    app.push_log("[APP] Memutuskan koneksi VPN...");
    app.notify("Memutuskan koneksi...", NotifLevel::Warning);

    let event_tx = app.event_tx.clone();
    let pid_store = app.runtime.vpn_pid.clone();
    tokio::spawn(async move {
        if let Err(e) = vpn::disconnect(pid_store, event_tx.clone()).await {
            let _ = event_tx.send(AppEvent::LogLine(format!("[APP] Gagal disconnect: {}", e)));
        }
    });

    Ok(())
}

pub async fn save_profile(app: &mut App) -> Result<()> {
    if app.profile_form.name.trim().is_empty() {
        app.notify("Nama profile tidak boleh kosong!", NotifLevel::Error);
        return Ok(());
    }
    if app.profile_form.host.trim().is_empty() {
        app.notify("Host tidak boleh kosong!", NotifLevel::Error);
        return Ok(());
    }

    let new_profile = app.profile_form.to_profile();
    let selected_name = new_profile.name.clone();
    let mut cfg = Config::load().unwrap_or_default();

    if app.ui_mode == UiMode::EditProfile {
        if let Some(old_name) = &app.profile_form.editing_profile_name {
            cfg.delete_profile(old_name);
        }
    }

    cfg.add_profile(new_profile);

    if let Err(e) = cfg.save() {
        app.notify(format!("Gagal simpan: {}", e), NotifLevel::Error);
        return Ok(());
    }

    app.profiles = cfg.profiles;
    app.selected_profile_index = app
        .profiles
        .iter()
        .position(|profile| profile.name == selected_name)
        .unwrap_or(0);
    app.apply_current_profile();
    app.ui_mode = UiMode::ProfileList;
    app.focus = Focus::ProfileList;
    app.delete_confirmation = None;
    app.notify(
        format!("Profile '{}' tersimpan!", app.profile_form.name),
        NotifLevel::Success,
    );
    app.push_log(format!("[APP] Profile '{}' tersimpan", app.profile_form.name));

    Ok(())
}

pub fn save_current_config(app: &App) {
    if let Err(e) = build_config_snapshot(app).save() {
        tracing::error!("Gagal simpan config: {}", e);
    } else {
        tracing::info!("Config tersimpan");
    }
}

pub fn save_all_config(app: &App) -> Result<()> {
    build_config_snapshot(app).save()
}

pub fn delete_selected_profile(app: &mut App) {
    let Some(profile_name) = app.get_current_profile().map(|profile| profile.name.clone()) else {
        return;
    };

    if app.delete_confirmation.as_ref() == Some(&profile_name) {
        let mut cfg = Config::load().unwrap_or_default();
        cfg.delete_profile(&profile_name);
        if let Err(e) = cfg.save() {
            app.notify(format!("Gagal hapus: {}", e), NotifLevel::Error);
        } else {
            app.replace_profiles(cfg.profiles);
            app.notify(
                format!("Profile '{}' dihapus", profile_name),
                NotifLevel::Success,
            );
            app.push_log(format!("[APP] Profile '{}' dihapus (F4/D)", profile_name));
        }
        app.delete_confirmation = None;
    } else {
        app.delete_confirmation = Some(profile_name.clone());
        app.notify(
            format!("Tekan F4 atau D lagi untuk hapus '{}'", profile_name),
            NotifLevel::Warning,
        );
    }
}

pub async fn submit_token(app: &mut App) -> Result<()> {
    let token = app.connection.token_input.trim().to_string();
    if token.is_empty() {
        app.notify("Token tidak boleh kosong!", NotifLevel::Error);
        return Ok(());
    }

    app.push_log("[TOKEN] Mengirim token OTP...");

    let event_tx = app.event_tx.clone();
    let pid_store = app.runtime.vpn_pid.clone();
    let waiting_flag = app.runtime.waiting_for_input_flag.clone();

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

    app.connection.token_input.clear();
    app.vpn_state = VpnState::Connecting;
    app.focus = Focus::Disconnect;

    Ok(())
}

pub async fn accept_cert_and_reconnect(app: &mut App) -> Result<()> {
    let Some(cert) = app.pending_cert.take() else {
        return Ok(());
    };

    app.push_log(format!("[CERT] ✔ Certificate diterima: {}", cert.subject_cn));
    app.connection.trusted_cert = Some(cert.hash.clone());

    if let Some(profile_name) = app.get_current_profile().map(|profile| profile.name.clone()) {
        app.update_profile_trusted_cert(&profile_name, &cert.hash);
        save_all_config(app).ok();
    }

    app.vpn_state = VpnState::Connecting;
    app.focus = Focus::Disconnect;
    app.push_log("[APP] Menghubungkan ulang dengan cert trusted...");

    let host = app.connection.host.clone();
    let port = app.connection.port;
    let username = app.connection.username.clone();
    let password = app.connection.password.clone();
    let trusted_cert = Some(cert.hash);
    let sudo_pwd = app.connection.sudo_password_option();
    let event_tx = app.event_tx.clone();
    let pid_store = app.runtime.vpn_pid.clone();
    let input_flag = app.runtime.waiting_for_input_flag.clone();

    tokio::spawn(async move {
        if let Err(e) = vpn::connect(
            &host,
            port,
            &username,
            &password,
            sudo_pwd,
            trusted_cert,
            event_tx.clone(),
            pid_store,
            input_flag,
        )
        .await
        {
            let _ = event_tx.send(AppEvent::LogLine(format!("[APP] Gagal connect ulang: {}", e)));
            let _ = event_tx.send(AppEvent::StateChanged(VpnState::Error(e.to_string())));
        }
    });

    Ok(())
}

pub fn deny_cert(app: &mut App) {
    let cn = app
        .pending_cert
        .as_ref()
        .map(|cert| cert.subject_cn.clone())
        .unwrap_or_default();
    app.push_log(format!("[CERT] ✖ Certificate ditolak: {}", cn));
    app.pending_cert = None;
    *app.runtime.waiting_for_input_flag.lock().unwrap() = false;
    app.vpn_state = VpnState::Disconnected;
    app.focus = Focus::Connect;
    app.notify("Certificate ditolak. Koneksi dibatalkan.", NotifLevel::Warning);
}

pub fn handle_text_input(field: &mut String, key: KeyEvent) {
    match key.code {
        KeyCode::Char(c) => field.push(c),
        KeyCode::Backspace => {
            field.pop();
        }
        KeyCode::Delete => field.clear(),
        _ => {}
    }
}

pub fn update_connection_host_port(app: &mut App, key: KeyEvent) {
    let mut combined = app.connection.host_port_display();
    handle_text_input(&mut combined, key);
    app.connection.update_host_port(&combined);
}

pub fn edit_profile_form_field(app: &mut App, key: KeyEvent) {
    match app.focus {
        Focus::ProfileName => handle_text_input(&mut app.profile_form.name, key),
        Focus::Host => handle_text_input(&mut app.profile_form.host, key),
        Focus::Port => handle_text_input(&mut app.profile_form.port, key),
        Focus::Username => handle_text_input(&mut app.profile_form.username, key),
        Focus::Password => handle_text_input(&mut app.profile_form.password, key),
        Focus::SudoPassword => handle_text_input(&mut app.profile_form.sudo_password, key),
        _ => {}
    }
}

pub fn apply_initial_config(app: &mut App, cfg: &Config) {
    app.load_profiles(cfg.profiles.clone());
    if let Some(selected) = cfg.selected_profile.as_ref() {
        if let Some(idx) = app.profiles.iter().position(|profile| &profile.name == selected) {
            app.select_profile(idx);
        }
    }
    if app.profiles.is_empty() {
        app.ui_mode = UiMode::NewProfile;
        app.focus = Focus::ProfileName;
        app.profile_form.reset_for_new();
    }
}

fn build_config_snapshot(app: &App) -> Config {
    Config {
        profiles: app.profiles.clone(),
        selected_profile: app.get_current_profile().map(|profile| profile.name.clone()),
    }
}

#[allow(dead_code)]
fn _profile(_profile: &VpnProfile) {}
