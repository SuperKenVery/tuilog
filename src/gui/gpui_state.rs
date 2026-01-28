#![cfg(feature = "gpui_gui")]
use crate::core::{FilterState, LogLine};
use crate::filter::parse_filter;
use crate::state::AppState;
use fancy_regex::Regex;

const LINE_HEIGHT: f64 = 20.0;

#[derive(Clone)]
pub struct LogViewerState {
    pub lines: Vec<LogLine>,
    pub filtered_indices: Vec<usize>,
    pub filter_state: FilterState,
    pub follow_tail: bool,
    pub show_time: bool,
    pub wrap_lines: bool,
    pub hide_text: String,
    pub filter_text: String,
    pub highlight_text: String,
    pub line_start_text: String,
    pub hide_error: Option<String>,
    pub filter_error: Option<String>,
    pub line_start_error: Option<String>,
    pub status_message: Option<String>,
    pub is_connected: bool,
    pub line_heights: Vec<f64>,
    pub line_offsets: Vec<f64>,
    pub scroll_y: f64,
    pub container_height: f64,
}

impl LogViewerState {
    pub fn new() -> Self {
        let state = AppState::load();
        let mut s = Self {
            lines: Vec::new(),
            filtered_indices: Vec::new(),
            filter_state: FilterState::default(),
            follow_tail: true,
            show_time: true,
            wrap_lines: state.wrap_lines,
            hide_text: state.hide_input.clone(),
            filter_text: state.filter_input.clone(),
            highlight_text: state.highlight_input.clone(),
            line_start_text: state.line_start_regex.clone(),
            hide_error: None,
            filter_error: None,
            line_start_error: None,
            status_message: None,
            is_connected: false,
            line_heights: Vec::new(),
            line_offsets: vec![0.0],
            scroll_y: 0.0,
            container_height: 600.0,
        };
        if !s.hide_text.trim().is_empty() {
            if let Ok(re) = Regex::new(&s.hide_text) {
                s.filter_state.hide_regex = Some(re);
            }
        }
        if !s.filter_text.trim().is_empty() {
            if let Ok(expr) = parse_filter(&s.filter_text) {
                s.filter_state.filter_expr = Some(expr);
            }
        }
        if !s.highlight_text.trim().is_empty() {
            if let Ok(expr) = parse_filter(&s.highlight_text) {
                s.filter_state.highlight_expr = Some(expr);
            }
        }
        s
    }

    fn matches_filter(&self, line: &LogLine) -> bool {
        let content = self
            .filter_state
            .apply_hide(&line.content)
            .unwrap_or_else(|_| line.content.clone());
        self.filter_state.matches_filter(&content)
    }

    fn rebuild_filtered_indices(&mut self) {
        self.filtered_indices.clear();
        for (i, line) in self.lines.iter().enumerate() {
            if self.matches_filter(line) {
                self.filtered_indices.push(i);
            }
        }
        self.reset_line_heights();
    }

    fn save_state(&self) {
        let state = AppState {
            hide_input: self.hide_text.clone(),
            filter_input: self.filter_text.clone(),
            highlight_input: self.highlight_text.clone(),
            wrap_lines: self.wrap_lines,
            line_start_regex: self.line_start_text.clone(),
        };
        state.save();
    }

    pub fn apply_hide(&mut self) {
        if self.hide_text.trim().is_empty() {
            self.filter_state.hide_regex = None;
            self.hide_error = None;
        } else {
            match Regex::new(&self.hide_text) {
                Ok(re) => {
                    self.filter_state.hide_regex = Some(re);
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
        if self.filter_text.trim().is_empty() {
            self.filter_state.filter_expr = None;
            self.filter_error = None;
        } else {
            match parse_filter(&self.filter_text) {
                Ok(expr) => {
                    self.filter_state.filter_expr = Some(expr);
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
        if self.highlight_text.trim().is_empty() {
            self.filter_state.highlight_expr = None;
        } else if let Ok(expr) = parse_filter(&self.highlight_text) {
            self.filter_state.highlight_expr = Some(expr);
        }
        self.save_state();
    }

    pub fn apply_line_start(&mut self) {
        if self.line_start_text.trim().is_empty() {
            self.line_start_error = None;
        } else {
            match Regex::new(&self.line_start_text) {
                Ok(_) => {
                    self.line_start_error = None;
                }
                Err(e) => {
                    self.line_start_error = Some(e.to_string());
                    return;
                }
            }
        }
        self.save_state();
        self.status_message = Some("Line start regex saved. Restart to apply.".to_string());
    }

    pub fn add_line(&mut self, content: String, timestamp: chrono::DateTime<chrono::Local>) {
        let line = LogLine {
            content: content.trim_end_matches(['\n', '\r']).to_string(),
            timestamp,
        };
        let idx = self.lines.len();
        let matches = self.matches_filter(&line);
        self.lines.push(line);
        if matches {
            self.filtered_indices.push(idx);
            let current_total = *self.line_offsets.last().unwrap_or(&0.0);
            self.line_heights.push(LINE_HEIGHT);
            self.line_offsets.push(current_total + LINE_HEIGHT);
        }
    }

    pub fn clear(&mut self) {
        self.lines.clear();
        self.filtered_indices.clear();
        self.status_message = None;
        self.line_heights.clear();
        self.line_offsets.clear();
        self.line_offsets.push(0.0);
        self.scroll_y = 0.0;
    }

    fn reset_line_heights(&mut self) {
        let count = self.filtered_indices.len();
        self.line_heights = vec![LINE_HEIGHT; count];
        self.rebuild_offsets();
    }

    fn rebuild_offsets(&mut self) {
        self.line_offsets.clear();
        let mut offset = 0.0;
        for &h in &self.line_heights {
            self.line_offsets.push(offset);
            offset += h;
        }
        self.line_offsets.push(offset);
    }

    pub fn total_height(&self) -> f64 {
        *self.line_offsets.last().unwrap_or(&0.0)
    }

    pub fn find_visible_range(&self, scroll_y: f64, viewport_height: f64) -> (usize, usize) {
        let start = self
            .line_offsets
            .partition_point(|&o| o <= scroll_y)
            .saturating_sub(1);
        let end_scroll = scroll_y + viewport_height;
        let end = self
            .line_offsets
            .partition_point(|&o| o < end_scroll)
            .min(self.filtered_indices.len());
        (start, end)
    }
}
