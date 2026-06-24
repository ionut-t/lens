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

const SUFFIXES: [&str; 8] = [
    "*.test.ts",
    "*.test.tsx",
    "*.test.js",
    "*.test.jsx",
    "*.spec.ts",
    "*.spec.tsx",
    "*.spec.js",
    "*.spec.jsx",
];

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
    let path = std::env::var("LENS_DEBUG").ok()?;
    let f = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(&path)
        .map_err(|e| eprintln!("[lens] failed to open log file {path:?}: {e}"))
        .ok()?;
    let lf = std::sync::Arc::new(std::sync::Mutex::new(f));
    write_log(&lf, "[lens] debug log started");
    Some(lf)
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
    /// Compiled glob patterns for files to skip during discovery.
    ignore_patterns: Vec<glob::Pattern>,
}

impl VitestRunner {
    pub fn new(
        workspace: PathBuf,
        project_root: Option<PathBuf>,
        ignore_patterns: Vec<String>,
    ) -> Self {
        let search_root = project_root.unwrap_or_else(|| workspace.clone());
        let ignore_patterns = ignore_patterns
            .iter()
            .filter_map(|p| glob::Pattern::new(p).ok())
            .collect();
        Self {
            workspace,
            search_root,
            log_file: open_log_file(),
            ignore_patterns,
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

        // Drop the write end of stdin immediately — we never send commands to vitest.
        // Keeping it as piped (vs null) preserves the pipe-based stdin mode that
        // Node.js / the reporter expects (null stdin changes event-loop behaviour).
        drop(child.stdin.take());
        let stdout = child.stdout.take().context("missing stdout")?;
        let stderr = child.stderr.take().context("missing stderr")?;

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
        let mut files = Vec::new();
        for suffix in &SUFFIXES {
            let pattern = workspace
                .join("**/")
                .join(suffix)
                .to_string_lossy()
                .to_string();
            for entry in glob::glob(&pattern)?.flatten() {
                let rel = entry.strip_prefix(&self.workspace).unwrap_or(&entry);
                let rel_str = rel.to_string_lossy();
                if entry.to_string_lossy().contains("node_modules")
                    || files.iter().any(|f: &DiscoveredFile| f.path == entry)
                    || self.ignore_patterns.iter().any(|p| p.matches(&rel_str))
                {
                    continue;
                }
                files.push(DiscoveredFile { path: entry });
            }
        }

        Ok(files)
    }

    async fn run_all(&self, tx: mpsc::UnboundedSender<TestEvent>) -> Result<()> {
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

    async fn run_files(&self, files: &[PathBuf], tx: mpsc::UnboundedSender<TestEvent>) -> Result<()> {
        let file_args: Vec<String> = files.iter().map(|f| f.to_string_lossy().to_string()).collect();
        let file_arg_strs: Vec<&str> = file_args.iter().map(String::as_str).collect();
        let configs = self.find_vitest_configs();
        if configs.is_empty() {
            self.spawn_and_stream(&file_arg_strs, tx, false, None, None).await
        } else {
            let reporter_file = self.write_reporter()?;
            let reporter_path = reporter_file.path().to_string_lossy().to_string();
            let workspace_config = self.write_workspace_config(&configs, &reporter_path)?;
            let ws_path = workspace_config.path().to_path_buf();
            let result = self
                .spawn_and_stream(&file_arg_strs, tx, false, Some(&ws_path), None)
                .await;
            drop(workspace_config);
            drop(reporter_file);
            result
        }
    }

    async fn run_file(&self, file: &Path, tx: mpsc::UnboundedSender<TestEvent>) -> Result<()> {
        let file_abs = file.to_string_lossy().to_string();
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

    async fn run_file_watch(
        &self,
        file: &Path,
        tx: mpsc::UnboundedSender<TestEvent>,
    ) -> Result<()> {
        let file_abs = file.to_string_lossy().to_string();
        if let Some(config) = self.find_config_for_file(file) {
            let reporter_file = self.write_reporter()?;
            let reporter_path = reporter_file.path().to_string_lossy().to_string();
            let workspace_config = self.write_workspace_config(&[config], &reporter_path)?;
            let ws_path = workspace_config.path().to_path_buf();
            let result = self
                .spawn_and_stream(&[&file_abs], tx, true, Some(&ws_path), None)
                .await;
            drop(workspace_config);
            drop(reporter_file);
            result
        } else {
            self.spawn_and_stream(&[&file_abs], tx, true, None, None)
                .await
        }
    }

    async fn run_test_watch(
        &self,
        file: &Path,
        test_name: &str,
        tx: mpsc::UnboundedSender<TestEvent>,
    ) -> Result<()> {
        let file_abs = file.to_string_lossy().to_string();
        if let Some(config) = self.find_config_for_file(file) {
            let reporter_file = self.write_reporter()?;
            let reporter_path = reporter_file.path().to_string_lossy().to_string();
            let workspace_config = self.write_workspace_config(&[config], &reporter_path)?;
            let ws_path = workspace_config.path().to_path_buf();
            let result = self
                .spawn_and_stream(
                    &[&file_abs, "-t", test_name],
                    tx,
                    true,
                    Some(&ws_path),
                    None,
                )
                .await;
            drop(workspace_config);
            drop(reporter_file);
            result
        } else {
            self.spawn_and_stream(&[&file_abs, "-t", test_name], tx, true, None, None)
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
                    error.map(|e| {
                        let expected = e.expected.map(|s| strip_ansi(&s));
                        let actual = e.actual.map(|s| strip_ansi(&s));
                        let expected_parsed = expected.as_deref().and_then(parse_value_string);
                        let actual_parsed = actual.as_deref().and_then(parse_value_string);
                        FailureDetail {
                            message: strip_ansi(&e.message.unwrap_or_default()),
                            expected,
                            actual,
                            expected_parsed,
                            actual_parsed,
                            diff: e.diff.map(|s| strip_ansi(&s)),
                            source_snippet: None,
                            stack_trace: e.stack.map(|s| strip_ansi(&s)),
                        }
                    })
                } else {
                    None
                };

                Some(TestEvent::TestFinished {
                    file,
                    name,
                    result: Box::new(TestResult {
                        status,
                        duration_ms: duration.map(|d| d as u64),
                        failure,
                    }),
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

/// Parse Vitest's Node.js inspect format into a JSON object or array value.
///
/// Vitest serializes values using Node's `util.inspect`, which produces output like:
///   `Object { "key": value, "nested": Object { ... }, "arr": Array [ ... ], }`
///   `Array [ 1, 2, 3, ]`
///
/// This is not valid JSON, so we normalize it first:
///   1. Replace `Object {` → `{` and `Array [` → `[`
///   2. Remove trailing commas before `}` and `]`
///
/// Returns `Some(Value::Object(...))` or `Some(Value::Array(...))` on success.
fn parse_value_string(s: &str) -> Option<serde_json::Value> {
    let s = s.trim();
    // Fast path: already valid JSON object or array
    if let Ok(v @ (serde_json::Value::Object(_) | serde_json::Value::Array(_))) =
        serde_json::from_str(s)
    {
        return Some(v);
    }
    if !s.contains("Object {") && !s.contains("Array [") {
        return None;
    }
    let normalized = normalize_inspect_format(s);
    match serde_json::from_str(&normalized) {
        Ok(v @ (serde_json::Value::Object(_) | serde_json::Value::Array(_))) => Some(v),
        _ => None,
    }
}

/// Convert Node.js inspect format to valid JSON.
fn normalize_inspect_format(s: &str) -> String {
    // Replace JS-specific tokens that are not valid JSON.
    // `undefined` has no JSON equivalent — encode it as a sentinel string so it
    // round-trips through serde_json and can be rendered as `undefined` in the UI.
    let s = s
        // Vitest truncates deeply nested values to `[Object]` / `[Array]` when
        // the depth exceeds its serialisation limit.  Encode them as sentinel
        // strings so the renderer can display a clear truncation marker.
        .replace("[Object]", "\"__truncated_object__\"")
        .replace("[Array]", "\"__truncated_array__\"")
        .replace("Object {", "{")
        .replace("Array [", "[")
        // Object value: `"key": undefined`
        .replace(": undefined", ": \"__js_undefined__\"")
        .replace(":undefined", ":\"__js_undefined__\"")
        // Array element: `[undefined` or `, undefined`
        .replace("[undefined", "[\"__js_undefined__\"")
        .replace("[ undefined", "[ \"__js_undefined__\"")
        .replace(", undefined", ", \"__js_undefined__\"");
    // Vitest's inspect format does not JSON-escape string contents, so bare
    // backslashes inside values break serde_json parsing.
    let s = fix_string_escapes(&s);
    remove_trailing_commas(&s)
}

/// Scan through an almost-JSON string and escape bare backslashes inside quoted
/// string values. Only touches characters inside `"..."` — structure is untouched.
fn fix_string_escapes(s: &str) -> String {
    let mut result = String::with_capacity(s.len() + 16);
    let mut chars = s.chars().peekable();
    let mut in_string = false;

    while let Some(c) = chars.next() {
        if in_string {
            match c {
                '\\' => match chars.peek() {
                    // Valid JSON escape sequences — pass both characters through
                    Some(&'"' | &'\\' | &'/' | &'b' | &'f' | &'n' | &'r' | &'t' | &'u') => {
                        result.push('\\');
                        result.push(chars.next().unwrap());
                    }
                    // Bare backslash not starting a valid escape — double it
                    _ => {
                        result.push('\\');
                        result.push('\\');
                    }
                },
                '"' => {
                    in_string = false;
                    result.push('"');
                }
                other => result.push(other),
            }
        } else {
            if c == '"' {
                in_string = true;
            }
            result.push(c);
        }
    }
    result
}

/// Remove commas that appear immediately before a `}` or `]` (ignoring whitespace).
fn remove_trailing_commas(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    let mut result = String::with_capacity(s.len());
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == ',' {
            let mut j = i + 1;
            while j < chars.len() && chars[j].is_whitespace() {
                j += 1;
            }
            if j < chars.len() && (chars[j] == '}' || chars[j] == ']') {
                i += 1;
                continue;
            }
        }
        result.push(chars[i]);
        i += 1;
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // ── parse_value_string ──────────────────────────────────────────────────

    #[test]
    fn parse_value_string_plain_json_object() {
        let result = parse_value_string(r#"{"a": 1, "b": "hello"}"#).unwrap();
        assert_eq!(result, json!({"a": 1, "b": "hello"}));
    }

    #[test]
    fn parse_value_string_plain_json_array() {
        let result = parse_value_string(r#"[1, 2, 3]"#).unwrap();
        assert_eq!(result, json!([1, 2, 3]));
    }

    #[test]
    fn parse_value_string_inspect_object() {
        let result = parse_value_string(r#"Object { "x": 1, "y": "hello", }"#).unwrap();
        assert_eq!(result, json!({"x": 1, "y": "hello"}));
    }

    #[test]
    fn parse_value_string_inspect_array() {
        let result = parse_value_string(r#"Array [ 1, 2, 3, ]"#).unwrap();
        assert_eq!(result, json!([1, 2, 3]));
    }

    #[test]
    fn parse_value_string_inspect_array_of_strings() {
        let result = parse_value_string(r#"Array [ "a", "b", "c", ]"#).unwrap();
        assert_eq!(result, json!(["a", "b", "c"]));
    }

    #[test]
    fn parse_value_string_inspect_array_with_undefined() {
        let result = parse_value_string(r#"Array [ 1, undefined, 3, ]"#).unwrap();
        assert_eq!(result, json!([1, "__js_undefined__", 3]));
    }

    #[test]
    fn parse_value_string_inspect_array_of_objects() {
        let result =
            parse_value_string(r#"Array [ Object { "id": 1, }, Object { "id": 2, }, ]"#).unwrap();
        assert_eq!(result, json!([{"id": 1}, {"id": 2}]));
    }

    #[test]
    fn parse_value_string_object_containing_array() {
        let result = parse_value_string(r#"Object { "items": Array [ 1, 2, ], }"#).unwrap();
        assert_eq!(result, json!({"items": [1, 2]}));
    }

    #[test]
    fn parse_value_string_inspect_object_with_undefined() {
        let result = parse_value_string(r#"Object { "x": undefined, }"#).unwrap();
        assert_eq!(result, json!({"x": "__js_undefined__"}));
    }

    #[test]
    fn parse_value_string_returns_none_for_primitive() {
        assert!(parse_value_string("42").is_none());
        assert!(parse_value_string("\"hello\"").is_none());
        assert!(parse_value_string("true").is_none());
        assert!(parse_value_string("null").is_none());
    }

    #[test]
    fn parse_value_string_returns_none_for_garbage() {
        assert!(parse_value_string("not json at all").is_none());
        assert!(parse_value_string("").is_none());
    }

    #[test]
    fn parse_value_string_trims_whitespace() {
        let result = parse_value_string("  [1, 2]  ").unwrap();
        assert_eq!(result, json!([1, 2]));
    }

    // ── normalize_inspect_format ────────────────────────────────────────────

    #[test]
    fn normalize_replaces_object_token() {
        let out = normalize_inspect_format(r#"Object { "a": 1 }"#);
        assert_eq!(out, r#"{ "a": 1 }"#);
    }

    #[test]
    fn normalize_replaces_array_token() {
        let out = normalize_inspect_format("Array [ 1, 2 ]");
        assert_eq!(out, "[ 1, 2 ]");
    }

    #[test]
    fn normalize_replaces_both_tokens() {
        let out = normalize_inspect_format(r#"Object { "arr": Array [ 1, 2, ] }"#);
        assert_eq!(out, r#"{ "arr": [ 1, 2 ] }"#);
    }

    #[test]
    fn normalize_replaces_truncated_object_placeholder() {
        let out = normalize_inspect_format(r#"Object { "nested": [Object], }"#);
        assert_eq!(out, r#"{ "nested": "__truncated_object__" }"#);
    }

    #[test]
    fn normalize_replaces_truncated_array_placeholder() {
        let out = normalize_inspect_format(r#"Object { "items": [Array], }"#);
        assert_eq!(out, r#"{ "items": "__truncated_array__" }"#);
    }

    #[test]
    fn parse_value_string_truncated_object_placeholder() {
        // Vitest emits [Object] for deeply nested values beyond its depth limit.
        let input = r#"Array [
  Object {
    "a": 1,
    "deep": [Object],
  },
]"#;
        let result = parse_value_string(input).unwrap();
        assert_eq!(result, json!([{"a": 1, "deep": "__truncated_object__"}]));
    }

    #[test]
    fn parse_value_string_truncated_array_placeholder() {
        let input = r#"Object { "items": [Array], }"#;
        let result = parse_value_string(input).unwrap();
        assert_eq!(result, json!({"items": "__truncated_array__"}));
    }

    #[test]
    fn normalize_encodes_undefined() {
        let out = normalize_inspect_format(r#"Object { "x": undefined }"#);
        assert_eq!(out, r#"{ "x": "__js_undefined__" }"#);
    }

    // ── fix_string_escapes ──────────────────────────────────────────────────

    #[test]
    fn fix_escapes_passes_through_valid_json_escapes() {
        let input = r#"{ "a": "hello\nworld" }"#;
        let out = fix_string_escapes(input);
        assert_eq!(out, input);
    }

    #[test]
    fn fix_escapes_doubles_bare_backslash() {
        // \U and \p are not valid JSON escape sequences — both should be doubled.
        // (\f, \n, \r, \t, \b, \u, \\, \/, \" are valid and passed through.)
        let input = r#"{ "path": "C:\Windows\path" }"#;
        let out = fix_string_escapes(input);
        assert_eq!(out, r#"{ "path": "C:\\Windows\\path" }"#);
    }

    #[test]
    fn fix_escapes_leaves_structure_untouched() {
        let input = r#"{ "a": 1 }"#;
        let out = fix_string_escapes(input);
        assert_eq!(out, input);
    }

    #[test]
    fn fix_escapes_handles_escaped_quote_inside_string() {
        let input = r#"{ "a": "say \"hi\"" }"#;
        let out = fix_string_escapes(input);
        assert_eq!(out, input);
    }

    // ── remove_trailing_commas ──────────────────────────────────────────────

    #[test]
    fn remove_trailing_comma_before_brace() {
        assert_eq!(remove_trailing_commas(r#"{"a": 1,}"#), r#"{"a": 1}"#);
    }

    #[test]
    fn remove_trailing_comma_before_bracket() {
        assert_eq!(remove_trailing_commas("[1, 2,]"), "[1, 2]");
    }

    #[test]
    fn remove_trailing_comma_with_whitespace() {
        assert_eq!(remove_trailing_commas("[1, 2,  ]"), "[1, 2  ]");
    }

    #[test]
    fn remove_trailing_comma_keeps_inner_commas() {
        let input = r#"{"a": 1, "b": 2}"#;
        assert_eq!(remove_trailing_commas(input), input);
    }

    #[test]
    fn remove_trailing_comma_nested() {
        let input = r#"{"a": [1, 2,], "b": 3,}"#;
        assert_eq!(remove_trailing_commas(input), r#"{"a": [1, 2], "b": 3}"#);
    }
}
