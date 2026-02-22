use std::path::PathBuf;

use tokio::sync::mpsc;

use crate::models::{RunSummary, TestTree};

pub mod actions;
pub mod events;

pub use actions::{Action, handle_action, trigger_action};
pub use events::{TestEvent, handle_test_event};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Panel {
    TestTree,
    FailedList,
    Detail,
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
    pub filter: tui_input::Input,
    pub discovering: bool,
    pub spinner_tick: usize,
    pub summary: Option<RunSummary>,
    pub run_start: Option<std::time::Instant>,
    pub project_name: Option<String>,
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
            filter: tui_input::Input::default(),
            discovering: true,
            spinner_tick: 0,
            summary: None,
            run_start: None,
            project_name: None,
        };
        (app, event_rx)
    }

    /// Returns visible nodes respecting the current filter query.
    pub fn visible_tree_nodes(&self) -> Vec<(usize, usize)> {
        let filter_query = self.filter.value();

        if filter_query.is_empty() {
            self.tree.visible_nodes()
        } else {
            self.tree.visible_nodes_filtered(filter_query)
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
