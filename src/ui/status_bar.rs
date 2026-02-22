use ratatui::{prelude::*, widgets::Paragraph};

use super::theme;
use crate::{app::App, models::RunSummary};

const SPINNER_FRAMES: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

enum CommandHelp {
    RunAll,
    Watch,
    Rerun,
    Filter,
    Edit,
    Quit,
    ApplyFilter,
    ExitFilter,
}

impl CommandHelp {
    fn get(filter_active: bool) -> Vec<Span<'static>> {
        let commands = if filter_active {
            vec![CommandHelp::ApplyFilter, CommandHelp::ExitFilter]
        } else {
            vec![
                CommandHelp::RunAll,
                CommandHelp::Watch,
                CommandHelp::Rerun,
                CommandHelp::Filter,
                CommandHelp::Edit,
                CommandHelp::Quit,
            ]
        };

        commands
            .into_iter()
            .flat_map(|cmd| {
                vec![
                    Span::styled(
                        format!("{} ", cmd.label()),
                        Style::default().fg(theme::BLUE),
                    ),
                    Span::raw(format!("{}  ", cmd.description())),
                ]
            })
            .collect()
    }

    fn label(&self) -> &'static str {
        match self {
            CommandHelp::Filter => "[f]",
            CommandHelp::Rerun => "[r]",
            CommandHelp::Edit => "[e]",
            CommandHelp::RunAll => "[a]",
            CommandHelp::Watch => "[w]",
            CommandHelp::Quit => "[q]",
            CommandHelp::ExitFilter => "[esc]",
            CommandHelp::ApplyFilter => "[enter]",
        }
    }

    fn description(&self) -> &'static str {
        match self {
            CommandHelp::Filter => "filter",
            CommandHelp::Rerun => "rerun failed",
            CommandHelp::Edit => "edit",
            CommandHelp::RunAll => "run all",
            CommandHelp::Watch => "watch",
            CommandHelp::Quit => "quit",
            CommandHelp::ExitFilter => "clear",
            CommandHelp::ApplyFilter => "apply",
        }
    }
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
        let mut spans = CommandHelp::get(app.filter_active);
        spans.push(Span::styled(
            watch_indicator,
            Style::default().fg(theme::TEAL),
        ));
        Line::from(spans)
    }
}

fn get_summary(app: &App) -> Line<'_> {
    if app.running {
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
    }
}
