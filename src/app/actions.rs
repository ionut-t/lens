use std::path::PathBuf;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::{
    app::{App, Panel, PendingRun},
    models::{NodeKind, TestStatus},
};

#[derive(Debug)]
pub enum Action {
    Quit,
    FocusNext,
    FocusPrevious,
    NavigateUp,
    NavigateDown,
    ScrollUp,
    ScrollDown,
    Expand,
    ExpandAll,
    Collapse,
    CollapseAll,
    JumpToStart,
    JumpToEnd,
    JumpToPrevFile,
    JumpToNextFile,
    Select,
    RunAll,
    RerunFailed,
    ToggleWatch,
    FilterEnter,
    FilterInput(char),
    FilterBackspace,
    FilterExit,
    FilterApply,
    OpenInEditor,
}

/// Process a keyboard action.
pub fn handle_action(app: &mut App, action: Action) {
    match action {
        Action::Quit => app.should_quit = true,

        Action::FocusNext => {
            app.active_panel = match app.active_panel {
                Panel::TestTree => Panel::FailedList,
                Panel::FailedList => Panel::Detail,
                Panel::Detail => Panel::TestTree,
            };
        }

        Action::FocusPrevious => {
            app.active_panel = match app.active_panel {
                Panel::TestTree => Panel::Detail,
                Panel::FailedList => Panel::TestTree,
                Panel::Detail => Panel::FailedList,
            };
        }

        Action::NavigateUp => match app.active_panel {
            Panel::TestTree => {
                app.selected_tree_index = app.selected_tree_index.saturating_sub(1);
                app.detail_scroll_offset = 0;
                app.adjust_tree_scroll();
            }

            Panel::FailedList => {
                app.selected_failed_index = app.selected_failed_index.saturating_sub(1);
                app.detail_scroll_offset = 0;
                app.adjust_failed_scroll();
            }

            Panel::Detail => {
                app.detail_scroll_offset = app.detail_scroll_offset.saturating_sub(1);
            }
        },

        Action::NavigateDown => match app.active_panel {
            Panel::TestTree => {
                let max = app.visible_tree_nodes().len().saturating_sub(1);
                app.selected_tree_index = (app.selected_tree_index + 1).min(max);
                app.detail_scroll_offset = 0;
                app.adjust_tree_scroll();
            }

            Panel::FailedList => {
                let max = app.tree.failed_nodes().len().saturating_sub(1);
                app.selected_failed_index = (app.selected_failed_index + 1).min(max);
                app.detail_scroll_offset = 0;
                app.adjust_failed_scroll();
            }

            Panel::Detail => {
                app.detail_scroll_offset = app.detail_scroll_offset.saturating_add(1);
            }
        },

        Action::ScrollUp => {
            let half = (app.tree_viewport_height / 2).max(1);
            match app.active_panel {
                Panel::TestTree => {
                    app.selected_tree_index = app.selected_tree_index.saturating_sub(half);
                    app.detail_scroll_offset = 0;
                    app.adjust_tree_scroll();
                }
                Panel::FailedList => {
                    app.selected_failed_index = app.selected_failed_index.saturating_sub(half);
                    app.detail_scroll_offset = 0;
                    app.adjust_failed_scroll();
                }
                Panel::Detail => {
                    app.detail_scroll_offset = app.detail_scroll_offset.saturating_sub(half as u16);
                }
            }
        }

        Action::ScrollDown => {
            let half = (app.tree_viewport_height / 2).max(1);
            match app.active_panel {
                Panel::TestTree => {
                    let max = app.visible_tree_nodes().len().saturating_sub(1);
                    app.selected_tree_index = (app.selected_tree_index + half).min(max);
                    app.detail_scroll_offset = 0;
                    app.adjust_tree_scroll();
                }
                Panel::FailedList => {
                    let max = app.tree.failed_nodes().len().saturating_sub(1);
                    app.selected_failed_index = (app.selected_failed_index + half).min(max);
                    app.detail_scroll_offset = 0;
                    app.adjust_failed_scroll();
                }
                Panel::Detail => {
                    app.detail_scroll_offset = app.detail_scroll_offset.saturating_add(half as u16);
                }
            }
        }

        Action::Expand => {
            if app.active_panel == Panel::TestTree
                && let Some(&(node_id, _)) = app.visible_tree_nodes().get(app.selected_tree_index)
                && let Some(node) = app.tree.get(node_id)
                && !node.children.is_empty()
            {
                app.tree.toggle_expanded(node_id);
            }
        }

        Action::ExpandAll => {
            if app.active_panel == Panel::TestTree {
                app.tree.expand_all();
            }
        }

        Action::CollapseAll => {
            if app.active_panel == Panel::TestTree {
                app.tree.collapse_all();
                app.selected_tree_index = 0;
                app.tree_scroll_offset = 0;
            }
        }

        Action::JumpToStart => match app.active_panel {
            Panel::TestTree => {
                app.selected_tree_index = 0;
                app.tree_scroll_offset = 0;
                app.detail_scroll_offset = 0;
            }
            Panel::FailedList => {
                app.selected_failed_index = 0;
                app.failed_scroll_offset = 0;
                app.detail_scroll_offset = 0;
            }
            Panel::Detail => {
                app.detail_scroll_offset = 0;
            }
        },

        Action::JumpToEnd => match app.active_panel {
            Panel::TestTree => {
                let max = app.visible_tree_nodes().len().saturating_sub(1);
                app.selected_tree_index = max;
                app.detail_scroll_offset = 0;
                app.adjust_tree_scroll();
            }
            Panel::FailedList => {
                let max = app.tree.failed_nodes().len().saturating_sub(1);
                app.selected_failed_index = max;
                app.detail_scroll_offset = 0;
                app.adjust_failed_scroll();
            }
            Panel::Detail => {
                app.detail_scroll_offset = u16::MAX;
            }
        },

        Action::JumpToPrevFile => {
            if app.active_panel == Panel::TestTree {
                let nodes = app.visible_tree_nodes();
                for i in (0..app.selected_tree_index).rev() {
                    if let Some(&(node_id, _)) = nodes.get(i)
                        && let Some(node) = app.tree.get(node_id)
                        && node.kind == NodeKind::File
                    {
                        app.selected_tree_index = i;
                        app.detail_scroll_offset = 0;
                        app.adjust_tree_scroll();
                        break;
                    }
                }
            }
        }

        Action::JumpToNextFile => {
            if app.active_panel == Panel::TestTree {
                let nodes = app.visible_tree_nodes();
                for i in (app.selected_tree_index + 1)..nodes.len() {
                    if let Some(&(node_id, _)) = nodes.get(i)
                        && let Some(node) = app.tree.get(node_id)
                        && node.kind == NodeKind::File
                    {
                        app.selected_tree_index = i;
                        app.detail_scroll_offset = 0;
                        app.adjust_tree_scroll();
                        break;
                    }
                }
            }
        }

        Action::Select => {
            if app.active_panel == Panel::TestTree
                && let Some(&(node_id, _)) = app.visible_tree_nodes().get(app.selected_tree_index)
                && let Some(node) = app.tree.get(node_id)
            {
                match node.kind {
                    NodeKind::File => {
                        let abs_path = resolve_file_path(app, node_id);
                        set_running_status(app, node_id);
                        app.pending_runs.push(PendingRun::File(abs_path));
                    }
                    NodeKind::Test | NodeKind::Suite => {
                        let (file_path, test_name) = resolve_test_path(app, node_id);
                        set_running_status(app, node_id);
                        app.pending_runs.push(PendingRun::Test {
                            file: file_path,
                            name: test_name,
                        });
                    }
                    _ => {
                        if !node.children.is_empty() {
                            app.tree.toggle_expanded(node_id);
                        }
                    }
                }
            }
        }
        Action::Collapse => {
            if app.active_panel == Panel::TestTree
                && let Some(&(node_id, _)) = app.visible_tree_nodes().get(app.selected_tree_index)
                && let Some(node) = app.tree.get(node_id)
            {
                if node.expanded && !node.children.is_empty() {
                    app.tree.toggle_expanded(node_id);
                } else if let Some(parent_id) = node.parent {
                    // Collapse navigates to parent if already collapsed
                    app.tree.toggle_expanded(parent_id);
                    // Move selection to parent
                    if let Some(pos) = app
                        .visible_tree_nodes()
                        .iter()
                        .position(|&(id, _)| id == parent_id)
                    {
                        app.selected_tree_index = pos;
                    }
                }
            }
        }
        Action::RunAll => {
            app.tree.reset();
            app.progress_done = 0;
            app.running = true;
            app.full_run = true;
        }

        Action::RerunFailed => {
            let failed_ids = app.tree.failed_nodes();
            if failed_ids.is_empty() {
                return;
            }
            let mut seen_files = std::collections::HashSet::new();
            for &node_id in &failed_ids {
                let (file_path, _) = resolve_test_path(app, node_id);
                if seen_files.insert(file_path.clone()) {
                    app.pending_runs.push(PendingRun::File(file_path));
                }
            }
            for &node_id in &failed_ids {
                set_running_status(app, node_id);
            }
            app.running = true;
        }

        Action::ToggleWatch => {
            app.watch_mode = !app.watch_mode;
        }

        Action::FilterEnter => {
            app.filter_active = true;
        }

        Action::FilterInput(c) => {
            app.filter_query.push(c);
            app.selected_tree_index = 0;
            app.tree_scroll_offset = 0;
        }

        Action::FilterBackspace => {
            app.filter_query.pop();
        }

        Action::FilterExit => {
            app.filter_query.clear();
            app.filter_active = false;
        }

        Action::FilterApply => {
            app.filter_active = false;
        }

        Action::OpenInEditor => {
            if let Some(node_id) = app.selected_node_id() {
                let node = app.tree.get(node_id);

                // For failed tests, open at failure location; otherwise at definition
                let (line, col) = node
                    .and_then(|n| n.result.as_ref())
                    .and_then(|r| r.failure.as_ref())
                    .and_then(|f| f.stack_trace.as_ref())
                    .and_then(|st| parse_line_col_from_stack(st))
                    .or_else(|| node.and_then(|n| n.location.map(|(l, c)| (Some(l), Some(c)))))
                    .unwrap_or((None, None));

                // Walk up to find the file node
                let mut current = Some(node_id);
                while let Some(id) = current {
                    if let Some(n) = app.tree.get(id) {
                        if n.kind == NodeKind::File {
                            let path = resolve_file_path(app, id);
                            app.pending_editor = Some((path, line, col));
                            break;
                        }
                        current = n.parent;
                    } else {
                        break;
                    }
                }
            }
        }
    }
}

pub fn trigger_action(key: KeyEvent, filter_active: bool) -> Option<Action> {
    if filter_active {
        match key.code {
            KeyCode::Esc => Some(Action::FilterExit),
            KeyCode::Enter => Some(Action::FilterApply),
            KeyCode::Backspace => Some(Action::FilterBackspace),
            KeyCode::Up => Some(Action::NavigateUp),
            KeyCode::Down => Some(Action::NavigateDown),
            KeyCode::Char(c) => Some(Action::FilterInput(c)),
            _ => None,
        }
    } else {
        map_key(key)
    }
}

fn map_key(key: KeyEvent) -> Option<Action> {
    if key.modifiers.contains(KeyModifiers::CONTROL) {
        return match key.code {
            KeyCode::Char('c') => Some(Action::Quit),
            KeyCode::Char('u') => Some(Action::ScrollUp),
            KeyCode::Char('d') => Some(Action::ScrollDown),
            _ => None,
        };
    }

    match key.code {
        KeyCode::Char('q') => Some(Action::Quit),
        KeyCode::Tab => Some(Action::FocusNext),
        KeyCode::BackTab => Some(Action::FocusPrevious),
        KeyCode::Up | KeyCode::Char('k') => Some(Action::NavigateUp),
        KeyCode::Down | KeyCode::Char('j') => Some(Action::NavigateDown),
        KeyCode::Right | KeyCode::Char('l') => Some(Action::Expand),
        KeyCode::Char('L') => Some(Action::ExpandAll),
        KeyCode::Left | KeyCode::Char('h') => Some(Action::Collapse),
        KeyCode::Char('H') => Some(Action::CollapseAll),
        KeyCode::Char('g') | KeyCode::Home => Some(Action::JumpToStart),
        KeyCode::Char('G') | KeyCode::End => Some(Action::JumpToEnd),
        KeyCode::Char('{') => Some(Action::JumpToPrevFile),
        KeyCode::Char('}') => Some(Action::JumpToNextFile),
        KeyCode::Enter => Some(Action::Select),
        KeyCode::Char('a') => Some(Action::RunAll),
        KeyCode::Char('r') => Some(Action::RerunFailed),
        KeyCode::Char('w') => Some(Action::ToggleWatch),
        KeyCode::Char('f') | KeyCode::Char('/') => Some(Action::FilterEnter),
        KeyCode::Char('e') => Some(Action::OpenInEditor),
        KeyCode::PageUp => Some(Action::ScrollUp),
        KeyCode::PageDown => Some(Action::ScrollDown),
        _ => None,
    }
}

/// Set a node and all its descendants to Running status.
fn set_running_status(app: &mut App, node_id: usize) {
    if let Some(node) = app.tree.get(node_id) {
        let children = node.children.clone();
        if let Some(node) = app.tree.get_mut(node_id) {
            node.status = TestStatus::Running;
        }
        for child_id in children {
            set_running_status(app, child_id);
        }
    }
}

/// Extract line and column from the first frame of a stack trace.
/// Matches patterns like `(file.ts:123:45)` or `file.ts:123:45`.
fn parse_line_col_from_stack(stack: &str) -> Option<(Option<u32>, Option<u32>)> {
    for segment in stack.split_whitespace() {
        let s = segment.trim_matches(|c| c == '(' || c == ')');
        let parts: Vec<&str> = s.rsplitn(3, ':').collect();
        if parts.len() >= 2 {
            let col = parts[0].parse::<u32>().ok();
            let line = parts[1].parse::<u32>().ok();
            if line.is_some() {
                return Some((line, col));
            }
        }
    }
    None
}

/// Resolve a file node's path to an absolute path.
fn resolve_file_path(app: &App, node_id: usize) -> PathBuf {
    if let Some(node) = app.tree.get(node_id) {
        if let Some(ref p) = node.path {
            if p.is_absolute() {
                return p.clone();
            }
            return app.workspace.join(p);
        }
        return app.workspace.join(&node.name);
    }
    app.workspace.clone()
}

/// Walk up from a test/suite node to find the file path and the node's own name.
fn resolve_test_path(app: &App, node_id: usize) -> (PathBuf, String) {
    let test_name = app
        .tree
        .get(node_id)
        .map(|n| n.name.clone())
        .unwrap_or_default();

    let mut current = Some(node_id);
    let mut file_id = None;
    while let Some(id) = current {
        if let Some(node) = app.tree.get(id) {
            if node.kind == NodeKind::File {
                file_id = Some(id);
                break;
            }
            current = node.parent;
        } else {
            break;
        }
    }

    let file_path = file_id
        .map(|id| resolve_file_path(app, id))
        .unwrap_or_else(|| app.workspace.clone());

    (file_path, test_name)
}
