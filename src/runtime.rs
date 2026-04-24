use std::time::Duration;

use anyhow::Result;
use crossterm::{
    event::{self, Event, KeyCode, KeyEvent, KeyModifiers},
    terminal::{disable_raw_mode, enable_raw_mode},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use tokio::sync::mpsc::error::TryRecvError;

use crate::{
    actions,
    app::{App, AppEvent, Focus, NotifLevel, UiMode, VpnState},
    ui,
};

pub async fn run(
    terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
    app: &mut App,
) -> Result<()> {
    let tick_rate = Duration::from_millis(100);

    loop {
        terminal.draw(|f| ui::render(f, app))?;
        drain_events(app).await;

        if event::poll(tick_rate)? && let Event::Key(key) = event::read()? {
            handle_key(app, key).await?;
        }

        app.tick_notification();
        if app.runtime.should_quit {
            break;
        }
    }

    Ok(())
}

pub fn init_terminal() -> Result<Terminal<CrosstermBackend<std::io::Stdout>>> {
    use crossterm::{
        execute,
        terminal::EnterAlternateScreen,
    };

    enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen, crossterm::event::EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    terminal.clear()?;
    Ok(terminal)
}

pub fn restore_terminal(
    terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
) -> Result<()> {
    use crossterm::{
        execute,
        terminal::LeaveAlternateScreen,
    };

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        crossterm::event::DisableMouseCapture
    )?;
    terminal.show_cursor()?;
    Ok(())
}

async fn drain_events(app: &mut App) {
    loop {
        match app.event_rx.try_recv() {
            Ok(event) => match event {
                AppEvent::LogLine(line) => app.push_log(line),
                AppEvent::DebugLog(line) => app.push_debug_log(line),
                AppEvent::StateChanged(new_state) => handle_state_change(app, new_state),
                AppEvent::NeedToken => open_token_prompt(app),
                AppEvent::CertError(cert_info) => open_cert_prompt(app, cert_info),
            },
            Err(TryRecvError::Empty | TryRecvError::Disconnected) => break,
        }
    }
}

fn handle_state_change(app: &mut App, new_state: VpnState) {
    let old = app.vpn_state.clone();
    app.vpn_state = new_state.clone();

    match (&old, &new_state) {
        (_, VpnState::Connected) => {
            app.notify("✔ VPN Terhubung!", NotifLevel::Success);
            app.push_log("[APP] ✔ Koneksi VPN berhasil");
            app.focus = Focus::Disconnect;
            *app.runtime.waiting_for_input_flag.lock().unwrap() = false;
        }
        (_, VpnState::Disconnected) => {
            if !matches!(old, VpnState::Disconnected | VpnState::WaitingCert) {
                app.notify("VPN Terputus", NotifLevel::Warning);
                app.push_log("[APP] VPN terputus");
            }
            if !app.has_modal() {
                app.focus = Focus::Connect;
            }
            *app.runtime.waiting_for_input_flag.lock().unwrap() = false;
        }
        (_, VpnState::Error(error)) => {
            app.notify(format!("Error: {}", error), NotifLevel::Error);
            app.push_log(format!("[APP] Error: {}", error));
            *app.runtime.waiting_for_input_flag.lock().unwrap() = false;
        }
        _ => {}
    }
}

fn open_token_prompt(app: &mut App) {
    if app.vpn_state == VpnState::WaitingToken {
        return;
    }

    app.vpn_state = VpnState::WaitingToken;
    app.focus = Focus::TokenInput;
    app.connection.token_input.clear();
    app.push_log("[APP] ⚡ Token OTP diminta — masukkan token dari email");
    app.notify("Masukkan token OTP dari email", NotifLevel::Info);
    *app.runtime.waiting_for_input_flag.lock().unwrap() = true;
}

fn open_cert_prompt(app: &mut App, cert_info: crate::app::CertInfo) {
    app.push_log(format!(
        "[CERT] ⚠ Certificate tidak dipercaya: CN={}",
        cert_info.subject_cn
    ));
    app.pending_cert = Some(cert_info);
    app.vpn_state = VpnState::WaitingCert;
    app.focus = Focus::CertAccept;
}

async fn handle_key(app: &mut App, key: KeyEvent) -> Result<()> {
    if app.ui_mode == UiMode::Help {
        if key.code == KeyCode::Esc || key.code == KeyCode::F(1) {
            app.hide_help();
        }
        return Ok(());
    }

    if key.code == KeyCode::F(1) {
        app.show_help();
        return Ok(());
    }

    if matches!(
        (key.modifiers, key.code),
        (KeyModifiers::CONTROL, KeyCode::Char('c'))
            | (KeyModifiers::CONTROL, KeyCode::Char('q'))
    ) {
        app.runtime.should_quit = true;
        return Ok(());
    }

    if key.code == KeyCode::Esc
        || matches!(
            (key.modifiers, key.code),
            (KeyModifiers::CONTROL, KeyCode::Char('b'))
        )
    {
        app.back_to_profile_list();
        return Ok(());
    }

    if app.vpn_state == VpnState::WaitingToken {
        return handle_token_popup(app, key).await;
    }
    if app.vpn_state == VpnState::WaitingCert {
        return handle_cert_dialog(app, key).await;
    }

    match app.ui_mode {
        UiMode::ProfileList => handle_profile_list_mode(app, key).await,
        UiMode::NewProfile | UiMode::EditProfile => handle_profile_form_mode(app, key).await,
        UiMode::Connect => handle_connect_mode(app, key).await,
        UiMode::Help => Ok(()),
    }
}

async fn handle_profile_list_mode(app: &mut App, key: KeyEvent) -> Result<()> {
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
            if let Some(name) = app.get_current_profile().map(|profile| profile.name.clone()) {
                app.ui_mode = UiMode::Connect;
                app.apply_current_profile();
                app.push_log(format!("[APP] Menggunakan profile: {}", name));
                actions::connect(app).await?;
            }
        }
        KeyCode::F(2) | KeyCode::Char('n') | KeyCode::Char('N') => app.open_new_profile_form(),
        KeyCode::F(3) | KeyCode::Char('e') | KeyCode::Char('E') => app.open_edit_profile_form(),
        KeyCode::F(4) | KeyCode::Char('d') | KeyCode::Char('D') => {
            actions::delete_selected_profile(app);
        }
        _ => {}
    }
    Ok(())
}

async fn handle_profile_form_mode(app: &mut App, key: KeyEvent) -> Result<()> {
    match key.code {
        KeyCode::Tab => app.cycle_profile_form_focus_forward(),
        KeyCode::BackTab => app.cycle_profile_form_focus_backward(),
        KeyCode::Char(' ') => match app.focus {
            Focus::SavePassword => {
                app.profile_form.save_password = !app.profile_form.save_password;
                if !app.profile_form.save_password {
                    app.profile_form.password.clear();
                }
            }
            Focus::UseSudoPassword => {
                app.profile_form.use_sudo_password = !app.profile_form.use_sudo_password;
                if !app.profile_form.use_sudo_password {
                    app.profile_form.sudo_password.clear();
                }
            }
            _ => {}
        },
        KeyCode::Enter => actions::save_profile(app).await?,
        KeyCode::Esc => app.back_to_profile_list(),
        _ => actions::edit_profile_form_field(app, key),
    }
    Ok(())
}

async fn handle_connect_mode(app: &mut App, key: KeyEvent) -> Result<()> {
    match (key.modifiers, key.code) {
        (KeyModifiers::CONTROL, KeyCode::Char('p')) => {
            app.show_password = !app.show_password;
        }
        (KeyModifiers::CONTROL, KeyCode::Char('s')) => actions::save_current_config(app),
        (_, KeyCode::Tab) => app.cycle_connect_focus_forward(),
        (KeyModifiers::SHIFT, KeyCode::BackTab) => app.cycle_connect_focus_backward(),
        (_, KeyCode::Up) if app.focus == Focus::Logs => app.scroll_logs_up(),
        (_, KeyCode::Down) if app.focus == Focus::Logs => app.scroll_logs_down(),
        (_, KeyCode::PageUp) => {
            for _ in 0..10 {
                app.scroll_logs_up();
            }
        }
        (_, KeyCode::PageDown) => {
            for _ in 0..10 {
                app.scroll_logs_down();
            }
        }
        (_, KeyCode::F(5)) => {
            if !app.logs.is_empty() {
                app.log_scroll = app.logs.len() - 1;
            }
        }
        _ => match app.focus {
            Focus::Host => actions::update_connection_host_port(app, key),
            Focus::Username => actions::handle_text_input(&mut app.connection.username, key),
            Focus::Password => actions::handle_text_input(&mut app.connection.password, key),
            Focus::SudoPassword => {
                actions::handle_text_input(&mut app.connection.sudo_password, key)
            }
            Focus::Connect if key.code == KeyCode::Enter => actions::connect(app).await?,
            Focus::Disconnect if key.code == KeyCode::Enter => actions::disconnect(app).await?,
            _ => {}
        },
    }
    Ok(())
}

async fn handle_token_popup(app: &mut App, key: KeyEvent) -> Result<()> {
    match key.code {
        KeyCode::Char(c) => app.connection.token_input.push(c),
        KeyCode::Backspace => {
            app.connection.token_input.pop();
        }
        KeyCode::Delete => app.connection.token_input.clear(),
        KeyCode::Enter => actions::submit_token(app).await?,
        KeyCode::Esc => {
            app.connection.token_input.clear();
            app.push_log("[TOKEN] ❌ Token dibatalkan");
            *app.runtime.waiting_for_input_flag.lock().unwrap() = false;
            actions::disconnect(app).await?;
        }
        _ => {}
    }
    Ok(())
}

async fn handle_cert_dialog(app: &mut App, key: KeyEvent) -> Result<()> {
    match key.code {
        KeyCode::Tab | KeyCode::Right | KeyCode::Left => {
            app.focus = match app.focus {
                Focus::CertAccept => Focus::CertDeny,
                _ => Focus::CertAccept,
            };
        }
        KeyCode::Char('y') | KeyCode::Char('Y') => actions::accept_cert_and_reconnect(app).await?,
        KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => actions::deny_cert(app),
        KeyCode::Enter => {
            if app.focus == Focus::CertAccept {
                actions::accept_cert_and_reconnect(app).await?;
            } else {
                actions::deny_cert(app);
            }
        }
        _ => {}
    }
    Ok(())
}
