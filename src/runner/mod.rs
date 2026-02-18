pub mod vitest;

use std::path::{Path, PathBuf};

use anyhow::Result;
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

    /// Display name for this runner (e.g., "Vitest").
    fn name(&self) -> &str;
}
