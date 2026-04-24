use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Padding, Paragraph},
    Frame,
};

use super::theme::{C_BORDER, C_DIM, C_FOCUS, C_TEXT};

pub fn render_input(
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
        Style::default().fg(C_FOCUS).add_modifier(Modifier::BOLD)
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
        Span::styled("•".repeat(value.len()), Style::default().fg(C_TEXT))
    } else {
        Span::styled(value, Style::default().fg(C_TEXT))
    };

    let content = if focused && !value.is_empty() {
        Line::from(vec![
            Span::styled(
                if masked {
                    "•".repeat(value.len())
                } else {
                    value.to_string()
                },
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

    f.render_widget(Paragraph::new(content).block(block), area);
}
