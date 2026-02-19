use ratatui::{prelude::*, widgets::Paragraph};

use crate::{app::App, models::RunSummary};

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
        let mut spans = vec![
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
        ];

        if app.running {
            let spinner = SPINNER_FRAMES[app.spinner_tick % SPINNER_FRAMES.len()];
            spans.push(Span::styled(
                format!(" {} running...", spinner),
                Style::default().fg(Color::Yellow),
            ));
        } else if let Some(summary) = app.test_summary() {
            let RunSummary {
                passed,
                failed,
                skipped,
                ..
            } = summary;

            spans.push(Span::styled(
                format!("  {:.1}s", summary.duration as f64 / 1000.0),
                Style::default().fg(Color::DarkGray),
            ));

            if passed + failed + skipped > 0 {
                spans.push(Span::styled(" ✔ ", Style::default().fg(Color::Green)));
                spans.push(Span::styled(
                    format!("{}", passed),
                    Style::default().fg(Color::Green),
                ));
                spans.push(Span::styled("  ✘ ", Style::default().fg(Color::Red)));
                spans.push(Span::styled(
                    format!("{}", failed),
                    Style::default().fg(Color::Red),
                ));
                spans.push(Span::styled("  ⊘ ", Style::default().fg(Color::Cyan)));
                spans.push(Span::styled(
                    format!("{}", skipped),
                    Style::default().fg(Color::Cyan),
                ));

                spans.push(Span::styled(
                    format!("  {:.1}s", summary.duration as f64 / 1000.0),
                    Style::default().fg(Color::LightMagenta),
                ));
            }
        }

        Line::from(spans)
    };

    let paragraph = Paragraph::new(bar).style(Style::default().bg(Color::DarkGray));
    frame.render_widget(paragraph, area);
}
