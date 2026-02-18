use ratatui::{prelude::*, widgets::Paragraph};

use crate::app::App;

pub fn draw(frame: &mut Frame, app: &App, area: Rect) {
    let watch_indicator = if app.watch_mode { " [watch] " } else { "" };
    let running_indicator = if app.running { " running..." } else { "" };

    let bar = if app.filter_active {
        Line::from(vec![
            Span::styled(" [esc]", Style::default().fg(Color::Yellow)),
            Span::raw(" clear  "),
            Span::styled("[enter]", Style::default().fg(Color::Yellow)),
            Span::raw(" apply"),
        ])
    } else {
        Line::from(vec![
            Span::styled(" [f]", Style::default().fg(Color::Yellow)),
            Span::raw(" filter  "),
            Span::styled("[r]", Style::default().fg(Color::Yellow)),
            Span::raw(" rerun  "),
            Span::styled("[e]", Style::default().fg(Color::Yellow)),
            Span::raw(" edit  "),
            Span::styled("[a]", Style::default().fg(Color::Yellow)),
            Span::raw(" run all  "),
            Span::styled("[w]", Style::default().fg(Color::Yellow)),
            Span::raw(" watch  "),
            Span::styled("[q]", Style::default().fg(Color::Yellow)),
            Span::raw(" quit"),
            Span::styled(watch_indicator, Style::default().fg(Color::Cyan)),
            Span::styled(running_indicator, Style::default().fg(Color::Yellow)),
        ])
    };

    let paragraph = Paragraph::new(bar).style(Style::default().bg(Color::DarkGray));
    frame.render_widget(paragraph, area);
}
