use ratatui::{
    prelude::*,
    widgets::{Block, Borders, List, ListItem},
};

use super::theme;
use crate::app::{App, Panel};

pub fn draw(frame: &mut Frame, app: &App, area: Rect) {
    let focused = app.active_panel == Panel::FailedList;
    let border_style = if focused {
        Style::default().fg(theme::BLUE)
    } else {
        Style::default().fg(theme::SURFACE2)
    };

    let block = Block::default()
        .title(" Failed Tests ")
        .borders(Borders::ALL)
        .border_style(border_style);

    let inner_height = app.failed_viewport_height;

    let failed_ids = app.tree.failed_nodes();
    let end = (app.failed_scroll_offset + inner_height).min(failed_ids.len());

    let items: Vec<ListItem> = failed_ids[app.failed_scroll_offset..end]
        .iter()
        .enumerate()
        .map(|(view_i, &node_id)| {
            let absolute_i = view_i + app.failed_scroll_offset;
            let node = app.tree.get(node_id).unwrap();

            let selected = absolute_i == app.selected_failed_index && focused;

            let item = ListItem::new(Line::from(vec![
                Span::styled("✘ ", Style::default().fg(theme::RED)),
                Span::styled(&node.name, Style::default().fg(theme::RED)),
            ]));

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
