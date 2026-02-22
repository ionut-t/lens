use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Clear, Paragraph},
};

use crate::app::{App, NotificationKind};

use super::theme;

pub fn draw(frame: &mut Frame, app: &App) {
    let Some(notification) = app.notifier.recent() else {
        return;
    };

    let icon = match notification.kind {
        NotificationKind::Info => "ℹ",
        NotificationKind::Error => "✗",
    };

    let text = format!("{} {}", icon, notification.message);

    let max_inner = (frame.area().width / 2) as usize;
    let inner_width = text.len().min(max_inner) as u16;
    let width = inner_width + 1; // +1 for left border

    let area = Rect {
        x: frame.area().width.saturating_sub(width + 1),
        y: frame.area().height.saturating_sub(3),
        width,
        height: 1,
    };

    let color = match notification.kind {
        NotificationKind::Info => theme::BLUE,
        NotificationKind::Error => theme::RED,
    };

    let block = Block::default()
        .borders(Borders::LEFT)
        .border_style(Style::default().fg(color));

    let paragraph = Paragraph::new(text.as_str())
        .block(block)
        .style(Style::default().fg(color).bg(theme::SURFACE0));

    frame.render_widget(Clear, area);
    frame.render_widget(paragraph, area);
}
