use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Gauge, Paragraph},
};

use super::theme;
use crate::app::{App, Panel};
use crate::models::{NodeKind, TestStatus};

pub fn draw(frame: &mut Frame, app: &App, scroll_offset: u16, area: Rect) -> u16 {
    let focused = app.active_panel == Panel::Output;
    let border_style = if focused {
        Style::default().fg(theme::BLUE)
    } else {
        Style::default().fg(theme::SURFACE2)
    };

    let block = Block::default()
        .title(" Output ")
        .borders(Borders::ALL)
        .border_style(border_style);

    let inner = block.inner(area);
    frame.render_widget(block, area);

    // Split inner area: progress bar on top, output content below
    let [progress_area, content_area] =
        Layout::vertical([Constraint::Length(1), Constraint::Min(1)]).areas(inner);

    // Progress bar
    let percent = (app.progress_percent() * 100.0).min(100.0) as u16;
    let gauge = Gauge::default()
        .gauge_style(Style::default().fg(theme::GREEN).bg(theme::SURFACE0))
        .percent(percent)
        .label(format!("{}%", percent));
    frame.render_widget(gauge, progress_area);

    // Output panel: show selected node's failure info + console output
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
                            lines.push(Line::from("No failure output available."));
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
                    for sub_line in log_line.lines() {
                        lines.push(Line::from(Span::styled(
                            sub_line.to_string(),
                            Style::default().fg(theme::SUBTEXT0),
                        )));
                    }
                }
            }

            Text::from(lines)
        } else {
            Text::from("Select a test to view its output.")
        }
    } else {
        Text::from("Select a test to view its output.")
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

/// Render a JSON array, comparing each element by index against `counterpart_arr` to highlight diffs.
fn push_json_array_lines<'a>(
    lines: &mut Vec<Line<'a>>,
    arr: &[serde_json::Value],
    counterpart_arr: Option<&[serde_json::Value]>,
    base_color: ratatui::prelude::Color,
    highlight_color: ratatui::prelude::Color,
    indent: &str,
) {
    let cp_len = counterpart_arr.map_or(0, |a| a.len());
    let max_len = arr.len().max(cp_len);
    if max_len == 0 {
        lines.push(Line::from(Span::styled(
            format!("{indent}[]"),
            Style::default().fg(base_color),
        )));
        return;
    }
    lines.push(Line::from(Span::styled(
        format!("{indent}["),
        Style::default().fg(base_color),
    )));
    let item_prefix = format!("{indent}  ");
    for i in 0..max_len {
        let item_cp = counterpart_arr.and_then(|a| a.get(i));
        if let Some(item) = arr.get(i) {
            push_json_value_lines(
                lines,
                item,
                item_cp,
                &item_prefix,
                indent,
                base_color,
                highlight_color,
            );
        } else {
            // Element present in counterpart but absent on this side.
            lines.push(Line::from(Span::styled(
                format!("{item_prefix}(absent)"),
                Style::default().fg(highlight_color),
            )));
        }
    }
    lines.push(Line::from(Span::styled(
        format!("{indent}]"),
        Style::default().fg(base_color),
    )));
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
    // Collect all keys from both sides so absent keys are also shown.
    let mut all_keys: std::collections::BTreeSet<&str> = map.keys().map(String::as_str).collect();
    if let Some(cp) = counterpart_map {
        all_keys.extend(cp.keys().map(String::as_str));
    }
    if all_keys.is_empty() {
        lines.push(Line::from(Span::styled(
            format!("{indent}{{}}"),
            Style::default().fg(base_color),
        )));
        return;
    }
    lines.push(Line::from(Span::styled(
        format!("{indent}{{"),
        Style::default().fg(base_color),
    )));
    for key in all_keys {
        let key_prefix = format!("{indent}  \"{key}\": ");
        if let Some(value) = map.get(key) {
            let counterpart = counterpart_map.and_then(|m| m.get(key));
            push_json_value_lines(
                lines,
                value,
                counterpart,
                &key_prefix,
                indent,
                base_color,
                highlight_color,
            );
        } else {
            // Key present in counterpart but absent on this side.
            lines.push(Line::from(Span::styled(
                format!("{key_prefix}(absent)"),
                Style::default().fg(highlight_color),
            )));
        }
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
            if map.is_empty() {
                let differs = counterpart != Some(value);
                let color = if differs { highlight_color } else { base_color };
                lines.push(Line::from(Span::styled(
                    format!("{key_prefix}{{}}"),
                    Style::default().fg(color),
                )));
            } else {
                let cp_map = counterpart.and_then(|c| c.as_object());
                let bracket_color = if cp_map.is_some() {
                    base_color
                } else {
                    highlight_color
                };
                lines.push(Line::from(Span::styled(
                    format!("{key_prefix}{{"),
                    Style::default().fg(bracket_color),
                )));
                // Union of own keys and counterpart keys so absent keys are shown.
                let mut all_keys: std::collections::BTreeSet<&str> =
                    map.keys().map(String::as_str).collect();
                if let Some(cp) = cp_map {
                    all_keys.extend(cp.keys().map(String::as_str));
                }
                for k in all_keys {
                    let child_prefix = format!("{deeper}  \"{k}\": ");
                    if let Some(v) = map.get(k) {
                        let child_cp = cp_map.and_then(|m| m.get(k));
                        push_json_value_lines(
                            lines,
                            v,
                            child_cp,
                            &child_prefix,
                            &deeper,
                            base_color,
                            highlight_color,
                        );
                    } else {
                        // Key present in counterpart, absent on this side.
                        lines.push(Line::from(Span::styled(
                            format!("{child_prefix}(absent)"),
                            Style::default().fg(highlight_color),
                        )));
                    }
                }
                lines.push(Line::from(Span::styled(
                    format!("{deeper}}}"),
                    Style::default().fg(bracket_color),
                )));
            }
        }
        serde_json::Value::Array(arr) => {
            let cp_arr = counterpart.and_then(|c| c.as_array());
            let cp_len = cp_arr.map_or(0, |a| a.len());
            let max_len = arr.len().max(cp_len);
            if max_len == 0 {
                let differs = counterpart != Some(value);
                let color = if differs { highlight_color } else { base_color };
                lines.push(Line::from(Span::styled(
                    format!("{key_prefix}[]"),
                    Style::default().fg(color),
                )));
            } else {
                let bracket_color = if cp_arr.is_some() {
                    base_color
                } else {
                    highlight_color
                };
                lines.push(Line::from(Span::styled(
                    format!("{key_prefix}["),
                    Style::default().fg(bracket_color),
                )));
                let item_prefix = format!("{deeper}  ");
                for i in 0..max_len {
                    let item_cp = cp_arr.and_then(|a| a.get(i));
                    if let Some(item) = arr.get(i) {
                        push_json_value_lines(
                            lines,
                            item,
                            item_cp,
                            &item_prefix,
                            &deeper,
                            base_color,
                            highlight_color,
                        );
                    } else {
                        // Element present in counterpart, absent on this side.
                        lines.push(Line::from(Span::styled(
                            format!("{item_prefix}(absent)"),
                            Style::default().fg(highlight_color),
                        )));
                    }
                }
                lines.push(Line::from(Span::styled(
                    format!("{deeper}]"),
                    Style::default().fg(bracket_color),
                )));
            }
        }
        serde_json::Value::String(s) => {
            let differs = counterpart != Some(value);
            let color = if differs { highlight_color } else { base_color };
            let rendered = if s == "__js_undefined__" {
                // JS `undefined` — encoded as sentinel during parsing
                format!("{key_prefix}undefined")
            } else if s == "__truncated_object__" {
                // Vitest depth-truncated object — was `[Object]` in the output
                format!("{key_prefix}{{ … }}")
            } else if s == "__truncated_array__" {
                // Vitest depth-truncated array — was `[Array]` in the output
                format!("{key_prefix}[ … ]")
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
    failure: &'a crate::models::FailureOutput,
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
        (Some(serde_json::Value::Object(exp_map)), Some(serde_json::Value::Object(act_map))) => {
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
        (Some(serde_json::Value::Array(exp_arr)), Some(serde_json::Value::Array(act_arr))) => {
            lines.push(Line::from(Span::styled(
                "  Expected:",
                Style::default().fg(theme::GREEN),
            )));
            push_json_array_lines(
                &mut lines,
                exp_arr,
                Some(act_arr),
                theme::GREEN,
                theme::YELLOW,
                "  ",
            );
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "  Actual:",
                Style::default().fg(theme::RED),
            )));
            push_json_array_lines(
                &mut lines,
                act_arr,
                Some(exp_arr),
                theme::RED,
                theme::YELLOW,
                "  ",
            );
        }
        _ => {
            if let Some(expected) = failure.expected.as_deref() {
                lines.push(Line::from(Span::styled(
                    "  Expected:",
                    Style::default().fg(theme::GREEN),
                )));
                for text_line in expected.lines() {
                    lines.push(Line::from(Span::styled(
                        format!("    {text_line}"),
                        Style::default().fg(theme::GREEN),
                    )));
                }
            }
            if let Some(actual) = failure.actual.as_deref() {
                lines.push(Line::from(Span::styled(
                    "  Actual:",
                    Style::default().fg(theme::RED),
                )));
                for text_line in actual.lines() {
                    lines.push(Line::from(Span::styled(
                        format!("    {text_line}"),
                        Style::default().fg(theme::RED),
                    )));
                }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::FailureOutput;
    use serde_json::json;

    fn line_text(line: &Line) -> String {
        line.spans.iter().map(|s| s.content.as_ref()).collect()
    }

    fn make_failure(
        expected_parsed: Option<serde_json::Value>,
        actual_parsed: Option<serde_json::Value>,
    ) -> FailureOutput {
        FailureOutput {
            message: "assertion failed".into(),
            expected: None,
            actual: None,
            expected_parsed,
            actual_parsed,
            diff: None,
            source_snippet: None,
            stack_trace: None,
        }
    }

    // ── push_json_array_lines ───────────────────────────────────────────────

    #[test]
    fn array_lines_renders_brackets() {
        let arr = vec![json!(1), json!(2)];
        let mut lines = vec![];
        push_json_array_lines(
            &mut lines,
            &arr,
            None,
            ratatui::prelude::Color::Green,
            ratatui::prelude::Color::Yellow,
            "  ",
        );
        assert_eq!(line_text(&lines[0]), "  [");
        assert_eq!(line_text(lines.last().unwrap()), "  ]");
    }

    #[test]
    fn array_lines_renders_primitive_elements() {
        let arr = vec![json!(1), json!("hello"), json!(true)];
        let mut lines = vec![];
        push_json_array_lines(
            &mut lines,
            &arr,
            None,
            ratatui::prelude::Color::Green,
            ratatui::prelude::Color::Yellow,
            "",
        );
        // lines: "[", "  1", "  \"hello\"", "  true", "]"
        assert_eq!(line_text(&lines[1]), "  1");
        assert_eq!(line_text(&lines[2]), "  \"hello\"");
        assert_eq!(line_text(&lines[3]), "  true");
    }

    #[test]
    fn array_lines_highlights_differing_element() {
        let arr = vec![json!(1), json!(99)];
        let counterpart = vec![json!(1), json!(2)];
        let mut lines = vec![];
        push_json_array_lines(
            &mut lines,
            &arr,
            Some(&counterpart),
            ratatui::prelude::Color::Green,
            ratatui::prelude::Color::Yellow,
            "",
        );
        // element at index 0 matches → base_color; index 1 differs → highlight_color
        let same_line = &lines[1]; // "  1"
        let diff_line = &lines[2]; // "  99"
        assert!(
            same_line
                .spans
                .iter()
                .all(|s| s.style.fg == Some(ratatui::prelude::Color::Green))
        );
        assert!(
            diff_line
                .spans
                .iter()
                .all(|s| s.style.fg == Some(ratatui::prelude::Color::Yellow))
        );
    }

    #[test]
    fn array_lines_treats_extra_element_as_differing() {
        // actual has an extra element that expected doesn't — counterpart index is None → differs
        let arr = vec![json!(1), json!(2), json!(3)];
        let counterpart = vec![json!(1), json!(2)];
        let mut lines = vec![];
        push_json_array_lines(
            &mut lines,
            &arr,
            Some(&counterpart),
            ratatui::prelude::Color::Green,
            ratatui::prelude::Color::Yellow,
            "",
        );
        let extra_line = &lines[3]; // "  3" — no counterpart
        assert!(
            extra_line
                .spans
                .iter()
                .all(|s| s.style.fg == Some(ratatui::prelude::Color::Yellow))
        );
    }

    #[test]
    fn array_lines_nested_objects() {
        let arr = vec![json!({"id": 1}), json!({"id": 2})];
        let mut lines = vec![];
        push_json_array_lines(
            &mut lines,
            &arr,
            None,
            ratatui::prelude::Color::Green,
            ratatui::prelude::Color::Yellow,
            "",
        );
        let texts: Vec<String> = lines.iter().map(line_text).collect();
        assert!(texts.iter().any(|t| t.contains("\"id\"")));
    }

    #[test]
    fn value_lines_truncated_object_sentinel_renders_marker() {
        let val = json!("__truncated_object__");
        let mut lines = vec![];
        push_json_value_lines(
            &mut lines,
            &val,
            None,
            "  \"deep\": ",
            "",
            ratatui::prelude::Color::Green,
            ratatui::prelude::Color::Yellow,
        );
        assert_eq!(lines.len(), 1);
        assert_eq!(line_text(&lines[0]), "  \"deep\": { … }");
    }

    #[test]
    fn value_lines_truncated_array_sentinel_renders_marker() {
        let val = json!("__truncated_array__");
        let mut lines = vec![];
        push_json_value_lines(
            &mut lines,
            &val,
            None,
            "  \"items\": ",
            "",
            ratatui::prelude::Color::Green,
            ratatui::prelude::Color::Yellow,
        );
        assert_eq!(lines.len(), 1);
        assert_eq!(line_text(&lines[0]), "  \"items\": [ … ]");
    }

    #[test]
    fn value_lines_truncated_sentinel_matching_counterpart_base_color() {
        let val = json!("__truncated_object__");
        let cp = json!("__truncated_object__");
        let mut lines = vec![];
        push_json_value_lines(
            &mut lines,
            &val,
            Some(&cp),
            "k: ",
            "",
            ratatui::prelude::Color::Green,
            ratatui::prelude::Color::Yellow,
        );
        // both sides truncated identically — no diff highlight
        assert!(
            lines[0]
                .spans
                .iter()
                .all(|s| s.style.fg == Some(ratatui::prelude::Color::Green))
        );
    }

    #[test]
    fn array_lines_renders_undefined_sentinel() {
        let arr = vec![json!("__js_undefined__")];
        let mut lines = vec![];
        push_json_array_lines(
            &mut lines,
            &arr,
            None,
            ratatui::prelude::Color::Green,
            ratatui::prelude::Color::Yellow,
            "",
        );
        // sentinel string is rendered without quotes as `undefined`
        assert_eq!(line_text(&lines[1]), "  undefined");
    }

    #[test]
    fn value_lines_empty_object_renders_inline() {
        let val = json!({});
        let mut lines = vec![];
        push_json_value_lines(
            &mut lines,
            &val,
            None,
            "  \"key\": ",
            "",
            ratatui::prelude::Color::Green,
            ratatui::prelude::Color::Yellow,
        );
        assert_eq!(lines.len(), 1, "empty object must be a single line");
        assert_eq!(line_text(&lines[0]), "  \"key\": {}");
    }

    #[test]
    fn value_lines_empty_array_renders_inline() {
        let val = json!([]);
        let mut lines = vec![];
        push_json_value_lines(
            &mut lines,
            &val,
            None,
            "  \"children\": ",
            "",
            ratatui::prelude::Color::Green,
            ratatui::prelude::Color::Yellow,
        );
        assert_eq!(lines.len(), 1, "empty array must be a single line");
        assert_eq!(line_text(&lines[0]), "  \"children\": []");
    }

    #[test]
    fn value_lines_empty_object_matching_counterpart_uses_base_color() {
        let val = json!({});
        let cp = json!({});
        let mut lines = vec![];
        push_json_value_lines(
            &mut lines,
            &val,
            Some(&cp),
            "k: ",
            "",
            ratatui::prelude::Color::Green,
            ratatui::prelude::Color::Yellow,
        );
        assert!(
            lines[0]
                .spans
                .iter()
                .all(|s| s.style.fg == Some(ratatui::prelude::Color::Green))
        );
    }

    #[test]
    fn value_lines_empty_object_differing_counterpart_uses_highlight_color() {
        let val = json!({});
        let cp = json!({"a": 1}); // non-empty counterpart → differs
        let mut lines = vec![];
        push_json_value_lines(
            &mut lines,
            &val,
            Some(&cp),
            "k: ",
            "",
            ratatui::prelude::Color::Green,
            ratatui::prelude::Color::Yellow,
        );
        assert!(
            lines[0]
                .spans
                .iter()
                .all(|s| s.style.fg == Some(ratatui::prelude::Color::Yellow))
        );
    }

    #[test]
    fn array_lines_with_empty_object_elements_renders_each_inline() {
        // Simulates [Object] truncation → each element is {}
        let arr = vec![json!({}), json!({}), json!({})];
        let mut lines = vec![];
        push_json_array_lines(
            &mut lines,
            &arr,
            None,
            ratatui::prelude::Color::Red,
            ratatui::prelude::Color::Yellow,
            "  ",
        );
        // "[", "{}", "{}", "{}", "]" — five lines total, no blank {} splits
        let texts: Vec<String> = lines.iter().map(line_text).collect();
        assert_eq!(texts[0], "  [");
        assert_eq!(texts[1], "    {}");
        assert_eq!(texts[2], "    {}");
        assert_eq!(texts[3], "    {}");
        assert_eq!(texts[4], "  ]");
        assert_eq!(lines.len(), 5);
    }

    #[test]
    fn array_lines_empty_renders_inline() {
        let mut lines = vec![];
        push_json_array_lines(
            &mut lines,
            &[],
            None,
            ratatui::prelude::Color::Green,
            ratatui::prelude::Color::Yellow,
            "  ",
        );
        assert_eq!(lines.len(), 1);
        assert_eq!(line_text(&lines[0]), "  []");
    }

    #[test]
    fn object_lines_empty_renders_inline() {
        let map = serde_json::Map::new();
        let mut lines = vec![];
        push_json_object_lines(
            &mut lines,
            &map,
            None,
            ratatui::prelude::Color::Green,
            ratatui::prelude::Color::Yellow,
            "  ",
        );
        assert_eq!(lines.len(), 1);
        assert_eq!(line_text(&lines[0]), "  {}");
    }

    // ── build_failure_text — array path ────────────────────────────────────

    #[test]
    fn build_failure_text_array_shows_expected_and_actual() {
        let failure = make_failure(Some(json!([1, 2, 3])), Some(json!([1, 2, 99])));
        let text = build_failure_text(&failure, "my test");
        let all_text: Vec<String> = text.lines.iter().map(line_text).collect();
        assert!(all_text.iter().any(|l| l.contains("Expected")));
        assert!(all_text.iter().any(|l| l.contains("Actual")));
        assert!(all_text.iter().any(|l| l.contains('[')));
        assert!(all_text.iter().any(|l| l.contains(']')));
    }

    #[test]
    fn build_failure_text_object_path_still_works() {
        let failure = make_failure(
            Some(json!({"a": 1, "b": 2})),
            Some(json!({"a": 1, "b": 99})),
        );
        let text = build_failure_text(&failure, "my test");
        let all_text: Vec<String> = text.lines.iter().map(line_text).collect();
        assert!(all_text.iter().any(|l| l.contains("Expected")));
        assert!(all_text.iter().any(|l| l.contains("Actual")));
        assert!(all_text.iter().any(|l| l.contains('"')));
    }

    #[test]
    fn build_failure_text_mixed_types_falls_back_to_raw() {
        // expected is array, actual is object — no structured diff
        let failure = FailureOutput {
            message: "oops".into(),
            expected: Some("exp".into()),
            actual: Some("act".into()),
            expected_parsed: Some(json!([1, 2])),
            actual_parsed: Some(json!({"a": 1})),
            diff: None,
            source_snippet: None,
            stack_trace: None,
        };
        let text = build_failure_text(&failure, "my test");
        let all_text: Vec<String> = text.lines.iter().map(line_text).collect();
        // falls through to raw string path
        assert!(all_text.iter().any(|l| l.contains("exp")));
        assert!(all_text.iter().any(|l| l.contains("act")));
    }

    #[test]
    fn build_failure_text_raw_fallback_splits_multiline_expected() {
        let failure = FailureOutput {
            message: "oops".into(),
            expected: Some("line one\nline two\nline three".into()),
            actual: Some("other".into()),
            expected_parsed: None,
            actual_parsed: None,
            diff: None,
            source_snippet: None,
            stack_trace: None,
        };
        let text = build_failure_text(&failure, "t");
        let all_text: Vec<String> = text.lines.iter().map(line_text).collect();
        assert!(all_text.iter().any(|l| l.contains("line one")));
        assert!(all_text.iter().any(|l| l.contains("line two")));
        assert!(all_text.iter().any(|l| l.contains("line three")));
        // must be separate lines, not one blob
        let one_line_blob = all_text.iter().any(|l| l.contains("line one\nline two"));
        assert!(!one_line_blob);
    }

    #[test]
    fn build_failure_text_raw_fallback_splits_multiline_actual() {
        let failure = FailureOutput {
            message: "oops".into(),
            expected: Some("[]".into()),
            actual: Some("Array [\n  \"a\",\n  \"b\",\n]".into()),
            expected_parsed: None,
            actual_parsed: None,
            diff: None,
            source_snippet: None,
            stack_trace: None,
        };
        let text = build_failure_text(&failure, "t");
        let all_text: Vec<String> = text.lines.iter().map(line_text).collect();
        assert!(all_text.iter().any(|l| l.contains("Array [")));
        assert!(all_text.iter().any(|l| l.contains("\"a\",")));
        assert!(all_text.iter().any(|l| l.contains("\"b\",")));
    }

    // ── push_json_value_lines — array branch ────────────────────────────────

    #[test]
    fn value_lines_array_with_matching_counterpart() {
        let val = json!([1, 2, 3]);
        let cp = json!([1, 2, 3]);
        let mut lines = vec![];
        push_json_value_lines(
            &mut lines,
            &val,
            Some(&cp),
            "key: ",
            "",
            ratatui::prelude::Color::Green,
            ratatui::prelude::Color::Yellow,
        );
        // All elements match → all green
        for line in &lines {
            for span in &line.spans {
                assert_eq!(span.style.fg, Some(ratatui::prelude::Color::Green));
            }
        }
    }

    #[test]
    fn value_lines_array_differing_counterpart() {
        let val = json!([1, 99]);
        let cp = json!([1, 2]);
        let mut lines = vec![];
        push_json_value_lines(
            &mut lines,
            &val,
            Some(&cp),
            "key: ",
            "",
            ratatui::prelude::Color::Green,
            ratatui::prelude::Color::Yellow,
        );
        let texts: Vec<String> = lines.iter().map(line_text).collect();
        // 99 differs from 2, so it appears and is highlighted
        assert!(texts.iter().any(|t| t.contains("99")));
    }

    // ── bracket highlight when counterpart is absent ─────────────────────────

    #[test]
    fn value_lines_object_no_counterpart_highlights_brackets() {
        let val = json!({"cacheTtl": 3600, "prefetch": true});
        let mut lines = vec![];
        push_json_value_lines(
            &mut lines,
            &val,
            None,
            "  \"state\": ",
            "",
            ratatui::prelude::Color::Red,
            ratatui::prelude::Color::Yellow,
        );
        // All lines — brackets and leaves — must be highlight_color
        for line in &lines {
            assert!(
                line.spans
                    .iter()
                    .all(|s| s.style.fg == Some(ratatui::prelude::Color::Yellow))
            );
        }
    }

    #[test]
    fn value_lines_object_with_object_counterpart_base_color_brackets() {
        let val = json!({"a": 1, "b": 99});
        let cp = json!({"a": 1, "b": 2});
        let mut lines = vec![];
        push_json_value_lines(
            &mut lines,
            &val,
            Some(&cp),
            "k: ",
            "",
            ratatui::prelude::Color::Red,
            ratatui::prelude::Color::Yellow,
        );
        // Opening "{" → base_color
        assert!(
            lines[0]
                .spans
                .iter()
                .all(|s| s.style.fg == Some(ratatui::prelude::Color::Red))
        );
        // Closing "}" → base_color
        assert!(
            lines
                .last()
                .unwrap()
                .spans
                .iter()
                .all(|s| s.style.fg == Some(ratatui::prelude::Color::Red))
        );
    }

    #[test]
    fn value_lines_object_absent_counterpart_key_shown_highlighted() {
        // val has "a" only; counterpart has "a" and "b".
        // "b" must appear as (absent) in highlight_color on val's side.
        let val = json!({"a": 1});
        let cp = json!({"a": 1, "b": 99});
        let mut lines = vec![];
        push_json_value_lines(
            &mut lines,
            &val,
            Some(&cp),
            "obj: ",
            "",
            ratatui::prelude::Color::Green,
            ratatui::prelude::Color::Yellow,
        );
        let texts: Vec<String> = lines.iter().map(line_text).collect();
        let absent_line = texts
            .iter()
            .find(|l| l.contains("\"b\""))
            .expect("absent key must appear");
        assert!(absent_line.contains("(absent)"));
        let absent_idx = texts.iter().position(|l| l.contains("(absent)")).unwrap();
        assert!(
            lines[absent_idx]
                .spans
                .iter()
                .all(|s| s.style.fg == Some(ratatui::prelude::Color::Yellow))
        );
    }

    #[test]
    fn value_lines_array_no_counterpart_highlights_brackets() {
        let val = json!(["AuthGuard", "MfaGuard"]);
        let mut lines = vec![];
        push_json_value_lines(
            &mut lines,
            &val,
            None,
            "  \"guard\": ",
            "",
            ratatui::prelude::Color::Red,
            ratatui::prelude::Color::Yellow,
        );
        assert!(
            lines[0]
                .spans
                .iter()
                .all(|s| s.style.fg == Some(ratatui::prelude::Color::Yellow))
        );
        assert!(
            lines
                .last()
                .unwrap()
                .spans
                .iter()
                .all(|s| s.style.fg == Some(ratatui::prelude::Color::Yellow))
        );
    }

    #[test]
    fn value_lines_array_with_array_counterpart_base_color_brackets() {
        let val = json!([1, 2]);
        let cp = json!([1, 99]);
        let mut lines = vec![];
        push_json_value_lines(
            &mut lines,
            &val,
            Some(&cp),
            "k: ",
            "",
            ratatui::prelude::Color::Red,
            ratatui::prelude::Color::Yellow,
        );
        assert!(
            lines[0]
                .spans
                .iter()
                .all(|s| s.style.fg == Some(ratatui::prelude::Color::Red))
        );
        assert!(
            lines
                .last()
                .unwrap()
                .spans
                .iter()
                .all(|s| s.style.fg == Some(ratatui::prelude::Color::Red))
        );
    }

    #[test]
    fn value_lines_array_absent_counterpart_element_shown_highlighted() {
        // val has 2 elements; counterpart has 3. Element at index 2 must appear as (absent).
        let val = json!([1, 2]);
        let cp = json!([1, 2, 3]);
        let mut lines = vec![];
        push_json_value_lines(
            &mut lines,
            &val,
            Some(&cp),
            "arr: ",
            "",
            ratatui::prelude::Color::Green,
            ratatui::prelude::Color::Yellow,
        );
        let texts: Vec<String> = lines.iter().map(line_text).collect();
        let absent_idx = texts
            .iter()
            .position(|l| l.contains("(absent)"))
            .expect("absent element must appear");
        assert!(
            lines[absent_idx]
                .spans
                .iter()
                .all(|s| s.style.fg == Some(ratatui::prelude::Color::Yellow))
        );
    }

    #[test]
    fn object_lines_absent_counterpart_key_shown_highlighted() {
        // Top-level object rendering: key missing on this side must appear as (absent).
        let map: serde_json::Map<String, _> =
            serde_json::from_str(r#"{"runGuardsAndResolvers": "pathParamsChange"}"#).unwrap();
        let cp_map: serde_json::Map<String, _> = serde_json::from_str(
            r#"{"runGuardsAndResolvers": "pathParamsChange", "state": {"cacheTtl": 86400}}"#,
        )
        .unwrap();
        let mut lines = vec![];
        push_json_object_lines(
            &mut lines,
            &map,
            Some(&cp_map),
            ratatui::prelude::Color::Green,
            ratatui::prelude::Color::Yellow,
            "  ",
        );
        let texts: Vec<String> = lines.iter().map(line_text).collect();
        let absent_line = texts
            .iter()
            .find(|l| l.contains("\"state\""))
            .expect("absent key must appear");
        assert!(absent_line.contains("(absent)"));
    }
}
