use crate::filter::{parse_filter, FilterExpr};
use crate::highlight::{apply_highlights, highlight_line};
use crate::netinfo::{get_network_interfaces, InterfaceInfo};
use crate::source::SourceEvent;
use crate::state::AppState;
use chrono::Local;
use fancy_regex::Regex;
use std::net::IpAddr;
use std::sync::mpsc::Receiver;

#[derive(Clone)]
pub struct LogLine {
    pub timestamp: String,
    pub content: String,
}

pub struct App {
    pub lines: Vec<LogLine>,
    pub filtered_indices: Vec<usize>,
    pub bottom_line_idx: usize,
    pub hide_input: String,
    pub hide_cursor: usize,
    pub hide_regex: Option<Regex>,
    pub hide_error: Option<String>,
    pub filter_input: String,
    pub filter_cursor: usize,
    pub filter_expr: Option<FilterExpr>,
    pub filter_error: Option<String>,
    pub highlight_input: String,
    pub highlight_cursor: usize,
    pub highlight_expr: Option<FilterExpr>,
    pub highlight_error: Option<String>,
    pub show_time: bool,
    pub heuristic_highlight: bool,
    pub json_highlight: bool,
    pub wrap_lines: bool,
    pub input_mode: InputMode,
    pub follow_tail: bool,
    pub source_rx: Receiver<SourceEvent>,
    pub status_message: Option<String>,
    pub listen_port: Option<u16>,
    pub has_connection: bool,
    pub network_interfaces: Vec<InterfaceInfo>,
    pub listen_display_mode: ListenDisplayMode,
    pub listen_addr_list: Vec<ListenAddrEntry>,
    pub listen_selected_idx: usize,
    pub listen_popup_area: Option<(u16, u16, u16, u16)>,
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

#[derive(Clone, Copy, PartialEq)]
pub enum InputMode {
    Normal,
    HideEdit,
    FilterEdit,
    HighlightEdit,
}

impl App {
    pub fn new(source_rx: Receiver<SourceEvent>, listen_port: Option<u16>) -> Self {
        let state = AppState::load();
        let hide_cursor = state.hide_input.chars().count();
        let filter_cursor = state.filter_input.chars().count();
        let highlight_cursor = state.highlight_input.chars().count();
        let network_interfaces = if listen_port.is_some() {
            get_network_interfaces()
        } else {
            Vec::new()
        };
        let mut app = Self {
            lines: Vec::new(),
            filtered_indices: Vec::new(),
            bottom_line_idx: 0,
            hide_input: state.hide_input,
            hide_cursor,
            hide_regex: None,
            hide_error: None,
            filter_input: state.filter_input,
            filter_cursor,
            filter_expr: None,
            filter_error: None,
            highlight_input: state.highlight_input,
            highlight_cursor,
            highlight_expr: None,
            highlight_error: None,
            show_time: true,
            heuristic_highlight: true,
            json_highlight: true,
            wrap_lines: false,
            input_mode: InputMode::Normal,
            follow_tail: true,
            source_rx,
            status_message: None,
            listen_port,
            has_connection: false,
            network_interfaces,
            listen_display_mode: ListenDisplayMode::default(),
            listen_addr_list: Vec::new(),
            listen_selected_idx: 0,
            listen_popup_area: None,
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
                    let line = LogLine {
                        timestamp: Local::now().format("%H:%M:%S").to_string(),
                        content,
                    };
                    let idx = self.lines.len();
                    self.lines.push(line);
                    if self.matches_filter(idx) {
                        self.filtered_indices.push(idx);
                    }
                }
                SourceEvent::Error(e) => {
                    self.status_message = Some(format!("Source error: {}", e));
                }
                SourceEvent::Connected(_peer) => {
                    self.has_connection = true;
                }
            }
        }
    }

    pub fn show_listen_popup(&self) -> bool {
        self.listen_port.is_some() && !self.has_connection
    }

    pub fn toggle_listen_display_mode(&mut self) {
        self.listen_display_mode = match self.listen_display_mode {
            ListenDisplayMode::AddrPort => ListenDisplayMode::NcCommand,
            ListenDisplayMode::NcCommand => ListenDisplayMode::AddrPort,
        };
    }

    pub fn listen_select_next(&mut self) {
        if !self.listen_addr_list.is_empty() {
            self.listen_selected_idx = (self.listen_selected_idx + 1) % self.listen_addr_list.len();
        }
    }

    pub fn listen_select_prev(&mut self) {
        if !self.listen_addr_list.is_empty() {
            self.listen_selected_idx = self.listen_selected_idx
                .checked_sub(1)
                .unwrap_or(self.listen_addr_list.len() - 1);
        }
    }

    pub fn get_selected_copy_text(&self) -> Option<String> {
        let port = self.listen_port?;
        let entry = self.listen_addr_list.get(self.listen_selected_idx)?;
        Some(match self.listen_display_mode {
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

    pub fn handle_listen_popup_click(&mut self, x: u16, y: u16) -> Option<String> {
        let (px, py, pw, ph) = self.listen_popup_area?;
        if x < px || x >= px + pw || y < py || y >= py + ph {
            return None;
        }

        for (idx, entry) in self.listen_addr_list.iter().enumerate() {
            if y == py + entry.row {
                self.listen_selected_idx = idx;
                return self.get_selected_copy_text();
            }
        }
        None
    }

    pub fn get_display_content(&self, line: &LogLine) -> String {
        match &self.hide_regex {
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
        if idx >= self.lines.len() {
            return false;
        }
        let content = self.get_display_content(&self.lines[idx]);
        match &self.filter_expr {
            Some(expr) => expr.matches(&content),
            None => true,
        }
    }

    fn save_state(&self) {
        let state = AppState {
            hide_input: self.hide_input.clone(),
            filter_input: self.filter_input.clone(),
            highlight_input: self.highlight_input.clone(),
        };
        state.save();
    }

    pub fn apply_hide(&mut self) {
        if self.hide_input.trim().is_empty() {
            self.hide_regex = None;
            self.hide_error = None;
        } else {
            match Regex::new(&self.hide_input) {
                Ok(re) => {
                    self.hide_regex = Some(re);
                    self.hide_error = None;
                }
                Err(e) => {
                    self.hide_error = Some(e.to_string());
                    return;
                }
            }
        }
        self.rebuild_filtered_indices();
        self.save_state();
    }

    pub fn apply_filter(&mut self) {
        if self.filter_input.trim().is_empty() {
            self.filter_expr = None;
            self.filter_error = None;
        } else {
            match parse_filter(&self.filter_input) {
                Ok(expr) => {
                    self.filter_expr = Some(expr);
                    self.filter_error = None;
                }
                Err(e) => {
                    self.filter_error = Some(e.to_string());
                    return;
                }
            }
        }
        self.rebuild_filtered_indices();
        self.save_state();
    }

    pub fn apply_highlight(&mut self) {
        if self.highlight_input.trim().is_empty() {
            self.highlight_expr = None;
            self.highlight_error = None;
        } else {
            match parse_filter(&self.highlight_input) {
                Ok(expr) => {
                    self.highlight_expr = Some(expr);
                    self.highlight_error = None;
                }
                Err(e) => {
                    self.highlight_error = Some(e.to_string());
                }
            }
        }
        self.save_state();
    }

    fn rebuild_filtered_indices(&mut self) {
        self.filtered_indices.clear();
        for i in 0..self.lines.len() {
            if self.matches_filter(i) {
                self.filtered_indices.push(i);
            }
        }
        self.bottom_line_idx = 0;
    }

    pub fn clear(&mut self) {
        self.lines.clear();
        self.filtered_indices.clear();
        self.bottom_line_idx = 0;
        self.status_message = Some("Cleared".to_string());
    }

    pub fn scroll_up(&mut self, amount: usize, _visible_height: usize) {
        if self.follow_tail {
            self.bottom_line_idx = self.filtered_indices.len().saturating_sub(1);
        }
        self.bottom_line_idx = self.bottom_line_idx.saturating_sub(amount);
        self.follow_tail = false;
    }

    pub fn scroll_down(&mut self, amount: usize, _visible_height: usize) {
        let max_idx = self.filtered_indices.len().saturating_sub(1);
        if self.follow_tail {
            return;
        }
        self.bottom_line_idx = (self.bottom_line_idx + amount).min(max_idx);
        if self.bottom_line_idx >= max_idx {
            self.follow_tail = true;
        }
    }

    pub fn scroll_to_end(&mut self, _visible_height: usize) {
        self.follow_tail = true;
        self.bottom_line_idx = self.filtered_indices.len().saturating_sub(1);
    }

    pub fn scroll_to_start(&mut self, _visible_height: usize) {
        self.bottom_line_idx = 0;
        self.follow_tail = false;
    }

    pub fn get_bottom_line_idx(&self) -> usize {
        if self.follow_tail {
            self.filtered_indices.len().saturating_sub(1)
        } else {
            self.bottom_line_idx.min(self.filtered_indices.len().saturating_sub(1))
        }
    }

    pub fn render_line(&self, line: &LogLine) -> Vec<(String, ratatui::style::Style)> {
        let content = self.get_display_content(line);
        let spans = highlight_line(
            &content,
            self.highlight_expr.as_ref(),
            self.heuristic_highlight,
            self.json_highlight,
        );
        apply_highlights(&content, &spans)
    }

    pub fn toggle_time(&mut self) {
        self.show_time = !self.show_time;
    }

    pub fn toggle_heuristic(&mut self) {
        self.heuristic_highlight = !self.heuristic_highlight;
    }

    pub fn toggle_json(&mut self) {
        self.json_highlight = !self.json_highlight;
    }

    pub fn toggle_wrap(&mut self) {
        self.wrap_lines = !self.wrap_lines;
    }
}
