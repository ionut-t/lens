use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Paragraph},
};

use super::theme;

pub fn draw(frame: &mut Frame, input: &tui_input::Input, active: bool, area: Rect) {
    let (border_color, text_style) = if active {
        (theme::BLUE, Style::default().fg(theme::TEXT))
    } else {
        (theme::SURFACE2, Style::default().fg(theme::OVERLAY0))
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color))
        .title(" Filter ");

    let paragraph = Paragraph::new(format!("/ {}", input.value()))
        .style(text_style)
        .block(block);

    frame.render_widget(paragraph, area);

    if active {
        // area.x+1 = inside left border, +2 = "/ " prefix
        frame.set_cursor_position((area.x + 1 + 2 + input.visual_cursor() as u16, area.y + 1));
    }
}
