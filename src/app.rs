use crate::constants::{PREFIX_WIDTH_WITHOUT_TIME, PREFIX_WIDTH_WITH_TIME};
use crate::filter::{parse_filter, FilterExpr};
use crate::highlight::{apply_highlights, highlight_line};
use crate::input::TextInput;
use crate::netinfo::{get_network_interfaces, InterfaceInfo};
use crate::source::SourceEvent;
use crate::state::AppState;
use chrono::Local;
use crossterm::event::KeyCode;
use fancy_regex::Regex;
use std::net::IpAddr;
use std::sync::mpsc::Receiver;

#[derive(Clone)]
pub struct LogLine {
    pub timestamp: String,
    pub content: String,
}

#[derive(Clone, Copy, PartialEq)]
pub enum InputMode {
    Normal,
    HideEdit,
    FilterEdit,
    HighlightEdit,
}

#[derive(Clone, Copy, PartialEq, Default)]
pub enum ListenDisplayMode {
    #[default]
    AddrPort,
    NcCommand,
}

#[derive(Clone)]
pub struct ListenAddrEntry {
    pub ip: IpAddr,
    pub is_v6: bool,
    #[allow(dead_code)]
    pub is_self_assigned: bool,
    pub row: u16,
}

pub struct LogState {
    pub lines: Vec<LogLine>,
    pub filtered_indices: Vec<usize>,
    pub bottom_line_idx: usize,
    pub follow_tail: bool,
}

impl Default for LogState {
    fn default() -> Self {
        Self {
            lines: Vec::new(),
            filtered_indices: Vec::new(),
            bottom_line_idx: 0,
            follow_tail: true,
        }
    }
}

impl LogState {
    pub fn add_line(&mut self, content: String) -> usize {
        let line = LogLine {
            timestamp: Local::now().format("%H:%M:%S").to_string(),
            content,
        };
        let idx = self.lines.len();
        self.lines.push(line);
        idx
    }

    pub fn clear(&mut self) {
        self.lines.clear();
        self.filtered_indices.clear();
        self.bottom_line_idx = 0;
    }

    pub fn scroll_up(&mut self, amount: usize) {
        if self.follow_tail {
            self.bottom_line_idx = self.filtered_indices.len().saturating_sub(1);
        }
        self.bottom_line_idx = self.bottom_line_idx.saturating_sub(amount);
        self.follow_tail = false;
    }

    pub fn scroll_down(&mut self, amount: usize) {
        let max_idx = self.filtered_indices.len().saturating_sub(1);
        if self.follow_tail {
            return;
        }
        self.bottom_line_idx = (self.bottom_line_idx + amount).min(max_idx);
        if self.bottom_line_idx >= max_idx {
            self.follow_tail = true;
        }
    }

    pub fn scroll_to_start(&mut self) {
        self.bottom_line_idx = 0;
        self.follow_tail = false;
    }

    pub fn scroll_to_end(&mut self) {
        self.follow_tail = true;
        self.bottom_line_idx = self.filtered_indices.len().saturating_sub(1);
    }

    pub fn get_bottom_line_idx(&self) -> usize {
        if self.follow_tail {
            self.filtered_indices.len().saturating_sub(1)
        } else {
            self.bottom_line_idx
                .min(self.filtered_indices.len().saturating_sub(1))
        }
    }
}

pub struct InputFields {
    pub hide: TextInput,
    pub filter: TextInput,
    pub highlight: TextInput,
}

impl InputFields {
    pub fn from_state(state: &AppState) -> Self {
        Self {
            hide: TextInput::new(state.hide_input.clone()),
            filter: TextInput::new(state.filter_input.clone()),
            highlight: TextInput::new(state.highlight_input.clone()),
        }
    }

    pub fn get_active_mut(&mut self, mode: InputMode) -> Option<&mut TextInput> {
        match mode {
            InputMode::HideEdit => Some(&mut self.hide),
            InputMode::FilterEdit => Some(&mut self.filter),
            InputMode::HighlightEdit => Some(&mut self.highlight),
            InputMode::Normal => None,
        }
    }
}

pub struct ListenState {
    pub port: Option<u16>,
    pub has_connection: bool,
    pub network_interfaces: Vec<InterfaceInfo>,
    pub display_mode: ListenDisplayMode,
    pub addr_list: Vec<ListenAddrEntry>,
    pub selected_idx: usize,
    pub popup_area: Option<(u16, u16, u16, u16)>,
}

impl ListenState {
    pub fn new(port: Option<u16>) -> Self {
        let network_interfaces = if port.is_some() {
            get_network_interfaces()
        } else {
            Vec::new()
        };
        Self {
            port,
            has_connection: false,
            network_interfaces,
            display_mode: ListenDisplayMode::default(),
            addr_list: Vec::new(),
            selected_idx: 0,
            popup_area: None,
        }
    }

    pub fn show_popup(&self) -> bool {
        self.port.is_some() && !self.has_connection
    }

    pub fn toggle_display_mode(&mut self) {
        self.display_mode = match self.display_mode {
            ListenDisplayMode::AddrPort => ListenDisplayMode::NcCommand,
            ListenDisplayMode::NcCommand => ListenDisplayMode::AddrPort,
        };
    }

    pub fn select_next(&mut self) {
        if !self.addr_list.is_empty() {
            self.selected_idx = (self.selected_idx + 1) % self.addr_list.len();
        }
    }

    pub fn select_prev(&mut self) {
        if !self.addr_list.is_empty() {
            self.selected_idx = self
                .selected_idx
                .checked_sub(1)
                .unwrap_or(self.addr_list.len() - 1);
        }
    }

    pub fn get_selected_copy_text(&self) -> Option<String> {
        let port = self.port?;
        let entry = self.addr_list.get(self.selected_idx)?;
        Some(match self.display_mode {
            ListenDisplayMode::AddrPort => {
                if entry.is_v6 {
                    format!("[{}]:{}", entry.ip, port)
                } else {
                    format!("{}:{}", entry.ip, port)
                }
            }
            ListenDisplayMode::NcCommand => {
                if entry.is_v6 {
                    format!("nc -6 {} {}", entry.ip, port)
                } else {
                    format!("nc {} {}", entry.ip, port)
                }
            }
        })
    }

    pub fn handle_click(&mut self, x: u16, y: u16) -> Option<String> {
        let (px, py, pw, ph) = self.popup_area?;
        if x < px || x >= px + pw || y < py || y >= py + ph {
            return None;
        }

        for (idx, entry) in self.addr_list.iter().enumerate() {
            if y == py + entry.row {
                self.selected_idx = idx;
                return self.get_selected_copy_text();
            }
        }
        None
    }
}

pub struct FilterState {
    pub hide_regex: Option<Regex>,
    pub filter_expr: Option<FilterExpr>,
    pub highlight_expr: Option<FilterExpr>,
}

impl Default for FilterState {
    fn default() -> Self {
        Self {
            hide_regex: None,
            filter_expr: None,
            highlight_expr: None,
        }
    }
}

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
                SourceEvent::Error(e) => {
                    self.status_message = Some(format!("Source error: {}", e));
                }
                SourceEvent::Connected(_peer) => {
                    self.listen_state.has_connection = true;
                }
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
            InputMode::Normal => {}
        }
    }

    pub fn get_display_content(&self, line: &LogLine) -> String {
        match &self.filter_state.hide_regex {
            Some(re) => {
                let content = &line.content;
                let mut ranges_to_remove: Vec<(usize, usize)> = Vec::new();
                let mut search_start = 0;

                while search_start < content.len() {
                    let hay = &content[search_start..];
                    match re.captures(hay) {
                        Ok(Some(caps)) => {
                            let full_match = caps.get(0).unwrap();
                            if caps.len() > 1 {
                                for i in 1..caps.len() {
                                    if let Some(group) = caps.get(i) {
                                        let abs_start = search_start + group.start();
                                        let abs_end = search_start + group.end();
                                        ranges_to_remove.push((abs_start, abs_end));
                                    }
                                }
                            } else {
                                let abs_start = search_start + full_match.start();
                                let abs_end = search_start + full_match.end();
                                ranges_to_remove.push((abs_start, abs_end));
                            }
                            search_start += search_start + full_match.end().max(1);
                            if search_start == 0 {
                                break;
                            }
                        }
                        _ => break,
                    }
                }

                if ranges_to_remove.is_empty() {
                    return content.clone();
                }

                ranges_to_remove.sort_by_key(|r| r.0);
                let mut merged: Vec<(usize, usize)> = Vec::new();
                for range in ranges_to_remove {
                    if let Some(last) = merged.last_mut() {
                        if range.0 <= last.1 {
                            last.1 = last.1.max(range.1);
                            continue;
                        }
                    }
                    merged.push(range);
                }

                let mut result = String::new();
                let mut pos = 0;
                for (start, end) in merged {
                    if start > pos && start <= content.len() {
                        result.push_str(&content[pos..start]);
                    }
                    pos = end.min(content.len());
                }
                if pos < content.len() {
                    result.push_str(&content[pos..]);
                }
                result
            }
            None => line.content.clone(),
        }
    }

    fn matches_filter(&self, idx: usize) -> bool {
        if idx >= self.log_state.lines.len() {
            return false;
        }
        let content = self.get_display_content(&self.log_state.lines[idx]);
        match &self.filter_state.filter_expr {
            Some(expr) => expr.matches(&content),
            None => true,
        }
    }

    fn save_state(&self) {
        let state = AppState {
            hide_input: self.input_fields.hide.text.clone(),
            filter_input: self.input_fields.filter.text.clone(),
            highlight_input: self.input_fields.highlight.text.clone(),
            wrap_lines: self.wrap_lines,
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

    pub fn render_line(&self, line: &LogLine) -> Vec<(String, ratatui::style::Style)> {
        let content = self.get_display_content(line);
        let spans = highlight_line(&content, self.filter_state.highlight_expr.as_ref(), true, true);
        apply_highlights(&content, &spans)
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
