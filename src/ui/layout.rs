use ratatui::prelude::*;

use crate::app::App;

use super::detail_panel;
use super::failure_list;
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

    if app.filter_active || !app.filter_query.is_empty() {
        let [search_area, filtered_tree_area] =
            Layout::vertical([Constraint::Length(3), Constraint::Min(1)]).areas(tree_area);
        search_box::draw(frame, &app.filter_query, app.filter_active, search_area);
        test_tree::draw(frame, app, filtered_tree_area);
    } else {
        test_tree::draw(frame, app, tree_area);
    }
    failure_list::draw(frame, app, failed_area);
    detail_panel::draw(frame, app, right_area);
    status_bar::draw(frame, app, status_area);
}
