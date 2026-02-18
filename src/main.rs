mod app;
mod models;
mod runner;
mod ui;

use std::io;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
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

async fn run(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> Result<()> {
    let workspace = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let (mut app, mut event_rx) = App::new(workspace.clone());
    let mut tick = interval(Duration::from_millis(100));
    let runner: Arc<dyn TestRunner> = Arc::new(VitestRunner::new(workspace.clone()));

    // Discover test files and populate the tree on startup
    if let Ok(files) = runner.discover(&workspace).await {
        for file in &files {
            let display = file
                .path
                .strip_prefix(&workspace)
                .unwrap_or(&file.path)
                .to_string_lossy()
                .to_string();
            app.find_or_create_file_node(&display, &display);
        }
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
                            match action {
                                Action::RunAll => {
                                    app.handle_action(action);
                                    let tx = app.event_tx.clone();
                                    let r = Arc::clone(&runner);
                                    tokio::spawn(async move {
                                        if let Err(e) = r.run_all(tx.clone()).await {
                                            let _ = tx.send(app::TestEvent::Error {
                                                message: format!("Runner error: {}", e),
                                            });
                                        }
                                    });
                                }
                                other => {
                                    app.handle_action(other);
                                    for pending in app.pending_runs.drain(..) {
                                        app.running = true;
                                        let tx = app.event_tx.clone();
                                        let r = Arc::clone(&runner);
                                        match pending {
                                            app::PendingRun::File(path) => {
                                                tokio::spawn(async move {
                                                    if let Err(e) = r.run_file(&path, tx.clone()).await {
                                                        let _ = tx.send(app::TestEvent::Error {
                                                            message: format!("Runner error: {}", e),
                                                        });
                                                    }
                                                });
                                            }
                                            app::PendingRun::Test { file, name } => {
                                                tokio::spawn(async move {
                                                    if let Err(e) = r.run_test(&file, &name, tx.clone()).await {
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
                    }
                }
            } => {}

            Some(test_event) = event_rx.recv() => {
                app.handle_test_event(test_event);
            }

            _ = tick.tick() => {}
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
        KeyCode::Up | KeyCode::Char('k') => Some(Action::NavigateUp),
        KeyCode::Down | KeyCode::Char('j') => Some(Action::NavigateDown),
        KeyCode::Right | KeyCode::Char('l') => Some(Action::Expand),
        KeyCode::Left | KeyCode::Char('h') => Some(Action::Collapse),
        KeyCode::Enter => Some(Action::Select),
        KeyCode::Char('a') => Some(Action::RunAll),
        KeyCode::Char('r') => Some(Action::RerunFailed),
        KeyCode::Char('w') => Some(Action::ToggleWatch),
        KeyCode::Char('f') | KeyCode::Char('/') => Some(Action::FilterEnter),
        KeyCode::Char('e') => Some(Action::OpenInEditor),
        _ => None,
    }
}
