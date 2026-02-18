use ratatui::{
    prelude::*,
    widgets::{Block, Borders, List, ListItem},
};

use crate::app::{App, Panel};

pub fn draw(frame: &mut Frame, app: &mut App, area: Rect) {
    let focused = app.active_panel == Panel::FailedList;
    let border_style = if focused {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let block = Block::default()
        .title(" Failed Tests ")
        .borders(Borders::ALL)
        .border_style(border_style);

    let inner_height = block.inner(area).height as usize;
    app.failed_viewport_height = inner_height;

    let failed_ids = app.tree.failed_nodes();
    let end = (app.failed_scroll_offset + inner_height).min(failed_ids.len());
    let items: Vec<ListItem> = failed_ids[app.failed_scroll_offset..end]
        .iter()
        .enumerate()
        .map(|(view_i, &node_id)| {
            let absolute_i = view_i + app.failed_scroll_offset;
            let node = app.tree.get(node_id).unwrap();
            let style = if absolute_i == app.selected_failed_index && focused {
                Style::default().bg(Color::DarkGray).fg(Color::White)
            } else {
                Style::default().fg(Color::Red)
            };

            ListItem::new(Line::from(vec![
                Span::styled("✘ ", Style::default().fg(Color::Red)),
                Span::styled(&node.name, style),
            ]))
        })
        .collect();

    let list = List::new(items).block(block);
    frame.render_widget(list, area);
}
