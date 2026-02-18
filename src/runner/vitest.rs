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

const REPORTER_SOURCE: &str = include_str!("../../reporters/vitest-reporter.mjs");

/// Vitest adapter that spawns vitest with a custom NDJSON reporter.
/// For Nx workspaces, finds vite/vitest configs and runs vitest directly
/// with `--config` to bypass nx's output buffering.
pub struct VitestRunner {
    workspace: PathBuf,
}

impl VitestRunner {
    pub fn new(workspace: PathBuf) -> Self {
        Self { workspace }
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
                .workspace
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
    async fn spawn_and_stream(
        &self,
        args: &[&str],
        tx: mpsc::UnboundedSender<TestEvent>,
    ) -> Result<()> {
        let reporter_file = self.write_reporter()?;
        let reporter_path = reporter_file.path().to_string_lossy().to_string();

        let mut cmd = Command::new("npx");
        cmd.arg("vitest")
            .arg("run")
            .args(args)
            .arg("--disableConsoleIntercept")
            .arg(format!("--reporter={}", reporter_path));

        let mut child = cmd
            .current_dir(&self.workspace)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .context("failed to spawn vitest")?;

        let stdout = child.stdout.take().context("missing stdout")?;
        let stderr = child.stderr.take().context("missing stderr")?;

        // Read stderr in background for error reporting
        let tx_err = tx.clone();
        let stderr_handle = tokio::spawn(async move {
            let reader = BufReader::new(stderr);
            let mut lines = reader.lines();
            while let Ok(Some(line)) = lines.next_line().await {
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

        let status = child.wait().await.context("failed to wait for vitest")?;
        stderr_handle.await.ok();

        // Keep the temp file alive until vitest exits
        drop(reporter_file);

        if !status.success() {
            let _ = tx.send(TestEvent::Error {
                message: format!("vitest exited with code {}", status.code().unwrap_or(-1)),
            });
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
        let configs = self.find_vitest_configs();
        if configs.is_empty() {
            // No configs found, run vitest from workspace root (non-Nx)
            self.spawn_and_stream(&[], tx).await
        } else {
            // Run vitest once per config (each represents a project)
            for config in &configs {
                let config_str = config.to_string_lossy().to_string();
                self.spawn_and_stream(&["--config", &config_str], tx.clone())
                    .await?;
            }
            Ok(())
        }
    }

    async fn run_file(&self, file: &Path, tx: mpsc::UnboundedSender<TestEvent>) -> Result<()> {
        let file_str = file.to_string_lossy().to_string();
        if let Some(config) = self.find_config_for_file(file) {
            let config_str = config.to_string_lossy().to_string();
            self.spawn_and_stream(&[&file_str, "--config", &config_str], tx)
                .await
        } else {
            self.spawn_and_stream(&[&file_str], tx).await
        }
    }

    async fn run_test(
        &self,
        file: &Path,
        test_name: &str,
        tx: mpsc::UnboundedSender<TestEvent>,
    ) -> Result<()> {
        let file_str = file.to_string_lossy().to_string();
        if let Some(config) = self.find_config_for_file(file) {
            let config_str = config.to_string_lossy().to_string();
            self.spawn_and_stream(&[&file_str, "--config", &config_str, "-t", test_name], tx)
                .await
        } else {
            self.spawn_and_stream(&[&file_str, "-t", test_name], tx)
                .await
        }
    }

    fn name(&self) -> &str {
        "Vitest"
    }
}

// --- NDJSON deserialization types ---

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
enum VitestEvent {
    RunStarted {
        total: usize,
    },
    FileStarted {
        file: String,
    },
    TestFinished {
        file: String,
        name: String,
        state: String,
        duration: Option<f64>,
        error: Option<VitestError>,
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
            VitestEvent::RunStarted { total } => Some(TestEvent::RunStarted { total }),
            VitestEvent::FileStarted { file } => Some(TestEvent::FileStarted { path: file }),
            VitestEvent::TestFinished {
                file,
                name,
                state,
                duration,
                error,
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
                })
            }
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
                    duration_ms: duration,
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
