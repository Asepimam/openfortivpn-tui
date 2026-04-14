use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, List, ListItem, Padding, Paragraph},
    Frame,
};
use crate::app::{App, Focus, NotifLevel, UiMode, VpnState};

// ─── Palette ─────────────────────────────────────────────────────────────────
const C_BG: Color        = Color::Rgb(18, 18, 28);
const C_SURFACE: Color   = Color::Rgb(28, 28, 42);
const C_BORDER: Color    = Color::Rgb(60, 60, 90);
const C_FOCUS: Color     = Color::Rgb(100, 180, 255);
const C_TEXT: Color      = Color::Rgb(220, 220, 240);
const C_DIM: Color       = Color::Rgb(100, 100, 130);
const C_GREEN: Color     = Color::Rgb(80, 220, 120);
const C_RED: Color       = Color::Rgb(255, 80, 80);
const C_YELLOW: Color    = Color::Rgb(255, 210, 60);
const C_ORANGE: Color    = Color::Rgb(255, 140, 40);
const C_CYAN: Color      = Color::Rgb(60, 210, 210);
const C_LOG_INFO: Color  = Color::Rgb(140, 200, 255);
const C_LOG_ERR: Color   = Color::Rgb(255, 100, 100);
const C_LOG_WARN: Color  = Color::Rgb(255, 200, 80);

// ─── Main render entry ────────────────────────────────────────────────────────
pub fn render(f: &mut Frame, app: &App) {
    let full = f.area();
    f.render_widget(
        Block::default().style(Style::default().bg(C_BG)),
        full,
    );
    
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(10),
            Constraint::Length(3),
        ])
        .split(full);

    render_title(f, app, outer[0]);
    render_body(f, app, outer[1]);
    render_statusbar(f, app, outer[2]);

    match app.vpn_state {
        VpnState::WaitingToken => {
            render_token_popup(f, app, full);
        }
        VpnState::WaitingCert => {
            if let Some(cert) = &app.pending_cert {
                render_cert_popup(f, app, cert, full);
            }
        }
        _ => {
            if let Some((msg, level)) = &app.notification {
                render_notification(f, msg, level, full);
            }
        }
    }
}

// ─── Title Bar ───────────────────────────────────────────────────────────────
fn render_title(f: &mut Frame, app: &App, area: Rect) {
    let (state_color, state_icon) = match &app.vpn_state {
        VpnState::Disconnected  => (C_DIM,     "● "),
        VpnState::Connecting    => (C_YELLOW,  "◌ "),
        VpnState::WaitingToken  => (C_ORANGE,  "◎ "),
        VpnState::WaitingCert   => (C_ORANGE,  "◎ "),
        VpnState::Connected     => (C_GREEN,   "● "),
        VpnState::Disconnecting => (C_YELLOW,  "◌ "),
        VpnState::Error(_)      => (C_RED,     "✖ "),
    };

    let mode_indicator = match app.ui_mode {
        UiMode::ProfileList => "📋 PROFILES",
        UiMode::NewProfile => "✨ NEW PROFILE",
        UiMode::EditProfile => "✏️ EDIT PROFILE",
        UiMode::Connect => "🔌 CONNECT",
    };

    let title_line = Line::from(vec![
        Span::styled("  FortiVPN  ", Style::default().fg(C_FOCUS).add_modifier(Modifier::BOLD)),
        Span::styled("TUI   ", Style::default().fg(C_DIM)),
        Span::styled("│   ", Style::default().fg(C_BORDER)),
        Span::styled(format!("{}   ", mode_indicator), Style::default().fg(C_CYAN).add_modifier(Modifier::BOLD)),
        Span::styled("│   ", Style::default().fg(C_BORDER)),
        Span::styled(format!("{}  ", state_icon), Style::default().fg(state_color)),
        Span::styled(
            app.vpn_state.label(),
            Style::default().fg(state_color).add_modifier(Modifier::BOLD),
        ),
    ]);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(C_BORDER))
        .style(Style::default().bg(C_SURFACE));

    let paragraph = Paragraph::new(title_line)
        .block(block)
        .alignment(Alignment::Left);

    f.render_widget(paragraph, area);
}

// ─── Main Body ───────────────────────────────────────────────────────────────
fn render_body(f: &mut Frame, app: &App, area: Rect) {
    match app.ui_mode {
        UiMode::ProfileList => {
            let panels = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([
                    Constraint::Percentage(40),
                    Constraint::Percentage(60),
                ])
                .split(area);
            render_profile_list(f, app, panels[0]);
            render_profile_details(f, app, panels[1]);
        }
        UiMode::NewProfile | UiMode::EditProfile => {
            render_profile_form(f, app, area);
        }
        UiMode::Connect => {
            let panels = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([
                    Constraint::Length(42),
                    Constraint::Min(10),
                ])
                .split(area);
            render_controls(f, app, panels[0]);
            render_logs(f, app, panels[1]);
        }
    }
}

// ─── Profile List ────────────────────────────────────────────────────────────
fn render_profile_list(f: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .title(" 📋 VPN Connections ")
        .title_style(Style::default().fg(C_FOCUS).add_modifier(Modifier::BOLD))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(C_BORDER))
        .style(Style::default().bg(C_SURFACE));
    
    let inner = block.inner(area);
    f.render_widget(&block, area);
    
    if app.profiles.is_empty() {
        let empty_msg = Paragraph::new(Span::styled(
            "Belum ada koneksi.\n\nTekan [N] untuk membuat baru",
            Style::default().fg(C_DIM),
        ))
        .alignment(Alignment::Center);
        f.render_widget(empty_msg, inner);
        return;
    }
    
    let items: Vec<ListItem> = app.profiles
        .iter()
        .enumerate()
        .map(|(i, profile)| {
            let is_selected = i == app.selected_profile_index;
            let is_focused = matches!(app.focus, Focus::ProfileItem(idx) if idx == i);
            
            let style = if is_focused {
                Style::default().fg(C_BG).bg(C_FOCUS).add_modifier(Modifier::BOLD)
            } else if is_selected {
                Style::default().fg(C_GREEN).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(C_TEXT)
            };
            
            let indicator = if is_selected { "▶ " } else { "  " };
            let cert_indicator = if profile.trusted_cert.is_some() { "✓" } else { "?" };
            
            ListItem::new(Line::from(vec![
                Span::styled(format!("{}{}", indicator, profile.name), style),
                Span::styled(format!(" @ {}:{}", profile.host, profile.port), Style::default().fg(C_DIM)),
                Span::styled(format!(" [{}]", cert_indicator), 
                    if profile.trusted_cert.is_some() { Style::default().fg(C_GREEN) } else { Style::default().fg(C_YELLOW) }),
            ]))
        })
        .collect();
    
    let list = List::new(items);
    f.render_widget(list, inner);
}

// ─── Profile Details ─────────────────────────────────────────────────────────
fn render_profile_details(f: &mut Frame, app: &App, area: Rect) {
    if app.profiles.is_empty() {
        return;
    }
    
    if let Some(profile) = app.profiles.get(app.selected_profile_index) {
        let block = Block::default()
            .title(" 📄 Connection Details ")
            .title_style(Style::default().fg(C_FOCUS).add_modifier(Modifier::BOLD))
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(C_BORDER))
            .style(Style::default().bg(C_SURFACE));
        
        let inner = block.inner(area);
        f.render_widget(block, area);
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .margin(1)
            .constraints([
                Constraint::Length(1),
                Constraint::Length(1),
                Constraint::Length(1),
                Constraint::Length(1),
                Constraint::Length(1),
                Constraint::Length(1),
                Constraint::Length(3),
                Constraint::Min(0),
            ])
            .split(inner);
        
        let label_style = Style::default().fg(C_DIM);
        let value_style = Style::default().fg(C_TEXT);
        
        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled("Nama:     ", label_style),
                Span::styled(&profile.name, value_style.add_modifier(Modifier::BOLD)),
            ])),
            rows[0],
        );
        
        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled("Host:     ", label_style),
                Span::styled(format!("{}:{}", profile.host, profile.port), value_style),
            ])),
            rows[1],
        );
        
        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled("Username: ", label_style),
                Span::styled(&profile.username, value_style),
            ])),
            rows[2],
        );
        
        let cert_status = if profile.trusted_cert.is_some() { "✓ Trusted" } else { "⚠ Not trusted" };
        let cert_color = if profile.trusted_cert.is_some() { C_GREEN } else { C_YELLOW };
        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled("Cert:     ", label_style),
                Span::styled(cert_status, Style::default().fg(cert_color)),
            ])),
            rows[3],
        );
        
        let password_status = if profile.save_password { "✓ Saved" } else { "✗ Not saved" };
        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled("Password: ", label_style),
                Span::styled(password_status, Style::default().fg(if profile.save_password { C_GREEN } else { C_DIM })),
            ])),
            rows[4],
        );
        
        let sudo_status = if profile.use_sudo_password { "✓ Enabled" } else { "✗ Disabled" };
        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled("Sudo:     ", label_style),
                Span::styled(sudo_status, Style::default().fg(if profile.use_sudo_password { C_GREEN } else { C_DIM })),
            ])),
            rows[5],
        );
        
        // Action buttons
        let btn_area = rows[6];
        let btn_rows = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(33),
                Constraint::Percentage(34),
                Constraint::Percentage(33),
            ])
            .split(btn_area);
        
        let connect_btn = Paragraph::new(" 🔌 Connect ")
            .block(Block::default().borders(Borders::ALL).border_type(BorderType::Rounded))
            .alignment(Alignment::Center);
        f.render_widget(connect_btn, btn_rows[0]);
        
        let edit_btn = Paragraph::new(" ✏️ Edit ")
            .block(Block::default().borders(Borders::ALL).border_type(BorderType::Rounded))
            .alignment(Alignment::Center);
        f.render_widget(edit_btn, btn_rows[1]);
        
        let delete_btn = Paragraph::new(" 🗑️ Delete ")
            .block(Block::default().borders(Borders::ALL).border_type(BorderType::Rounded))
            .alignment(Alignment::Center);
        f.render_widget(delete_btn, btn_rows[2]);
    }
}

// ─── Profile Form ────────────────────────────────────────────────────────────
fn render_profile_form(f: &mut Frame, app: &App, area: Rect) {
    let title = match app.ui_mode {
        UiMode::NewProfile => " ✨ New VPN Connection ",
        UiMode::EditProfile => " ✏️ Edit VPN Connection ",
        _ => " VPN Connection ",
    };
    
    let block = Block::default()
        .title(Span::styled(title, Style::default().fg(C_FOCUS).add_modifier(Modifier::BOLD)))
        .borders(Borders::ALL)
        .border_type(BorderType::Thick)
        .border_style(Style::default().fg(C_FOCUS))
        .style(Style::default().bg(C_SURFACE));
    
    let inner = block.inner(area);
    f.render_widget(block, area);
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(3),
            Constraint::Min(0),
        ])
        .split(inner);
    
    render_input(f, rows[0], " Profile Name ", &app.profile_name,
        matches!(app.focus, Focus::ProfileName), false, "My VPN");
    render_input(f, rows[1], " Host ", &app.profile_host,
        matches!(app.focus, Focus::Host), false, "vpn.example.com");
    render_input(f, rows[2], " Port ", &app.profile_port,
        matches!(app.focus, Focus::Port), false, "443");
    render_input(f, rows[3], " Username ", &app.profile_username,
        matches!(app.focus, Focus::Username), false, "user@domain");
    render_input(f, rows[4], " Password ", &app.profile_password,
        matches!(app.focus, Focus::Password), !app.show_password, "••••••••");
    render_input(f, rows[5], " Sudo Password ", &app.profile_sudo_password,
        matches!(app.focus, Focus::SudoPassword), true, "kosongkan jika NOPASSWD");
    
    let save_pwd_style = if matches!(app.focus, Focus::SavePassword) {
        Style::default().fg(C_FOCUS).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(C_DIM)
    };
    let save_pwd_check = if app.profile_save_password { "[✓]" } else { "[ ]" };
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(format!(" {} Simpan password", save_pwd_check), save_pwd_style),
        ])),
        rows[6],
    );
    
    let use_sudo_style = if matches!(app.focus, Focus::UseSudoPassword) {
        Style::default().fg(C_FOCUS).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(C_DIM)
    };
    let use_sudo_check = if app.profile_use_sudo_password { "[✓]" } else { "[ ]" };
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(format!(" {} Gunakan sudo password", use_sudo_check), use_sudo_style),
        ])),
        rows[7],
    );
    
    let btn_area = rows[8];
    let btn_rows = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(btn_area);
    
    let save_btn = Paragraph::new(" 💾 Save ")
        .block(Block::default().borders(Borders::ALL).border_type(BorderType::Rounded))
        .alignment(Alignment::Center);
    let cancel_btn = Paragraph::new(" ❌ Cancel ")
        .block(Block::default().borders(Borders::ALL).border_type(BorderType::Rounded))
        .alignment(Alignment::Center);
    
    f.render_widget(save_btn, btn_rows[0]);
    f.render_widget(cancel_btn, btn_rows[1]);
}

// ─── Controls Panel ──────────────────────────────────────────────────────────
fn render_controls(f: &mut Frame, app: &App, area: Rect) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(1),
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Min(0),
        ])
        .split(area);
    
    let outer = Block::default()
        .title(" ⚙ Configuration ")
        .title_style(Style::default().fg(C_FOCUS).add_modifier(Modifier::BOLD))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(C_BORDER))
        .style(Style::default().bg(C_SURFACE));
    f.render_widget(outer, area);

    let host_display = format!("{}:{}", app.host, app.port);
    render_input(f, rows[0], " Host:Port  ", &host_display,
        app.focus == Focus::Host, false, "vpn.example.com:443");
    render_input(f, rows[1], " Username  ", &app.username,
        app.focus == Focus::Username, false, "user@domain");
    render_input(f, rows[2], " Password  ", &app.password,
        app.focus == Focus::Password, !app.show_password, "••••••••");

    let sudo_hint = if app.sudo_password.is_empty() {
        "kosongkan jika sudoers/pkexec"
    } else { "••• terisi" };
    render_input(f, rows[3], " Sudo Password  ", &app.sudo_password,
        app.focus == Focus::SudoPassword, true, sudo_hint);

    let can_connect = matches!(app.vpn_state, VpnState::Disconnected | VpnState::Error(_));
    let conn_focused = app.focus == Focus::Connect;
    let conn_style = match (can_connect, conn_focused) {
        (true, true) => Style::default().fg(C_BG).bg(C_GREEN).add_modifier(Modifier::BOLD),
        (true, false) => Style::default().fg(C_GREEN).add_modifier(Modifier::BOLD),
        _ => Style::default().fg(C_DIM),
    };
    f.render_widget(
        Paragraph::new(if conn_focused { " ▶  CONNECT  [Enter]  " } else { " ▶  CONNECT  " })
            .block(Block::default().borders(Borders::ALL)
                .border_type(if conn_focused { BorderType::Thick } else { BorderType::Rounded })
                .border_style(Style::default().fg(if conn_focused { C_GREEN } else { C_BORDER })))
            .style(conn_style).alignment(Alignment::Center),
        rows[5],
    );

    let can_disconn = matches!(app.vpn_state,
        VpnState::Connected | VpnState::Connecting | VpnState::WaitingToken | VpnState::WaitingCert);
    let disc_focused = app.focus == Focus::Disconnect;
    let disc_style = match (can_disconn, disc_focused) {
        (true, true) => Style::default().fg(C_BG).bg(C_RED).add_modifier(Modifier::BOLD),
        (true, false) => Style::default().fg(C_RED).add_modifier(Modifier::BOLD),
        _ => Style::default().fg(C_DIM),
    };
    f.render_widget(
        Paragraph::new(if disc_focused { " ■  DISCONNECT  [Enter]  " } else { " ■  DISCONNECT  " })
            .block(Block::default().borders(Borders::ALL)
                .border_type(if disc_focused { BorderType::Thick } else { BorderType::Rounded })
                .border_style(Style::default().fg(if disc_focused { C_RED } else { C_BORDER })))
            .style(disc_style).alignment(Alignment::Center),
        rows[6],
    );
}

// ─── Input Widget ─────────────────────────────────────────────────────────────
fn render_input(
    f: &mut Frame,
    area: Rect,
    label: &str,
    value: &str,
    focused: bool,
    masked: bool,
    placeholder: &str,
) {
    let border_style = if focused {
        Style::default().fg(C_FOCUS)
    } else {
        Style::default().fg(C_BORDER)
    };
    let border_type = if focused {
        BorderType::Thick
    } else {
        BorderType::Rounded
    };

    let label_style = if focused {
        Style::default()
            .fg(C_FOCUS)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(C_DIM)
    };

    let block = Block::default()
        .title(Span::styled(label, label_style))
        .borders(Borders::ALL)
        .border_type(border_type)
        .border_style(border_style)
        .padding(Padding::horizontal(1));

    let display = if value.is_empty() {
        Span::styled(placeholder, Style::default().fg(C_DIM))
    } else if masked {
        Span::styled(
            "•".repeat(value.len()),
            Style::default().fg(C_TEXT),
        )
    } else {
        Span::styled(value, Style::default().fg(C_TEXT))
    };

    let content = if focused && !value.is_empty() {
        Line::from(vec![
            Span::styled(
                if masked { "•".repeat(value.len()) } else { value.to_string() },
                Style::default().fg(C_TEXT),
            ),
            Span::styled("█", Style::default().fg(C_FOCUS)),
        ])
    } else if focused && value.is_empty() {
        Line::from(vec![
            Span::styled(placeholder, Style::default().fg(C_DIM)),
            Span::styled("█", Style::default().fg(C_FOCUS)),
        ])
    } else {
        Line::from(display)
    };

    f.render_widget(
        Paragraph::new(content).block(block),
        area,
    );
}

// ─── Log Panel ───────────────────────────────────────────────────────────────
fn render_logs(f: &mut Frame, app: &App, area: Rect) {
    let focused = app.focus == Focus::Logs;
    let border_style = if focused {
        Style::default().fg(C_FOCUS)
    } else {
        Style::default().fg(C_BORDER)
    };

    let title_style = if focused {
        Style::default().fg(C_FOCUS).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(C_DIM)
    };

    let log_count = app.logs.len();
    let title = format!(" 📋 Log ({} baris)  ", log_count);

    let block = Block::default()
        .title(Span::styled(&title, title_style))
        .borders(Borders::ALL)
        .border_type(if focused { BorderType::Thick } else { BorderType::Rounded })
        .border_style(border_style)
        .style(Style::default().bg(C_SURFACE));

    let inner = block.inner(area);
    f.render_widget(block, area);

    if app.logs.is_empty() {
        let empty = Paragraph::new(Span::styled(
            "Belum ada log. Pilih profile dan tekan Connect untuk memulai...",
            Style::default().fg(C_DIM),
        ))
        .alignment(Alignment::Center);
        f.render_widget(empty, inner);
        return;
    }

    let visible_height = inner.height as usize;
    let total = app.logs.len();
    let scroll = app.log_scroll;

    let start = if total > visible_height {
        scroll.min(total - visible_height)
    } else {
        0
    };
    let end = (start + visible_height).min(total);

    let items: Vec<ListItem> = app.logs[start..end]
        .iter()
        .map(|line| {
            let style = if line.contains("[ERR]") || line.contains("ERROR") {
                Style::default().fg(C_LOG_ERR)
            } else if line.contains("[WARN]") {
                Style::default().fg(C_LOG_WARN)
            } else if line.contains("[TOKEN]") {
                Style::default().fg(C_ORANGE)
            } else if line.contains("Connected") || line.contains("tunnel is up") {
                Style::default().fg(C_GREEN).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(C_LOG_INFO)
            };
            ListItem::new(Line::from(Span::styled(line.clone(), style)))
        })
        .collect();

    if total > visible_height {
        let pct = ((scroll as f64 / (total - visible_height) as f64) * 100.0) as u8;
        let scroll_info = format!(" {}% ↕  ", pct);
        let scroll_area = Rect {
            x: area.x + area.width - scroll_info.len() as u16 - 2,
            y: area.y,
            width: scroll_info.len() as u16,
            height: 1,
        };
        f.render_widget(
            Paragraph::new(Span::styled(scroll_info, Style::default().fg(C_DIM))),
            scroll_area,
        );
    }

    f.render_widget(List::new(items), inner);
}

// ─── Status / Hint Bar ───────────────────────────────────────────────────────
fn render_statusbar(f: &mut Frame, app: &App, area: Rect) {
    let hints = match app.ui_mode {
        UiMode::ProfileList => vec![
            ("↑/↓ ", "Pilih profile "),
            ("Enter ", "Connect "),
            ("N ", "New "),
            ("E ", "Edit "),
            ("D ", "Delete "),
            ("Q/^C ", "Keluar "),
        ],
        UiMode::NewProfile | UiMode::EditProfile => vec![
            ("Tab ", "Next field "),
            ("Enter ", "Save "),
            ("Esc ", "Cancel "),
            ("Q/^C ", "Keluar "),
        ],
        UiMode::Connect => vec![
            ("Tab ", "Fokus "),
            ("Enter ", "Aksi "),
            ("^P ", "Show/Hide "),
            ("Back ", "Back to Profiles "),
            ("Q/^C ", "Keluar "),
        ],
    };
    
    let mut spans = vec![Span::styled("  ", Style::default())];
    for (i, (key, desc)) in hints.iter().enumerate() {
        spans.push(Span::styled(
            format!("[{}]", key),
            Style::default().fg(C_CYAN).add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::styled(
            format!(" {} ", desc),
            Style::default().fg(C_DIM),
        ));
        if i < hints.len() - 1 {
            spans.push(Span::styled(" │ ", Style::default().fg(C_BORDER)));
        }
    }

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(C_BORDER))
        .style(Style::default().bg(C_SURFACE));

    f.render_widget(
        Paragraph::new(Line::from(spans)).block(block),
        area,
    );
}

// ─── Floating Notification ───────────────────────────────────────────────────
fn render_notification(f: &mut Frame, msg: &str, level: &NotifLevel, area: Rect) {
    let (bg, fg, icon) = match level {
        NotifLevel::Info    => (Color::Rgb(30, 50, 80),  C_FOCUS,   "ℹ "),
        NotifLevel::Success => (Color::Rgb(20, 60, 30),  C_GREEN,   "✔ "),
        NotifLevel::Warning => (Color::Rgb(70, 55, 10),  C_YELLOW,  "⚠ "),
        NotifLevel::Error   => (Color::Rgb(70, 20, 20),  C_RED,     "✖ "),
    };
    let width = (msg.len() as u16 + 8).min(area.width - 4).max(20);
    let height = 3u16;
    let x = area.x + (area.width - width) / 2;
    let y = area.y + 1;

    let notif_area = Rect { x, y, width, height };
    f.render_widget(Clear, notif_area);

    let content = format!("{} {}", icon, msg);
    f.render_widget(
        Paragraph::new(Span::styled(content, Style::default().fg(fg).add_modifier(Modifier::BOLD)))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .border_style(Style::default().fg(fg))
                    .style(Style::default().bg(bg)),
            )
            .alignment(Alignment::Center),
        notif_area,
    );
}

// ─── Certificate Approval Popup ───────────────────────────────────────────────
fn render_cert_popup(f: &mut Frame, app: &App, cert: &crate::app::CertInfo, area: Rect) {
    let popup_w = (area.width as f32 * 0.72) as u16;
    let popup_h = 18u16;
    let popup_x = area.x + (area.width.saturating_sub(popup_w)) / 2;
    let popup_y = area.y + (area.height.saturating_sub(popup_h)) / 2;
    let popup_area = Rect { x: popup_x, y: popup_y, width: popup_w, height: popup_h };
    f.render_widget(Clear, popup_area);

    let block = Block::default()
        .title(Span::styled(
            " ⚠  CERTIFICATE TIDAK DIPERCAYA   ",
            Style::default().fg(Color::Rgb(255, 200, 50)).add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_type(BorderType::Thick)
        .border_style(Style::default().fg(Color::Rgb(255, 200, 50)))
        .style(Style::default().bg(Color::Rgb(20, 18, 10)));
    f.render_widget(block, popup_area);

    let inner = Rect {
        x: popup_area.x + 2,
        y: popup_area.y + 1,
        width: popup_area.width.saturating_sub(4),
        height: popup_area.height.saturating_sub(2),
    };

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(3),
            Constraint::Min(0),
        ])
        .split(inner);

    f.render_widget(
        Paragraph::new(Span::styled(
            "Gateway VPN menggunakan certificate yang tidak ada di whitelist:",
            Style::default().fg(Color::Rgb(220, 200, 140)),
        )),
        rows[0],
    );

    let detail_style = Style::default().fg(Color::Rgb(180, 220, 255));
    let label_style = Style::default().fg(Color::Rgb(100, 160, 220));

    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("  Subject CN  :  ", label_style),
            Span::styled(&cert.subject_cn, detail_style.add_modifier(Modifier::BOLD)),
        ])),
        rows[2],
    );
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("  Org         :  ", label_style),
            Span::styled(&cert.subject_org, detail_style),
        ])),
        rows[3],
    );
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("  Issuer CN   :  ", label_style),
            Span::styled(&cert.issuer_cn, detail_style),
        ])),
        rows[4],
    );

    f.render_widget(
        Paragraph::new(Span::styled("  SHA256 Digest: ", label_style)),
        rows[6],
    );

    let hash_display = if cert.hash.len() > (inner.width as usize).saturating_sub(4) {
        format!("  {}... ", &cert.hash[..inner.width.saturating_sub(8) as usize])
    } else {
        format!("  {} ", cert.hash)
    };
    f.render_widget(
        Paragraph::new(Span::styled(hash_display, Style::default().fg(Color::Rgb(255, 220, 80)).add_modifier(Modifier::BOLD))),
        rows[7],
    );

    f.render_widget(
        Paragraph::new(Span::styled(
            "Apakah Anda mempercayai certificate ini?",
            Style::default().fg(Color::Rgb(220, 220, 220)).add_modifier(Modifier::BOLD),
        ))
        .alignment(Alignment::Center),
        rows[9],
    );

    let btn_area = rows[11];
    let btn_rows = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(btn_area);

    let accept_focused = app.focus == Focus::CertAccept;
    let deny_focused = app.focus == Focus::CertDeny;

    let accept_style = if accept_focused {
        Style::default().fg(Color::Rgb(20, 20, 20)).bg(Color::Rgb(80, 220, 120)).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::Rgb(80, 220, 120)).add_modifier(Modifier::BOLD)
    };
    let accept_block = Block::default()
        .borders(Borders::ALL)
        .border_type(if accept_focused { BorderType::Thick } else { BorderType::Rounded })
        .border_style(Style::default().fg(Color::Rgb(80, 220, 120)));

    f.render_widget(
        Paragraph::new(" ✔  PERCAYA  & CONNECT  [Y/Enter] ")
            .block(accept_block)
            .style(accept_style)
            .alignment(Alignment::Center),
        btn_rows[0],
    );

    let deny_style = if deny_focused {
        Style::default().fg(Color::Rgb(20, 20, 20)).bg(Color::Rgb(255, 80, 80)).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::Rgb(255, 80, 80)).add_modifier(Modifier::BOLD)
    };
    let deny_block = Block::default()
        .borders(Borders::ALL)
        .border_type(if deny_focused { BorderType::Thick } else { BorderType::Rounded })
        .border_style(Style::default().fg(Color::Rgb(255, 80, 80)));

    f.render_widget(
        Paragraph::new(" ✖  TOLAK  [N/Esc] ")
            .block(deny_block)
            .style(deny_style)
            .alignment(Alignment::Center),
        btn_rows[1],
    );

    let hint_area = Rect {
        x: popup_area.x + 2,
        y: popup_area.y + popup_area.height - 1,
        width: popup_area.width.saturating_sub(4),
        height: 1,
    };
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("[Tab/←/→] ", Style::default().fg(Color::Rgb(60, 210, 210)).add_modifier(Modifier::BOLD)),
            Span::styled("pindah    ", Style::default().fg(Color::Rgb(80, 80, 100))),
            Span::styled("[Y] ", Style::default().fg(Color::Rgb(60, 210, 210)).add_modifier(Modifier::BOLD)),
            Span::styled("accept    ", Style::default().fg(Color::Rgb(80, 80, 100))),
            Span::styled("[N/Esc] ", Style::default().fg(Color::Rgb(60, 210, 210)).add_modifier(Modifier::BOLD)),
            Span::styled("tolak   ", Style::default().fg(Color::Rgb(80, 80, 100))),
        ])),
        hint_area,
    );
}

// ─── Token Input Popup ────────────────────────────────────────────────────────
fn render_token_popup(f: &mut Frame, app: &App, area: Rect) {
    let popup_w = 52u16;
    let popup_h = 11u16;
    let popup_x = area.x + area.width.saturating_sub(popup_w) / 2;
    let popup_y = area.y + area.height.saturating_sub(popup_h) / 2;
    let popup_area = Rect { x: popup_x, y: popup_y, width: popup_w, height: popup_h };
    f.render_widget(Clear, popup_area);

    f.render_widget(
        Block::default()
            .title(Span::styled(
                " ⚡  TWO-FACTOR AUTHENTICATION   ",
                Style::default().fg(C_ORANGE).add_modifier(Modifier::BOLD),
            ))
            .borders(Borders::ALL)
            .border_type(BorderType::Thick)
            .border_style(Style::default().fg(C_ORANGE))
            .style(Style::default().bg(Color::Rgb(20, 14, 5))),
        popup_area,
    );

    let inner = Rect {
        x: popup_area.x + 2,
        y: popup_area.y + 1,
        width: popup_area.width.saturating_sub(4),
        height: popup_area.height.saturating_sub(2),
    };

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(3),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(inner);

    f.render_widget(
        Paragraph::new(Span::styled(
            "Masukkan OTP token yang dikirim ke email/app Anda:",
            Style::default().fg(Color::Rgb(220, 200, 140)),
        )),
        rows[0],
    );

    let token_display = &app.token_input;
    let cursor = if (f.count() / 5) % 2 == 0 { "█ " } else { "  " };
    let input_content = if token_display.is_empty() {
        Line::from(vec![
            Span::styled("   ", Style::default()),
            Span::styled("Ketik token di sini... ", Style::default().fg(C_DIM)),
            Span::styled(cursor, Style::default().fg(C_ORANGE)),
        ])
    } else {
        Line::from(vec![
            Span::styled("   ", Style::default()),
            Span::styled(
                token_display.as_str(),
                Style::default().fg(Color::Rgb(255, 230, 80)).add_modifier(Modifier::BOLD),
            ),
            Span::styled(cursor, Style::default().fg(C_ORANGE)),
        ])
    };

    f.render_widget(
        Paragraph::new(input_content)
            .block(
                Block::default()
                    .title(Span::styled(" OTP Token  ", Style::default().fg(C_ORANGE)))
                    .borders(Borders::ALL)
                    .border_type(BorderType::Thick)
                    .border_style(Style::default().fg(C_ORANGE)),
            ),
        rows[2],
    );

    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("[Enter] ", Style::default().fg(C_CYAN).add_modifier(Modifier::BOLD)),
            Span::styled("submit    ", Style::default().fg(C_DIM)),
            Span::styled("[Backspace] ", Style::default().fg(C_CYAN).add_modifier(Modifier::BOLD)),
            Span::styled("hapus    ", Style::default().fg(C_DIM)),
            Span::styled("[Esc] ", Style::default().fg(Color::Rgb(255, 120, 80)).add_modifier(Modifier::BOLD)),
            Span::styled("batalkan ", Style::default().fg(C_DIM)),
        ])).alignment(Alignment::Center),
        rows[4],
    );
}