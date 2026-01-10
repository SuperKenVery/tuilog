use crate::core::{FilterState, LogLine};
use crate::filter::{parse_filter, FilterExpr};
use crate::source::{start_source, LogSource, SourceEvent};
use crate::state::AppState;
use dioxus::html::geometry::WheelDelta;
use dioxus::prelude::*;
use fancy_regex::Regex;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tracing::info;

const BATCH_SIZE: usize = 100;
const UPDATE_INTERVAL_MS: u64 = 16;
const LINE_HEIGHT: f64 = 20.0;

#[derive(Clone)]
pub struct GuiAppState {
    pub lines: Vec<LogLine>,
    pub filtered_indices: Vec<usize>,
    pub filter_state: FilterState,
    pub follow_tail: bool,
    pub show_time: bool,
    pub hide_text: String,
    pub filter_text: String,
    pub highlight_text: String,
    pub hide_error: Option<String>,
    pub filter_error: Option<String>,
    pub status_message: Option<String>,
    pub scroll_y: f64,
    pub container_height: f64,
    pub version: u64,
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
            hide_text: state.hide_input.clone(),
            filter_text: state.filter_input.clone(),
            highlight_text: state.highlight_input.clone(),
            hide_error: None,
            filter_error: None,
            status_message: None,
            scroll_y: 0.0,
            container_height: 600.0,
            version: 0,
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

    pub fn get_display_content(&self, line: &LogLine) -> String {
        match &self.filter_state.hide_regex {
            Some(re) => {
                let content = &line.content;
                match re.replace_all(content, "") {
                    std::borrow::Cow::Borrowed(_) => content.clone(),
                    std::borrow::Cow::Owned(s) => s,
                }
            }
            None => line.content.clone(),
        }
    }

    fn matches_filter(&self, line: &LogLine) -> bool {
        let content = self.get_display_content(line);
        match &self.filter_state.filter_expr {
            Some(expr) => expr.matches(&content),
            None => true,
        }
    }

    fn rebuild_filtered_indices(&mut self) {
        self.filtered_indices.clear();
        for (i, line) in self.lines.iter().enumerate() {
            if self.matches_filter(line) {
                self.filtered_indices.push(i);
            }
        }
        self.clamp_scroll();
        self.version += 1;
    }

    fn save_state(&self) {
        let state = AppState {
            hide_input: self.hide_text.clone(),
            filter_input: self.filter_text.clone(),
            highlight_input: self.highlight_text.clone(),
            wrap_lines: false,
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
        } else {
            if let Ok(expr) = parse_filter(&self.highlight_text) {
                self.filter_state.highlight_expr = Some(expr);
            }
        }
        self.version += 1;
        self.save_state();
    }

    pub fn add_line(&mut self, content: String) {
        let line = LogLine {
            content: content
                .trim_end_matches('\n')
                .trim_end_matches('\r')
                .to_string(),
            timestamp: chrono::Local::now().format("%H:%M:%S%.3f").to_string(),
        };
        let idx = self.lines.len();
        let matches = self.matches_filter(&line);
        self.lines.push(line);
        if matches {
            self.filtered_indices.push(idx);
        }
    }

    pub fn clear(&mut self) {
        self.lines.clear();
        self.filtered_indices.clear();
        self.scroll_y = 0.0;
        self.version += 1;
    }

    pub fn total_height(&self) -> f64 {
        self.filtered_indices.len() as f64 * LINE_HEIGHT
    }

    pub fn max_scroll(&self) -> f64 {
        (self.total_height() - self.container_height).max(0.0)
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

#[derive(Props, Clone, PartialEq)]
pub struct GuiAppProps {
    pub file: Option<PathBuf>,
    pub port: Option<u16>,
}

fn highlight_content(content: &str, highlight_expr: &Option<FilterExpr>) -> Vec<(String, bool)> {
    let Some(expr) = highlight_expr else {
        return vec![(content.to_string(), false)];
    };

    let matches = expr.find_all_matches(content);
    if matches.is_empty() {
        return vec![(content.to_string(), false)];
    }

    let mut result = Vec::new();
    let mut last_end = 0;

    for (start, end) in matches {
        if start > last_end {
            result.push((content[last_end..start].to_string(), false));
        }
        if end > start {
            result.push((content[start..end].to_string(), true));
        }
        last_end = end;
    }

    if last_end < content.len() {
        result.push((content[last_end..].to_string(), false));
    }

    result
}

#[component]
pub fn GuiApp(props: GuiAppProps) -> Element {
    let mut app_state = use_signal(|| GuiAppState::new());
    let mut source_rx: Signal<Option<Arc<Mutex<Receiver<SourceEvent>>>>> = use_signal(|| None);

    use_effect({
        let file = props.file.clone();
        let port = props.port;
        move || {
            let (tx, rx) = mpsc::channel::<SourceEvent>();

            let source = if let Some(port) = port {
                LogSource::Network(port)
            } else if let Some(ref path) = file {
                LogSource::File(path.clone())
            } else {
                LogSource::Stdin
            };

            if let Err(e) = start_source(source, tx) {
                app_state.write().status_message = Some(format!("Failed to start source: {}", e));
            } else {
                source_rx.write().replace(Arc::new(Mutex::new(rx)));
            }
        }
    });

    use_future(move || async move {
        let mut last_update = Instant::now();
        let mut pending_lines: Vec<String> = Vec::new();

        loop {
            async_std::task::sleep(Duration::from_millis(UPDATE_INTERVAL_MS)).await;

            if let Some(ref rx_arc) = *source_rx.read() {
                if let Ok(rx) = rx_arc.lock() {
                    for _ in 0..BATCH_SIZE {
                        match rx.try_recv() {
                            Ok(SourceEvent::Line(content)) => {
                                pending_lines.push(content);
                            }
                            Ok(SourceEvent::Error(e)) => {
                                app_state.write().status_message = Some(format!("Error: {}", e));
                            }
                            Ok(SourceEvent::Connected(peer)) => {
                                app_state.write().status_message =
                                    Some(format!("Connected: {}", peer));
                            }
                            Err(TryRecvError::Empty) => break,
                            Err(TryRecvError::Disconnected) => break,
                        }
                    }
                }
            }

            if !pending_lines.is_empty()
                && last_update.elapsed() >= Duration::from_millis(UPDATE_INTERVAL_MS)
            {
                let lines_to_add: Vec<String> = pending_lines.drain(..).collect();
                let mut state = app_state.write();
                let was_at_bottom = state.follow_tail;
                for line in lines_to_add {
                    state.add_line(line);
                }
                if was_at_bottom {
                    state.scroll_to_bottom();
                }
                state.version += 1;
                last_update = Instant::now();
            }
        }
    });

    let state = app_state.read();
    let total_lines = state.lines.len();
    let filtered_count = state.filtered_indices.len();
    let scroll_y = state.scroll_y;
    let container_height = state.container_height;
    let follow_tail = state.follow_tail;
    let show_time = state.show_time;
    let hide_text = state.hide_text.clone();
    let filter_text = state.filter_text.clone();
    let highlight_text = state.highlight_text.clone();
    let hide_error = state.hide_error.clone();
    let filter_error = state.filter_error.clone();
    let status_message = state.status_message.clone();
    let highlight_expr = state.filter_state.highlight_expr.clone();
    let total_height = state.total_height();
    let max_scroll = state.max_scroll();
    let version = state.version;
    drop(state);

    let start_idx = (scroll_y / LINE_HEIGHT).floor() as usize;
    let visible_count = (container_height / LINE_HEIGHT).ceil() as usize + 2;
    let end_idx = (start_idx + visible_count).min(filtered_count);
    let top_offset = scroll_y % LINE_HEIGHT;

    let visible_lines: Vec<(usize, usize, LogLine, String)> = {
        let state = app_state.read();
        (start_idx..end_idx)
            .enumerate()
            .filter_map(|(view_idx, filter_idx)| {
                state
                    .filtered_indices
                    .get(filter_idx)
                    .and_then(|&line_idx| {
                        state.lines.get(line_idx).map(|line| {
                            let content = state.get_display_content(line);
                            (view_idx, line_idx, line.clone(), content)
                        })
                    })
            })
            .collect()
    };

    let scroll_thumb_height = if total_height > 0.0 {
        (container_height / total_height * container_height)
            .max(30.0)
            .min(container_height)
    } else {
        container_height
    };
    let scroll_thumb_top = if max_scroll > 0.0 {
        scroll_y / max_scroll * (container_height - scroll_thumb_height)
    } else {
        0.0
    };

    rsx! {
        style { {CSS} }
        div { class: "app",
            div { class: "toolbar",
                div { class: "filter-group",
                    label { "Hide:" }
                    input {
                        r#type: "text",
                        class: if hide_error.is_some() { "error" } else { "" },
                        placeholder: "regex to hide...",
                        value: "{hide_text}",
                        oninput: move |e| app_state.write().hide_text = e.value(),
                        onkeydown: move |e| {
                            if e.key() == Key::Enter {
                                app_state.write().apply_hide();
                            }
                        },
                    }
                }
                div { class: "filter-group",
                    label { "Filter:" }
                    input {
                        r#type: "text",
                        class: if filter_error.is_some() { "error" } else { "" },
                        placeholder: "filter expression...",
                        value: "{filter_text}",
                        oninput: move |e| app_state.write().filter_text = e.value(),
                        onkeydown: move |e| {
                            if e.key() == Key::Enter {
                                app_state.write().apply_filter();
                            }
                        },
                    }
                }
                div { class: "filter-group",
                    label { "Highlight:" }
                    input {
                        r#type: "text",
                        placeholder: "highlight expression...",
                        value: "{highlight_text}",
                        oninput: move |e| app_state.write().highlight_text = e.value(),
                        onkeydown: move |e| {
                            if e.key() == Key::Enter {
                                app_state.write().apply_highlight();
                            }
                        },
                    }
                }
                div { class: "toolbar-actions",
                    button {
                        class: if show_time { "active" } else { "" },
                        onclick: move |_| {
                            let mut s = app_state.write();
                            s.show_time = !s.show_time;
                            s.version += 1;
                        },
                        "Time"
                    }
                    button {
                        class: if follow_tail { "active" } else { "" },
                        onclick: move |_| {
                            let mut s = app_state.write();
                            s.follow_tail = !s.follow_tail;
                            if s.follow_tail {
                                s.scroll_to_bottom();
                            }
                            s.version += 1;
                        },
                        "Follow"
                    }
                    button {
                        onclick: move |_| {
                            app_state.write().clear();
                        },
                        "Clear"
                    }
                }
            }

            div { class: "log-wrapper",
                div {
                    class: "log-container",
                    tabindex: "0",
                    onmounted: move |e| async move {
                        if let Ok(rect) = e.get_client_rect().await {
                            let mut s = app_state.write();
                            s.container_height = rect.size.height;
                            s.clamp_scroll();
                        }
                    },
                    onwheel: move |e| {
                        e.prevent_default();
                        let wheel_delta = e.delta();
                        let delta_y = match wheel_delta {
                            WheelDelta::Pixels(p) => p.y,
                            WheelDelta::Lines(l) => l.y * LINE_HEIGHT,
                            WheelDelta::Pages(p) => p.y * container_height,
                        };
                        let mut s = app_state.write();
                        s.scroll_y += delta_y;
                        s.clamp_scroll();
                        s.follow_tail = s.is_at_bottom();
                        s.version += 1;
                    },
                    onkeydown: move |e| {
                        let mut s = app_state.write();
                        match e.key() {
                            Key::ArrowUp => {
                                s.scroll_y -= LINE_HEIGHT;
                                s.follow_tail = false;
                            }
                            Key::ArrowDown => {
                                s.scroll_y += LINE_HEIGHT;
                                s.follow_tail = s.is_at_bottom();
                            }
                            Key::PageUp => {
                                s.scroll_y -= s.container_height;
                                s.follow_tail = false;
                            }
                            Key::PageDown => {
                                s.scroll_y += s.container_height;
                                s.follow_tail = s.is_at_bottom();
                            }
                            Key::Home => {
                                s.scroll_y = 0.0;
                                s.follow_tail = false;
                            }
                            Key::End => {
                                s.scroll_to_bottom();
                                s.follow_tail = true;
                            }
                            _ => return,
                        }
                        s.clamp_scroll();
                        s.version += 1;
                    },
                    div {
                        class: "log-list",
                        key: "{version}",
                        style: "transform: translateY(-{top_offset}px);",
                        for (_view_idx, line_idx, line, content) in visible_lines {
                            div {
                                class: "log-line",
                                key: "{line_idx}",
                                if show_time {
                                    span { class: "timestamp", "{line.timestamp}" }
                                }
                                span { class: "line-num", "{line_idx + 1}" }
                                span { class: "content",
                                    for (text, is_highlight) in highlight_content(&content, &highlight_expr) {
                                        if is_highlight {
                                            mark { class: "highlight", "{text}" }
                                        } else {
                                            "{text}"
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                if total_height > container_height {
                    div { class: "scrollbar",
                        div {
                            class: "scrollbar-thumb",
                            style: "top: {scroll_thumb_top}px; height: {scroll_thumb_height}px;",
                        }
                    }
                }
            }

            div { class: "statusbar",
                span { class: "status-info",
                    "{filtered_count} / {total_lines} lines"
                    if follow_tail { " • Following" }
                }
                if let Some(msg) = status_message {
                    span { class: "status-msg", "{msg}" }
                }
            }
        }
    }
}

const CSS: &str = r#"
* {
    box-sizing: border-box;
    margin: 0;
    padding: 0;
}

.app {
    display: flex;
    flex-direction: column;
    height: 100vh;
    background: light-dark(#ffffff, #1e1e1e);
    color: light-dark(#1e1e1e, #d4d4d4);
    font-family: system-ui, -apple-system, sans-serif;
    font-size: 13px;
    color-scheme: light dark;
}

.toolbar {
    display: flex;
    gap: 16px;
    padding: 8px 12px;
    background: light-dark(#f3f3f3, #252526);
    border-bottom: 1px solid light-dark(#d4d4d4, #3c3c3c);
    flex-wrap: wrap;
    align-items: center;
}

.filter-group {
    display: flex;
    align-items: center;
    gap: 6px;
}

.filter-group label {
    color: light-dark(#616161, #858585);
    font-size: 12px;
    min-width: 55px;
}

.filter-group input {
    background: light-dark(#ffffff, #3c3c3c);
    border: 1px solid light-dark(#c4c4c4, #4c4c4c);
    border-radius: 3px;
    color: light-dark(#1e1e1e, #d4d4d4);
    padding: 4px 8px;
    font-size: 12px;
    width: 180px;
}

.filter-group input:focus {
    outline: none;
    border-color: #007acc;
}

.filter-group input.error {
    border-color: #f44747;
}

.toolbar-actions {
    display: flex;
    gap: 6px;
    margin-left: auto;
}

.toolbar-actions button {
    background: light-dark(#e0e0e0, #3c3c3c);
    border: 1px solid light-dark(#c4c4c4, #4c4c4c);
    border-radius: 3px;
    color: light-dark(#1e1e1e, #d4d4d4);
    padding: 4px 10px;
    font-size: 12px;
    cursor: pointer;
}

.toolbar-actions button:hover {
    background: light-dark(#d0d0d0, #4c4c4c);
}

.toolbar-actions button.active {
    background: #007acc;
    border-color: #007acc;
    color: #ffffff;
}

.log-wrapper {
    flex: 1;
    display: flex;
    overflow: hidden;
    position: relative;
}

.log-container {
    flex: 1;
    overflow: hidden;
    background: light-dark(#ffffff, #1e1e1e);
    outline: none;
    position: relative;
}

.log-container:focus {
    outline: none;
}

.log-list {
    position: relative;
    will-change: transform;
}

.log-line {
    display: flex;
    padding: 1px 12px;
    font-family: 'SF Mono', Menlo, Monaco, 'Courier New', monospace;
    font-size: 12px;
    height: 20px;
    line-height: 18px;
    white-space: nowrap;
}

.log-line:hover {
    background: light-dark(#f0f0f0, #2a2d2e);
}

.timestamp {
    color: light-dark(#098658, #6a9955);
    margin-right: 12px;
    flex-shrink: 0;
}

.line-num {
    color: light-dark(#858585, #858585);
    margin-right: 12px;
    min-width: 50px;
    text-align: right;
    flex-shrink: 0;
}

.content {
    color: light-dark(#1e1e1e, #d4d4d4);
    overflow: hidden;
    text-overflow: ellipsis;
}

.highlight {
    background: light-dark(#ffff00, #ffcc00);
    color: light-dark(#000000, #000000);
    padding: 0 2px;
    border-radius: 2px;
}

.scrollbar {
    width: 14px;
    background: light-dark(#f0f0f0, #1e1e1e);
    border-left: 1px solid light-dark(#d4d4d4, #3c3c3c);
    position: relative;
}

.scrollbar-thumb {
    position: absolute;
    width: 10px;
    left: 2px;
    background: light-dark(#c4c4c4, #5a5a5a);
    border-radius: 5px;
    min-height: 30px;
}

.scrollbar-thumb:hover {
    background: light-dark(#a0a0a0, #787878);
}

.statusbar {
    display: flex;
    justify-content: space-between;
    padding: 4px 12px;
    background: #007acc;
    color: #fff;
    font-size: 12px;
}

.status-info {
    opacity: 0.9;
}

.status-msg {
    opacity: 0.8;
}
"#;
