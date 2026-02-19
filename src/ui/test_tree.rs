use ratatui::{
    prelude::*,
    widgets::{Block, Borders, List, ListItem},
};

use crate::{
    app::{App, Panel},
    models::{NodeKind, TestStatus},
};

pub fn draw(frame: &mut Frame, app: &mut App, area: Rect) {
    let focused = app.active_panel == Panel::TestTree;
    let border_style = if focused {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
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
                TestStatus::Passed => Color::Green,
                TestStatus::Failed => Color::Red,
                TestStatus::Running => Color::Yellow,
                TestStatus::Skipped => Color::DarkGray,
                TestStatus::Pending => Color::White,
            };

            let selected = absolute_i == app.selected_tree_index && focused;
            let name_style = if selected {
                Style::default().bg(Color::DarkGray).fg(Color::White)
            } else {
                Style::default()
            };

            let content = Line::from(vec![
                Span::raw(indent),
                Span::styled(icon, Style::default().fg(status_color)),
                Span::styled(&node.name, name_style),
            ]);

            ListItem::new(content)
        })
        .collect();

    let list = List::new(items).block(block);
    frame.render_widget(list, area);
}
