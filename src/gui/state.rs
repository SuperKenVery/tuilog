use crate::core::{FilterState, LogLine};
use crate::filter::{parse_filter, FilterExpr};
use crate::highlight::{apply_highlights, highlight_line, HighlightStyle};
use crate::state::AppState;
use fancy_regex::Regex;

const LINE_HEIGHT: f64 = 20.0;

pub fn highlight_content(content: &str, highlight_expr: &Option<FilterExpr>) -> Vec<(String, HighlightStyle)> {
    let spans = highlight_line(content, highlight_expr.as_ref(), true, true);
    apply_highlights(content, &spans)
}

#[derive(Clone)]
pub struct GuiAppState {
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
    pub scroll_y: f64,
    pub scroll_x: f64,
    pub container_height: f64,
    pub container_width: f64,
    pub max_content_width: f64,
    pub version: u64,
    pub line_heights: Vec<f64>,
    pub line_offsets: Vec<f64>,
    pub last_update_time: Option<chrono::DateTime<chrono::Local>>,
}

impl GuiAppState {
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
            scroll_y: 0.0,
            scroll_x: 0.0,
            container_height: 600.0,
            container_width: 800.0,
            max_content_width: 0.0,
            version: 0,
            line_heights: Vec::new(),
            line_offsets: Vec::new(),
            last_update_time: None,
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

    pub fn get_display_content(&self, line: &LogLine) -> Result<String, String> {
        self.filter_state.apply_hide(&line.content)
    }

    fn matches_filter(&self, line: &LogLine) -> bool {
        let content = self.get_display_content(line).unwrap_or_else(|_| line.content.clone());
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
        self.clamp_scroll();
        self.version += 1;
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
        self.line_offsets.last().copied().unwrap_or(0.0)
    }

    pub fn set_line_height(&mut self, filtered_idx: usize, height: f64) {
        if filtered_idx < self.line_heights.len() && (self.line_heights[filtered_idx] - height).abs() > 0.5 {
            self.line_heights[filtered_idx] = height;
            self.rebuild_offsets();
            self.version += 1;
        }
    }

    pub fn find_visible_range(&self, scroll_y: f64, viewport_height: f64) -> (usize, usize) {
        let start = self.line_offsets.partition_point(|&o| o <= scroll_y).saturating_sub(1);
        let end_scroll = scroll_y + viewport_height;
        let end = self.line_offsets.partition_point(|&o| o < end_scroll).min(self.filtered_indices.len());
        (start, end)
    }

    pub fn get_line_offset(&self, filtered_idx: usize) -> f64 {
        self.line_offsets.get(filtered_idx).copied().unwrap_or(0.0)
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

    pub fn max_scroll_x(&self) -> f64 {
        (self.max_content_width - self.container_width).max(0.0)
    }

    pub fn clamp_scroll_x(&mut self) {
        let max = self.max_scroll_x();
        self.scroll_x = self.scroll_x.clamp(0.0, max);
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
        } else {
            if let Ok(expr) = parse_filter(&self.highlight_text) {
                self.filter_state.highlight_expr = Some(expr);
            }
        }
        self.version += 1;
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

    pub fn add_line(&mut self, content: String) {
        self.add_line_with_update(content, true);
    }

    pub fn add_line_with_update(&mut self, content: String, update_time: bool) {
        let now = chrono::Local::now();
        let line = LogLine {
            content: content
                .trim_end_matches('\n')
                .trim_end_matches('\r')
                .to_string(),
            timestamp: now,
        };
        let idx = self.lines.len();
        let matches = self.matches_filter(&line);
        let estimated_width = self.estimate_line_width(&line);
        if estimated_width > self.max_content_width {
            self.max_content_width = estimated_width;
        }
        self.lines.push(line);
        if update_time {
            self.last_update_time = Some(now);
        }
        if matches {
            self.filtered_indices.push(idx);
            if self.line_offsets.is_empty() {
                self.line_offsets.push(0.0);
            }
            let current_total = self.line_offsets.last().copied().unwrap_or(0.0);
            self.line_heights.push(LINE_HEIGHT);
            self.line_offsets.push(current_total + LINE_HEIGHT);
        }
    }

    fn estimate_line_width(&self, line: &LogLine) -> f64 {
        let content = self.get_display_content(line).unwrap_or_else(|_| line.content.clone());
        let char_width = 7.2;
        let timestamp_width = if self.show_time { 32.0 } else { 0.0 };
        let line_num_width = 62.0;
        let padding = 24.0;
        timestamp_width + line_num_width + (content.len() as f64 * char_width) + padding
    }

    pub fn clear(&mut self) {
        self.lines.clear();
        self.filtered_indices.clear();
        self.line_heights.clear();
        self.line_offsets.clear();
        self.line_offsets.push(0.0);
        self.scroll_y = 0.0;
        self.scroll_x = 0.0;
        self.max_content_width = 0.0;
        self.version += 1;
        self.last_update_time = None;
    }

    pub fn max_scroll(&self) -> f64 {
        (self.total_height() - self.container_height + LINE_HEIGHT).max(0.0)
    }

    pub fn clamp_scroll(&mut self) {
        let max = self.max_scroll();
        self.scroll_y = self.scroll_y.clamp(0.0, max);
    }

    pub fn scroll_to_bottom(&mut self) {
        self.scroll_y = self.max_scroll();
    }

    pub fn is_at_bottom(&self) -> bool {
        self.scroll_y >= self.max_scroll() - 1.0
    }
}
