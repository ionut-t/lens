use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Gauge, Paragraph},
};

use super::theme;
use crate::app::{App, Panel};
use crate::models::{NodeKind, TestStatus};

pub fn draw(frame: &mut Frame, app: &mut App, area: Rect) {
    let focused = app.active_panel == Panel::Detail;
    let border_style = if focused {
        Style::default().fg(theme::BLUE)
    } else {
        Style::default().fg(theme::SURFACE2)
    };

    let block = Block::default()
        .title(" Detail ")
        .borders(Borders::ALL)
        .border_style(border_style);

    let inner = block.inner(area);
    frame.render_widget(block, area);

    // Split inner area: progress bar on top, detail content below
    let [progress_area, content_area] =
        Layout::vertical([Constraint::Length(1), Constraint::Min(1)]).areas(inner);

    // Progress bar
    let percent = (app.progress_percent() * 100.0).min(100.0) as u16;
    let gauge = Gauge::default()
        .gauge_style(Style::default().fg(theme::GREEN).bg(theme::SURFACE0))
        .percent(percent)
        .label(format!("{}%", percent));
    frame.render_widget(gauge, progress_area);

    // Detail content: show selected node's failure info + console output
    let content = if let Some(node_id) = app.selected_node_id() {
        if let Some(node) = app.tree.get(node_id) {
            let mut lines: Vec<Line> = Vec::new();
            let mut breadcrumbs = Vec::new();
            let mut current = Some(node_id);

            while let Some(id) = current {
                if let Some(ancestor) = app.tree.get(id) {
                    breadcrumbs.push(ancestor.name.clone());
                    current = ancestor.parent;
                } else {
                    break;
                }
            }
            breadcrumbs.reverse();

            if !breadcrumbs.is_empty() {
                lines.push(Line::from(vec![Span::styled(
                    breadcrumbs.join(" > "),
                    Style::default().fg(theme::OVERLAY0).bold(),
                )]));
                lines.push(Line::from(""));
            }

            if node.kind == NodeKind::Test {
                if node.status == TestStatus::Failed {
                    if let Some(ref result) = node.result {
                        if let Some(ref failure) = result.failure {
                            let failure_text = build_failure_text(failure, &node.name);
                            lines.extend(failure_text.lines);
                        } else {
                            lines.push(Line::from("No failure details available."));
                        }
                    }
                } else {
                    lines.push(Line::from(vec![
                        Span::styled(&node.name, Style::default().fg(node.status.color())),
                        Span::raw(" "),
                        Span::styled(node.status.icon(), Style::default().fg(node.status.color())),
                    ]));
                }
            } else {
                // File/suite node — show child test summary
                let (p, f, s) = count_descendants(&app.tree, node_id);

                if p + f + s > 0 {
                    lines.push(Line::from(vec![
                        Span::styled(
                            TestStatus::Passed.icon(),
                            Style::default().fg(TestStatus::Passed.color()),
                        ),
                        Span::raw(" "),
                        Span::styled(
                            format!("{}", p),
                            Style::default().fg(TestStatus::Passed.color()),
                        ),
                        Span::raw("   "),
                        Span::styled(
                            TestStatus::Failed.icon(),
                            Style::default().fg(TestStatus::Failed.color()),
                        ),
                        Span::raw(" "),
                        Span::styled(
                            format!("{}", f),
                            Style::default().fg(TestStatus::Failed.color()),
                        ),
                        Span::raw("   "),
                        Span::styled(
                            TestStatus::Skipped.icon(),
                            Style::default().fg(TestStatus::Skipped.color()),
                        ),
                        Span::raw(" "),
                        Span::styled(
                            format!("{}", s),
                            Style::default().fg(TestStatus::Skipped.color()),
                        ),
                    ]));
                }

                // Show individual failures below
                let failed_ids = collect_failed_descendants(&app.tree, node_id);
                for (i, fid) in failed_ids.iter().enumerate() {
                    if let Some(failed_node) = app.tree.get(*fid)
                        && let Some(ref result) = failed_node.result
                        && let Some(ref failure) = result.failure
                    {
                        if i == 0 {
                            lines.push(Line::from(""));
                        }
                        let failure_text = build_failure_text(failure, &failed_node.name);
                        lines.extend(failure_text.lines);
                        lines.push(Line::from(""));
                    }
                }
            }

            // Console output from the file this test belongs to
            let console_output = get_file_console_output(&app.tree, node_id);
            if !console_output.is_empty() {
                lines.push(Line::from(""));
                lines.push(Line::from(Span::styled(
                    "━━ Console Output ━━",
                    Style::default().fg(theme::YELLOW),
                )));
                lines.push(Line::from(""));
                for log_line in console_output {
                    lines.push(Line::from(Span::styled(
                        log_line.clone(),
                        Style::default().fg(theme::SUBTEXT0),
                    )));
                }
            }

            Text::from(lines)
        } else {
            Text::from("Select a test to view details.")
        }
    } else {
        Text::from("Select a test to view details.")
    };

    let content_height = content.height() as u16;
    let viewport_height = content_area.height;
    let max_scroll = content_height.saturating_sub(viewport_height);
    app.detail_scroll_offset = app.detail_scroll_offset.min(max_scroll);

    let paragraph = Paragraph::new(content)
        .wrap(ratatui::widgets::Wrap { trim: false })
        .scroll((app.detail_scroll_offset, 0));
    frame.render_widget(paragraph, content_area);
}

/// Walk up the tree to find the ancestor file node and return its console output.
fn get_file_console_output(tree: &crate::models::TestTree, node_id: usize) -> &[String] {
    let mut current = Some(node_id);
    while let Some(id) = current {
        if let Some(node) = tree.get(id) {
            if node.kind == NodeKind::File {
                return &node.console_output;
            }
            current = node.parent;
        } else {
            break;
        }
    }
    &[]
}

fn count_descendants(tree: &crate::models::TestTree, node_id: usize) -> (usize, usize, usize) {
    let mut passed = 0;
    let mut failed = 0;
    let mut skipped = 0;

    if let Some(node) = tree.get(node_id) {
        for &child in &node.children {
            if let Some(child_node) = tree.get(child) {
                if child_node.kind == NodeKind::Test {
                    match child_node.status {
                        TestStatus::Passed => passed += 1,
                        TestStatus::Failed => failed += 1,
                        TestStatus::Skipped => skipped += 1,
                        _ => {}
                    }
                }
                let (p, f, s) = count_descendants(tree, child);
                passed += p;
                failed += f;
                skipped += s;
            }
        }
    }
    (passed, failed, skipped)
}

fn collect_failed_descendants(tree: &crate::models::TestTree, node_id: usize) -> Vec<usize> {
    let mut result = Vec::new();
    if let Some(node) = tree.get(node_id) {
        for &child in &node.children {
            if let Some(child_node) = tree.get(child) {
                if child_node.kind == NodeKind::Test && child_node.status == TestStatus::Failed {
                    result.push(child);
                }
                result.extend(collect_failed_descendants(tree, child));
            }
        }
    }
    result
}

fn build_failure_text<'a>(
    failure: &'a crate::models::FailureDetail,
    test_name: &'a str,
) -> Text<'a> {
    let mut lines: Vec<Line> = vec![
        Line::from(vec![
            Span::styled("✘ failed: ", Style::default().fg(theme::RED)),
            Span::styled(test_name, Style::default().fg(theme::RED).bold()),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            &failure.message,
            Style::default().fg(theme::TEXT),
        )),
        Line::from(""),
    ];

    // Expected / Actual
    if let Some(ref expected) = failure.expected {
        lines.push(Line::from(vec![
            Span::styled("  Expected: ", Style::default().fg(theme::GREEN)),
            Span::styled(expected.as_str(), Style::default().fg(theme::GREEN)),
        ]));
    }
    if let Some(ref actual) = failure.actual {
        lines.push(Line::from(vec![
            Span::styled("  Actual:   ", Style::default().fg(theme::RED)),
            Span::styled(actual.as_str(), Style::default().fg(theme::RED).bold()),
        ]));
    }

    // Diff (only show if we don't already have expected/actual)
    if failure.expected.is_none()
        && failure.actual.is_none()
        && let Some(ref diff) = failure.diff
    {
        lines.push(Line::from(""));
        for diff_line in diff.lines() {
            let style = if diff_line.starts_with('+') {
                Style::default().fg(theme::GREEN)
            } else if diff_line.starts_with('-') {
                Style::default().fg(theme::RED)
            } else {
                Style::default()
            };
            lines.push(Line::from(Span::styled(diff_line, style)));
        }
    }

    // Stack trace (filter out noise)
    if let Some(ref stack) = failure.stack_trace {
        let filtered: Vec<&str> = stack
            .lines()
            .filter(|line| !line.contains("node_modules"))
            .collect();
        if !filtered.is_empty() {
            lines.push(Line::from(""));
            for stack_line in filtered {
                lines.push(Line::from(Span::styled(
                    stack_line,
                    Style::default().fg(theme::OVERLAY0),
                )));
            }
        }
    }

    Text::from(lines)
}
