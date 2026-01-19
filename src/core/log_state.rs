use chrono::{DateTime, Local};

#[derive(Clone, PartialEq)]
pub struct LogLine {
    pub timestamp: DateTime<Local>,
    pub content: String,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum TimeAge {
    VeryRecent,
    Recent,
    Minutes,
    Hours,
    Days,
}

pub fn format_relative_time(timestamp: DateTime<Local>) -> String {
    let now = Local::now();
    let duration = now.signed_duration_since(timestamp);
    
    let total_secs = duration.num_seconds();
    if total_secs < 0 {
        return "+0s".to_string();
    }
    
    if total_secs < 60 {
        format!("-{}s", total_secs)
    } else if total_secs < 3600 {
        format!("-{}m", total_secs / 60)
    } else if total_secs < 86400 {
        format!("-{}h", total_secs / 3600)
    } else {
        format!("-{}d", total_secs / 86400)
    }
}

pub fn get_time_age(timestamp: DateTime<Local>) -> TimeAge {
    let now = Local::now();
    let duration = now.signed_duration_since(timestamp);
    let total_secs = duration.num_seconds();
    
    if total_secs < 15 {
        TimeAge::VeryRecent
    } else if total_secs < 60 {
        TimeAge::Recent
    } else if total_secs < 3600 {
        TimeAge::Minutes
    } else if total_secs < 86400 {
        TimeAge::Hours
    } else {
        TimeAge::Days
    }
}

#[derive(Clone)]
pub struct LogState {
    pub lines: Vec<LogLine>,
    pub filtered_indices: Vec<usize>,
    pub bottom_line_idx: usize,
    pub follow_tail: bool,
    pub last_update_time: Option<DateTime<Local>>,
}

impl Default for LogState {
    fn default() -> Self {
        Self {
            lines: Vec::new(),
            filtered_indices: Vec::new(),
            bottom_line_idx: 0,
            follow_tail: true,
            last_update_time: None,
        }
    }
}

impl LogState {
    pub fn add_line(&mut self, content: String) -> usize {
        self.add_line_with_update(content, true)
    }

    pub fn add_line_with_update(&mut self, content: String, update_time: bool) -> usize {
        let now = Local::now();
        let line = LogLine {
            timestamp: now,
            content,
        };
        let idx = self.lines.len();
        self.lines.push(line);
        if update_time {
            self.last_update_time = Some(now);
        }
        idx
    }

    pub fn clear(&mut self) {
        self.lines.clear();
        self.filtered_indices.clear();
        self.bottom_line_idx = 0;
        self.last_update_time = None;
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
