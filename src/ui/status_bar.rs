use ratatui::{prelude::*, widgets::Paragraph};

use super::theme;
use crate::{app::App, models::RunSummary};

const SPINNER_FRAMES: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

pub fn draw(frame: &mut Frame, app: &App, area: Rect) {
    let watch_indicator = if app.watch_mode { " [watch] " } else { "" };

    let left = if app.discovering {
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
        let spans = vec![
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

        Line::from(spans)
    };

    let right = if app.running {
        let spinner = SPINNER_FRAMES[app.spinner_tick % SPINNER_FRAMES.len()];
        Line::from(vec![Span::styled(
            format!("{} running... ", spinner),
            Style::default().fg(theme::YELLOW),
        )])
    } else if !app.discovering {
        if let Some(summary) = &app.summary {
            let RunSummary {
                passed,
                failed,
                skipped,
                ..
            } = summary;

            if passed + failed + skipped > 0 {
                Line::from(vec![
                    Span::styled("✔ ", Style::default().fg(theme::GREEN)),
                    Span::styled(format!("{}", passed), Style::default().fg(theme::GREEN)),
                    Span::styled("  ✘ ", Style::default().fg(theme::RED)),
                    Span::styled(format!("{}", failed), Style::default().fg(theme::RED)),
                    Span::styled("  ⊘ ", Style::default().fg(theme::TEAL)),
                    Span::styled(format!("{}", skipped), Style::default().fg(theme::TEAL)),
                    Span::styled("  ⏲ ", Style::default().fg(theme::MAUVE)),
                    Span::styled(
                        format!("{:.1}s ", summary.duration as f64 / 1000.0),
                        Style::default().fg(theme::MAUVE),
                    ),
                ])
            } else {
                Line::from("")
            }
        } else {
            Line::from("")
        }
    } else {
        Line::from("")
    };

    let [left_area, right_area] =
        Layout::horizontal([Constraint::Min(1), Constraint::Length(right.width() as u16)])
            .areas(area);

    let left_para = Paragraph::new(left).style(Style::default().bg(theme::SURFACE0));
    let right_para = Paragraph::new(right)
        .style(Style::default().bg(theme::SURFACE0))
        .alignment(Alignment::Right);

    frame.render_widget(left_para, left_area);
    frame.render_widget(right_para, right_area);
}
