use ratatui::prelude::*;

use crate::app::{App, LayoutMode};

/// Below this width `LayoutMode::Auto` stacks the panels vertically —
/// side-by-side panels leave too little room to read test output
/// (e.g. when running inside an editor split).
const AUTO_STACK_MAX_WIDTH: u16 = 100;

use super::failure_list;
use super::help_overlay;
use super::notifications;
use super::output_panel;
use super::search_box;
use super::status_bar;
use super::test_tree;

pub fn draw(frame: &mut Frame, app: &mut App) {
    let [main_area, status_area] =
        Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).areas(frame.area());

    let stacked = match app.layout_mode {
        LayoutMode::Horizontal => false,
        LayoutMode::Vertical => true,
        LayoutMode::Auto => main_area.width < AUTO_STACK_MAX_WIDTH,
    };

    let (tree_area, failed_area, output_area) = match (stacked, app.show_failed_panel) {
        (true, true) => {
            let [tree, failed, output] = Layout::vertical([
                Constraint::Percentage(30),
                Constraint::Percentage(20),
                Constraint::Percentage(50),
            ])
            .areas(main_area);
            (tree, Some(failed), output)
        }
        (true, false) => {
            let [tree, output] =
                Layout::vertical([Constraint::Percentage(40), Constraint::Percentage(60)])
                    .areas(main_area);
            (tree, None, output)
        }
        (false, true) => {
            let [left_area, right_area] =
                Layout::horizontal([Constraint::Percentage(40), Constraint::Percentage(60)])
                    .areas(main_area);

            let [tree, failed] =
                Layout::vertical([Constraint::Percentage(60), Constraint::Percentage(40)])
                    .areas(left_area);
            (tree, Some(failed), right_area)
        }
        (false, false) => {
            let [tree, output] =
                Layout::horizontal([Constraint::Percentage(40), Constraint::Percentage(60)])
                    .areas(main_area);
            (tree, None, output)
        }
    };

    app.failed_viewport_height = failed_area
        .map(|a| a.height.saturating_sub(2) as usize)
        .unwrap_or(0);

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

    if let Some(failed_area) = failed_area {
        failure_list::draw(frame, app, failed_area);
    }

    app.output_scroll_offset =
        output_panel::draw(frame, app, app.output_scroll_offset, output_area);

    status_bar::draw(frame, app, status_area);
    notifications::draw(frame, app);

    if app.show_help {
        help_overlay::draw(frame);
    }
}
