use std::path::PathBuf;

use crate::{
    app::{App, PendingRun, StartupRun, WatchScope},
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
        result: Box<TestResult>,
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
    /// Test file discovery failed (e.g. glob error, Nx project not found).
    DiscoveryFailed {
        message: String,
    },
}

/// Process a test event from a runner.
pub fn handle_test_event(app: &mut App, event: TestEvent) {
    // Any of these means the current run is over — no more nodes are coming,
    // so an unresolved `--test` selection should be dropped rather than left
    // to fire on an unrelated later run.
    let run_over = matches!(
        event,
        TestEvent::RunFinished { .. } | TestEvent::Error { .. } | TestEvent::WatchStopped
    );

    match event {
        TestEvent::RunStarted => {
            if app.full_run {
                app.tree.reset();
                app.output_lines.clear();
            }
            app.progress_total = 0;
            app.progress_done = 0;
            app.running = true;
            // For manual runs run_start is set in main before the runner spawns,
            // preserving warmup time. For watch re-runs it won't be set, so we fall back here.
            app.run_start.get_or_insert_with(std::time::Instant::now);
        }

        TestEvent::TestsCollected { count } => {
            app.progress_total += count;
        }

        TestEvent::FileStarted { path } => {
            let file_name = file_display_name(app, &path);
            let file_id = find_or_create_file_node(app, &file_name, &path);
            if let Some(node) = app.tree.get_mut(file_id) {
                node.console_output.clear();
            }
            app.tree.mark_children_stale(file_id);
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
                app.tree.update_result(test_id, *result);
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

        TestEvent::FileFinished { path } => {
            let display = file_display_name(app, &path);
            let filename = basename(&display).to_string();
            if let Some(file_id) = app.tree.find_file_by_filename(&filename) {
                app.tree.purge_stale_children(file_id);
            }
        }

        TestEvent::RunFinished { mut summary } => {
            app.running = false;
            app.full_run = false;
            summary.duration = app
                .run_start
                .take()
                .map(|start| start.elapsed().as_millis() as u64)
                .unwrap_or(summary.duration);

            app.summary = Some(summary);
            // Tree may have gained new nodes during the run; recompute watched set.
            app.watched_ids_stale = true;
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
            app.watch_scope = WatchScope::None;
            app.watched_ids_stale = true;
        }

        TestEvent::DiscoveryComplete { files } => {
            if !files.is_empty() {
                let prefix = common_directory_prefix(&files);

                // Find or create the workspace root node
                let workspace_id = if let Some(id) = app.tree.find_root_by_name(&prefix) {
                    id
                } else {
                    app.tree.add_root(NodeKind::Workspace, prefix.clone(), None)
                };

                for path in &files {
                    let filename = basename(path).to_string();
                    if app.tree.find_file_by_filename(&filename).is_some() {
                        continue;
                    }
                    // relative path from workspace prefix (e.g. "todos/todos.service.spec.ts")
                    let relative = if prefix.is_empty() {
                        path.as_str()
                    } else {
                        path.strip_prefix(&format!("{prefix}/")).unwrap_or(path)
                    };
                    let parts: Vec<&str> = relative.split('/').collect();
                    let dir_parts = &parts[..parts.len().saturating_sub(1)];

                    let mut parent_id = workspace_id;
                    for &dir in dir_parts {
                        if let Some(id) = app.tree.find_child_by_name(parent_id, dir) {
                            parent_id = id;
                        } else {
                            parent_id = app.tree.add_child(
                                parent_id,
                                NodeKind::Project,
                                dir.to_string(),
                                None,
                            );
                        }
                    }
                    app.tree.add_child(
                        parent_id,
                        NodeKind::File,
                        filename,
                        Some(PathBuf::from(path)),
                    );
                }
            }
            app.discovering = false;

            if let Some(target) = app.startup_run.take() {
                queue_startup_run(app, target);
            }
        }

        TestEvent::DiscoveryFailed { message } => {
            app.discovering = false;
            app.notifier.error(message);

            // Still honour a CLI-requested run: the runner falls back to the
            // workspace root, and the tree node is created once it reports in.
            if let Some(target) = app.startup_run.take() {
                queue_startup_run(app, target);
            }
        }
    }

    if app.pending_select.is_some() {
        try_pending_select(app, run_over);
    }
}

/// Queue the CLI-requested run (`--file` / `--test`) and move the cursor to the
/// file's node in the tree. Runs even if the file wasn't discovered (the tree
/// node is created lazily when the runner reports it).
fn queue_startup_run(app: &mut App, target: StartupRun) {
    let file = if target.file.is_absolute() {
        target.file
    } else {
        app.workspace.join(&target.file)
    };

    let filename = file
        .file_name()
        .and_then(|f| f.to_str())
        .unwrap_or_default();
    if let Some(file_id) = app.tree.find_file_by_filename(filename) {
        if let Some(pos) = app
            .visible_tree_nodes()
            .iter()
            .position(|&(id, _)| id == file_id)
        {
            app.selected_tree_index = pos;
            app.adjust_tree_scroll();
        }
        if target.test.is_none() {
            crate::app::actions::set_running_status(app, file_id);
        }
    }

    let run = match target.test {
        Some(name) => {
            app.pending_select = Some((filename.to_string(), name.clone()));
            PendingRun::Test { file, name }
        }
        None => PendingRun::File(file),
    };
    app.pending_runs.push(run);
}

/// Move the cursor to the pending `--test` target if its node exists by now.
/// Cleared once resolved, or when the run ends (finished, errored, or watch
/// stopped) if the name never matched.
fn try_pending_select(app: &mut App, run_over: bool) {
    let Some((filename, target)) = &app.pending_select else {
        return;
    };

    let mut found = None;
    if let Some(file_id) = app.tree.find_file_by_filename(filename) {
        let mut stack: Vec<usize> = app
            .tree
            .get(file_id)
            .map(|n| n.children.clone())
            .unwrap_or_default();
        while let Some(id) = stack.pop() {
            let Some(node) = app.tree.get(id) else {
                continue;
            };
            if (node.kind == NodeKind::Test || node.kind == NodeKind::Suite) && node.name == *target
            {
                found = Some(id);
                break;
            }
            stack.extend(node.children.iter().copied());
        }
    }

    if let Some(id) = found {
        if let Some(pos) = app
            .visible_tree_nodes()
            .iter()
            .position(|&(nid, _)| nid == id)
        {
            app.selected_tree_index = pos;
            app.adjust_tree_scroll();
        }
        app.pending_select = None;
    } else if run_over {
        app.pending_select = None;
    }
}

/// Find or create a file node anywhere in the tree, using just the filename (basename).
/// Falls back to creating a root-level File node if not found (e.g. new file in watch mode).
fn find_or_create_file_node(app: &mut App, display_name: &str, path: &str) -> usize {
    let filename = basename(display_name);
    if let Some(id) = app.tree.find_file_by_filename(filename) {
        return id;
    }
    // Not found — create as a root fallback (watch mode new file)
    app.tree.add_root(
        NodeKind::File,
        filename.to_string(),
        Some(PathBuf::from(path)),
    )
}

fn basename(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

/// Compute the common parent directory shared by all file paths.
/// E.g. ["apps/todos/src/app/app.spec.ts", "apps/todos/src/app/todos/foo.ts"]
///      → "apps/todos/src/app"
fn common_directory_prefix(paths: &[String]) -> String {
    if paths.is_empty() {
        return String::new();
    }
    let dirs: Vec<Vec<&str>> = paths
        .iter()
        .map(|p| {
            let parts: Vec<&str> = p.split('/').collect();
            parts[..parts.len().saturating_sub(1)].to_vec()
        })
        .collect();

    let first = &dirs[0];
    let mut common_len = first.len();
    for dir in dirs.iter().skip(1) {
        let match_len = first
            .iter()
            .zip(dir.iter())
            .take_while(|(a, b)| a == b)
            .count();
        common_len = common_len.min(match_len);
    }
    first[..common_len].join("/")
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
        // Clear stale on every node in the path (suites included), so they aren't
        // purged by purge_stale_children when the file run finishes.
        if let Some(node) = app.tree.get_mut(parent_id) {
            node.stale = false;
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn app_with_startup(test: Option<&str>) -> App {
        let (mut app, _rx) = App::new(PathBuf::from("/ws"));
        app.startup_run = Some(StartupRun {
            file: PathBuf::from("/ws/src/math.test.ts"),
            test: test.map(str::to_owned),
        });
        handle_test_event(
            &mut app,
            TestEvent::DiscoveryComplete {
                files: vec!["src/math.test.ts".into(), "src/other.test.ts".into()],
            },
        );
        app
    }

    fn selected_name(app: &App) -> String {
        let (id, _) = app.visible_tree_nodes()[app.selected_tree_index];
        app.tree.get(id).unwrap().name.clone()
    }

    #[test]
    fn startup_file_run_selects_file_node() {
        let app = app_with_startup(None);
        assert_eq!(selected_name(&app), "math.test.ts");
        assert!(matches!(app.pending_runs[..], [PendingRun::File(_)]));
        assert!(app.pending_select.is_none());
    }

    #[test]
    fn startup_test_run_selects_test_node_once_it_appears() {
        let mut app = app_with_startup(Some("adds"));
        assert_eq!(selected_name(&app), "math.test.ts");
        assert!(app.pending_select.is_some());

        handle_test_event(
            &mut app,
            TestEvent::TestStarted {
                file: "/ws/src/math.test.ts".into(),
                name: "math > adds".into(),
            },
        );

        assert_eq!(selected_name(&app), "adds");
        assert!(app.pending_select.is_none());
    }

    #[test]
    fn startup_suite_run_selects_suite_node() {
        let mut app = app_with_startup(Some("math"));
        handle_test_event(
            &mut app,
            TestEvent::TestStarted {
                file: "/ws/src/math.test.ts".into(),
                name: "math > adds".into(),
            },
        );
        assert_eq!(selected_name(&app), "math");
    }

    #[test]
    fn startup_run_still_fires_when_discovery_fails() {
        let (mut app, _rx) = App::new(PathBuf::from("/ws"));
        app.startup_run = Some(StartupRun {
            file: PathBuf::from("/ws/src/math.test.ts"),
            test: None,
        });
        handle_test_event(
            &mut app,
            TestEvent::DiscoveryFailed {
                message: "Nx project 'foo' not found".into(),
            },
        );
        assert!(matches!(app.pending_runs[..], [PendingRun::File(_)]));
        assert!(app.startup_run.is_none());
    }

    #[test]
    fn pending_select_cleared_when_run_errors() {
        let mut app = app_with_startup(Some("adds"));
        assert!(app.pending_select.is_some());
        handle_test_event(
            &mut app,
            TestEvent::Error {
                message: "Runner error: failed to spawn vitest".into(),
            },
        );
        assert!(app.pending_select.is_none());
    }

    #[test]
    fn pending_select_cleared_when_watch_stops() {
        let mut app = app_with_startup(Some("adds"));
        assert!(app.pending_select.is_some());
        handle_test_event(&mut app, TestEvent::WatchStopped);
        assert!(app.pending_select.is_none());
    }

    #[test]
    fn pending_select_cleared_when_never_matched() {
        let mut app = app_with_startup(Some("no such test"));
        handle_test_event(
            &mut app,
            TestEvent::TestStarted {
                file: "/ws/src/math.test.ts".into(),
                name: "math > adds".into(),
            },
        );
        assert!(app.pending_select.is_some());
        handle_test_event(
            &mut app,
            TestEvent::RunFinished {
                summary: RunSummary {
                    total: 1,
                    passed: 1,
                    failed: 0,
                    skipped: 0,
                    duration: 10,
                },
            },
        );
        assert_eq!(selected_name(&app), "math.test.ts");
        assert!(app.pending_select.is_none());
    }
}
