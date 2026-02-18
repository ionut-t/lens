use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum TestStatus {
    #[default]
    Pending,
    Running,
    Passed,
    Failed,
    Skipped,
}

impl TestStatus {
    pub fn icon(&self) -> &'static str {
        match self {
            TestStatus::Pending => "◌",
            TestStatus::Running => "⟳",
            TestStatus::Passed => "✔",
            TestStatus::Failed => "✘",
            TestStatus::Skipped => "⊘",
        }
    }

    pub fn is_terminal(&self) -> bool {
        matches!(self, TestStatus::Passed | TestStatus::Failed)
    }

    pub fn priority(&self) -> u8 {
        match self {
            TestStatus::Failed => 4,
            TestStatus::Running => 3,
            TestStatus::Pending => 2,
            TestStatus::Passed => 1,
            TestStatus::Skipped => 0,
        }
    }
}
