pub mod vitest;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result};
use async_trait::async_trait;
use tokio::sync::mpsc;

use crate::app::TestEvent;

/// A discovered test file before any tests have been run.
#[derive(Debug, Clone)]
pub struct DiscoveredFile {
    pub path: PathBuf,
}

/// Trait for framework-specific test runner adapters.
#[async_trait]
pub trait TestRunner: Send + Sync {
    /// Discover test files in the workspace.
    async fn discover(&self, workspace: &Path) -> Result<Vec<DiscoveredFile>>;

    /// Run all tests, streaming events over the channel.
    async fn run_all(&self, tx: mpsc::UnboundedSender<TestEvent>) -> Result<()>;

    /// Run a single test file.
    async fn run_file(&self, file: &Path, tx: mpsc::UnboundedSender<TestEvent>) -> Result<()>;

    /// Run a specific test or suite by file path and name pattern.
    async fn run_test(
        &self,
        file: &Path,
        test_name: &str,
        tx: mpsc::UnboundedSender<TestEvent>,
    ) -> Result<()>;

    /// Run all tests in watch mode (re-runs on file changes).
    /// The process stays alive until the task is aborted.
    async fn run_all_watch(&self, tx: mpsc::UnboundedSender<TestEvent>) -> Result<()>;

    /// Run a single test file in watch mode (stays alive, re-runs on file changes).
    async fn run_file_watch(&self, file: &Path, tx: mpsc::UnboundedSender<TestEvent>)
    -> Result<()>;

    /// Run a specific test in watch mode (stays alive, re-runs on file changes).
    async fn run_test_watch(
        &self,
        file: &Path,
        test_name: &str,
        tx: mpsc::UnboundedSender<TestEvent>,
    ) -> Result<()>;

    /// Display name for this runner (e.g., "Vitest").
    #[allow(dead_code)]
    fn name(&self) -> &str;
}

/// Detect and construct the appropriate runner for the given workspace.
pub fn detect(
    workspace: PathBuf,
    project_root: Option<PathBuf>,
    ignore_patterns: Vec<String>,
) -> Arc<dyn TestRunner> {
    Arc::new(vitest::VitestRunner::new(
        workspace,
        project_root,
        ignore_patterns,
    ))
}

/// Resolve an Nx project name to its root directory (relative to workspace).
pub fn resolve_nx_project(workspace: &Path, name: &str) -> Result<PathBuf> {
    let output = std::process::Command::new("npx")
        .args(["nx", "show", "project", name, "--json"])
        .current_dir(workspace)
        .output()
        .context("failed to run `npx nx show project`")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("nx project '{}' not found: {}", name, stderr.trim());
    }

    let json: serde_json::Value =
        serde_json::from_slice(&output.stdout).context("failed to parse nx project JSON")?;

    let root = json["root"]
        .as_str()
        .context("nx project JSON missing 'root' field")?;

    Ok(workspace.join(root))
}
