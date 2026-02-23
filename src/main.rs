mod app;
mod editor;
mod models;
mod runner;
mod ui;

use std::io;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use crossterm::{
    ExecutableCommand,
    event::{Event, EventStream},
    terminal::{self, EnterAlternateScreen, LeaveAlternateScreen},
};
use futures_util::StreamExt;
use ratatui::prelude::*;
use tokio::sync::mpsc;
use tokio::time::{Duration, interval};

use app::{Action, App, handle_action, handle_test_event, trigger_action};
use runner::{TestRunner, resolve_nx_project};

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

async fn run(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> Result<()> {
    let workspace = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let project = std::env::args().nth(1);

    let (mut app, mut event_rx) = App::new(workspace.clone());
    app.project_name = project.clone();
    let mut tick = interval(Duration::from_millis(100));
    let mut test_runner: Option<Arc<dyn TestRunner>> = None;
    let mut runner_rx = Some(start_runner(workspace, project, app.event_tx.clone()));
    let mut event_stream = EventStream::new();

    loop {
        terminal.draw(|frame| ui::draw(frame, &mut app))?;

        tokio::select! {
            maybe_event = event_stream.next() => {
                match maybe_event {
                None => break,
                Some(Err(e)) => return Err(e.into()),
                Some(Ok(Event::Key(key))) => {
                    let action = trigger_action(key, app.filter_active);

                    if let Some(action) = action {
                        if let Some(ref runner) = test_runner {
                            match action {
                                Action::RunAll => {
                                    handle_action(&mut app, action);
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
                                    handle_action(&mut app, Action::ToggleWatch);
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
                                        runner.stop_watch();
                                        if let Some(handle) = app.watch_handle.take() {
                                            handle.abort();
                                        }
                                        app.running = false;
                                    }
                                }
                                other => {
                                    handle_action(&mut app, other);
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
                                other => handle_action(&mut app, other),
                            }
                        }
                    }
                }
                Some(Ok(_)) => {}
                }
            }

            result = async { runner_rx.as_mut().unwrap().await }, if runner_rx.is_some() => {
                runner_rx = None;
                match result {
                    Ok(r) => {
                        test_runner = Some(r);
                    }
                    Err(_) => {
                        app.notifier.error("Failed to initialize test runner");
                        app.discovering = false;
                    }
                }
            }

            Some(test_event) = event_rx.recv() => {
                handle_test_event(&mut app, test_event);
            }

            _ = tick.tick() => {
                if app.discovering || app.running {
                    app.spinner_tick = app.spinner_tick.wrapping_add(1);
                }
                app.notifier.prune_expired();
            }
        }

        if let Some((path, line, col)) = app.pending_editor.take()
            && let Err(e) = editor::open(terminal, path, line, col)
        {
            app.notifier.error(e.to_string());
        }

        if app.should_quit {
            break;
        }
    }

    Ok(())
}

/// Spawn the async runner-init task and return a receiver for the constructed runner.
fn start_runner(
    workspace: PathBuf,
    project: Option<String>,
    event_tx: mpsc::UnboundedSender<app::TestEvent>,
) -> tokio::sync::oneshot::Receiver<Arc<dyn TestRunner>> {
    let (runner_tx, runner_rx) = tokio::sync::oneshot::channel();
    tokio::spawn(async move {
        let project_root = match project {
            Some(name) => {
                let ws_clone = workspace.clone();
                let name_clone = name.clone();
                let result = tokio::task::spawn_blocking(move || {
                    resolve_nx_project(&ws_clone, &name_clone).ok()
                })
                .await
                .ok()
                .flatten();
                if result.is_none() {
                    let r: Arc<dyn TestRunner> = runner::detect(workspace, None);
                    let _ = runner_tx.send(Arc::clone(&r));
                    let _ = event_tx.send(app::TestEvent::DiscoveryFailed {
                        message: format!("Nx project '{}' not found", name),
                    });
                    return;
                }
                result
            }
            None => None,
        };

        let discover_root = project_root.as_deref().unwrap_or(&workspace).to_path_buf();
        let r: Arc<dyn TestRunner> = runner::detect(workspace.clone(), project_root);
        let _ = runner_tx.send(Arc::clone(&r));

        match r.discover(&discover_root).await {
            Ok(files) => {
                let displays: Vec<String> = files
                    .iter()
                    .map(|f| {
                        f.path
                            .strip_prefix(&workspace)
                            .unwrap_or(&f.path)
                            .to_string_lossy()
                            .to_string()
                    })
                    .collect();
                let _ = event_tx.send(app::TestEvent::DiscoveryComplete { files: displays });
            }
            Err(_) => {
                let _ = event_tx.send(app::TestEvent::DiscoveryFailed {
                    message: "Failed to discover test files".into(),
                });
            }
        }
    });
    runner_rx
}
