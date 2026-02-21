use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use async_trait::async_trait;
use serde::Deserialize;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::mpsc;

use crate::app::TestEvent;
use crate::models::{FailureDetail, RunSummary, TestResult, TestStatus};

use super::{DiscoveredFile, TestRunner};

/// Guard that kills the child process (and its entire process group) on drop.
struct ChildGuard {
    child: Option<tokio::process::Child>,
    /// Process group ID saved at spawn time so we can kill the whole group.
    #[cfg(unix)]
    pgid: Option<u32>,
}

impl ChildGuard {
    fn new(child: tokio::process::Child) -> Self {
        #[cfg(unix)]
        let pgid = child.id();
        Self {
            child: Some(child),
            #[cfg(unix)]
            pgid,
        }
    }
}

impl Drop for ChildGuard {
    fn drop(&mut self) {
        // Kill the entire process group so vitest worker processes don't become orphans.
        #[cfg(unix)]
        if let Some(pgid) = self.pgid {
            unsafe { libc::kill(-(pgid as libc::pid_t), libc::SIGKILL) };
        }
        // Fallback / non-Unix: kill just the direct child.
        if let Some(ref mut child) = self.child {
            let _ = child.start_kill();
        }
    }
}

const REPORTER_SOURCE: &str = include_str!("../../reporters/vitest-reporter.mjs");

/// Open a debug log file if `LENS_DEBUG` env var is set.
type LogFile = std::sync::Arc<std::sync::Mutex<std::fs::File>>;

fn open_log_file() -> Option<LogFile> {
    std::env::var("LENS_DEBUG").ok().and_then(|path| {
        std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(path)
            .ok()
            .map(|f| std::sync::Arc::new(std::sync::Mutex::new(f)))
    })
}

/// Commands sent to the vitest watch process via stdin using the LENS_RUN protocol.
enum WatchRunCommand<'a> {
    All,
    File { file: &'a str },
    Test { file: &'a str, name: &'a str },
}

impl WatchRunCommand<'_> {
    fn to_stdin_line(&self) -> String {
        let json = match self {
            WatchRunCommand::All => r#"{"type":"run-all"}"#.to_string(),
            WatchRunCommand::File { file } => format!(r#"{{"type":"run-file","file":"{}"}}"#, file),
            WatchRunCommand::Test { file, name } => {
                format!(
                    r#"{{"type":"run-test","file":"{}","name":"{}"}}"#,
                    file, name
                )
            }
        };
        format!("LENS_RUN:{}\n", json)
    }
}

fn write_log(lf: &LogFile, msg: &str) {
    use std::io::Write;
    if let Ok(mut f) = lf.lock() {
        let _ = writeln!(f, "{}", msg);
    }
}

/// Vitest adapter that spawns vitest with a custom NDJSON reporter.
/// For Nx workspaces, finds vite/vitest configs and runs vitest directly
/// with `--config` to bypass nx's output buffering.
pub struct VitestRunner {
    workspace: PathBuf,
    /// Root directory to search for configs and test files.
    /// Defaults to workspace, but can be narrowed to a single project.
    search_root: PathBuf,
    log_file: Option<LogFile>,
    /// Channel to send commands to the stdin of the active watch process.
    watch_stdin: std::sync::Arc<std::sync::Mutex<Option<mpsc::UnboundedSender<String>>>>,
}

impl VitestRunner {
    pub fn new(workspace: PathBuf, project_root: Option<PathBuf>) -> Self {
        let search_root = project_root.unwrap_or_else(|| workspace.clone());
        Self {
            workspace,
            search_root,
            log_file: open_log_file(),
            watch_stdin: std::sync::Arc::new(std::sync::Mutex::new(None)),
        }
    }

    fn log(&self, msg: &str) {
        if let Some(ref lf) = self.log_file {
            write_log(lf, msg);
        }
    }

    /// Write the embedded reporter to a temp file and return its path.
    fn write_reporter(&self) -> Result<tempfile::NamedTempFile> {
        let mut file = tempfile::Builder::new()
            .prefix("lens-vitest-reporter-")
            .suffix(".mjs")
            .tempfile()
            .context("failed to create temp reporter file")?;

        use std::io::Write;
        file.write_all(REPORTER_SOURCE.as_bytes())
            .context("failed to write reporter to temp file")?;

        Ok(file)
    }

    /// Generate a temporary workspace config that lists all project directories
    /// in `test.projects`, enabling single-process vitest execution.
    fn write_workspace_config(
        &self,
        configs: &[PathBuf],
        reporter_path: &str,
    ) -> Result<tempfile::NamedTempFile> {
        let mut project_dirs: Vec<String> = Vec::new();
        for config in configs {
            if let Some(parent) = config.parent() {
                let abs = parent.to_string_lossy().to_string();
                if !project_dirs.contains(&abs) {
                    project_dirs.push(abs);
                }
            }
        }

        let projects_json = project_dirs
            .iter()
            .map(|p| format!("    '{}'", p.replace('\\', "/")))
            .collect::<Vec<_>>()
            .join(",\n");

        let content = format!(
            "export default {{\n  test: {{\n    reporters: ['{}'],\n    coverage: {{ enabled: false }},\n    projects: [\n{}\n    ]\n  }}\n}}\n",
            reporter_path.replace('\\', "/"),
            projects_json,
        );

        let mut file = tempfile::Builder::new()
            .prefix("lens-vitest-workspace-")
            .suffix(".mjs")
            .tempfile()
            .context("failed to create temp workspace config")?;

        use std::io::Write;
        file.write_all(content.as_bytes())
            .context("failed to write workspace config")?;

        Ok(file)
    }

    /// Find all vite/vitest config files in the workspace (these define test projects).
    fn find_vitest_configs(&self) -> Vec<PathBuf> {
        let mut configs = Vec::new();
        let names = [
            "vite.config.mjs",
            "vite.config.js",
            "vite.config.ts",
            "vite.config.mts",
            "vitest.config.mjs",
            "vitest.config.js",
            "vitest.config.ts",
            "vitest.config.mts",
        ];
        for name in &names {
            let pattern = self
                .search_root
                .join("**/")
                .join(name)
                .to_string_lossy()
                .to_string();
            if let Ok(entries) = glob::glob(&pattern) {
                for entry in entries.flatten() {
                    let path_str = entry.to_string_lossy();
                    if !path_str.contains("node_modules") && !configs.contains(&entry) {
                        configs.push(entry);
                    }
                }
            }
        }
        configs
    }

    /// Spawn vitest with the given args and stream NDJSON events from stdout.
    ///
    /// When `watch` is true, omits the `run` subcommand so vitest stays alive
    /// and re-runs on file changes. Non-zero exit is not treated as an error
    /// in watch mode (the process is killed on toggle-off).
    ///
    /// When `workspace_config` is provided, uses `-c <path>` and omits the
    /// `--reporter` CLI flag (the reporter is embedded in the workspace config).
    async fn spawn_and_stream(
        &self,
        args: &[&str],
        tx: mpsc::UnboundedSender<TestEvent>,
        watch: bool,
        workspace_config: Option<&Path>,
        cwd: Option<&Path>,
    ) -> Result<()> {
        let reporter_file = if workspace_config.is_none() {
            Some(self.write_reporter()?)
        } else {
            None
        };

        let mut cmd = Command::new("npx");
        cmd.arg("vitest");
        cmd.arg(if watch { "watch" } else { "run" });
        cmd.args(args)
            .arg("--disableConsoleIntercept")
            .arg("--includeTaskLocation");

        if let Some(ws_config) = workspace_config {
            cmd.arg("-c").arg(ws_config);
        } else if let Some(ref rf) = reporter_file {
            cmd.arg(format!("--reporter={}", rf.path().to_string_lossy()));
        }

        // Log the full command for debugging (LENS_DEBUG=path)
        let effective_cwd = cwd.unwrap_or(&self.workspace);
        self.log(&format!("[cmd] {:?}", cmd.as_std()));
        self.log(&format!("[cwd] {:?}", effective_cwd));

        // Put the child in its own process group so killing it (via ChildGuard) also
        // takes out any worker processes vitest forks (prevents orphans).
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            cmd.as_std_mut().process_group(0);
        }

        let mut child = cmd
            .current_dir(effective_cwd)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .context("failed to spawn vitest")?;

        let mut stdin = child.stdin.take().context("missing stdin")?;
        let stdout = child.stdout.take().context("missing stdout")?;
        let stderr = child.stderr.take().context("missing stderr")?;

        // Enable control over the watch process via the custom reporter.
        if watch {
            let (tx, mut rx) = mpsc::unbounded_channel::<String>();
            *self.watch_stdin.lock().unwrap() = Some(tx);

            let log_file = self.log_file.clone();
            let watch_stdin_clear = std::sync::Arc::clone(&self.watch_stdin);
            tokio::spawn(async move {
                use tokio::io::AsyncWriteExt;
                while let Some(line) = rx.recv().await {
                    if let Some(ref lf) = log_file {
                        write_log(lf, &format!("[stdin] sending: {}", line.trim()));
                    }
                    if stdin.write_all(line.as_bytes()).await.is_err() {
                        break;
                    }
                    let _ = stdin.flush().await;
                }
                *watch_stdin_clear.lock().unwrap() = None;
            });
        }

        // Wrap child in a guard that kills the process group on drop.
        // The child stays in the guard at all times so it is always killed if this
        // future is dropped (e.g. task aborted, app closed mid-run).
        let mut child_guard = ChildGuard::new(child);

        // Read stderr in background for error reporting
        let tx_err = tx.clone();
        let log_err = self.log_file.clone();
        let stderr_handle = tokio::spawn(async move {
            let reader = BufReader::new(stderr);
            let mut lines = reader.lines();
            while let Ok(Some(line)) = lines.next_line().await {
                if let Some(ref lf) = log_err {
                    write_log(lf, &format!("[stderr] {}", line));
                }
                let _ = tx_err.send(TestEvent::Output { line });
            }
        });

        // Parse NDJSON from stdout
        let reader = BufReader::new(stdout);
        let mut lines = reader.lines();

        while let Ok(Some(line)) = lines.next_line().await {
            let line = line.trim().to_string();
            if line.is_empty() {
                continue;
            }

            self.log(&format!("[stdout] {}", line));

            match serde_json::from_str::<VitestEvent>(&line) {
                Ok(event) => {
                    if let Some(test_event) = event.into_test_event() {
                        let _ = tx.send(test_event);
                    }
                }
                Err(_) => {
                    // Non-JSON output from vitest (e.g. banner), forward as raw output
                    let _ = tx.send(TestEvent::Output { line });
                }
            }
        }

        stderr_handle.await.ok();

        // Keep the temp file alive until vitest exits
        drop(reporter_file);

        if watch {
            self.stop_watch();
        }

        if !watch && let Some(ref mut child) = child_guard.child {
            let status = child.wait().await.context("failed to wait for vitest")?;
            if !status.success() {
                let _ = tx.send(TestEvent::Error {
                    message: format!("vitest exited with code {}", status.code().unwrap_or(-1)),
                });
            }
        }

        Ok(())
    }

    /// Try to route `cmd` through the active watch process stdin.
    /// Returns true if the command was sent, false if no watch process is running.
    fn try_run_via_watch(&self, cmd: WatchRunCommand<'_>) -> bool {
        let mut guard = self.watch_stdin.lock().unwrap();
        if let Some(tx) = guard.as_ref() {
            if tx.send(cmd.to_stdin_line()).is_ok() {
                return true;
            }
            // Sender broken — clear the stale entry
            *guard = None;
        }
        false
    }

    fn find_config_for_file(&self, file: &Path) -> Option<PathBuf> {
        let configs = self.find_vitest_configs();
        for config in configs {
            if file.starts_with(config.parent()?) {
                return Some(config);
            }
        }
        None
    }
}

#[async_trait]
impl TestRunner for VitestRunner {
    async fn discover(&self, workspace: &Path) -> Result<Vec<DiscoveredFile>> {
        let suffixes = [
            "*.test.ts",
            "*.test.tsx",
            "*.test.js",
            "*.test.jsx",
            "*.spec.ts",
            "*.spec.tsx",
            "*.spec.js",
            "*.spec.jsx",
        ];

        let mut files = Vec::new();
        for suffix in &suffixes {
            let pattern = workspace
                .join("**/")
                .join(suffix)
                .to_string_lossy()
                .to_string();
            for entry in glob::glob(&pattern)?.flatten() {
                if !entry.to_string_lossy().contains("node_modules")
                    && !files.iter().any(|f: &DiscoveredFile| f.path == entry)
                {
                    files.push(DiscoveredFile { path: entry });
                }
            }
        }

        Ok(files)
    }

    async fn run_all(&self, tx: mpsc::UnboundedSender<TestEvent>) -> Result<()> {
        if self.try_run_via_watch(WatchRunCommand::All) {
            return Ok(());
        }

        let configs = self.find_vitest_configs();
        if configs.is_empty() {
            // No configs found, run vitest from workspace root (non-Nx)
            self.spawn_and_stream(&[], tx, false, None, None).await
        } else {
            // Generate a workspace config and run all projects in a single process
            let reporter_file = self.write_reporter()?;
            let reporter_path = reporter_file.path().to_string_lossy().to_string();
            let workspace_config = self.write_workspace_config(&configs, &reporter_path)?;
            let ws_path = workspace_config.path().to_path_buf();
            let result = self
                .spawn_and_stream(&[], tx, false, Some(&ws_path), None)
                .await;
            // Keep temp files alive until vitest exits
            drop(workspace_config);
            drop(reporter_file);
            result
        }
    }

    async fn run_file(&self, file: &Path, tx: mpsc::UnboundedSender<TestEvent>) -> Result<()> {
        let file_abs = file.to_string_lossy().to_string();
        if self.try_run_via_watch(WatchRunCommand::File { file: &file_abs }) {
            return Ok(());
        }

        if let Some(config) = self.find_config_for_file(file) {
            let reporter_file = self.write_reporter()?;
            let reporter_path = reporter_file.path().to_string_lossy().to_string();
            let workspace_config = self.write_workspace_config(&[config], &reporter_path)?;
            let ws_path = workspace_config.path().to_path_buf();
            let result = self
                .spawn_and_stream(&[&file_abs], tx, false, Some(&ws_path), None)
                .await;
            drop(workspace_config);
            drop(reporter_file);
            result
        } else {
            self.spawn_and_stream(&[&file_abs], tx, false, None, None)
                .await
        }
    }

    async fn run_test(
        &self,
        file: &Path,
        test_name: &str,
        tx: mpsc::UnboundedSender<TestEvent>,
    ) -> Result<()> {
        let file_abs = file.to_string_lossy().to_string();
        if self.try_run_via_watch(WatchRunCommand::Test {
            file: &file_abs,
            name: test_name,
        }) {
            return Ok(());
        }

        if let Some(config) = self.find_config_for_file(file) {
            let reporter_file = self.write_reporter()?;
            let reporter_path = reporter_file.path().to_string_lossy().to_string();
            let workspace_config = self.write_workspace_config(&[config], &reporter_path)?;
            let ws_path = workspace_config.path().to_path_buf();
            let result = self
                .spawn_and_stream(
                    &[&file_abs, "-t", test_name],
                    tx,
                    false,
                    Some(&ws_path),
                    None,
                )
                .await;
            drop(workspace_config);
            drop(reporter_file);
            result
        } else {
            self.spawn_and_stream(&[&file_abs, "-t", test_name], tx, false, None, None)
                .await
        }
    }

    async fn run_all_watch(&self, tx: mpsc::UnboundedSender<TestEvent>) -> Result<()> {
        let configs = self.find_vitest_configs();
        if configs.is_empty() {
            self.spawn_and_stream(&[], tx, true, None, None).await
        } else {
            // Generate a workspace config and watch all projects in a single process
            let reporter_file = self.write_reporter()?;
            let reporter_path = reporter_file.path().to_string_lossy().to_string();
            let workspace_config = self.write_workspace_config(&configs, &reporter_path)?;
            let ws_path = workspace_config.path().to_path_buf();
            let result = self
                .spawn_and_stream(&[], tx, true, Some(&ws_path), None)
                .await;
            // Keep temp files alive until vitest exits
            drop(workspace_config);
            drop(reporter_file);
            result
        }
    }

    fn stop_watch(&self) {
        *self.watch_stdin.lock().unwrap() = None;
    }

    fn name(&self) -> &str {
        "Vitest"
    }
}

// --- NDJSON deserialization types ---

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
enum VitestEvent {
    RunStarted {
        total: usize,
    },
    TestsCollected {
        file: String,
        count: usize,
    },
    FileStarted {
        file: String,
    },
    TestStarted {
        file: String,
        name: String,
    },
    TestFinished {
        file: String,
        name: String,
        state: String,
        duration: Option<f64>,
        error: Option<VitestError>,
        location: Option<VitestLocation>,
    },
    SuiteLocation {
        file: String,
        name: String,
        location: VitestLocation,
    },
    ConsoleLog {
        file: String,
        content: String,
    },
    FileFinished {
        file: String,
    },
    RunFinished {
        total: usize,
        passed: usize,
        failed: usize,
        skipped: usize,
        duration: u64,
    },
}

#[derive(Debug, Deserialize)]
struct VitestLocation {
    line: u32,
    column: u32,
}

#[derive(Debug, Deserialize)]
struct VitestError {
    message: Option<String>,
    expected: Option<String>,
    actual: Option<String>,
    diff: Option<String>,
    stack: Option<String>,
}

impl VitestEvent {
    fn into_test_event(self) -> Option<TestEvent> {
        match self {
            VitestEvent::RunStarted { .. } => Some(TestEvent::RunStarted),
            VitestEvent::TestsCollected { count, .. } => Some(TestEvent::TestsCollected { count }),
            VitestEvent::FileStarted { file } => Some(TestEvent::FileStarted { path: file }),
            VitestEvent::TestStarted { file, name } => Some(TestEvent::TestStarted { file, name }),
            VitestEvent::TestFinished {
                file,
                name,
                state,
                duration,
                error,
                location,
            } => {
                let status = match state.as_str() {
                    "passed" => TestStatus::Passed,
                    "failed" => TestStatus::Failed,
                    "skipped" => TestStatus::Skipped,
                    _ => TestStatus::Pending,
                };

                let failure = if status == TestStatus::Failed {
                    error.map(|e| FailureDetail {
                        message: strip_ansi(&e.message.unwrap_or_default()),
                        expected: e.expected.map(|s| strip_ansi(&s)),
                        actual: e.actual.map(|s| strip_ansi(&s)),
                        diff: e.diff.map(|s| strip_ansi(&s)),
                        source_snippet: None,
                        stack_trace: e.stack.map(|s| strip_ansi(&s)),
                    })
                } else {
                    None
                };

                Some(TestEvent::TestFinished {
                    file,
                    name,
                    result: TestResult {
                        status,
                        duration_ms: duration.map(|d| d as u64),
                        failure,
                    },
                    location: location.map(|l| (l.line, l.column)),
                })
            }
            VitestEvent::SuiteLocation {
                file,
                name,
                location,
            } => Some(TestEvent::SuiteLocation {
                file,
                name,
                location: (location.line, location.column),
            }),
            VitestEvent::ConsoleLog { file, content } => {
                Some(TestEvent::ConsoleLog { file, content })
            }
            VitestEvent::FileFinished { file } => Some(TestEvent::FileFinished { path: file }),
            VitestEvent::RunFinished {
                total,
                passed,
                failed,
                skipped,
                duration,
            } => Some(TestEvent::RunFinished {
                summary: RunSummary {
                    total,
                    passed,
                    failed,
                    skipped,
                    duration,
                },
            }),
        }
    }
}

/// Strip ANSI escape sequences from a string.
fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            // Skip until we hit a letter (end of escape sequence)
            for c2 in chars.by_ref() {
                if c2.is_ascii_alphabetic() {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}
