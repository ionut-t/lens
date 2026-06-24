use std::path::PathBuf;

use arboard::Clipboard;
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
    JumpToPrevError,
    JumpToNextError,
    Select,
    RunAll,
    RunFiltered,
    RerunFailed,
    ToggleWatch,
    FilterEnter,
    FilterByFile,
    FilterByDir,
    FilterKey(KeyEvent),
    FilterExit,
    FilterApply,
    OpenInEditor,
    YankPath,
    YankFailureLocation,
    YankOutput,
    ToggleHelp,
}

/// Process a keyboard action.
pub fn handle_action(app: &mut App, action: Action) {
    match action {
        Action::Quit => app.should_quit = true,

        Action::FocusNext => {
            app.active_panel = match app.active_panel {
                Panel::TestTree => Panel::FailedList,
                Panel::FailedList => Panel::Output,
                Panel::Output => Panel::TestTree,
            };
        }

        Action::FocusPrevious => {
            app.active_panel = match app.active_panel {
                Panel::TestTree => Panel::Output,
                Panel::FailedList => Panel::TestTree,
                Panel::Output => Panel::FailedList,
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

            Panel::Output => {
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

            Panel::Output => {
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
                Panel::Output => {
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
                Panel::Output => {
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
            Panel::Output => {
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
            Panel::Output => {
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

        Action::JumpToPrevError => {
            if app.active_panel == Panel::TestTree {
                let nodes = app.visible_tree_nodes();
                for i in (0..app.selected_tree_index).rev() {
                    if let Some(&(node_id, _)) = nodes.get(i)
                        && let Some(node) = app.tree.get(node_id)
                        && node.status == TestStatus::Failed
                    {
                        app.selected_tree_index = i;
                        app.detail_scroll_offset = 0;
                        app.adjust_tree_scroll();
                        break;
                    }
                }
            }
        }

        Action::JumpToNextError => {
            if app.active_panel == Panel::TestTree {
                let nodes = app.visible_tree_nodes();
                for i in (app.selected_tree_index + 1)..nodes.len() {
                    if let Some(&(node_id, _)) = nodes.get(i)
                        && let Some(node) = app.tree.get(node_id)
                        && node.status == TestStatus::Failed
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

        Action::RunFiltered => {
            let visible = app.visible_tree_nodes();
            let file_ids: Vec<usize> = visible
                .into_iter()
                .filter(|&(id, _)| app.tree.get(id).is_some_and(|n| n.kind == NodeKind::File))
                .map(|(id, _)| id)
                .collect();
            if file_ids.is_empty() {
                return;
            }
            let paths: Vec<std::path::PathBuf> = file_ids
                .iter()
                .map(|&id| resolve_file_path(app, id))
                .collect();
            for &file_id in &file_ids {
                set_running_status(app, file_id);
            }
            app.pending_runs.push(PendingRun::Files(paths));
            app.running = true;
            app.progress_done = 0;
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

        Action::FilterByFile => {
            if let Some(file_id) = find_file_node_for_selection(app)
                && let Some(node) = app.tree.get(file_id)
            {
                app.filter = tui_input::Input::from(node.name.clone());
                app.filter_active = false;
                app.selected_tree_index = 0;
                app.tree_scroll_offset = 0;
            }
        }

        Action::FilterByDir => {
            if let Some(file_id) = find_file_node_for_selection(app)
                && let Some(node) = app.tree.get(file_id)
            {
                let dir = std::path::Path::new(&node.name)
                    .parent()
                    .filter(|p| !p.as_os_str().is_empty())
                    .map(|p| p.to_string_lossy().into_owned());
                if let Some(dir) = dir {
                    app.filter = tui_input::Input::from(dir);
                    app.filter_active = false;
                    app.selected_tree_index = 0;
                    app.tree_scroll_offset = 0;
                }
            }
        }

        Action::FilterKey(key) => {
            use tui_input::backend::crossterm::EventHandler;
            app.filter.handle_event(&crossterm::event::Event::Key(key));
            app.selected_tree_index = 0;
            app.tree_scroll_offset = 0;
        }

        Action::FilterExit => {
            app.filter.reset();
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

        Action::YankPath => {
            if let Some(node_id) = app.selected_node_id() {
                // Capture the selected node's kind and location before walking up.
                let (selected_kind, selected_location) = app
                    .tree
                    .get(node_id)
                    .map(|n| (n.kind, n.location))
                    .unwrap_or((NodeKind::File, None));

                let mut current = Some(node_id);
                while let Some(id) = current {
                    if let Some(node) = app.tree.get(id) {
                        if node.kind == NodeKind::File {
                            // For test/suite nodes append :line:col when the location is known.
                            let path_str =
                                build_yank_string(&node.name, selected_kind, selected_location);
                            match Clipboard::new() {
                                Ok(mut cb) => match cb.set_text(path_str) {
                                    Ok(_) => app.notifier.info("Path yanked", 1),
                                    Err(_) => app.notifier.error("Failed to copy to clipboard"),
                                },

                                Err(_) => app.notifier.error("Clipboard unavailable"),
                            }
                            break;
                        }
                        current = node.parent;
                    } else {
                        break;
                    }
                }
            }
        }

        Action::YankOutput => {
            if let Some(text) = build_detail_plain_text(app) {
                match Clipboard::new() {
                    Ok(mut cb) => match cb.set_text(text) {
                        Ok(_) => app.notifier.info("Output copied", 1),
                        Err(_) => app.notifier.error("Failed to copy to clipboard"),
                    },
                    Err(_) => app.notifier.error("Clipboard unavailable"),
                }
            }
        }

        Action::ToggleHelp => {
            app.show_help = !app.show_help;
        }

        Action::YankFailureLocation => {
            if let Some(node_id) = app.selected_node_id() {
                let node = app.tree.get(node_id);

                // Prefer the failure stack-trace location; fall back to the test definition.
                let (line, col) = node
                    .and_then(|n| n.result.as_ref())
                    .and_then(|r| r.failure.as_ref())
                    .and_then(|f| f.stack_trace.as_ref())
                    .and_then(|st| parse_line_col_from_stack(st))
                    .or_else(|| node.and_then(|n| n.location.map(|(l, c)| (Some(l), Some(c)))))
                    .unwrap_or((None, None));

                let mut current = Some(node_id);
                while let Some(id) = current {
                    if let Some(n) = app.tree.get(id) {
                        if n.kind == NodeKind::File {
                            let path_str = build_failure_yank_string(&n.name, line, col);
                            match Clipboard::new() {
                                Ok(mut cb) => match cb.set_text(path_str) {
                                    Ok(_) => app.notifier.info("Location yanked", 1),
                                    Err(_) => app.notifier.error("Failed to copy to clipboard"),
                                },
                                Err(_) => app.notifier.error("Clipboard unavailable"),
                            }
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

pub fn trigger_action(key: KeyEvent, filter_active: bool, show_help: bool) -> Option<Action> {
    // Any key while the help overlay is open just closes it.
    if show_help {
        return Some(Action::ToggleHelp);
    }

    if filter_active {
        match key.code {
            KeyCode::Esc => Some(Action::FilterExit),
            KeyCode::Enter => Some(Action::FilterApply),
            KeyCode::Up => Some(Action::NavigateUp),
            KeyCode::Down => Some(Action::NavigateDown),
            _ => Some(Action::FilterKey(key)),
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
        KeyCode::Char('[') => Some(Action::JumpToPrevError),
        KeyCode::Char(']') => Some(Action::JumpToNextError),
        KeyCode::Enter => Some(Action::Select),
        KeyCode::Char('a') => Some(Action::RunFiltered),
        KeyCode::Char('A') => Some(Action::RunAll),
        KeyCode::Char('r') => Some(Action::RerunFailed),
        KeyCode::Char('w') => Some(Action::ToggleWatch),
        KeyCode::Char('/') => Some(Action::FilterEnter),
        KeyCode::Char('f') => Some(Action::FilterByFile),
        KeyCode::Char('F') => Some(Action::FilterByDir),
        KeyCode::Char('e') => Some(Action::OpenInEditor),
        KeyCode::PageUp => Some(Action::ScrollUp),
        KeyCode::PageDown => Some(Action::ScrollDown),
        KeyCode::Char('y') => Some(Action::YankPath),
        KeyCode::Char('Y') => Some(Action::YankFailureLocation),
        KeyCode::Char('c') => Some(Action::YankOutput),
        KeyCode::Char('?') => Some(Action::ToggleHelp),
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

/// Build the string that gets written to the clipboard for `YankPath`.
///
/// File nodes yield just the filename.  Test/Suite nodes append `:line:col`
/// when a source location is available, so the result can be pasted directly
/// into an editor's "go to file" prompt.
fn build_yank_string(file_name: &str, kind: NodeKind, location: Option<(u32, u32)>) -> String {
    match (kind, location) {
        (NodeKind::Test | NodeKind::Suite, Some((line, col))) => {
            format!("{}:{}:{}", file_name, line, col)
        }
        _ => file_name.to_owned(),
    }
}

/// Build the string written to the clipboard for `YankFailureLocation`.
///
/// Appends `:line:col` when both are known, `:line` when only the line is known,
/// and falls back to the bare filename when no location is available.
fn build_failure_yank_string(file_name: &str, line: Option<u32>, col: Option<u32>) -> String {
    match (line, col) {
        (Some(l), Some(c)) => format!("{}:{}:{}", file_name, l, c),
        (Some(l), None) => format!("{}:{}", file_name, l),
        _ => file_name.to_owned(),
    }
}

/// Extract line and column from the first frame of a stack trace.
/// Matches patterns like `(file.ts:123:45)` or `file.ts:123:45` or `file.ts:123`.
fn parse_line_col_from_stack(stack: &str) -> Option<(Option<u32>, Option<u32>)> {
    for segment in stack.split_whitespace() {
        let s = segment.trim_matches(|c| c == '(' || c == ')');
        let parts: Vec<&str> = s.rsplitn(3, ':').collect();

        if parts.len() == 3 {
            let col = parts[0].parse::<u32>().ok();
            let line = parts[1].parse::<u32>().ok();
            if line.is_some() {
                return Some((line, col));
            }
        } else if parts.len() == 2 {
            let line = parts[0].parse::<u32>().ok();
            if line.is_some() {
                return Some((line, None));
            }
        }
    }
    None
}

/// Build a plain-text copy of the detail panel content for the selected node.
fn build_detail_plain_text(app: &App) -> Option<String> {
    let node_id = app.selected_node_id()?;
    let node = app.tree.get(node_id)?;

    let mut out = String::new();

    // Breadcrumbs
    let mut crumbs: Vec<String> = Vec::new();
    let mut cur = Some(node_id);
    while let Some(id) = cur {
        if let Some(n) = app.tree.get(id) {
            crumbs.push(n.name.clone());
            cur = n.parent;
        } else {
            break;
        }
    }
    crumbs.reverse();
    out.push_str(&crumbs.join(" > "));

    if node.kind == NodeKind::Test {
        if let Some(ref result) = node.result
            && let Some(ref failure) = result.failure
        {
            out.push_str("\n\n");
            out.push_str(&failure.message);
            if let (Some(e), Some(a)) = (&failure.expected, &failure.actual) {
                out.push_str(&format!("\n\nExpected: {}\nActual:   {}", e, a));
            }
            if let Some(ref diff) = failure.diff {
                out.push('\n');
                out.push_str(diff);
            }
            if let Some(ref stack) = failure.stack_trace {
                out.push('\n');
                out.push_str(stack);
            }
        }
    } else {
        for fid in detail_failed_descendants(&app.tree, node_id) {
            if let Some(fnode) = app.tree.get(fid)
                && let Some(ref result) = fnode.result
                && let Some(ref failure) = result.failure
            {
                out.push_str(&format!("\n\n✗ {}\n", fnode.name));
                out.push_str(&failure.message);
                if let (Some(e), Some(a)) = (&failure.expected, &failure.actual) {
                    out.push_str(&format!("\nExpected: {}\nActual:   {}", e, a));
                }
                if let Some(ref stack) = failure.stack_trace {
                    out.push('\n');
                    out.push_str(stack);
                }
            }
        }
    }

    // Console output — walk up to the file node
    let mut cur = Some(node_id);
    while let Some(id) = cur {
        if let Some(n) = app.tree.get(id) {
            if n.kind == NodeKind::File && !n.console_output.is_empty() {
                out.push_str("\n\n━━ Console Output ━━\n");
                out.push_str(&n.console_output.join("\n"));
                break;
            }
            cur = n.parent;
        } else {
            break;
        }
    }

    if out.trim().is_empty() {
        None
    } else {
        Some(out)
    }
}

fn detail_failed_descendants(tree: &crate::models::TestTree, node_id: usize) -> Vec<usize> {
    let mut result = Vec::new();
    if let Some(node) = tree.get(node_id) {
        for &child in &node.children {
            if let Some(child_node) = tree.get(child) {
                if child_node.kind == NodeKind::Test && child_node.status == TestStatus::Failed {
                    result.push(child);
                }
                result.extend(detail_failed_descendants(tree, child));
            }
        }
    }
    result
}

/// Walk up from the currently selected node to its ancestor file node.
fn find_file_node_for_selection(app: &App) -> Option<usize> {
    let node_id = app.selected_node_id()?;
    let mut current = Some(node_id);
    while let Some(id) = current {
        let node = app.tree.get(id)?;
        if node.kind == NodeKind::File {
            return Some(id);
        }
        current = node.parent;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_line_col_standard_frame() {
        // Standard Node.js stack frame: file:line:col
        let stack = "AssertionError: expected 1 to equal 2\n    at /project/src/foo.test.ts:42:5";
        assert_eq!(parse_line_col_from_stack(stack), Some((Some(42), Some(5))));
    }

    #[test]
    fn test_parse_line_col_parenthesised_frame() {
        // Parenthesised form: at functionName (file:line:col)
        let stack = "Error: fail\n    at Object.<anonymous> (/project/src/foo.test.ts:10:20)";
        assert_eq!(parse_line_col_from_stack(stack), Some((Some(10), Some(20))));
    }

    #[test]
    fn test_parse_line_col_no_column() {
        // Only line, no column
        let stack = "Error: fail\n    at /project/src/foo.test.ts:99";
        assert_eq!(parse_line_col_from_stack(stack), Some((Some(99), None)));
    }

    #[test]
    fn test_parse_line_col_picks_first_frame() {
        // First frame is the test file; subsequent frames are node_modules
        let stack = "AssertionError: fail\n    at /project/src/foo.test.ts:69:24\n    at file:///node_modules/@vitest/runner/dist/index.js:145:11";
        assert_eq!(parse_line_col_from_stack(stack), Some((Some(69), Some(24))));
    }

    #[test]
    fn test_parse_line_col_error_message_does_not_match() {
        // Error message tokens like "id:" or "AssertionError:" must not produce a match
        let stack = "AssertionError: expected { id: '1', title: undefined } to deeply equal { id: '1', title: 'Test' }\n    at /project/src/foo.test.ts:30:5";
        assert_eq!(parse_line_col_from_stack(stack), Some((Some(30), Some(5))));
    }

    #[test]
    fn test_parse_line_col_file_url_frame() {
        // file:// URL form used by vitest for node_modules frames
        let stack = "Error: fail\n    at /project/src/foo.test.ts:5:3\n    at file:///node_modules/vitest/dist/runner.js:1653:37";
        assert_eq!(parse_line_col_from_stack(stack), Some((Some(5), Some(3))));
    }

    #[test]
    fn test_parse_line_col_empty_stack() {
        assert_eq!(parse_line_col_from_stack(""), None);
    }

    #[test]
    fn test_parse_line_col_no_frames() {
        // Error message only, no stack frames
        assert_eq!(
            parse_line_col_from_stack("Error: something went wrong"),
            None
        );
    }

    // --- build_yank_string ---

    #[test]
    fn test_yank_string_file_node_is_plain_path() {
        // Selecting a file node always yields just the path, no suffix.
        let s = build_yank_string("src/foo.test.ts", NodeKind::File, Some((10, 5)));
        assert_eq!(s, "src/foo.test.ts");
    }

    #[test]
    fn test_yank_string_test_with_location_appends_line_col() {
        let s = build_yank_string("src/foo.test.ts", NodeKind::Test, Some((42, 7)));
        assert_eq!(s, "src/foo.test.ts:42:7");
    }

    #[test]
    fn test_yank_string_suite_with_location_appends_line_col() {
        let s = build_yank_string("src/bar.test.ts", NodeKind::Suite, Some((1, 1)));
        assert_eq!(s, "src/bar.test.ts:1:1");
    }

    #[test]
    fn test_yank_string_test_without_location_is_plain_path() {
        // No location recorded yet (e.g. test discovered but not yet run).
        let s = build_yank_string("src/foo.test.ts", NodeKind::Test, None);
        assert_eq!(s, "src/foo.test.ts");
    }

    #[test]
    fn test_yank_string_suite_without_location_is_plain_path() {
        let s = build_yank_string("src/bar.test.ts", NodeKind::Suite, None);
        assert_eq!(s, "src/bar.test.ts");
    }

    #[test]
    fn test_yank_string_workspace_node_is_plain_path() {
        // Workspace/Project nodes that have no location should never get a suffix.
        let s = build_yank_string("my-project", NodeKind::Workspace, Some((1, 1)));
        assert_eq!(s, "my-project");
    }

    // --- build_failure_yank_string ---

    #[test]
    fn test_failure_yank_both_line_and_col() {
        assert_eq!(
            build_failure_yank_string("src/foo.test.ts", Some(42), Some(7)),
            "src/foo.test.ts:42:7"
        );
    }

    #[test]
    fn test_failure_yank_line_only() {
        // Some stack traces only give a line number.
        assert_eq!(
            build_failure_yank_string("src/foo.test.ts", Some(99), None),
            "src/foo.test.ts:99"
        );
    }

    #[test]
    fn test_failure_yank_no_location_falls_back_to_plain_path() {
        // No failure and no definition location → plain path.
        assert_eq!(
            build_failure_yank_string("src/foo.test.ts", None, None),
            "src/foo.test.ts"
        );
    }

    #[test]
    fn test_failure_yank_col_without_line_falls_back_to_plain_path() {
        // A col without a line is nonsensical; treat as no location.
        assert_eq!(
            build_failure_yank_string("src/foo.test.ts", None, Some(5)),
            "src/foo.test.ts"
        );
    }
}
