use ratatui::{
    prelude::*,
    widgets::{Block, Borders, List, ListItem},
};

use super::theme;
use crate::{
    app::{App, Panel},
    models::{NodeKind, TestNode, TestStatus},
};

pub fn draw(frame: &mut Frame, app: &App, area: Rect) {
    let focused = app.active_panel == Panel::TestTree;
    let border_style = if focused {
        Style::default().fg(theme::BLUE)
    } else {
        Style::default().fg(theme::SURFACE2)
    };

    let title = match &app.project_name {
        Some(name) => format!(" Tests — {} ", name),
        None => " Tests ".to_string(),
    };

    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(border_style);

    let available_width = block.inner(area).width as usize;
    let inner_height = app.tree_viewport_height;

    let visible = app.visible_tree_nodes();
    let n = visible.len();

    // Precompute is_last: a node is last if no later visible node appears at the same or
    // shallower depth (meaning no more siblings at this level follow).
    let is_last: Vec<bool> = (0..n)
        .map(|i| {
            let (_, depth) = visible[i];
            let next = visible[i + 1..].iter().find(|&&(_, d)| d <= depth);
            match next {
                None => true,
                Some(&(_, d)) => d < depth,
            }
        })
        .collect();

    let start = app.tree_scroll_offset;
    let end = (start + inner_height).min(n);

    let items: Vec<ListItem> = visible[start..end]
        .iter()
        .enumerate()
        .map(|(view_i, &(node_id, depth))| {
            let abs_i = view_i + start;
            let node = app.tree.get(node_id).unwrap();
            let selected = abs_i == app.selected_tree_index && focused;

            let branch = branch_prefix(depth, is_last[abs_i]);
            let (icon, icon_color) = node_icon(node, app.spinner_tick);
            let show_folder = matches!(node.kind, NodeKind::Workspace | NodeKind::Project);
            let name = node_display_name(app.project_name.as_ref(), node);
            let name_color = node_name_color(node, app);

            // Build left spans first so we can measure them via Span::width()
            let mut left_spans: Vec<Span> = Vec::new();
            left_spans.push(Span::styled(branch, Style::default().fg(theme::SURFACE2)));
            if !icon.is_empty() {
                left_spans.push(Span::styled(icon, Style::default().fg(icon_color)));
            }
            if show_folder {
                left_spans.push(Span::styled("📁 ", Style::default().fg(theme::PEACH)));
            }
            left_spans.push(Span::styled(
                name.to_string(),
                Style::default().fg(name_color),
            ));

            let left_w: usize = left_spans.iter().map(|s| s.width()).sum();
            let right_spans = right_content(node, app, node_id);
            let right_w: usize = right_spans.iter().map(|s| s.width()).sum();

            let padding = available_width.saturating_sub(left_w + right_w);

            let mut spans = left_spans;
            spans.push(Span::raw(" ".repeat(padding)));
            spans.extend(right_spans);

            let content = Line::from(spans);
            let item = ListItem::new(content);
            if selected {
                item.style(Style::default().bg(theme::SURFACE1))
            } else {
                item
            }
        })
        .collect();

    let list = List::new(items).block(block);
    frame.render_widget(list, area);
}

/// Branch prefix using simplified algorithm: background columns are always "│  ",
/// only the connector column distinguishes last (└─) from non-last (├─).
fn branch_prefix(depth: usize, is_last: bool) -> String {
    if depth == 0 {
        return String::new();
    }
    let mut s = "│  ".repeat(depth - 1);
    if is_last {
        s.push_str("└─ ");
    } else {
        s.push_str("├─ ");
    }
    s
}

/// Returns (icon_string_with_trailing_space, icon_color).
fn node_icon(node: &TestNode, spinner_tick: usize) -> (String, Color) {
    match node.kind {
        NodeKind::Workspace => (String::new(), theme::SUBTEXT0),
        NodeKind::Project | NodeKind::File | NodeKind::Suite => {
            let icon = if node.expanded { "▼ " } else { "▶ " };
            (icon.to_string(), theme::SUBTEXT0)
        }
        NodeKind::Test => match node.status {
            TestStatus::Running => {
                const FRAMES: &[&str] =
                    &["⠋ ", "⠙ ", "⠹ ", "⠸ ", "⠼ ", "⠴ ", "⠦ ", "⠧ ", "⠇ ", "⠏ "];
                (
                    FRAMES[spinner_tick % FRAMES.len()].to_string(),
                    theme::YELLOW,
                )
            }
            _ => ("■ ".to_string(), node.status.color()),
        },
    }
}

fn node_name_color(node: &TestNode, app: &App) -> Color {
    if app.watched_ids.contains(&node.id) {
        return theme::TEAL;
    }
    match node.kind {
        NodeKind::Test => match node.status {
            TestStatus::Failed => theme::RED,
            TestStatus::Passed => theme::SUBTEXT0,
            TestStatus::Skipped => theme::OVERLAY0,
            _ => theme::TEXT,
        },
        _ => theme::TEXT,
    }
}

/// Build right-aligned spans. Width is measured by the caller via Span::width().
fn right_content(node: &TestNode, app: &App, node_id: usize) -> Vec<Span<'static>> {
    match node.kind {
        NodeKind::Workspace => {
            let (_, _, total_tests) = app.tree.subtree_test_counts(node_id);
            let file_count = app.tree.subtree_file_count(node_id);
            let s = format!("{} files · {} tests", file_count, total_tests);
            vec![Span::styled(s, Style::default().fg(theme::SUBTEXT0))]
        }
        NodeKind::Project | NodeKind::File | NodeKind::Suite => {
            let (passed, failed, total) = app.tree.subtree_test_counts(node_id);
            if total == 0 {
                return vec![];
            }
            build_gauge_spans(passed, failed, total)
        }
        NodeKind::Test => {
            if let Some(ms) = node.result.as_ref().and_then(|r| r.duration_ms) {
                let s = format!("{}ms", ms);
                vec![Span::styled(s, Style::default().fg(theme::SUBTEXT0))]
            } else {
                vec![]
            }
        }
    }
}

/// Count indicator: "passed/total" with passed coloured by status.
fn build_gauge_spans(passed: usize, _failed: usize, total: usize) -> Vec<Span<'static>> {
    let passed_color = if passed == total {
        theme::GREEN
    } else if passed == 0 {
        theme::RED
    } else {
        theme::YELLOW
    };
    vec![
        Span::styled(passed.to_string(), Style::default().fg(passed_color)),
        Span::styled(format!("/{}", total), Style::default().fg(theme::SUBTEXT0)),
    ]
}

fn node_display_name<'a>(_project: Option<&'a String>, node: &'a TestNode) -> &'a str {
    &node.name
}
