use std::path::PathBuf;
use std::{io, path::Path};

use anyhow::Result;
use crossterm::{
    ExecutableCommand,
    terminal::{self, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::prelude::*;

/// Suspend the TUI, open the editor at the given location, then restore the TUI.
/// `command_override` takes priority over `$EDITOR`.
pub fn open(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    path: PathBuf,
    line: Option<u32>,
    col: Option<u32>,
    editor_cmd: Option<&str>,
) -> Result<()> {
    terminal::disable_raw_mode()?;
    io::stdout().execute(LeaveAlternateScreen)?;

    let editor_env = editor_cmd
        .map(str::to_owned)
        .or_else(|| std::env::var("EDITOR").ok())
        .unwrap_or_else(|| "vim".into());

    let parts: Vec<String> =
        shell_words::split(&editor_env).unwrap_or_else(|_| vec![editor_env.clone()]);

    if parts.is_empty() {
        return Err(anyhow::anyhow!("empty editor command"));
    }

    let mut cmd = std::process::Command::new(&parts[0]);
    for arg in &parts[1..] {
        cmd.arg(arg);
    }

    build_args(&mut cmd, &parts[0], &path, line, col);
    let result = cmd.status();

    io::stdout().execute(EnterAlternateScreen)?;
    terminal::enable_raw_mode()?;
    terminal.clear()?;

    result.map_err(|_| anyhow::anyhow!("editor '{}' not found or failed to launch", editor_env))?;
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
        EditorKind::Vim | EditorKind::Neovim => {
            // nvim +10 -c "call cursor(10,5)" file
            if let Some(l) = line {
                cmd.arg(format!("+{}", l));
                if let Some(c) = col {
                    cmd.arg("-c").arg(format!("call cursor({},{})", l, c));
                }
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
    Neovim,
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
        "nvim" | "neovim" => EditorKind::Neovim,
        "hx" | "helix" => EditorKind::Helix,
        "code" | "code-insiders" | "codium" | "cursor" => EditorKind::VSCode,
        "webstorm" | "wstorm" => EditorKind::WebStorm,
        "zed" => EditorKind::Zed,
        _ => EditorKind::Vim,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args_for(editor: &str, path: &str, line: Option<u32>, col: Option<u32>) -> Vec<String> {
        let mut cmd = std::process::Command::new("true");
        build_args(&mut cmd, editor, Path::new(path), line, col);
        cmd.get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn test_editor_kind_nvim_binary_name() {
        assert!(matches!(editor_kind("nvim"), EditorKind::Neovim));
    }

    #[test]
    fn test_editor_kind_nvim_full_path() {
        assert!(matches!(
            editor_kind("/usr/local/bin/nvim"),
            EditorKind::Neovim
        ));
    }

    #[test]
    fn test_editor_kind_vim() {
        assert!(matches!(editor_kind("vim"), EditorKind::Vim));
    }

    #[test]
    fn test_editor_kind_helix() {
        assert!(matches!(editor_kind("hx"), EditorKind::Helix));
        assert!(matches!(editor_kind("helix"), EditorKind::Helix));
    }

    #[test]
    fn test_editor_kind_vscode_variants() {
        for bin in &["code", "code-insiders", "codium", "cursor"] {
            assert!(
                matches!(editor_kind(bin), EditorKind::VSCode),
                "{bin} should be VSCode"
            );
        }
    }

    #[test]
    fn test_editor_kind_unknown_falls_back_to_vim() {
        assert!(matches!(editor_kind("emacs"), EditorKind::Vim));
        assert!(matches!(editor_kind("nano"), EditorKind::Vim));
    }

    #[test]
    fn test_nvim_line_and_col() {
        let args = args_for("nvim", "foo.ts", Some(42), Some(5));
        assert_eq!(args, vec!["+42", "-c", "call cursor(42,5)", "foo.ts"]);
    }

    #[test]
    fn test_nvim_line_only() {
        let args = args_for("nvim", "foo.ts", Some(10), None);
        assert_eq!(args, vec!["+10", "foo.ts"]);
    }

    #[test]
    fn test_nvim_no_location() {
        let args = args_for("nvim", "foo.ts", None, None);
        assert_eq!(args, vec!["foo.ts"]);
    }

    #[test]
    fn test_helix_line_and_col() {
        let args = args_for("hx", "foo.ts", Some(42), Some(5));
        assert_eq!(args, vec!["foo.ts:42:5"]);
    }

    #[test]
    fn test_helix_line_only() {
        let args = args_for("hx", "foo.ts", Some(10), None);
        assert_eq!(args, vec!["foo.ts:10"]);
    }

    #[test]
    fn test_helix_no_location() {
        let args = args_for("hx", "foo.ts", None, None);
        assert_eq!(args, vec!["foo.ts"]);
    }

    #[test]
    fn test_vscode_line_and_col() {
        let args = args_for("code", "foo.ts", Some(42), Some(5));
        assert_eq!(args, vec!["--goto", "foo.ts:42:5"]);
    }

    #[test]
    fn test_vscode_no_location() {
        let args = args_for("code", "foo.ts", None, None);
        assert_eq!(args, vec!["--goto", "foo.ts"]);
    }

    #[test]
    fn test_webstorm_line_and_col() {
        let args = args_for("webstorm", "foo.ts", Some(42), Some(5));
        assert_eq!(args, vec!["--line", "42", "--column", "5", "foo.ts"]);
    }

    #[test]
    fn test_webstorm_line_only() {
        let args = args_for("webstorm", "foo.ts", Some(10), None);
        assert_eq!(args, vec!["--line", "10", "foo.ts"]);
    }
}
