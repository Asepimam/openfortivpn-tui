use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::Style,
    widgets::Block,
    Frame,
};

use crate::app::{App, UiMode, VpnState};

use super::{
    popups::{render_cert_popup, render_help_popup, render_notification, render_token_popup},
    theme::C_BG,
    views::{
        render_connect_screen, render_previous_screen, render_profile_form_screen,
        render_profile_list_screen, render_statusbar, render_title,
    },
};

pub fn render(f: &mut Frame, app: &App) {
    let full = f.area();
    f.render_widget(Block::default().style(Style::default().bg(C_BG)), full);

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
    render_overlays(f, app, full);
}

fn render_body(f: &mut Frame, app: &App, area: Rect) {
    match app.ui_mode {
        UiMode::ProfileList => render_profile_list_screen(f, app, area),
        UiMode::NewProfile | UiMode::EditProfile => render_profile_form_screen(f, app, area),
        UiMode::Connect => render_connect_screen(f, app, area),
        UiMode::Help => render_previous_screen(f, app, area),
    }
}

fn render_overlays(f: &mut Frame, app: &App, area: Rect) {
    if app.ui_mode == UiMode::Help {
        render_help_popup(f, area);
    }

    match app.vpn_state {
        VpnState::WaitingToken => render_token_popup(f, app, area),
        VpnState::WaitingCert => {
            if let Some(cert) = &app.pending_cert {
                render_cert_popup(f, app, cert, area);
            }
        }
        _ => {
            if let Some((msg, level)) = &app.notification {
                render_notification(f, msg, level, area);
            }
        }
    }
}
