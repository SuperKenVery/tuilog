use crate::constants::{PREFIX_WIDTH_WITHOUT_TIME, PREFIX_WIDTH_WITH_TIME};
use crate::core::{FilterState, InputFields, InputMode, ListenState, LogLine, LogState};
use crate::filter::parse_filter;
use crate::highlight::{apply_highlights_ratatui, highlight_line};
use crate::source::SourceEvent;
use crate::state::AppState;
use crossterm::event::KeyCode;
use fancy_regex::Regex;
use std::sync::mpsc::Receiver;

pub struct App {
    pub log_state: LogState,
    pub input_fields: InputFields,
    pub filter_state: FilterState,
    pub listen_state: ListenState,
    pub show_time: bool,
    pub wrap_lines: bool,
    pub input_mode: InputMode,
    pub source_rx: Receiver<SourceEvent>,
    pub status_message: Option<String>,
    pub show_quit_confirm: bool,
}

impl App {
    pub fn new(source_rx: Receiver<SourceEvent>, listen_port: Option<u16>) -> Self {
        let state = AppState::load();
        let mut app = Self {
            log_state: LogState::default(),
            input_fields: InputFields::from_state(&state),
            filter_state: FilterState::default(),
            listen_state: ListenState::new(listen_port),
            show_time: true,
            wrap_lines: state.wrap_lines,
            input_mode: InputMode::Normal,
            source_rx,
            status_message: None,
            show_quit_confirm: false,
        };
        app.apply_hide();
        app.apply_filter();
        app.apply_highlight();
        app
    }

    pub fn poll_source(&mut self) {
        while let Ok(event) = self.source_rx.try_recv() {
            match event {
                SourceEvent::Line(content) => {
                    let idx = self.log_state.add_line(content);
                    if self.matches_filter(idx) {
                        self.log_state.filtered_indices.push(idx);
                    }
                }
                SourceEvent::SystemLine(content) => {
                    let idx = self.log_state.add_line_with_update(content, false);
                    if self.matches_filter(idx) {
                        self.log_state.filtered_indices.push(idx);
                    }
                }
                SourceEvent::Error(e) => {
                    self.status_message = Some(format!("Source error: {}", e));
                }
                SourceEvent::Connected(_peer) => {
                    self.listen_state.has_connection = true;
                }
                SourceEvent::Disconnected(_peer) => {}
            }
        }
    }

    pub fn handle_input_key(&mut self, key_code: KeyCode) -> bool {
        if let Some(input) = self.input_fields.get_active_mut(self.input_mode) {
            match key_code {
                KeyCode::Left => input.move_cursor_left(),
                KeyCode::Right => input.move_cursor_right(),
                KeyCode::Home => input.move_cursor_to_start(),
                KeyCode::End => input.move_cursor_to_end(),
                KeyCode::Char(c) => input.insert_char(c),
                KeyCode::Backspace => input.delete_char_before_cursor(),
                KeyCode::Delete => input.delete_char_at_cursor(),
                KeyCode::Enter => return true,
                KeyCode::Esc => {
                    self.input_mode = InputMode::Normal;
                }
                _ => {}
            }
        }
        false
    }

    pub fn apply_current_input(&mut self) {
        match self.input_mode {
            InputMode::HideEdit => {
                self.apply_hide();
                if !self.input_fields.hide.has_error() {
                    self.input_mode = InputMode::Normal;
                }
            }
            InputMode::FilterEdit => {
                self.apply_filter();
                if !self.input_fields.filter.has_error() {
                    self.input_mode = InputMode::Normal;
                }
            }
            InputMode::HighlightEdit => {
                self.apply_highlight();
                if !self.input_fields.highlight.has_error() {
                    self.input_mode = InputMode::Normal;
                }
            }
            InputMode::LineStartEdit => {
                self.apply_line_start();
                if !self.input_fields.line_start.has_error() {
                    self.input_mode = InputMode::Normal;
                }
            }
            InputMode::Normal => {}
        }
    }

    pub fn get_display_content(&self, line: &LogLine) -> Result<String, String> {
        self.filter_state.apply_hide(&line.content)
    }

    fn matches_filter(&self, idx: usize) -> bool {
        if idx >= self.log_state.lines.len() {
            return false;
        }
        let line = &self.log_state.lines[idx];
        let content = self.get_display_content(line).unwrap_or_else(|_| line.content.clone());
        self.filter_state.matches_filter(&content)
    }

    fn save_state(&self) {
        let state = AppState {
            hide_input: self.input_fields.hide.text.clone(),
            filter_input: self.input_fields.filter.text.clone(),
            highlight_input: self.input_fields.highlight.text.clone(),
            wrap_lines: self.wrap_lines,
            line_start_regex: self.input_fields.line_start.text.clone(),
        };
        state.save();
    }

    pub fn apply_hide(&mut self) {
        if self.input_fields.hide.is_empty() {
            self.filter_state.hide_regex = None;
            self.input_fields.hide.clear_error();
        } else {
            match Regex::new(&self.input_fields.hide.text) {
                Ok(re) => {
                    self.filter_state.hide_regex = Some(re);
                    self.input_fields.hide.clear_error();
                }
                Err(e) => {
                    self.input_fields.hide.set_error(Some(e.to_string()));
                    return;
                }
            }
        }
        self.rebuild_filtered_indices();
        self.save_state();
    }

    pub fn apply_filter(&mut self) {
        if self.input_fields.filter.is_empty() {
            self.filter_state.filter_expr = None;
            self.input_fields.filter.clear_error();
        } else {
            match parse_filter(&self.input_fields.filter.text) {
                Ok(expr) => {
                    self.filter_state.filter_expr = Some(expr);
                    self.input_fields.filter.clear_error();
                }
                Err(e) => {
                    self.input_fields.filter.set_error(Some(e.to_string()));
                    return;
                }
            }
        }
        self.rebuild_filtered_indices();
        self.save_state();
    }

    pub fn apply_highlight(&mut self) {
        if self.input_fields.highlight.is_empty() {
            self.filter_state.highlight_expr = None;
            self.input_fields.highlight.clear_error();
        } else {
            match parse_filter(&self.input_fields.highlight.text) {
                Ok(expr) => {
                    self.filter_state.highlight_expr = Some(expr);
                    self.input_fields.highlight.clear_error();
                }
                Err(e) => {
                    self.input_fields.highlight.set_error(Some(e.to_string()));
                }
            }
        }
        self.save_state();
    }

    pub fn apply_line_start(&mut self) {
        if self.input_fields.line_start.is_empty() {
            self.input_fields.line_start.clear_error();
        } else {
            match Regex::new(&self.input_fields.line_start.text) {
                Ok(_) => {
                    self.input_fields.line_start.clear_error();
                }
                Err(e) => {
                    self.input_fields.line_start.set_error(Some(e.to_string()));
                    return;
                }
            }
        }
        self.save_state();
        self.status_message = Some("Line start regex saved. Restart to apply.".to_string());
    }

    fn rebuild_filtered_indices(&mut self) {
        self.log_state.filtered_indices.clear();
        for i in 0..self.log_state.lines.len() {
            if self.matches_filter(i) {
                self.log_state.filtered_indices.push(i);
            }
        }
        self.log_state.bottom_line_idx = 0;
    }

    pub fn clear(&mut self) {
        self.log_state.clear();
        self.status_message = Some("Cleared".to_string());
    }

    pub fn render_line(&mut self, line: &LogLine) -> Vec<(String, ratatui::style::Style)> {
        let content = match self.get_display_content(line) {
            Ok(c) => c,
            Err(e) => {
                self.input_fields.hide.set_error(Some(format!("Runtime error: {}", e)));
                line.content.clone()
            }
        };
        let spans = highlight_line(
            &content,
            self.filter_state.highlight_expr.as_ref(),
            true,
            true,
        );
        apply_highlights_ratatui(&content, &spans)
    }

    pub fn toggle_time(&mut self) {
        self.show_time = !self.show_time;
    }

    pub fn toggle_wrap(&mut self) {
        self.wrap_lines = !self.wrap_lines;
        self.save_state();
    }

    pub fn prefix_width(&self) -> usize {
        if self.show_time {
            PREFIX_WIDTH_WITH_TIME
        } else {
            PREFIX_WIDTH_WITHOUT_TIME
        }
    }
}
