mod app;
mod models;
mod runner;
mod ui;

use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context as _, Result};
use crossterm::{
    ExecutableCommand,
    event::{self, Event, KeyCode, KeyEvent, KeyModifiers},
    terminal::{self, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::prelude::*;
use tokio::time::{Duration, interval};

use app::{Action, App};
use runner::TestRunner;
use runner::vitest::VitestRunner;

#[tokio::main]
async fn main() -> Result<()> {
    // Setup terminal
    terminal::enable_raw_mode()?;
    io::stdout().execute(EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(io::stdout());
    let mut terminal = Terminal::new(backend)?;

    let result = run(&mut terminal).await;

    // Teardown terminal
    terminal::disable_raw_mode()?;
    io::stdout().execute(LeaveAlternateScreen)?;

    result
}

/// Resolve an Nx project name to its root directory (relative to workspace).
fn resolve_nx_project(workspace: &Path, name: &str) -> Result<PathBuf> {
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

async fn run(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> Result<()> {
    let workspace = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let project = std::env::args().nth(1);

    let (mut app, mut event_rx) = App::new(workspace.clone());
    app.project_name = project.clone();
    let mut tick = interval(Duration::from_millis(100));
    let mut test_runner: Option<Arc<dyn TestRunner>> = None;

    // Resolve Nx project and discover files asynchronously
    let (runner_tx, runner_rx) = tokio::sync::oneshot::channel::<Arc<dyn TestRunner>>();
    let mut runner_rx = Some(runner_rx);
    {
        let tx = app.event_tx.clone();
        let ws = workspace.clone();
        tokio::spawn(async move {
            // Resolve Nx project root if a project name was given
            let project_root = if let Some(name) = project {
                let ws_clone = ws.clone();
                tokio::task::spawn_blocking(move || resolve_nx_project(&ws_clone, &name).ok())
                    .await
                    .ok()
                    .flatten()
            } else {
                None
            };

            let discover_root = project_root.as_deref().unwrap_or(&ws).to_path_buf();

            let r: Arc<dyn TestRunner> = Arc::new(VitestRunner::new(ws.clone(), project_root));
            let _ = runner_tx.send(Arc::clone(&r));

            if let Ok(files) = r.discover(&discover_root).await {
                let displays: Vec<String> = files
                    .iter()
                    .map(|f| {
                        f.path
                            .strip_prefix(&ws)
                            .unwrap_or(&f.path)
                            .to_string_lossy()
                            .to_string()
                    })
                    .collect();
                let _ = tx.send(app::TestEvent::DiscoveryComplete { files: displays });
            }
        });
    }

    loop {
        terminal.draw(|frame| ui::draw(frame, &mut app))?;

        tokio::select! {
            _ = async {
                if event::poll(Duration::from_millis(16)).unwrap_or(false) &&
                     let Ok(Event::Key(key)) = event::read() {
                    let action = if app.filter_active {
                        match key.code {
                            KeyCode::Esc => Some(Action::FilterExit),
                            KeyCode::Enter => Some(Action::FilterApply),
                            KeyCode::Backspace => Some(Action::FilterBackspace),
                            KeyCode::Up => Some(Action::NavigateUp),
                            KeyCode::Down => Some(Action::NavigateDown),
                            KeyCode::Char(c) => Some(Action::FilterInput(c)),
                            _ => None,
                        }
                    } else {
                        map_key(key)
                    };
                    if let Some(action) = action {
                        if let Some(ref runner) = test_runner {
                            match action {
                                Action::RunAll => {
                                    app.handle_action(action);
                                    app.run_start = Some(std::time::Instant::now());
                                    let tx = app.event_tx.clone();
                                    let runner = Arc::clone(runner);

                                    tokio::spawn(async move {
                                        if let Err(e) = runner.run_all(tx.clone()).await {
                                            let _ = tx.send(app::TestEvent::Error {
                                                message: format!("Runner error: {}", e),
                                            });
                                        }
                                    });
                                }
                                Action::ToggleWatch => {
                                    app.handle_action(Action::ToggleWatch);
                                    if app.watch_mode {
                                        // Start watch mode
                                        let tx = app.event_tx.clone();
                                        let runner = Arc::clone(runner);
                                        let handle = tokio::spawn(async move {
                                            if let Err(e) = runner.run_all_watch(tx.clone()).await {
                                                let _ = tx.send(app::TestEvent::Error {
                                                    message: format!("Watch error: {}", e),
                                                });
                                            }
                                            // Notify app that watch process exited
                                            let _ = tx.send(app::TestEvent::WatchStopped);
                                        });
                                        app.watch_handle = Some(handle);
                                    } else {
                                        // Stop watch mode
                                        if let Some(handle) = app.watch_handle.take() {
                                            handle.abort();
                                        }
                                        app.running = false;
                                    }
                                }
                                other => {
                                    app.handle_action(other);
                                    for pending in app.pending_runs.drain(..) {
                                        app.running = true;
                                        app.run_start = Some(std::time::Instant::now());
                                        let tx = app.event_tx.clone();
                                        let runner = Arc::clone(runner);
                                        match pending {
                                            app::PendingRun::File(path) => {
                                                tokio::spawn(async move {
                                                    if let Err(e) = runner.run_file(&path, tx.clone()).await {
                                                        let _ = tx.send(app::TestEvent::Error {
                                                            message: format!("Runner error: {}", e),
                                                        });
                                                    }
                                                });
                                            }
                                            app::PendingRun::Test { file, name } => {
                                                tokio::spawn(async move {
                                                    if let Err(e) = runner.run_test(&file, &name, tx.clone()).await {
                                                        let _ = tx.send(app::TestEvent::Error {
                                                            message: format!("Runner error: {}", e),
                                                        });
                                                    }
                                                });
                                            }
                                        }
                                    }
                                }
                            }
                        } else {
                            // Runner not ready yet — handle navigation/UI actions, but skip run actions
                            match action {
                                Action::RunAll | Action::RerunFailed | Action::ToggleWatch | Action::Select => {
                                    app.output_lines.push("[INFO] Runner is still loading...".into());
                                }
                                other => app.handle_action(other),
                            }
                        }
                    }
                }
            } => {}

            result = async { runner_rx.as_mut().unwrap().await }, if runner_rx.is_some() => {
                runner_rx = None;
                match result {
                    Ok(r) => {
                        test_runner = Some(r);
                    }
                    Err(_) => {
                        app.handle_test_event(app::TestEvent::Error {
                            message: "Failed to initialize test runner".into(),
                        });
                        app.discovering = false;
                    }
                }
            }

            Some(test_event) = event_rx.recv() => {
                app.handle_test_event(test_event);
            }

            _ = tick.tick() => {
                if app.discovering || app.running {
                    app.spinner_tick = app.spinner_tick.wrapping_add(1);
                }
            }
        }

        if let Some((path, line, col)) = app.pending_editor.take() {
            // Suspend TUI, open editor, restore TUI
            terminal::disable_raw_mode()?;
            io::stdout().execute(LeaveAlternateScreen)?;

            let editor = std::env::var("EDITOR").unwrap_or_else(|_| "vim".into());
            let path_str = path.to_string_lossy().to_string();
            let mut cmd = std::process::Command::new(&editor);

            match (line, col) {
                (Some(l), Some(c)) => {
                    // +call cursor(line,col) works in vim and nvim
                    cmd.arg(format!("+call cursor({},{})", l, c));
                }
                (Some(l), None) => {
                    cmd.arg(format!("+{}", l));
                }
                _ => {}
            }
            cmd.arg(&path_str);
            let _ = cmd.status();

            io::stdout().execute(EnterAlternateScreen)?;
            terminal::enable_raw_mode()?;
            terminal.clear()?;
        }

        if app.should_quit {
            break;
        }
    }

    Ok(())
}

fn map_key(key: KeyEvent) -> Option<Action> {
    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
        return Some(Action::Quit);
    }

    match key.code {
        KeyCode::Char('q') => Some(Action::Quit),
        KeyCode::Tab => Some(Action::FocusNext),
        KeyCode::BackTab => Some(Action::FocusPrevious),
        KeyCode::Up | KeyCode::Char('k') => Some(Action::NavigateUp),
        KeyCode::Down | KeyCode::Char('j') => Some(Action::NavigateDown),
        KeyCode::Right | KeyCode::Char('l') => Some(Action::Expand),
        KeyCode::Char('L') => Some(Action::ExpandAll),
        KeyCode::Left | KeyCode::Char('h') => Some(Action::Collapse),
        KeyCode::Char('H') => Some(Action::CollapseAll),
        KeyCode::Char('g') | KeyCode::Home => Some(Action::JumpToStart),
        KeyCode::Char('G') | KeyCode::End => Some(Action::JumpToEnd),
        KeyCode::Enter => Some(Action::Select),
        KeyCode::Char('a') => Some(Action::RunAll),
        KeyCode::Char('r') => Some(Action::RerunFailed),
        KeyCode::Char('w') => Some(Action::ToggleWatch),
        KeyCode::Char('f') | KeyCode::Char('/') => Some(Action::FilterEnter),
        KeyCode::Char('e') => Some(Action::OpenInEditor),
        _ => None,
    }
}
