use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Gauge, Paragraph},
};

use super::theme;
use crate::app::{App, Panel};
use crate::models::{NodeKind, TestStatus};

pub fn draw(frame: &mut Frame, app: &App, scroll_offset: u16, area: Rect) -> u16 {
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

    let max_scroll = content
        .height()
        .saturating_sub(content_area.height as usize) as u16;
    let effective_scroll = scroll_offset.min(max_scroll);

    let paragraph = Paragraph::new(content)
        .wrap(ratatui::widgets::Wrap { trim: false })
        .scroll((effective_scroll, 0));

    frame.render_widget(paragraph, content_area);

    effective_scroll
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

/// Render a JSON object, comparing each key against `counterpart_map` to highlight diffs.
fn push_json_object_lines<'a>(
    lines: &mut Vec<Line<'a>>,
    map: &serde_json::Map<String, serde_json::Value>,
    counterpart_map: Option<&serde_json::Map<String, serde_json::Value>>,
    base_color: ratatui::prelude::Color,
    highlight_color: ratatui::prelude::Color,
    indent: &str,
) {
    lines.push(Line::from(Span::styled(
        format!("{indent}{{"),
        Style::default().fg(base_color),
    )));
    let mut sorted_keys: Vec<&String> = map.keys().collect();
    sorted_keys.sort();
    for key in sorted_keys {
        let value = &map[key];
        let counterpart = counterpart_map.and_then(|m| m.get(key));
        let key_prefix = format!("{indent}  \"{key}\": ");
        push_json_value_lines(
            lines,
            value,
            counterpart,
            &key_prefix,
            indent,
            base_color,
            highlight_color,
        );
    }
    lines.push(Line::from(Span::styled(
        format!("{indent}}}"),
        Style::default().fg(base_color),
    )));
}

/// Render a JSON value, drilling into objects/arrays to highlight only the differing leaves.
fn push_json_value_lines<'a>(
    lines: &mut Vec<Line<'a>>,
    value: &serde_json::Value,
    counterpart: Option<&serde_json::Value>,
    key_prefix: &str,
    indent: &str,
    base_color: ratatui::prelude::Color,
    highlight_color: ratatui::prelude::Color,
) {
    let deeper = format!("{indent}  ");
    match value {
        serde_json::Value::Object(map) => {
            let cp_map = counterpart.and_then(|c| c.as_object());
            lines.push(Line::from(Span::styled(
                format!("{key_prefix}{{"),
                Style::default().fg(base_color),
            )));
            let mut sorted_keys: Vec<&String> = map.keys().collect();
            sorted_keys.sort();
            for k in sorted_keys {
                let child_cp = cp_map.and_then(|m| m.get(k));
                let child_prefix = format!("{deeper}  \"{k}\": ");
                push_json_value_lines(
                    lines,
                    &map[k],
                    child_cp,
                    &child_prefix,
                    &deeper,
                    base_color,
                    highlight_color,
                );
            }
            lines.push(Line::from(Span::styled(
                format!("{deeper}}}"),
                Style::default().fg(base_color),
            )));
        }
        serde_json::Value::Array(arr) => {
            let cp_arr = counterpart.and_then(|c| c.as_array());
            lines.push(Line::from(Span::styled(
                format!("{key_prefix}["),
                Style::default().fg(base_color),
            )));
            for (i, item) in arr.iter().enumerate() {
                let item_cp = cp_arr.and_then(|a| a.get(i));
                let item_prefix = format!("{deeper}  ");
                push_json_value_lines(
                    lines,
                    item,
                    item_cp,
                    &item_prefix,
                    &deeper,
                    base_color,
                    highlight_color,
                );
            }
            lines.push(Line::from(Span::styled(
                format!("{deeper}]"),
                Style::default().fg(base_color),
            )));
        }
        serde_json::Value::String(s) => {
            let differs = counterpart != Some(value);
            let color = if differs { highlight_color } else { base_color };
            // Render JS `undefined` without quotes (it was encoded as a sentinel during parsing)
            let rendered = if s == "__js_undefined__" {
                format!("{key_prefix}undefined")
            } else {
                format!("{key_prefix}\"{}\"", s)
            };
            lines.push(Line::from(Span::styled(
                rendered,
                Style::default().fg(color),
            )));
        }
        other => {
            let differs = counterpart != Some(value);
            let color = if differs { highlight_color } else { base_color };
            lines.push(Line::from(Span::styled(
                format!("{key_prefix}{other}"),
                Style::default().fg(color),
            )));
        }
    }
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
    match (&failure.expected_parsed, &failure.actual_parsed) {
        (Some(exp_map), Some(act_map)) => {
            lines.push(Line::from(Span::styled(
                "  Expected:",
                Style::default().fg(theme::GREEN),
            )));
            push_json_object_lines(
                &mut lines,
                exp_map,
                Some(act_map),
                theme::GREEN,
                theme::YELLOW,
                "  ",
            );
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "  Actual:",
                Style::default().fg(theme::RED),
            )));
            push_json_object_lines(
                &mut lines,
                act_map,
                Some(exp_map),
                theme::RED,
                theme::YELLOW,
                "  ",
            );
        }
        _ => {
            if let Some(expected) = failure.expected.as_deref() {
                lines.push(Line::from(vec![
                    Span::styled("  Expected: ", Style::default().fg(theme::GREEN)),
                    Span::styled(expected, Style::default().fg(theme::GREEN)),
                ]));
            }
            if let Some(actual) = failure.actual.as_deref() {
                lines.push(Line::from(vec![
                    Span::styled("  Actual:   ", Style::default().fg(theme::RED)),
                    Span::styled(actual, Style::default().fg(theme::RED).bold()),
                ]));
            }
        }
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
