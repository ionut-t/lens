use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Paragraph},
};

use super::theme;

pub fn draw(frame: &mut Frame, query: &str, active: bool, area: Rect) {
    let (display, border_color, text_style) = if active {
        (
            format!("/ {}│", query),
            theme::BLUE,
            Style::default().fg(theme::TEXT),
        )
    } else {
        (
            format!("/ {}", query),
            theme::SURFACE2,
            Style::default().fg(theme::OVERLAY0),
        )
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color))
        .title(" Filter ");
    let paragraph = Paragraph::new(display).style(text_style).block(block);
    frame.render_widget(paragraph, area);
}
