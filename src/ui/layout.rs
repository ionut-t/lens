use ratatui::prelude::*;

use crate::app::App;

use super::detail_panel;
use super::failure_list;
use super::notifications;
use super::search_box;
use super::status_bar;
use super::test_tree;

pub fn draw(frame: &mut Frame, app: &mut App) {
    let [main_area, status_area] =
        Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).areas(frame.area());

    let [left_area, right_area] =
        Layout::horizontal([Constraint::Percentage(40), Constraint::Percentage(60)])
            .areas(main_area);

    let [tree_area, failed_area] =
        Layout::vertical([Constraint::Percentage(60), Constraint::Percentage(40)]).areas(left_area);

    app.failed_viewport_height = failed_area.height.saturating_sub(2) as usize;

    if app.filter_active || !app.filter.value().is_empty() {
        let [search_area, filtered_tree_area] =
            Layout::vertical([Constraint::Length(3), Constraint::Min(1)]).areas(tree_area);

        app.tree_viewport_height = filtered_tree_area.height.saturating_sub(2) as usize;

        search_box::draw(frame, &app.filter, app.filter_active, search_area);
        test_tree::draw(frame, app, filtered_tree_area);
    } else {
        app.tree_viewport_height = tree_area.height.saturating_sub(2) as usize;

        test_tree::draw(frame, app, tree_area);
    }

    failure_list::draw(frame, app, failed_area);

    app.detail_scroll_offset = detail_panel::draw(frame, app, app.detail_scroll_offset, right_area);

    status_bar::draw(frame, app, status_area);
    notifications::draw(frame, app);
}
