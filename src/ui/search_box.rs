use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Paragraph},
};

pub fn draw(frame: &mut Frame, query: &str, active: bool, area: Rect) {
    let (display, border_color, text_style) = if active {
        (
            format!("/ {}│", query),
            Color::Cyan,
            Style::default().fg(Color::White),
        )
    } else {
        (
            format!("/ {}", query),
            Color::DarkGray,
            Style::default().fg(Color::DarkGray),
        )
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color))
        .title(" Filter ");
    let paragraph = Paragraph::new(display).style(text_style).block(block);
    frame.render_widget(paragraph, area);
}
