use ratatui::{
    prelude::*,
    widgets::{Block, Borders, List, ListItem},
};

use super::theme;
use crate::{
    app::{App, Panel},
    models::{NodeKind, TestNode, TestStatus},
};

pub fn draw(frame: &mut Frame, app: &mut App, area: Rect) {
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

    // Calculate viewport height (inner area minus borders)
    let inner_height = block.inner(area).height as usize;
    app.tree_viewport_height = inner_height;

    let visible = app.visible_tree_nodes();
    let end = (app.tree_scroll_offset + inner_height).min(visible.len());
    let items: Vec<ListItem> = visible[app.tree_scroll_offset..end]
        .iter()
        .enumerate()
        .map(|(view_i, &(node_id, depth))| {
            let absolute_i = view_i + app.tree_scroll_offset;
            let node = app.tree.get(node_id).unwrap();
            let indent = "  ".repeat(depth);
            let icon = match node.kind {
                NodeKind::Workspace | NodeKind::Project | NodeKind::File | NodeKind::Suite => {
                    if node.expanded {
                        "▼ "
                    } else {
                        "▶ "
                    }
                }
                NodeKind::Test => match node.status {
                    TestStatus::Passed => "✔ ",
                    TestStatus::Failed => "✘ ",
                    TestStatus::Running => {
                        const FRAMES: &[&str] =
                            &["⠋ ", "⠙ ", "⠹ ", "⠸ ", "⠼ ", "⠴ ", "⠦ ", "⠧ ", "⠇ ", "⠏ "];
                        FRAMES[app.spinner_tick % FRAMES.len()]
                    }
                    TestStatus::Skipped => "⊘ ",
                    TestStatus::Pending => "◌ ",
                },
            };

            let status_color = match node.status {
                TestStatus::Passed => theme::GREEN,
                TestStatus::Failed => theme::RED,
                TestStatus::Running => theme::YELLOW,
                TestStatus::Skipped => theme::OVERLAY0,
                TestStatus::Pending => theme::SUBTEXT0,
            };

            let selected = absolute_i == app.selected_tree_index && focused;
            let name_style = if selected {
                Style::default().bg(theme::SURFACE1).fg(theme::TEXT)
            } else {
                Style::default().fg(theme::TEXT)
            };

            let name = node_display_name(app.project_name.as_ref(), node);

            let content = Line::from(vec![
                Span::raw(indent),
                Span::styled(icon, Style::default().fg(status_color)),
                Span::styled(name, name_style),
            ]);

            ListItem::new(content)
        })
        .collect();

    let list = List::new(items).block(block);
    frame.render_widget(list, area);
}

fn node_display_name<'a>(project: Option<&'a String>, node: &'a TestNode) -> &'a str {
    if let Some(project_name) = project
        && node.kind == NodeKind::File
    {
        let common_prefix = common_prefix_to_exclude(&node.name);
        let prefix = format!("{project_name}/{common_prefix}");

        node.name
            .split_once(&prefix)
            .map(|(_, suffix)| suffix)
            .unwrap_or(&node.name)
    } else {
        &node.name
    }
}

fn common_prefix_to_exclude(name: &str) -> &str {
    if name.contains("/src/app/") {
        "src/app/"
    } else if name.contains("src/lib/") {
        "src/lib/"
    } else if name.contains("src/") {
        "src/"
    } else {
        "/"
    }
}
