mod app;
mod filter;
mod highlight;
mod netinfo;
mod source;
mod state;
mod ui;

use anyhow::Result;
use app::{App, InputMode};
use clap::Parser;
use crossterm::{
    event::{
        self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyModifiers,
        MouseButton, MouseEventKind,
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

        if event::poll(Duration::from_millis(50))? {
            let ev = event::read()?;

            if let Event::Mouse(mouse) = &ev {
                if mouse.kind == MouseEventKind::Down(MouseButton::Left) {
                    if app.show_listen_popup() {
                        if let Some(text) = app.handle_listen_popup_click(mouse.column, mouse.row) {
                            copy_to_clipboard(&text);
                            app.status_message = Some(format!("Copied: {}", text));
                        }
                    }
                }
            }

            if let Event::Key(key) = ev {
                app.status_message = None;

                if app.show_listen_popup() {
                    match key.code {
                        KeyCode::Char('q') => return Ok(()),
                        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                            return Ok(())
                        }
                        KeyCode::Tab => app.toggle_listen_display_mode(),
                        KeyCode::Up | KeyCode::Char('k') => app.listen_select_prev(),
                        KeyCode::Down | KeyCode::Char('j') => app.listen_select_next(),
                        KeyCode::Enter => {
                            if let Some(text) = app.get_selected_copy_text() {
                                copy_to_clipboard(&text);
                                app.status_message = Some(format!("Copied: {}", text));
                            }
                        }
                        _ => {}
                    }
                    continue;
                }

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
                        KeyCode::Char('J') => app.toggle_json(),
                        KeyCode::Char('w') => app.toggle_wrap(),
                        KeyCode::Char('g') => app.scroll_to_start(visible_height),
                        KeyCode::Char('G') => app.scroll_to_end(visible_height),
                        KeyCode::Up | KeyCode::Char('k') => app.scroll_up(1, visible_height),
                        KeyCode::Down | KeyCode::Char('j') => app.scroll_down(1, visible_height),
                        KeyCode::PageUp => app.scroll_up(visible_height, visible_height),
                        KeyCode::PageDown => app.scroll_down(visible_height, visible_height),
                        KeyCode::Home => app.scroll_to_start(visible_height),
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
                        KeyCode::Left => {
                            app.hide_cursor = app.hide_cursor.saturating_sub(1);
                        }
                        KeyCode::Right => {
                            let len = app.hide_input.chars().count();
                            if app.hide_cursor < len {
                                app.hide_cursor += 1;
                            }
                        }
                        KeyCode::Home => {
                            app.hide_cursor = 0;
                        }
                        KeyCode::End => {
                            app.hide_cursor = app.hide_input.chars().count();
                        }
                        KeyCode::Char(c) => {
                            let byte_idx = app.hide_input.char_indices()
                                .nth(app.hide_cursor)
                                .map(|(i, _)| i)
                                .unwrap_or(app.hide_input.len());
                            app.hide_input.insert(byte_idx, c);
                            app.hide_cursor += 1;
                        }
                        KeyCode::Backspace => {
                            if app.hide_cursor > 0 {
                                let byte_idx = app.hide_input.char_indices()
                                    .nth(app.hide_cursor - 1)
                                    .map(|(i, _)| i)
                                    .unwrap_or(0);
                                app.hide_input.remove(byte_idx);
                                app.hide_cursor -= 1;
                            }
                        }
                        KeyCode::Delete => {
                            let len = app.hide_input.chars().count();
                            if app.hide_cursor < len {
                                let byte_idx = app.hide_input.char_indices()
                                    .nth(app.hide_cursor)
                                    .map(|(i, _)| i)
                                    .unwrap_or(app.hide_input.len());
                                app.hide_input.remove(byte_idx);
                            }
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
                        KeyCode::Left => {
                            app.filter_cursor = app.filter_cursor.saturating_sub(1);
                        }
                        KeyCode::Right => {
                            let len = app.filter_input.chars().count();
                            if app.filter_cursor < len {
                                app.filter_cursor += 1;
                            }
                        }
                        KeyCode::Home => {
                            app.filter_cursor = 0;
                        }
                        KeyCode::End => {
                            app.filter_cursor = app.filter_input.chars().count();
                        }
                        KeyCode::Char(c) => {
                            let byte_idx = app.filter_input.char_indices()
                                .nth(app.filter_cursor)
                                .map(|(i, _)| i)
                                .unwrap_or(app.filter_input.len());
                            app.filter_input.insert(byte_idx, c);
                            app.filter_cursor += 1;
                        }
                        KeyCode::Backspace => {
                            if app.filter_cursor > 0 {
                                let byte_idx = app.filter_input.char_indices()
                                    .nth(app.filter_cursor - 1)
                                    .map(|(i, _)| i)
                                    .unwrap_or(0);
                                app.filter_input.remove(byte_idx);
                                app.filter_cursor -= 1;
                            }
                        }
                        KeyCode::Delete => {
                            let len = app.filter_input.chars().count();
                            if app.filter_cursor < len {
                                let byte_idx = app.filter_input.char_indices()
                                    .nth(app.filter_cursor)
                                    .map(|(i, _)| i)
                                    .unwrap_or(app.filter_input.len());
                                app.filter_input.remove(byte_idx);
                            }
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
                        KeyCode::Left => {
                            app.highlight_cursor = app.highlight_cursor.saturating_sub(1);
                        }
                        KeyCode::Right => {
                            let len = app.highlight_input.chars().count();
                            if app.highlight_cursor < len {
                                app.highlight_cursor += 1;
                            }
                        }
                        KeyCode::Home => {
                            app.highlight_cursor = 0;
                        }
                        KeyCode::End => {
                            app.highlight_cursor = app.highlight_input.chars().count();
                        }
                        KeyCode::Char(c) => {
                            let byte_idx = app.highlight_input.char_indices()
                                .nth(app.highlight_cursor)
                                .map(|(i, _)| i)
                                .unwrap_or(app.highlight_input.len());
                            app.highlight_input.insert(byte_idx, c);
                            app.highlight_cursor += 1;
                        }
                        KeyCode::Backspace => {
                            if app.highlight_cursor > 0 {
                                let byte_idx = app.highlight_input.char_indices()
                                    .nth(app.highlight_cursor - 1)
                                    .map(|(i, _)| i)
                                    .unwrap_or(0);
                                app.highlight_input.remove(byte_idx);
                                app.highlight_cursor -= 1;
                            }
                        }
                        KeyCode::Delete => {
                            let len = app.highlight_input.chars().count();
                            if app.highlight_cursor < len {
                                let byte_idx = app.highlight_input.char_indices()
                                    .nth(app.highlight_cursor)
                                    .map(|(i, _)| i)
                                    .unwrap_or(app.highlight_input.len());
                                app.highlight_input.remove(byte_idx);
                            }
                        }
                        _ => {}
                    },
                }
            }
        }
    }
}

fn copy_to_clipboard(text: &str) {
    #[cfg(target_os = "macos")]
    {
        use std::io::Write;
        use std::process::{Command, Stdio};
        if let Ok(mut child) = Command::new("pbcopy")
            .stdin(Stdio::piped())
            .spawn()
        {
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
        if let Ok(mut child) = Command::new("clip")
            .stdin(Stdio::piped())
            .spawn()
        {
            if let Some(stdin) = child.stdin.as_mut() {
                let _ = stdin.write_all(text.as_bytes());
            }
            let _ = child.wait();
        }
    }
}
