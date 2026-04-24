use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, Paragraph},
    Frame,
};

use crate::app::{App, CertInfo, Focus, NotifLevel};

use super::theme::{C_CYAN, C_DIM, C_FOCUS, C_GREEN, C_ORANGE, C_RED, C_YELLOW};

pub fn render_help_popup(f: &mut Frame, area: Rect) {
    let popup_area = centered_rect(area, 72, 28);
    f.render_widget(Clear, popup_area);

    let block = Block::default()
        .title(Span::styled(" 🛈  HELP - Keyboard Shortcuts  🛈 ", Style::default().fg(C_CYAN).add_modifier(Modifier::BOLD)))
        .borders(Borders::ALL)
        .border_type(BorderType::Thick)
        .border_style(Style::default().fg(C_CYAN))
        .style(Style::default().bg(Color::Rgb(20, 20, 30)));
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
            Constraint::Length(7),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(6),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(7),
            Constraint::Length(1),
            Constraint::Length(2),
            Constraint::Min(0),
        ])
        .split(inner);

    let title_style = Style::default().fg(C_YELLOW).add_modifier(Modifier::BOLD);
    let key_style = Style::default().fg(C_CYAN).add_modifier(Modifier::BOLD);
    let desc_style = Style::default().fg(Color::Rgb(220, 220, 240));

    render_shortcut_group(f, rows[1], rows[2], "═══════════════════════ GLOBAL SHORTCUTS ═══════════════════════", &[("F1", "Buka help ini"), ("ESC / Ctrl+B", "Kembali ke daftar profile / Tutup help"), ("Ctrl+Q / Ctrl+C", "Keluar aplikasi")], title_style, key_style, desc_style);
    render_shortcut_group(f, rows[4], rows[5], "═══════════════════════ PROFILE LIST MODE ═══════════════════════", &[("↑ / ↓", "Pilih profile"), ("Enter / F5", "Connect ke profile terpilih"), ("F2 / N", "Buat profile baru"), ("F3 / E", "Edit profile terpilih"), ("F4 / D", "Hapus profile terpilih (konfirmasi dua kali)")], title_style, key_style, desc_style);
    render_shortcut_group(f, rows[7], rows[8], "═══════════════════════ CONNECT MODE ═══════════════════════", &[("Tab", "Pindah fokus ke field berikutnya"), ("Shift+Tab", "Pindah fokus ke field sebelumnya"), ("Enter", "Connect / Disconnect"), ("Ctrl+P", "Tampilkan/sembunyikan password"), ("Ctrl+S", "Simpan konfigurasi ke profile"), ("↑ / ↓ / PgUp / PgDn", "Scroll log"), ("F5", "Scroll ke bottom log")], title_style, key_style, desc_style);

    f.render_widget(
        Paragraph::new(vec![
            Line::from(Span::styled("  Debug log: jalankan `openfortivpn-tui -d` untuk simpan log ke /tmp/openfortivpn-tui.log", Style::default().fg(C_DIM))),
            Line::from(Span::styled("  Tekan ESC atau F1 untuk menutup help ini", Style::default().fg(C_DIM))),
        ]).alignment(Alignment::Center),
        rows[10],
    );
}

pub fn render_notification(f: &mut Frame, msg: &str, level: &NotifLevel, area: Rect) {
    let (bg, fg, icon) = match level {
        NotifLevel::Info => (Color::Rgb(30, 50, 80), C_FOCUS, "ℹ "),
        NotifLevel::Success => (Color::Rgb(20, 60, 30), C_GREEN, "✔ "),
        NotifLevel::Warning => (Color::Rgb(70, 55, 10), C_YELLOW, "⚠ "),
        NotifLevel::Error => (Color::Rgb(70, 20, 20), C_RED, "✖ "),
    };
    let width = (msg.len() as u16 + 8).min(area.width - 4).max(20);
    let notif_area = Rect {
        x: area.x + (area.width - width) / 2,
        y: area.y + 1,
        width,
        height: 3,
    };

    f.render_widget(Clear, notif_area);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(fg))
        .style(Style::default().bg(bg));

    f.render_widget(
        Paragraph::new(Span::styled(format!("{} {}", icon, msg), Style::default().fg(fg).add_modifier(Modifier::BOLD))).block(block).alignment(Alignment::Center),
        notif_area,
    );
}

pub fn render_cert_popup(f: &mut Frame, app: &App, cert: &CertInfo, area: Rect) {
    let popup_area = centered_rect(area, (area.width as f32 * 0.72) as u16, 20);
    f.render_widget(Clear, popup_area);

    let block = Block::default()
        .title(Span::styled(" ⚠  CERTIFICATE TIDAK DIPERCAYA   ", Style::default().fg(Color::Rgb(255, 200, 50)).add_modifier(Modifier::BOLD)))
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
            Constraint::Length(2),
            Constraint::Length(1),
            Constraint::Length(3),
            Constraint::Min(0),
        ])
        .split(inner);

    let detail_style = Style::default().fg(Color::Rgb(180, 220, 255));
    let label_style = Style::default().fg(Color::Rgb(100, 160, 220));

    f.render_widget(Paragraph::new(Span::styled("Gateway VPN menggunakan certificate yang tidak ada di whitelist:", Style::default().fg(Color::Rgb(220, 200, 140)))), rows[0]);
    render_label_value(f, rows[2], "  Subject CN  :  ", &cert.subject_cn, detail_style.add_modifier(Modifier::BOLD), label_style);
    render_label_value(f, rows[3], "  Org         :  ", &cert.subject_org, detail_style, label_style);
    render_label_value(f, rows[4], "  Issuer CN   :  ", &cert.issuer_cn, detail_style, label_style);
    render_label_value(f, rows[5], "  Detail      :  ", cert.raw_lines.first().map(String::as_str).unwrap_or("-"), Style::default().fg(Color::Rgb(200, 180, 120)), label_style);

    f.render_widget(Paragraph::new(Span::styled("  SHA256 Digest: ", label_style)), rows[6]);
    let hash_display = if cert.hash.len() > inner.width.saturating_sub(4) as usize {
        format!("  {}... ", &cert.hash[..inner.width.saturating_sub(8) as usize])
    } else {
        format!("  {} ", cert.hash)
    };
    f.render_widget(Paragraph::new(Span::styled(hash_display, Style::default().fg(Color::Rgb(255, 220, 80)).add_modifier(Modifier::BOLD))), rows[7]);

    f.render_widget(
        Paragraph::new(Span::styled("Apakah Anda mempercayai certificate ini?", Style::default().fg(Color::Rgb(220, 220, 220)).add_modifier(Modifier::BOLD))).alignment(Alignment::Center),
        rows[9],
    );

    let btn_rows = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(rows[10]);
    render_modal_button(f, btn_rows[0], " ✔  PERCAYA  & CONNECT  [Y/Enter] ", app.focus == Focus::CertAccept, Color::Rgb(80, 220, 120));
    render_modal_button(f, btn_rows[1], " ✖  TOLAK  [N/Esc] ", app.focus == Focus::CertDeny, Color::Rgb(255, 80, 80));
}

pub fn render_token_popup(f: &mut Frame, app: &App, area: Rect) {
    let popup_area = centered_rect(area, 52, 11);
    f.render_widget(Clear, popup_area);

    let block = Block::default()
        .title(Span::styled(" ⚡  TWO-FACTOR AUTHENTICATION   ", Style::default().fg(C_ORANGE).add_modifier(Modifier::BOLD)))
        .borders(Borders::ALL)
        .border_type(BorderType::Thick)
        .border_style(Style::default().fg(C_ORANGE))
        .style(Style::default().bg(Color::Rgb(20, 14, 5)));
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
            Constraint::Length(3),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(inner);

    f.render_widget(Paragraph::new(Span::styled("Masukkan OTP token yang dikirim ke email/app Anda:", Style::default().fg(Color::Rgb(220, 200, 140)))), rows[0]);
    let cursor = if (f.count() / 5) % 2 == 0 { "█ " } else { "  " };
    let input_content = if app.connection.token_input.is_empty() {
        Line::from(vec![
            Span::styled("   ", Style::default()),
            Span::styled("Ketik token di sini... ", Style::default().fg(C_DIM)),
            Span::styled(cursor, Style::default().fg(C_ORANGE)),
        ])
    } else {
        Line::from(vec![
            Span::styled("   ", Style::default()),
            Span::styled(app.connection.token_input.as_str(), Style::default().fg(Color::Rgb(255, 230, 80)).add_modifier(Modifier::BOLD)),
            Span::styled(cursor, Style::default().fg(C_ORANGE)),
        ])
    };

    let input_block = Block::default()
        .title(Span::styled(" OTP Token  ", Style::default().fg(C_ORANGE)))
        .borders(Borders::ALL)
        .border_type(BorderType::Thick)
        .border_style(Style::default().fg(C_ORANGE));
    f.render_widget(Paragraph::new(input_content).block(input_block), rows[2]);

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

fn render_shortcut_group(f: &mut Frame, title_area: Rect, content_area: Rect, title: &str, items: &[(&str, &str)], title_style: Style, key_style: Style, desc_style: Style) {
    f.render_widget(Paragraph::new(Span::styled(title, title_style)), title_area);
    let lines: Vec<Line> = items.iter().map(|(key, desc)| {
        Line::from(vec![
            Span::styled(format!("  {:<14}", key), key_style),
            Span::styled(format!("  {}", desc), desc_style),
        ])
    }).collect();
    f.render_widget(Paragraph::new(lines), content_area);
}

fn render_modal_button(f: &mut Frame, area: Rect, label: &str, focused: bool, accent: Color) {
    let style = if focused {
        Style::default().fg(Color::Rgb(20, 20, 20)).bg(accent).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(accent).add_modifier(Modifier::BOLD)
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(if focused { BorderType::Thick } else { BorderType::Rounded })
        .border_style(Style::default().fg(accent));
    f.render_widget(Paragraph::new(label).block(block).style(style).alignment(Alignment::Center), area);
}

fn render_label_value(f: &mut Frame, area: Rect, label: &str, value: &str, value_style: Style, label_style: Style) {
    f.render_widget(Paragraph::new(Line::from(vec![Span::styled(label, label_style), Span::styled(value, value_style)])), area);
}

fn centered_rect(area: Rect, width: u16, height: u16) -> Rect {
    Rect {
        x: area.x + (area.width.saturating_sub(width)) / 2,
        y: area.y + (area.height.saturating_sub(height)) / 2,
        width,
        height,
    }
}
