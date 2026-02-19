use std::path::PathBuf;

use tokio::sync::mpsc;

use crate::models::{NodeKind, RunSummary, TestStatus, TestTree};

/// Events streamed from test runner adapters into the app.
#[derive(Debug)]
pub enum TestEvent {
    RunStarted {
        total: usize,
    },
    FileStarted {
        path: String,
    },
    TestStarted {
        file: String,
        name: String,
    },
    TestFinished {
        file: String,
        name: String,
        result: crate::models::TestResult,
        location: Option<(u32, u32)>,
    },
    FileFinished {
        path: String,
    },
    RunFinished {
        summary: RunSummary,
    },
    Output {
        line: String,
    },
    SuiteLocation {
        file: String,
        name: String,
        location: (u32, u32),
    },
    ConsoleLog {
        file: String,
        content: String,
    },
    Error {
        message: String,
    },
    /// Watch process exited (either normally or with error).
    WatchStopped,
    /// Test file discovery completed.
    DiscoveryComplete {
        files: Vec<String>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Panel {
    TestTree,
    FailedList,
    Detail,
}

#[derive(Debug)]
pub enum Action {
    Quit,
    FocusNext,
    FocusPrevious,
    NavigateUp,
    NavigateDown,
    Expand,
    Collapse,
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

#[derive(Debug)]
pub enum PendingRun {
    File(PathBuf),
    Test { file: PathBuf, name: String },
}

pub struct App {
    pub workspace: PathBuf,
    pub tree: TestTree,
    pub active_panel: Panel,
    pub selected_tree_index: usize,
    pub selected_failed_index: usize,
    pub tree_scroll_offset: usize,
    pub failed_scroll_offset: usize,
    pub tree_viewport_height: usize,
    pub failed_viewport_height: usize,
    pub detail_scroll_offset: u16,
    pub running: bool,
    pub full_run: bool,
    pub watch_mode: bool,
    pub watch_handle: Option<tokio::task::JoinHandle<()>>,
    pub progress_total: usize,
    pub progress_done: usize,
    pub event_tx: mpsc::UnboundedSender<TestEvent>,
    pub output_lines: Vec<String>,
    pub pending_runs: Vec<PendingRun>,
    /// (file_path, line, column)
    pub pending_editor: Option<(PathBuf, Option<u32>, Option<u32>)>,
    pub should_quit: bool,
    pub filter_active: bool,
    pub filter_query: String,
    pub discovering: bool,
    pub spinner_tick: usize,
    pub summary: Option<RunSummary>,
    pub run_start: Option<std::time::Instant>,
}

impl App {
    pub fn new(workspace: PathBuf) -> (Self, mpsc::UnboundedReceiver<TestEvent>) {
        let (event_tx, event_rx) = mpsc::unbounded_channel();
        let app = Self {
            workspace,
            tree: TestTree::new(),
            active_panel: Panel::TestTree,
            selected_tree_index: 0,
            selected_failed_index: 0,
            tree_scroll_offset: 0,
            failed_scroll_offset: 0,
            tree_viewport_height: 0,
            failed_viewport_height: 0,
            detail_scroll_offset: 0,
            running: false,
            full_run: false,
            watch_mode: false,
            watch_handle: None,
            progress_total: 0,
            progress_done: 0,
            event_tx,
            output_lines: Vec::new(),
            pending_runs: Vec::new(),
            pending_editor: None,
            should_quit: false,
            filter_active: false,
            filter_query: String::new(),
            discovering: true,
            spinner_tick: 0,
            summary: None,
            run_start: None,
        };
        (app, event_rx)
    }

    /// Process a keyboard action.
    pub fn handle_action(&mut self, action: Action) {
        match action {
            Action::Quit => self.should_quit = true,

            Action::FocusNext => {
                self.active_panel = match self.active_panel {
                    Panel::TestTree => Panel::FailedList,
                    Panel::FailedList => Panel::Detail,
                    Panel::Detail => Panel::TestTree,
                };
            }

            Action::FocusPrevious => {
                self.active_panel = match self.active_panel {
                    Panel::TestTree => Panel::Detail,
                    Panel::FailedList => Panel::TestTree,
                    Panel::Detail => Panel::FailedList,
                };
            }

            Action::NavigateUp => match self.active_panel {
                Panel::TestTree => {
                    self.selected_tree_index = self.selected_tree_index.saturating_sub(1);
                    self.detail_scroll_offset = 0;
                    self.adjust_tree_scroll();
                }

                Panel::FailedList => {
                    self.selected_failed_index = self.selected_failed_index.saturating_sub(1);
                    self.detail_scroll_offset = 0;
                    self.adjust_failed_scroll();
                }

                Panel::Detail => {
                    self.detail_scroll_offset = self.detail_scroll_offset.saturating_sub(1);
                }
            },

            Action::NavigateDown => match self.active_panel {
                Panel::TestTree => {
                    let max = self.visible_tree_nodes().len().saturating_sub(1);
                    self.selected_tree_index = (self.selected_tree_index + 1).min(max);
                    self.detail_scroll_offset = 0;
                    self.adjust_tree_scroll();
                }

                Panel::FailedList => {
                    let max = self.tree.failed_nodes().len().saturating_sub(1);
                    self.selected_failed_index = (self.selected_failed_index + 1).min(max);
                    self.detail_scroll_offset = 0;
                    self.adjust_failed_scroll();
                }

                Panel::Detail => {
                    self.detail_scroll_offset = self.detail_scroll_offset.saturating_add(1);
                }
            },

            Action::Expand => {
                if self.active_panel == Panel::TestTree
                    && let Some(&(node_id, _)) =
                        self.visible_tree_nodes().get(self.selected_tree_index)
                    && let Some(node) = self.tree.get(node_id)
                    && !node.children.is_empty()
                {
                    self.tree.toggle_expanded(node_id);
                }
            }

            Action::Select => {
                if self.active_panel == Panel::TestTree
                    && let Some(&(node_id, _)) =
                        self.visible_tree_nodes().get(self.selected_tree_index)
                    && let Some(node) = self.tree.get(node_id)
                {
                    match node.kind {
                        NodeKind::File => {
                            let abs_path = self.resolve_file_path(node_id);
                            self.set_running_status(node_id);
                            self.pending_runs.push(PendingRun::File(abs_path));
                        }
                        NodeKind::Test | NodeKind::Suite => {
                            let (file_path, test_name) = self.resolve_test_path(node_id);
                            self.set_running_status(node_id);
                            self.pending_runs.push(PendingRun::Test {
                                file: file_path,
                                name: test_name,
                            });
                        }
                        _ => {
                            if !node.children.is_empty() {
                                self.tree.toggle_expanded(node_id);
                            }
                        }
                    }
                }
            }
            Action::Collapse => {
                if self.active_panel == Panel::TestTree
                    && let Some(&(node_id, _)) =
                        self.visible_tree_nodes().get(self.selected_tree_index)
                    && let Some(node) = self.tree.get(node_id)
                {
                    if node.expanded && !node.children.is_empty() {
                        self.tree.toggle_expanded(node_id);
                    } else if let Some(parent_id) = node.parent {
                        // Collapse navigates to parent if already collapsed
                        self.tree.toggle_expanded(parent_id);
                        // Move selection to parent
                        if let Some(pos) = self
                            .visible_tree_nodes()
                            .iter()
                            .position(|&(id, _)| id == parent_id)
                        {
                            self.selected_tree_index = pos;
                        }
                    }
                }
            }
            Action::RunAll => {
                self.tree.reset();
                self.progress_done = 0;
                self.running = true;
                self.full_run = true;
            }

            Action::RerunFailed => {
                let failed_ids = self.tree.failed_nodes();
                if failed_ids.is_empty() {
                    return;
                }
                let mut seen_files = std::collections::HashSet::new();
                for &node_id in &failed_ids {
                    let (file_path, _) = self.resolve_test_path(node_id);
                    if seen_files.insert(file_path.clone()) {
                        self.pending_runs.push(PendingRun::File(file_path));
                    }
                }
                for &node_id in &failed_ids {
                    self.set_running_status(node_id);
                }
                self.running = true;
            }

            Action::ToggleWatch => {
                self.watch_mode = !self.watch_mode;
            }

            Action::FilterEnter => {
                self.filter_active = true;
            }

            Action::FilterInput(c) => {
                self.filter_query.push(c);
                self.selected_tree_index = 0;
                self.tree_scroll_offset = 0;
            }

            Action::FilterBackspace => {
                self.filter_query.pop();
            }

            Action::FilterExit => {
                self.filter_query.clear();
                self.filter_active = false;
            }

            Action::FilterApply => {
                self.filter_active = false;
            }

            Action::OpenInEditor => {
                if let Some(node_id) = self.selected_node_id() {
                    let node = self.tree.get(node_id);

                    // For failed tests, open at failure location; otherwise at definition
                    let (line, col) = node
                        .and_then(|n| n.result.as_ref())
                        .and_then(|r| r.failure.as_ref())
                        .and_then(|f| f.stack_trace.as_ref())
                        .and_then(|st| Self::parse_line_col_from_stack(st))
                        .or_else(|| node.and_then(|n| n.location.map(|(l, c)| (Some(l), Some(c)))))
                        .unwrap_or((None, None));

                    // Walk up to find the file node
                    let mut current = Some(node_id);
                    while let Some(id) = current {
                        if let Some(n) = self.tree.get(id) {
                            if n.kind == NodeKind::File {
                                let path = self.resolve_file_path(id);
                                self.pending_editor = Some((path, line, col));
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

    /// Process a test event from a runner.
    pub fn handle_test_event(&mut self, event: TestEvent) {
        match event {
            TestEvent::RunStarted { total } => {
                if self.full_run {
                    self.tree.reset();
                    self.output_lines.clear();
                }
                self.progress_total = total;
                self.progress_done = 0;
                self.running = true;
            }

            TestEvent::FileStarted { path } => {
                let file_name = self.file_display_name(&path);
                self.find_or_create_file_node(&file_name, &path);
            }

            TestEvent::TestStarted { file, name } => {
                let file_name = self.file_display_name(&file);
                let file_id = self.find_or_create_file_node(&file_name, &file);
                let test_id = self.find_or_create_test_node(file_id, &name);
                if let Some(node) = self.tree.get_mut(test_id) {
                    node.status = TestStatus::Running;
                }
            }

            TestEvent::TestFinished {
                file,
                name,
                result,
                location,
            } => {
                self.progress_done += 1;
                let file_name = self.file_display_name(&file);
                let file_id = self.find_or_create_file_node(&file_name, &file);
                let test_id = self.find_or_create_test_node(file_id, &name);
                // Don't overwrite a real result with "skipped" (happens with -t filtering)
                let dominated = result.status == TestStatus::Skipped
                    && self
                        .tree
                        .get(test_id)
                        .is_some_and(|n| n.status.is_terminal());
                if !dominated {
                    self.tree.update_result(test_id, result);
                }
                if let Some(loc) = location
                    && let Some(node) = self.tree.get_mut(test_id)
                {
                    node.location = Some(loc);
                }
            }

            TestEvent::SuiteLocation {
                file,
                name,
                location,
            } => {
                let file_name = self.file_display_name(&file);
                let file_id = self.find_or_create_file_node(&file_name, &file);
                let suite_id = self.find_or_create_test_node(file_id, &name);
                if let Some(node) = self.tree.get_mut(suite_id) {
                    node.location = Some(location);
                }
            }

            TestEvent::FileFinished { .. } => {}

            TestEvent::RunFinished { mut summary } => {
                self.running = false;
                self.full_run = false;
                summary.duration = self
                    .run_start
                    .map(|start| start.elapsed().as_millis() as u64)
                    .unwrap_or(summary.duration);

                self.summary = Some(summary);
            }

            TestEvent::ConsoleLog { file, content } => {
                let file_name = self.file_display_name(&file);
                let file_id = self.find_or_create_file_node(&file_name, &file);
                if let Some(node) = self.tree.get_mut(file_id) {
                    node.console_output.push(content);
                }
            }

            TestEvent::Output { line } => {
                self.output_lines.push(line);
            }

            TestEvent::Error { message } => {
                self.output_lines.push(format!("[ERROR] {}", message));
            }

            TestEvent::WatchStopped => {
                self.watch_mode = false;
                self.watch_handle = None;
                self.running = false;
            }

            TestEvent::DiscoveryComplete { files } => {
                for display in &files {
                    self.find_or_create_file_node(display, display);
                }
                self.discovering = false;
            }
        }
    }

    /// Find or create a file node at the root level.
    pub fn find_or_create_file_node(&mut self, display_name: &str, path: &str) -> usize {
        if let Some(id) = self.tree.find_root_by_name(display_name) {
            id
        } else {
            self.tree.add_root(
                NodeKind::File,
                display_name.to_string(),
                Some(PathBuf::from(path)),
            )
        }
    }

    /// Find or create a test node under a file. Handles suite nesting via ` > ` separator.
    fn find_or_create_test_node(&mut self, file_id: usize, full_name: &str) -> usize {
        // Vitest uses " > " to separate suite/test hierarchy in fullName
        let parts: Vec<&str> = full_name.split(" > ").collect();
        let mut parent_id = file_id;

        for (i, part) in parts.iter().enumerate() {
            let is_last = i == parts.len() - 1;
            let kind = if is_last {
                NodeKind::Test
            } else {
                NodeKind::Suite
            };

            if let Some(id) = self.tree.find_child_by_name(parent_id, part) {
                parent_id = id;
            } else {
                parent_id = self.tree.add_child(parent_id, kind, part.to_string(), None);
            }
        }

        parent_id
    }

    fn file_display_name(&self, path: &str) -> String {
        let workspace_str = self.workspace.to_string_lossy();
        let stripped = path
            .strip_prefix(workspace_str.as_ref())
            .unwrap_or(path)
            .trim_start_matches('/');
        stripped.to_string()
    }

    /// Set a node and all its descendants to Running status.
    fn set_running_status(&mut self, node_id: usize) {
        if let Some(node) = self.tree.get(node_id) {
            let children = node.children.clone();
            if let Some(node) = self.tree.get_mut(node_id) {
                node.status = TestStatus::Running;
            }
            for child_id in children {
                self.set_running_status(child_id);
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
    fn resolve_file_path(&self, node_id: usize) -> PathBuf {
        if let Some(node) = self.tree.get(node_id) {
            if let Some(ref p) = node.path {
                if p.is_absolute() {
                    return p.clone();
                }
                return self.workspace.join(p);
            }
            return self.workspace.join(&node.name);
        }
        self.workspace.clone()
    }

    /// Walk up from a test/suite node to find the file path and the node's own name.
    fn resolve_test_path(&self, node_id: usize) -> (PathBuf, String) {
        let test_name = self
            .tree
            .get(node_id)
            .map(|n| n.name.clone())
            .unwrap_or_default();

        let mut current = Some(node_id);
        let mut file_id = None;
        while let Some(id) = current {
            if let Some(node) = self.tree.get(id) {
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
            .map(|id| self.resolve_file_path(id))
            .unwrap_or_else(|| self.workspace.clone());

        (file_path, test_name)
    }

    /// Returns visible nodes respecting the current filter query.
    pub fn visible_tree_nodes(&self) -> Vec<(usize, usize)> {
        if self.filter_query.is_empty() {
            self.tree.visible_nodes()
        } else {
            self.tree.visible_nodes_filtered(&self.filter_query)
        }
    }

    /// Get the currently selected node id in the test tree (if any).
    pub fn selected_node_id(&self) -> Option<usize> {
        match self.active_panel {
            Panel::FailedList => self
                .tree
                .failed_nodes()
                .get(self.selected_failed_index)
                .copied(),
            _ => self
                .visible_tree_nodes()
                .get(self.selected_tree_index)
                .map(|&(id, _)| id),
        }
    }

    pub fn test_summary(&self) -> Option<&RunSummary> {
        self.summary.as_ref()
    }

    pub fn progress_percent(&self) -> f64 {
        if self.progress_total == 0 {
            0.0
        } else {
            self.progress_done as f64 / self.progress_total as f64
        }
    }

    fn adjust_tree_scroll(&mut self) {
        if self.tree_viewport_height == 0 {
            return;
        }
        if self.selected_tree_index < self.tree_scroll_offset {
            self.tree_scroll_offset = self.selected_tree_index;
        } else if self.selected_tree_index >= self.tree_scroll_offset + self.tree_viewport_height {
            self.tree_scroll_offset = self.selected_tree_index - self.tree_viewport_height + 1;
        }
    }

    fn adjust_failed_scroll(&mut self) {
        if self.failed_viewport_height == 0 {
            return;
        }
        if self.selected_failed_index < self.failed_scroll_offset {
            self.failed_scroll_offset = self.selected_failed_index;
        } else if self.selected_failed_index
            >= self.failed_scroll_offset + self.failed_viewport_height
        {
            self.failed_scroll_offset =
                self.selected_failed_index - self.failed_viewport_height + 1;
        }
    }
}
