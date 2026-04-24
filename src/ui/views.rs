use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, List, ListItem, Paragraph},
    Frame,
};

use crate::app::{App, Focus, UiMode, VpnState};

use super::{
    theme::{
        C_BG, C_BORDER, C_CYAN, C_DIM, C_FOCUS, C_GREEN, C_LOG_ERR, C_LOG_INFO, C_LOG_WARN,
        C_ORANGE, C_RED, C_SURFACE, C_TEXT, C_YELLOW,
    },
    widgets::render_input,
};

pub fn render_title(f: &mut Frame, app: &App, area: Rect) {
    let (state_color, state_icon) = match &app.vpn_state {
        VpnState::Disconnected => (C_DIM, "● "),
        VpnState::Connecting => (C_YELLOW, "◌ "),
        VpnState::WaitingToken | VpnState::WaitingCert => (C_ORANGE, "◎ "),
        VpnState::Connected => (C_GREEN, "● "),
        VpnState::Disconnecting => (C_YELLOW, "◌ "),
        VpnState::Error(_) => (C_RED, "✖ "),
    };

    let mode_indicator = match app.ui_mode {
        UiMode::ProfileList => "📋 PROFILES",
        UiMode::NewProfile => "✨ NEW PROFILE",
        UiMode::EditProfile => "✏️ EDIT PROFILE",
        UiMode::Connect => "🔌 CONNECT",
        UiMode::Help => "🛈 HELP",
    };

    let title_line = Line::from(vec![
        Span::styled("  FortiVPN  ", Style::default().fg(C_FOCUS).add_modifier(Modifier::BOLD)),
        Span::styled("TUI   ", Style::default().fg(C_DIM)),
        Span::styled("│   ", Style::default().fg(C_BORDER)),
        Span::styled(format!("{}   ", mode_indicator), Style::default().fg(C_CYAN).add_modifier(Modifier::BOLD)),
        Span::styled("│   ", Style::default().fg(C_BORDER)),
        Span::styled(format!("{}  ", state_icon), Style::default().fg(state_color)),
        Span::styled(app.vpn_state.label(), Style::default().fg(state_color).add_modifier(Modifier::BOLD)),
    ]);

    let block = panel_block("").style(Style::default().bg(C_SURFACE));
    f.render_widget(Paragraph::new(title_line).block(block), area);
}

pub fn render_profile_list_screen(f: &mut Frame, app: &App, area: Rect) {
    let panels = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
        .split(area);
    render_profile_list(f, app, panels[0]);
    render_profile_details(f, app, panels[1]);
}

pub fn render_profile_form_screen(f: &mut Frame, app: &App, area: Rect) {
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

    let form = &app.profile_form;
    render_input(f, rows[0], " Profile Name ", &form.name, matches!(app.focus, Focus::ProfileName), false, "My VPN");
    render_input(f, rows[1], " Host ", &form.host, matches!(app.focus, Focus::Host), false, "vpn.example.com");
    render_input(f, rows[2], " Port ", &form.port, matches!(app.focus, Focus::Port), false, "443");
    render_input(f, rows[3], " Username ", &form.username, matches!(app.focus, Focus::Username), false, "user@domain");
    render_input(f, rows[4], " Password ", &form.password, matches!(app.focus, Focus::Password), !app.show_password, "••••••••");
    render_input(f, rows[5], " Sudo Password ", &form.sudo_password, matches!(app.focus, Focus::SudoPassword), true, "kosongkan jika NOPASSWD");

    render_checkbox(f, rows[6], "Simpan password", form.save_password, matches!(app.focus, Focus::SavePassword));
    render_checkbox(f, rows[7], "Gunakan sudo password", form.use_sudo_password, matches!(app.focus, Focus::UseSudoPassword));

    let btn_rows = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(rows[8]);
    render_action_button(f, btn_rows[0], " 💾 Save ");
    render_action_button(f, btn_rows[1], " ❌ Cancel ");
}

pub fn render_connect_screen(f: &mut Frame, app: &App, area: Rect) {
    let panels = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(42), Constraint::Min(10)])
        .split(area);
    render_controls(f, app, panels[0]);
    render_logs(f, app, panels[1]);
}

pub fn render_previous_screen(f: &mut Frame, app: &App, area: Rect) {
    match app.previous_ui_mode.as_ref() {
        Some(UiMode::ProfileList) => render_profile_list_screen(f, app, area),
        Some(UiMode::Connect) => render_connect_screen(f, app, area),
        _ => {}
    }
}

pub fn render_statusbar(f: &mut Frame, app: &App, area: Rect) {
    let hints = if app.ui_mode == UiMode::Help {
        vec![("ESC / F1", "Tutup Help")]
    } else {
        match app.ui_mode {
            UiMode::ProfileList => vec![
                ("↑/↓", "Pilih"),
                ("Enter/F5", "Connect"),
                ("F2/N", "New"),
                ("F3/E", "Edit"),
                ("F4/D", "Delete"),
                ("ESC/Ctrl+B", "Back"),
                ("F1", "Help"),
                ("Ctrl+Q", "Quit"),
            ],
            UiMode::NewProfile | UiMode::EditProfile => vec![
                ("Tab", "Next"),
                ("Shift+Tab", "Prev"),
                ("Space", "Toggle"),
                ("Enter", "Save"),
                ("ESC/Ctrl+B", "Cancel"),
                ("F1", "Help"),
            ],
            UiMode::Connect => vec![
                ("Tab", "Focus"),
                ("Enter", "Action"),
                ("Ctrl+P", "Show/Hide"),
                ("Ctrl+S", "Save"),
                ("ESC/Ctrl+B", "Back"),
                ("F1", "Help"),
                ("Ctrl+Q", "Quit"),
            ],
            UiMode::Help => vec![],
        }
    };

    let mut spans = vec![Span::styled("  ", Style::default())];
    for (index, (key, desc)) in hints.iter().enumerate() {
        spans.push(Span::styled(format!("[{}]", key), Style::default().fg(C_CYAN).add_modifier(Modifier::BOLD)));
        spans.push(Span::styled(format!("{} ", desc), Style::default().fg(C_DIM)));
        if index < hints.len().saturating_sub(1) {
            spans.push(Span::styled("│ ", Style::default().fg(C_BORDER)));
        }
    }

    let block = panel_block("").style(Style::default().bg(C_SURFACE));
    f.render_widget(Paragraph::new(Line::from(spans)).block(block), area);
}

fn render_profile_list(f: &mut Frame, app: &App, area: Rect) {
    let block = panel_block(" 📋 VPN Connections ");
    let inner = block.inner(area);
    f.render_widget(block, area);

    if app.profiles.is_empty() {
        f.render_widget(
            Paragraph::new(Span::styled("Belum ada koneksi.\n\nTekan [F2] atau [N] untuk membuat baru", Style::default().fg(C_DIM))).alignment(Alignment::Center),
            inner,
        );
        return;
    }

    let items: Vec<ListItem> = app.profiles.iter().enumerate().map(|(i, profile)| {
        let is_selected = i == app.selected_profile_index;
        let style = if is_selected { Style::default().fg(C_GREEN).add_modifier(Modifier::BOLD) } else { Style::default().fg(C_TEXT) };
        let cert_style = if profile.trusted_cert.is_some() { Style::default().fg(C_GREEN) } else { Style::default().fg(C_YELLOW) };
        ListItem::new(Line::from(vec![
            Span::styled(if is_selected { format!("▶ {}", profile.name) } else { format!("  {}", profile.name) }, style),
            Span::styled(format!(" @ {}:{}", profile.host, profile.port), Style::default().fg(C_DIM)),
            Span::styled(format!(" [{}]", if profile.trusted_cert.is_some() { "✓" } else { "?" }), cert_style),
        ]))
    }).collect();

    f.render_widget(List::new(items), inner);
}

fn render_profile_details(f: &mut Frame, app: &App, area: Rect) {
    let Some(profile) = app.get_current_profile() else {
        return;
    };

    let block = panel_block(" 📄 Connection Details ");
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
            Constraint::Length(4),
            Constraint::Min(0),
        ])
        .split(inner);

    let label_style = Style::default().fg(C_DIM);
    let value_style = Style::default().fg(C_TEXT);
    render_label_value(f, rows[0], "Nama:     ", &profile.name, value_style.add_modifier(Modifier::BOLD), label_style);
    render_label_value(f, rows[1], "Host:     ", &format!("{}:{}", profile.host, profile.port), value_style, label_style);
    render_label_value(f, rows[2], "Username: ", &profile.username, value_style, label_style);
    render_label_value(f, rows[3], "Cert:     ", if profile.trusted_cert.is_some() { "✓ Trusted" } else { "⚠ Not trusted" }, Style::default().fg(if profile.trusted_cert.is_some() { C_GREEN } else { C_YELLOW }), label_style);
    render_label_value(f, rows[4], "Password: ", if profile.save_password { "✓ Saved" } else { "✗ Not saved" }, Style::default().fg(if profile.save_password { C_GREEN } else { C_DIM }), label_style);
    render_label_value(f, rows[5], "Sudo:     ", if profile.use_sudo_password { "✓ Enabled" } else { "✗ Disabled" }, Style::default().fg(if profile.use_sudo_password { C_GREEN } else { C_DIM }), label_style);

    let btn_rows = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(33), Constraint::Percentage(34), Constraint::Percentage(33)])
        .split(rows[6]);
    render_action_button(f, btn_rows[0], " 🔌 Connect ");
    render_action_button(f, btn_rows[1], " ✏️ Edit ");
    render_action_button(f, btn_rows[2], " 🗑️ Delete ");
}

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

    f.render_widget(panel_block(" ⚙ Configuration "), area);
    render_input(f, rows[0], " Host:Port  ", &app.connection.host_port_display(), app.focus == Focus::Host, false, "vpn.example.com:443");
    render_input(f, rows[1], " Username  ", &app.connection.username, app.focus == Focus::Username, false, "user@domain");
    render_input(f, rows[2], " Password  ", &app.connection.password, app.focus == Focus::Password, !app.show_password, "••••••••");

    let sudo_hint = if app.connection.sudo_password.is_empty() { "kosongkan jika sudoers/pkexec" } else { "••• terisi" };
    render_input(f, rows[3], " Sudo Password  ", &app.connection.sudo_password, app.focus == Focus::SudoPassword, true, sudo_hint);

    let can_connect = matches!(app.vpn_state, VpnState::Disconnected | VpnState::Error(_));
    render_stateful_button(f, rows[5], " ▶  CONNECT  ", " ▶  CONNECT  [Enter]  ", can_connect, app.focus == Focus::Connect, C_GREEN);

    let can_disconnect = matches!(app.vpn_state, VpnState::Connected | VpnState::Connecting | VpnState::WaitingToken | VpnState::WaitingCert);
    render_stateful_button(f, rows[6], " ■  DISCONNECT  ", " ■  DISCONNECT  [Enter]  ", can_disconnect, app.focus == Focus::Disconnect, C_RED);
}

fn render_logs(f: &mut Frame, app: &App, area: Rect) {
    let focused = app.focus == Focus::Logs;
    let border_style = if focused { Style::default().fg(C_FOCUS) } else { Style::default().fg(C_BORDER) };
    let title_style = if focused { Style::default().fg(C_FOCUS).add_modifier(Modifier::BOLD) } else { Style::default().fg(C_DIM) };

    let block = Block::default()
        .title(Span::styled(format!(" 📋 Log ({} baris)  ", app.logs.len()), title_style))
        .borders(Borders::ALL)
        .border_type(if focused { BorderType::Thick } else { BorderType::Rounded })
        .border_style(border_style)
        .style(Style::default().bg(C_SURFACE));
    let inner = block.inner(area);
    f.render_widget(block, area);

    if app.logs.is_empty() {
        f.render_widget(
            Paragraph::new(Span::styled("Belum ada log. Pilih profile dan tekan Connect untuk memulai...", Style::default().fg(C_DIM))).alignment(Alignment::Center),
            inner,
        );
        return;
    }

    let visible_height = inner.height as usize;
    let total = app.logs.len();
    let scroll = app.log_scroll;
    let start = if total > visible_height { scroll.min(total - visible_height) } else { 0 };
    let end = (start + visible_height).min(total);

    let items: Vec<ListItem> = app.logs[start..end].iter().map(|line| {
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
    }).collect();

    if total > visible_height {
        let pct = ((scroll as f64 / (total - visible_height) as f64) * 100.0) as u8;
        let scroll_info = format!(" {}% ↕  ", pct);
        let scroll_area = Rect {
            x: area.x + area.width - scroll_info.len() as u16 - 2,
            y: area.y,
            width: scroll_info.len() as u16,
            height: 1,
        };
        f.render_widget(Paragraph::new(Span::styled(scroll_info, Style::default().fg(C_DIM))), scroll_area);
    }

    f.render_widget(List::new(items), inner);
}

fn render_checkbox(f: &mut Frame, area: Rect, label: &str, checked: bool, focused: bool) {
    let style = if focused { Style::default().fg(C_FOCUS).add_modifier(Modifier::BOLD) } else { Style::default().fg(C_DIM) };
    f.render_widget(Paragraph::new(Line::from(vec![Span::styled(format!(" {} {}", if checked { "[✓]" } else { "[ ]" }, label), style)])), area);
}

fn render_stateful_button(f: &mut Frame, area: Rect, label: &str, focused_label: &str, enabled: bool, focused: bool, accent: Color) {
    let style = match (enabled, focused) {
        (true, true) => Style::default().fg(C_BG).bg(accent).add_modifier(Modifier::BOLD),
        (true, false) => Style::default().fg(accent).add_modifier(Modifier::BOLD),
        _ => Style::default().fg(C_DIM),
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(if focused { BorderType::Thick } else { BorderType::Rounded })
        .border_style(Style::default().fg(if focused { accent } else { C_BORDER }));
    f.render_widget(Paragraph::new(if focused { focused_label } else { label }).block(block).style(style).alignment(Alignment::Center), area);
}

fn render_action_button(f: &mut Frame, area: Rect, label: &str) {
    f.render_widget(Paragraph::new(label).block(Block::default().borders(Borders::ALL).border_type(BorderType::Rounded)).alignment(Alignment::Center), area);
}

fn render_label_value(f: &mut Frame, area: Rect, label: &str, value: &str, value_style: Style, label_style: Style) {
    f.render_widget(Paragraph::new(Line::from(vec![Span::styled(label, label_style), Span::styled(value, value_style)])), area);
}

fn panel_block<'a>(title: &'a str) -> Block<'a> {
    Block::default()
        .title(title)
        .title_style(Style::default().fg(C_FOCUS).add_modifier(Modifier::BOLD))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(C_BORDER))
        .style(Style::default().bg(C_SURFACE))
}
