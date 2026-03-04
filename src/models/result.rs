use serde::{Deserialize, Serialize};

use super::status::TestStatus;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TestResult {
    pub status: TestStatus,
    pub duration_ms: Option<u64>,
    pub failure: Option<FailureDetail>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailureDetail {
    pub message: String,
    pub expected: Option<String>,
    pub actual: Option<String>,
    /// Parsed structured form of `expected`, if the runner could produce it.
    #[serde(skip)]
    pub expected_parsed: Option<serde_json::Map<String, serde_json::Value>>,
    /// Parsed structured form of `actual`, if the runner could produce it.
    #[serde(skip)]
    pub actual_parsed: Option<serde_json::Map<String, serde_json::Value>>,
    pub diff: Option<String>,
    pub source_snippet: Option<String>,
    pub stack_trace: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RunSummary {
    pub total: usize,
    pub passed: usize,
    pub failed: usize,
    pub skipped: usize,
    pub duration: u64,
}
