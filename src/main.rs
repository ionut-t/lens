mod app;
mod config;
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

use app::{Action, App, LayoutMode, StartupRun, handle_action, handle_test_event, trigger_action};
use runner::{TestRunner, resolve_nx_project};

use crate::config::Config;

const USAGE: &str = "\
Usage: lens [PROJECT] [OPTIONS]

Arguments:
  [PROJECT]          Nx project name to scope discovery to

Options:
      --file <PATH>  Run this test file on startup
  -t, --test <NAME>  Run only tests/suites matching NAME (requires --file)
  -w, --watch        Start in watch mode
      --hide-failed  Start with the failed-tests panel hidden (toggle with x)
      --layout <L>   Panel layout: auto, horizontal or vertical (cycle with v)
  -h, --help         Print help";

struct Cli {
    project: Option<String>,
    file: Option<PathBuf>,
    test: Option<String>,
    watch: bool,
    hide_failed: bool,
    layout: LayoutMode,
}

fn parse_cli() -> Cli {
    let mut cli = Cli {
        project: None,
        file: None,
        test: None,
        watch: false,
        hide_failed: false,
        layout: LayoutMode::Auto,
    };

    let fail = |msg: &str| -> ! {
        eprintln!("{msg}\n\n{USAGE}");
        std::process::exit(2);
    };

    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--file" => match args.next() {
                Some(path) => cli.file = Some(PathBuf::from(path)),
                None => fail("--file requires a path"),
            },
            "-t" | "--test" => match args.next() {
                Some(name) => cli.test = Some(name),
                None => fail("--test requires a name"),
            },
            "-w" | "--watch" => cli.watch = true,
            "--hide-failed" => cli.hide_failed = true,
            "--layout" => match args.next().as_deref() {
                Some("auto") => cli.layout = LayoutMode::Auto,
                Some("horizontal") | Some("h") => cli.layout = LayoutMode::Horizontal,
                Some("vertical") | Some("v") => cli.layout = LayoutMode::Vertical,
                Some(other) => fail(&format!(
                    "invalid layout '{other}' (expected auto, horizontal or vertical)"
                )),
                None => fail("--layout requires a value: auto, horizontal or vertical"),
            },
            "-h" | "--help" => {
                println!("{USAGE}");
                std::process::exit(0);
            }
            _ if arg.starts_with('-') => fail(&format!("unknown option: {arg}")),
            _ if cli.project.is_none() => cli.project = Some(arg),
            _ => fail(&format!("unexpected argument: {arg}")),
        }
    }

    if cli.test.is_some() && cli.file.is_none() {
        fail("--test requires --file");
    }

    cli
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = parse_cli();

    // Setup terminal
    terminal::enable_raw_mode()?;
    io::stdout().execute(EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(io::stdout());
    let mut terminal = Terminal::new(backend)?;

    let result = run(&mut terminal, cli).await;

    // Teardown terminal
    terminal::disable_raw_mode()?;
    io::stdout().execute(LeaveAlternateScreen)?;

    result
}

async fn run(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>, cli: Cli) -> Result<()> {
    let workspace = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let cfg = Config::load(&workspace);

    let (mut app, mut event_rx) = App::new(workspace.clone());
    app.project_name = cli.project.clone();
    app.watch_mode = cli.watch;
    app.show_failed_panel = !cli.hide_failed;
    app.layout_mode = cli.layout;
    app.startup_run = cli.file.map(|file| StartupRun {
        file,
        test: cli.test,
    });
    let mut tick = interval(Duration::from_millis(100));
    let mut test_runner: Option<Arc<dyn TestRunner>> = None;
    let mut runner_rx = Some(start_runner(
        workspace,
        cli.project,
        cfg.discovery.ignore,
        app.event_tx.clone(),
    ));
    let editor_command = cfg.editor.command;
    let mut event_stream = EventStream::new();

    loop {
        if app.watched_ids_stale {
            app.refresh_watched_ids();
        }
        terminal.draw(|frame| ui::draw(frame, &mut app))?;

        tokio::select! {
            maybe_event = event_stream.next() => {
                match maybe_event {
                None => break,
                Some(Err(e)) => return Err(e.into()),
                Some(Ok(Event::Key(key))) => {
                    let action = trigger_action(key, app.filter_active, app.show_help);

                    if let Some(action) = action {
                        if let Some(ref runner) = test_runner {
                            match action {
                                Action::RunAll => {
                                    handle_action(&mut app, action);
                                    app.run_start = Some(std::time::Instant::now());
                                    let tx = app.event_tx.clone();
                                    if app.watch_mode {
                                        // Stop any previous watch, then start a new global watch
                                        if let Some(h) = app.watch_handle.take() {
                                            h.abort();
                                        }
                                        let runner = Arc::clone(runner);
                                        let handle = tokio::spawn(async move {
                                            if let Err(e) = runner.run_all_watch(tx.clone()).await {
                                                let _ = tx.send(app::TestEvent::Error {
                                                    message: format!("Watch error: {}", e),
                                                });
                                            }
                                            let _ = tx.send(app::TestEvent::WatchStopped);
                                        });
                                        app.watch_handle = Some(handle);
                                        app.watch_scope = app::WatchScope::All;
                                        app.watched_ids_stale = true;
                                    } else {
                                        let runner = Arc::clone(runner);
                                        tokio::spawn(async move {
                                            if let Err(e) = runner.run_all(tx.clone()).await {
                                                let _ = tx.send(app::TestEvent::Error {
                                                    message: format!("Runner error: {}", e),
                                                });
                                            }
                                        });
                                    }
                                }
                                Action::ToggleWatch => {
                                    handle_action(&mut app, Action::ToggleWatch);
                                    if !app.watch_mode {
                                        // Turned OFF — kill any active watch process
                                        if let Some(handle) = app.watch_handle.take() {
                                            handle.abort();
                                        }
                                        app.running = false;
                                        app.watch_scope = app::WatchScope::None;
                                        app.watched_ids_stale = true;
                                    }
                                    // Turned ON — nothing, process starts lazily on first run
                                }
                                other => {
                                    handle_action(&mut app, other);
                                }
                            }
                        } else {
                            // Runner not ready yet — handle navigation/UI actions, but skip run actions
                            match action {
                                Action::RunAll | Action::RunFiltered | Action::RerunFailed | Action::ToggleWatch | Action::Select => {
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

        if !app.pending_runs.is_empty()
            && let Some(ref runner) = test_runner
        {
            spawn_pending_runs(&mut app, runner);
        }

        if let Some((path, line, col)) = app.pending_editor.take()
            && let Err(e) = editor::open(terminal, path, line, col, editor_command.as_deref())
        {
            app.notifier.error(e.to_string());
        }

        if app.should_quit {
            break;
        }
    }

    Ok(())
}

/// Spawn a runner task for every queued pending run, honouring watch mode.
fn spawn_pending_runs(app: &mut App, runner: &Arc<dyn TestRunner>) {
    for pending in std::mem::take(&mut app.pending_runs) {
        app.running = true;
        app.run_start = Some(std::time::Instant::now());
        let tx = app.event_tx.clone();

        // In watch mode, stop the previous watch before starting a new one
        if app.watch_mode
            && let Some(h) = app.watch_handle.take()
        {
            h.abort();
        }

        let runner_clone = Arc::clone(runner);
        match pending {
            app::PendingRun::Files(paths) => {
                if app.watch_mode {
                    app.watch_scope = app::WatchScope::All;
                    app.watched_ids_stale = true;
                    let handle = tokio::spawn(async move {
                        if let Err(e) = runner_clone.run_all_watch(tx.clone()).await {
                            let _ = tx.send(app::TestEvent::Error {
                                message: format!("Watch error: {}", e),
                            });
                        }
                        let _ = tx.send(app::TestEvent::WatchStopped);
                    });
                    app.watch_handle = Some(handle);
                } else {
                    tokio::spawn(async move {
                        if let Err(e) = runner_clone.run_files(&paths, tx.clone()).await {
                            let _ = tx.send(app::TestEvent::Error {
                                message: format!("Runner error: {}", e),
                            });
                        }
                    });
                }
            }
            app::PendingRun::File(path) => {
                if app.watch_mode {
                    app.watch_scope = app::WatchScope::File(path.clone());
                    app.watched_ids_stale = true;
                    let handle = tokio::spawn(async move {
                        if let Err(e) = runner_clone.run_file_watch(&path, tx.clone()).await {
                            let _ = tx.send(app::TestEvent::Error {
                                message: format!("Watch error: {}", e),
                            });
                        }
                        let _ = tx.send(app::TestEvent::WatchStopped);
                    });
                    app.watch_handle = Some(handle);
                } else {
                    tokio::spawn(async move {
                        if let Err(e) = runner_clone.run_file(&path, tx.clone()).await {
                            let _ = tx.send(app::TestEvent::Error {
                                message: format!("Runner error: {}", e),
                            });
                        }
                    });
                }
            }
            app::PendingRun::Test { file, name } => {
                if app.watch_mode {
                    app.watch_scope = app::WatchScope::Test {
                        file: file.clone(),
                        name: name.clone(),
                    };
                    app.watched_ids_stale = true;
                    let handle = tokio::spawn(async move {
                        if let Err(e) = runner_clone.run_test_watch(&file, &name, tx.clone()).await
                        {
                            let _ = tx.send(app::TestEvent::Error {
                                message: format!("Watch error: {}", e),
                            });
                        }
                        let _ = tx.send(app::TestEvent::WatchStopped);
                    });
                    app.watch_handle = Some(handle);
                } else {
                    tokio::spawn(async move {
                        if let Err(e) = runner_clone.run_test(&file, &name, tx.clone()).await {
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

/// Spawn the async runner-init task and return a receiver for the constructed runner.
fn start_runner(
    workspace: PathBuf,
    project: Option<String>,
    ignore_patterns: Vec<String>,
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
                    let r: Arc<dyn TestRunner> = runner::detect(workspace, None, ignore_patterns);
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
        let r: Arc<dyn TestRunner> =
            runner::detect(workspace.clone(), project_root, ignore_patterns);
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
