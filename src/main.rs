mod app;
mod filter;
mod highlight;
mod source;
mod ui;

use anyhow::Result;
use app::{App, InputMode};
use clap::Parser;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use source::{start_source, LogSource, SourceEvent};
use std::io;
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Duration;

#[derive(Parser)]
#[command(name = "logviewer")]
#[command(about = "Interactive log viewer with filtering and highlighting")]
struct Cli {
    #[arg(help = "Log file to view (reads from stdin if not provided)")]
    file: Option<PathBuf>,

    #[arg(short = 'l', long = "listen", help = "Listen on TCP port for incoming logs")]
    port: Option<u16>,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    let (tx, rx) = mpsc::channel::<SourceEvent>();

    let source = if let Some(port) = cli.port {
        eprintln!("Listening on port {}...", port);
        LogSource::Network(port)
    } else if let Some(path) = cli.file {
        LogSource::File(path)
    } else {
        LogSource::Stdin
    };

    start_source(source, tx)?;

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let result = run_app(&mut terminal, rx);

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    if let Err(e) = result {
        eprintln!("Error: {}", e);
    }

    Ok(())
}

fn run_app(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    rx: mpsc::Receiver<SourceEvent>,
) -> Result<()> {
    let mut app = App::new(rx);

    loop {
        app.poll_source();

        let visible_height = terminal.size()?.height.saturating_sub(9) as usize;

        terminal.draw(|f| ui::draw(f, &app))?;

        if event::poll(Duration::from_millis(50))? {
            if let Event::Key(key) = event::read()? {
                app.status_message = None;

                match app.input_mode {
                    InputMode::Normal => match key.code {
                        KeyCode::Char('q') => return Ok(()),
                        KeyCode::Char('c')
                            if key.modifiers.contains(KeyModifiers::CONTROL) =>
                        {
                            return Ok(())
                        }
                        KeyCode::Char('d') => app.input_mode = InputMode::HideEdit,
                        KeyCode::Char('f') => app.input_mode = InputMode::FilterEdit,
                        KeyCode::Char('h') => app.input_mode = InputMode::HighlightEdit,
                        KeyCode::Char('c') => app.clear(),
                        KeyCode::Char('t') => app.toggle_time(),
                        KeyCode::Char('s') => app.toggle_heuristic(),
                        KeyCode::Char('w') => app.toggle_wrap(),
                        KeyCode::Char('g') => app.scroll_to_start(),
                        KeyCode::Char('G') => app.scroll_to_end(visible_height),
                        KeyCode::Up | KeyCode::Char('k') => app.scroll_up(1),
                        KeyCode::Down | KeyCode::Char('j') => app.scroll_down(1, visible_height),
                        KeyCode::PageUp => app.scroll_up(visible_height),
                        KeyCode::PageDown => app.scroll_down(visible_height, visible_height),
                        KeyCode::Home => app.scroll_to_start(),
                        KeyCode::End => app.scroll_to_end(visible_height),
                        _ => {}
                    },
                    InputMode::HideEdit => match key.code {
                        KeyCode::Enter => {
                            app.apply_hide();
                            if app.hide_error.is_none() {
                                app.input_mode = InputMode::Normal;
                            }
                        }
                        KeyCode::Esc => app.input_mode = InputMode::Normal,
                        KeyCode::Char(c) => app.hide_input.push(c),
                        KeyCode::Backspace => {
                            app.hide_input.pop();
                        }
                        _ => {}
                    },
                    InputMode::FilterEdit => match key.code {
                        KeyCode::Enter => {
                            app.apply_filter();
                            if app.filter_error.is_none() {
                                app.input_mode = InputMode::Normal;
                            }
                        }
                        KeyCode::Esc => app.input_mode = InputMode::Normal,
                        KeyCode::Char(c) => app.filter_input.push(c),
                        KeyCode::Backspace => {
                            app.filter_input.pop();
                        }
                        _ => {}
                    },
                    InputMode::HighlightEdit => match key.code {
                        KeyCode::Enter => {
                            app.apply_highlight();
                            if app.highlight_error.is_none() {
                                app.input_mode = InputMode::Normal;
                            }
                        }
                        KeyCode::Esc => app.input_mode = InputMode::Normal,
                        KeyCode::Char(c) => app.highlight_input.push(c),
                        KeyCode::Backspace => {
                            app.highlight_input.pop();
                        }
                        _ => {}
                    },
                }
            }
        }
    }
}
