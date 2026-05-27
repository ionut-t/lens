use ratatui::{prelude::*, widgets::Paragraph};

use super::theme;
use crate::{app::App, models::NodeKind};

const SPINNER_FRAMES: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

/// Keybinding hints shown permanently in the status bar.
/// Kept intentionally short — full reference is in the help overlay ([?]).
fn primary_hints(filter_active: bool) -> Vec<Span<'static>> {
    if filter_active {
        return vec![
            Span::styled("[enter] ", Style::default().fg(theme::BLUE)),
            Span::raw("apply  "),
            Span::styled("[esc] ", Style::default().fg(theme::BLUE)),
            Span::raw("clear  "),
        ];
    }

    vec![
        Span::styled("[a] ", Style::default().fg(theme::BLUE)),
        Span::raw("run all  "),
        Span::styled("[r] ", Style::default().fg(theme::BLUE)),
        Span::raw("rerun  "),
        Span::styled("[w] ", Style::default().fg(theme::BLUE)),
        Span::raw("watch  "),
        Span::styled("[?] ", Style::default().fg(theme::BLUE)),
        Span::raw("help  "),
    ]
}

pub fn draw(frame: &mut Frame, app: &App, area: Rect) {
    let left = get_help(app);
    let right = get_summary(app);

    let [left_area, right_area] =
        Layout::horizontal([Constraint::Min(1), Constraint::Length(right.width() as u16)])
            .areas(area);

    let left_paragraph = Paragraph::new(left).style(Style::default().bg(theme::SURFACE0));
    let right_paragraph = Paragraph::new(right)
        .style(Style::default().bg(theme::SURFACE0))
        .alignment(Alignment::Right);

    frame.render_widget(left_paragraph, left_area);
    frame.render_widget(right_paragraph, right_area);
}

fn get_help(app: &App) -> Line<'_> {
    let watch_indicator = if app.watch_mode { " [watch] " } else { "" };

    if app.discovering {
        let spinner = SPINNER_FRAMES[app.spinner_tick % SPINNER_FRAMES.len()];
        Line::from(vec![Span::styled(
            format!(" {} Discovering tests...", spinner),
            Style::default().fg(theme::YELLOW),
        )])
    } else {
        let mut spans = primary_hints(app.filter_active);
        spans.push(Span::styled(
            watch_indicator,
            Style::default().fg(theme::TEAL),
        ));
        Line::from(spans)
    }
}

fn get_summary(app: &App) -> Line<'_> {
    let file_count = app.tree.count_kind(NodeKind::File);
    let test_count = app.tree.count_kind(NodeKind::Test);
    let counts: Vec<Span> = if file_count > 0 || test_count > 0 {
        let mut counts = vec![
            Span::styled(
                format!("{}", file_count),
                Style::default().fg(theme::OVERLAY0),
            ),
            Span::styled(" files  ", Style::default().fg(theme::OVERLAY0)),
        ];

        if test_count > 0 {
            counts.extend([
                Span::styled(
                    format!("{}", test_count),
                    Style::default().fg(theme::OVERLAY0),
                ),
                Span::styled(" tests  ", Style::default().fg(theme::OVERLAY0)),
            ]);
        }

        counts
    } else {
        vec![]
    };

    if app.running {
        let spinner = SPINNER_FRAMES[app.spinner_tick % SPINNER_FRAMES.len()];
        let mut spans = counts;
        spans.push(Span::styled(
            format!("{} running... ", spinner),
            Style::default().fg(theme::YELLOW),
        ));
        Line::from(spans)
    } else if !app.discovering {
        if let Some(summary) = &app.summary {
            let (passed, failed, skipped) = app.tree.count_tests_by_status();

            if passed + failed + skipped > 0 {
                let mut spans = counts;
                spans.extend([
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
                ]);
                Line::from(spans)
            } else {
                Line::from(counts)
            }
        } else {
            Line::from(counts)
        }
    } else {
        Line::from("")
    }
}
