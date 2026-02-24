use std::path::Path;

use serde::Deserialize;

#[derive(Debug, Default, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub discovery: DiscoveryConfig,
    #[serde(default)]
    pub editor: EditorConfig,
}

/// Controls which files are excluded during test discovery.
#[derive(Debug, Default, Deserialize)]
pub struct DiscoveryConfig {
    /// Glob patterns (relative to workspace root) of files to skip.
    /// Example: ["src/legacy/**", "**/*.contract.test.ts"]
    #[serde(default)]
    pub ignore: Vec<String>,
}

/// Overrides the editor used when opening files.
#[derive(Debug, Default, Deserialize)]
pub struct EditorConfig {
    /// Binary name or path to use instead of `$EDITOR`.
    /// The argument format is auto-detected from the binary name.
    /// Example: "nvim" or "/usr/local/bin/hx"
    pub command: Option<String>,
}

impl Config {
    /// Load `lens.toml` from the workspace root, falling back to defaults if absent or invalid.
    pub fn load(workspace: &Path) -> Self {
        let path = workspace.join("lens.toml");
        let Ok(content) = std::fs::read_to_string(&path) else {
            return Self::default();
        };
        toml::from_str(&content).unwrap_or_default()
    }
}
