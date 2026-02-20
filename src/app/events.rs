use std::path::PathBuf;

use crate::{
    app::App,
    models::{NodeKind, RunSummary, TestResult, TestStatus},
};

/// Events streamed from test runner adapters into the app.
#[derive(Debug)]
pub enum TestEvent {
    RunStarted,
    TestsCollected {
        count: usize,
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
        result: TestResult,
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

/// Process a test event from a runner.
pub fn handle_test_event(app: &mut App, event: TestEvent) {
    match event {
        TestEvent::RunStarted => {
            if app.full_run {
                app.tree.reset();
                app.output_lines.clear();
            }
            app.progress_total = 0;
            app.progress_done = 0;
            app.running = true;
        }

        TestEvent::TestsCollected { count } => {
            app.progress_total += count;
        }

        TestEvent::FileStarted { path } => {
            let file_name = file_display_name(app, &path);
            find_or_create_file_node(app, &file_name, &path);
        }

        TestEvent::TestStarted { file, name } => {
            let file_name = file_display_name(app, &file);
            let file_id = find_or_create_file_node(app, &file_name, &file);
            let test_id = find_or_create_test_node(app, file_id, &name);
            if let Some(node) = app.tree.get_mut(test_id) {
                node.status = TestStatus::Running;
            }
        }

        TestEvent::TestFinished {
            file,
            name,
            result,
            location,
        } => {
            app.progress_done += 1;
            let file_name = file_display_name(app, &file);
            let file_id = find_or_create_file_node(app, &file_name, &file);
            let test_id = find_or_create_test_node(app, file_id, &name);
            // Don't overwrite a real result with "skipped" (happens with -t filtering)
            let dominated = result.status == TestStatus::Skipped
                && app
                    .tree
                    .get(test_id)
                    .is_some_and(|n| n.status.is_terminal());
            if !dominated {
                app.tree.update_result(test_id, result);
            }
            if let Some(loc) = location
                && let Some(node) = app.tree.get_mut(test_id)
            {
                node.location = Some(loc);
            }
        }

        TestEvent::SuiteLocation {
            file,
            name,
            location,
        } => {
            let file_name = file_display_name(app, &file);
            let file_id = find_or_create_file_node(app, &file_name, &file);
            let suite_id = find_or_create_test_node(app, file_id, &name);
            if let Some(node) = app.tree.get_mut(suite_id) {
                node.location = Some(location);
            }
        }

        TestEvent::FileFinished { path: _path } => {}

        TestEvent::RunFinished { mut summary } => {
            app.running = false;
            app.full_run = false;
            summary.duration = app
                .run_start
                .map(|start| start.elapsed().as_millis() as u64)
                .unwrap_or(summary.duration);

            app.summary = Some(summary);
        }

        TestEvent::ConsoleLog { file, content } => {
            let file_name = file_display_name(app, &file);
            let file_id = find_or_create_file_node(app, &file_name, &file);
            if let Some(node) = app.tree.get_mut(file_id) {
                node.console_output.push(content);
            }
        }

        TestEvent::Output { line } => {
            app.output_lines.push(line);
        }

        TestEvent::Error { message } => {
            app.output_lines.push(format!("[ERROR] {}", message));
        }

        TestEvent::WatchStopped => {
            app.watch_mode = false;
            app.watch_handle = None;
            app.running = false;
        }

        TestEvent::DiscoveryComplete { files } => {
            for display in &files {
                find_or_create_file_node(app, display, display);
            }
            app.discovering = false;
        }
    }
}

/// Find or create a file node at the root level.
fn find_or_create_file_node(app: &mut App, display_name: &str, path: &str) -> usize {
    if let Some(id) = app.tree.find_root_by_name(display_name) {
        id
    } else {
        app.tree.add_root(
            NodeKind::File,
            display_name.to_string(),
            Some(PathBuf::from(path)),
        )
    }
}

/// Find or create a test node under a file. Handles suite nesting via ` > ` separator.
fn find_or_create_test_node(app: &mut App, file_id: usize, full_name: &str) -> usize {
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

        if let Some(id) = app.tree.find_child_by_name(parent_id, part) {
            parent_id = id;
        } else {
            parent_id = app.tree.add_child(parent_id, kind, part.to_string(), None);
        }
    }

    parent_id
}

fn file_display_name(app: &App, path: &str) -> String {
    let workspace_str = app.workspace.to_string_lossy();
    let stripped = path
        .strip_prefix(workspace_str.as_ref())
        .unwrap_or(path)
        .trim_start_matches('/');
    stripped.to_string()
}
