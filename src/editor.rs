use std::path::PathBuf;
use std::{io, path::Path};

use anyhow::Result;
use crossterm::{
    ExecutableCommand,
    terminal::{self, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::prelude::*;

/// Suspend the TUI, open `$EDITOR` at the given location, then restore the TUI.
pub fn open(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    path: PathBuf,
    line: Option<u32>,
    col: Option<u32>,
) -> Result<()> {
    terminal::disable_raw_mode()?;
    io::stdout().execute(LeaveAlternateScreen)?;

    let editor = std::env::var("EDITOR").unwrap_or_else(|_| "vim".into());
    let mut cmd = std::process::Command::new(&editor);

    build_args(&mut cmd, &editor, &path, line, col);
    let result = cmd.status();

    io::stdout().execute(EnterAlternateScreen)?;
    terminal::enable_raw_mode()?;
    terminal.clear()?;

    result.map_err(|_| anyhow::anyhow!("editor '{}' not found or failed to launch", editor))?;
    Ok(())
}

fn build_args(
    cmd: &mut std::process::Command,
    editor: &str,
    path: &Path,
    line: Option<u32>,
    col: Option<u32>,
) {
    let path_str = path.to_string_lossy();

    match editor_kind(editor) {
        EditorKind::Vim => {
            // vim +call cursor(line,col) file
            match (line, col) {
                (Some(l), Some(c)) => {
                    cmd.arg(format!("+call cursor({},{})", l, c));
                }
                (Some(l), None) => {
                    cmd.arg(format!("+{}", l));
                }
                _ => {}
            }
            cmd.arg(path_str.as_ref());
        }

        EditorKind::Helix | EditorKind::Zed => {
            // hx file:line:col  |  zed file:line:col
            match (line, col) {
                (Some(l), Some(c)) => cmd.arg(format!("{}:{}:{}", path_str, l, c)),
                (Some(l), None) => cmd.arg(format!("{}:{}", path_str, l)),
                _ => cmd.arg(path_str.as_ref()),
            };
        }

        EditorKind::VSCode => {
            // code --goto file:line:col
            cmd.arg("--goto");
            match (line, col) {
                (Some(l), Some(c)) => cmd.arg(format!("{}:{}:{}", path_str, l, c)),
                (Some(l), None) => cmd.arg(format!("{}:{}", path_str, l)),
                _ => cmd.arg(path_str.as_ref()),
            };
        }

        EditorKind::WebStorm => {
            // webstorm --line <n> --column <n> file
            if let Some(l) = line {
                cmd.arg("--line").arg(l.to_string());
            }
            if let Some(c) = col {
                cmd.arg("--column").arg(c.to_string());
            }
            cmd.arg(path_str.as_ref());
        }
    }
}

enum EditorKind {
    Vim,
    Helix,
    VSCode,
    WebStorm,
    Zed,
}

fn editor_kind(editor: &str) -> EditorKind {
    let bin = Path::new(editor)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(editor);

    match bin {
        "hx" | "helix" => EditorKind::Helix,
        "code" | "code-insiders" | "codium" => EditorKind::VSCode,
        "webstorm" | "wstorm" => EditorKind::WebStorm,
        "zed" => EditorKind::Zed,
        _ => EditorKind::Vim,
    }
}
