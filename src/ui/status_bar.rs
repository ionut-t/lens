use ratatui::{prelude::*, widgets::Paragraph};

use super::theme;
use crate::{app::App, models::RunSummary};

const SPINNER_FRAMES: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

pub fn draw(frame: &mut Frame, app: &App, area: Rect) {
    let watch_indicator = if app.watch_mode { " [watch] " } else { "" };

    let bar = if app.discovering {
        let spinner = SPINNER_FRAMES[app.spinner_tick % SPINNER_FRAMES.len()];
        Line::from(vec![Span::styled(
            format!(" {} Discovering tests...", spinner),
            Style::default().fg(theme::YELLOW),
        )])
    } else if app.filter_active {
        Line::from(vec![
            Span::styled(" [esc]", Style::default().fg(theme::YELLOW)),
            Span::raw(" clear  "),
            Span::styled("[enter]", Style::default().fg(theme::YELLOW)),
            Span::raw(" apply"),
        ])
    } else {
        let mut spans = vec![
            Span::styled(" [f]", Style::default().fg(theme::YELLOW)),
            Span::raw(" filter  "),
            Span::styled("[r]", Style::default().fg(theme::YELLOW)),
            Span::raw(" rerun  "),
            Span::styled("[e]", Style::default().fg(theme::YELLOW)),
            Span::raw(" edit  "),
            Span::styled("[a]", Style::default().fg(theme::YELLOW)),
            Span::raw(" run all  "),
            Span::styled("[w]", Style::default().fg(theme::YELLOW)),
            Span::raw(" watch  "),
            Span::styled("[q]", Style::default().fg(theme::YELLOW)),
            Span::raw(" quit"),
            Span::styled(watch_indicator, Style::default().fg(theme::TEAL)),
        ];

        if app.running {
            let spinner = SPINNER_FRAMES[app.spinner_tick % SPINNER_FRAMES.len()];
            spans.push(Span::styled(
                format!(" {} running...", spinner),
                Style::default().fg(theme::YELLOW),
            ));
        } else if let Some(summary) = &app.summary {
            let RunSummary {
                passed,
                failed,
                skipped,
                ..
            } = summary;

            spans.push(Span::styled(
                format!("  {:.1}s", summary.duration as f64 / 1000.0),
                Style::default().fg(theme::OVERLAY0),
            ));

            if passed + failed + skipped > 0 {
                spans.push(Span::styled(" ✔ ", Style::default().fg(theme::GREEN)));
                spans.push(Span::styled(
                    format!("{}", passed),
                    Style::default().fg(theme::GREEN),
                ));
                spans.push(Span::styled("  ✘ ", Style::default().fg(theme::RED)));
                spans.push(Span::styled(
                    format!("{}", failed),
                    Style::default().fg(theme::RED),
                ));
                spans.push(Span::styled("  ⊘ ", Style::default().fg(theme::TEAL)));
                spans.push(Span::styled(
                    format!("{}", skipped),
                    Style::default().fg(theme::TEAL),
                ));

                spans.push(Span::styled(
                    format!("  {:.1}s", summary.duration as f64 / 1000.0),
                    Style::default().fg(theme::MAUVE),
                ));
            }
        }

        Line::from(spans)
    };

    let paragraph = Paragraph::new(bar).style(Style::default().bg(theme::SURFACE0));
    frame.render_widget(paragraph, area);
}
