mod app;
mod constants;
mod filter;
mod highlight;
mod input;
mod netinfo;
mod source;
mod state;
mod ui;

use anyhow::Result;
use app::{App, InputMode};
use clap::Parser;
use constants::POLL_INTERVAL_MS;
use crossterm::{
    event::{
        self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyModifiers, MouseButton,
        MouseEventKind,
    },
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

    #[arg(
        short = 'l',
        long = "listen",
        help = "Listen on TCP port for incoming logs"
    )]
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

    let result = run_app(&mut terminal, rx, cli.port);

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
    listen_port: Option<u16>,
) -> Result<()> {
    let mut app = App::new(rx, listen_port);

    loop {
        app.poll_source();

        let visible_height = terminal.size()?.height.saturating_sub(9) as usize;

        terminal.draw(|f| ui::draw(f, &mut app))?;

        if event::poll(Duration::from_millis(POLL_INTERVAL_MS))? {
            let ev = event::read()?;

            if let Event::Mouse(mouse) = &ev {
                if mouse.kind == MouseEventKind::Down(MouseButton::Left)
                    && app.listen_state.show_popup()
                {
                    if let Some(text) = app.listen_state.handle_click(mouse.column, mouse.row) {
                        copy_to_clipboard(&text);
                        app.status_message = Some(format!("Copied: {}", text));
                    }
                }
            }

            if let Event::Key(key) = ev {
                app.status_message = None;

                if app.show_quit_confirm {
                    handle_quit_confirm(&mut app, key.code)?;
                    continue;
                }

                if app.listen_state.show_popup() {
                    handle_listen_popup(&mut app, key.code, key.modifiers);
                    continue;
                }

                match app.input_mode {
                    InputMode::Normal => {
                        handle_normal_mode(&mut app, key.code, key.modifiers, visible_height)?
                    }
                    _ => {
                        if app.handle_input_key(key.code) {
                            app.apply_current_input();
                        }
                    }
                }
            }
        }
    }
}

fn handle_quit_confirm(app: &mut App, key_code: KeyCode) -> Result<()> {
    match key_code {
        KeyCode::Char('y') | KeyCode::Char('Y') => std::process::exit(0),
        KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc | KeyCode::Char('q') => {
            app.show_quit_confirm = false;
        }
        _ => {}
    }
    Ok(())
}

fn handle_listen_popup(app: &mut App, key_code: KeyCode, modifiers: KeyModifiers) {
    match key_code {
        KeyCode::Char('q') => app.show_quit_confirm = true,
        KeyCode::Char('c') if modifiers.contains(KeyModifiers::CONTROL) => {
            app.show_quit_confirm = true
        }
        KeyCode::Tab => app.listen_state.toggle_display_mode(),
        KeyCode::Up | KeyCode::Char('k') => app.listen_state.select_prev(),
        KeyCode::Down | KeyCode::Char('j') => app.listen_state.select_next(),
        KeyCode::Enter => {
            if let Some(text) = app.listen_state.get_selected_copy_text() {
                copy_to_clipboard(&text);
                app.status_message = Some(format!("Copied: {}", text));
            }
        }
        _ => {}
    }
}

fn handle_normal_mode(
    app: &mut App,
    key_code: KeyCode,
    modifiers: KeyModifiers,
    visible_height: usize,
) -> Result<()> {
    match key_code {
        KeyCode::Char('q') => app.show_quit_confirm = true,
        KeyCode::Char('c') if modifiers.contains(KeyModifiers::CONTROL) => {
            app.show_quit_confirm = true
        }
        KeyCode::Char('d') => app.input_mode = InputMode::HideEdit,
        KeyCode::Char('f') => app.input_mode = InputMode::FilterEdit,
        KeyCode::Char('h') => app.input_mode = InputMode::HighlightEdit,
        KeyCode::Char('c') => app.clear(),
        KeyCode::Char('t') => app.toggle_time(),
        KeyCode::Char('w') => app.toggle_wrap(),
        KeyCode::Char('g') => app.log_state.scroll_to_start(),
        KeyCode::Char('G') => app.log_state.scroll_to_end(),
        KeyCode::Up | KeyCode::Char('k') => app.log_state.scroll_up(1),
        KeyCode::Down | KeyCode::Char('j') => app.log_state.scroll_down(1),
        KeyCode::PageUp => app.log_state.scroll_up(visible_height),
        KeyCode::PageDown => app.log_state.scroll_down(visible_height),
        KeyCode::Home => app.log_state.scroll_to_start(),
        KeyCode::End => app.log_state.scroll_to_end(),
        _ => {}
    }
    Ok(())
}

fn copy_to_clipboard(text: &str) {
    #[cfg(target_os = "macos")]
    {
        use std::io::Write;
        use std::process::{Command, Stdio};
        if let Ok(mut child) = Command::new("pbcopy").stdin(Stdio::piped()).spawn() {
            if let Some(stdin) = child.stdin.as_mut() {
                let _ = stdin.write_all(text.as_bytes());
            }
            let _ = child.wait();
        }
    }

    #[cfg(target_os = "linux")]
    {
        use std::io::Write;
        use std::process::{Command, Stdio};
        let cmds = ["xclip", "xsel"];
        for cmd in cmds {
            if let Ok(mut child) = Command::new(cmd)
                .args(if cmd == "xclip" {
                    &["-selection", "clipboard"][..]
                } else {
                    &["--clipboard", "--input"][..]
                })
                .stdin(Stdio::piped())
                .spawn()
            {
                if let Some(stdin) = child.stdin.as_mut() {
                    let _ = stdin.write_all(text.as_bytes());
                }
                let _ = child.wait();
                break;
            }
        }
    }

    #[cfg(target_os = "windows")]
    {
        use std::io::Write;
        use std::process::{Command, Stdio};
        if let Ok(mut child) = Command::new("clip").stdin(Stdio::piped()).spawn() {
            if let Some(stdin) = child.stdin.as_mut() {
                let _ = stdin.write_all(text.as_bytes());
            }
            let _ = child.wait();
        }
    }
}
