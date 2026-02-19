use ratatui::{prelude::*, widgets::Paragraph};

use crate::app::App;

const SPINNER_FRAMES: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

pub fn draw(frame: &mut Frame, app: &App, area: Rect) {
    let watch_indicator = if app.watch_mode { " [watch] " } else { "" };

    let bar = if app.discovering {
        let spinner = SPINNER_FRAMES[app.spinner_tick % SPINNER_FRAMES.len()];
        Line::from(vec![Span::styled(
            format!(" {} Discovering tests...", spinner),
            Style::default().fg(Color::Yellow),
        )])
    } else if app.filter_active {
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
            if app.running {
                let spinner = SPINNER_FRAMES[app.spinner_tick % SPINNER_FRAMES.len()];
                Span::styled(
                    format!(" {} running...", spinner),
                    Style::default().fg(Color::Yellow),
                )
            } else {
                Span::styled("", Style::default())
            },
        ])
    };

    let paragraph = Paragraph::new(bar).style(Style::default().bg(Color::DarkGray));
    frame.render_widget(paragraph, area);
}
