use crate::core::{format_relative_time, FilterState, ListenDisplayMode, ListenState, LogLine};
use crate::filter::{parse_filter, FilterExpr};
use crate::highlight::{apply_highlights, highlight_line, HighlightStyle};
use crate::source::{start_source, LogSource, SourceEvent};
use crate::state::AppState;
use async_channel::Receiver;
use dioxus::html::MountedData;
use dioxus::prelude::*;
use std::rc::Rc;
use fancy_regex::Regex;
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::{Duration, Instant};

// =============================================================================
// VIRTUAL SCROLLING IMPLEMENTATION
// =============================================================================
//
// This module implements virtual scrolling to efficiently render large log files.
// Instead of rendering all lines in the DOM (which would be slow for 100k+ lines),
// we only render the lines currently visible in the viewport.
//
// ## Core Data Structures
//
// - `line_heights: Vec<f64>` - Stores the actual rendered height of each filtered line.
//   Initially set to LINE_HEIGHT (20px), updated when lines are measured after mount.
//
// - `line_offsets: Vec<f64>` - Cumulative Y positions. `line_offsets[i]` is the pixel
//   position where line `i` starts. Has length = filtered_count + 1, where the last
//   element is the total content height.
//
// ## How It Works
//
// 1. **Finding visible lines**: Use binary search on `line_offsets` to find which
//    lines intersect the current viewport (scroll_y to scroll_y + container_height).
//
// 2. **Rendering**: Only render the visible lines. Each line is absolutely positioned
//    at its calculated Y offset from `line_offsets`.
//
// 3. **Height measurement**: When a line is first rendered, `onmounted` callback
//    measures its actual height. If different from the stored height (e.g., line
//    wraps to multiple visual lines), we update `line_heights` and rebuild offsets.
//
// 4. **Scroll handling**: Native browser scroll is used (`overflow-y: auto`).
//    The `onscroll` event updates our scroll_y state. We only trigger re-renders
//    when the visible line range changes, not on every scroll pixel.
//
// ## Key Design Decisions
//
// - **Absolute positioning**: Each line has `position: absolute; top: {offset}px`.
//   This allows lines to have variable heights without affecting each other.
//
// - **Fixed container height**: The `.log-list` container has a fixed height equal
//   to `total_height()` (sum of all line heights). This gives the scrollbar the
//   correct size even though most lines aren't rendered.
//
// - **Native scroll**: We use the browser's native scrolling instead of manual
//   scroll handling. This provides smooth scrolling, momentum, and accessibility.
//
// ## Pitfalls & Edge Cases
//
// 1. **Initial height estimation**: New lines start with LINE_HEIGHT (20px). If a
//    line wraps, it will briefly overlap with the next line until `onmounted`
//    measures the actual height and triggers a re-render.
//
// 2. **Height changes cause re-renders**: When `set_line_height` detects a height
//    change, it increments `version` to trigger re-render. This can cause a brief
//    visual jump as positions are recalculated.
//
// 3. **Filter changes reset heights**: When filters change, `rebuild_filtered_indices`
//    calls `reset_line_heights`, resetting all heights to LINE_HEIGHT. Previously
//    measured heights are lost and must be re-measured.
//
// 4. **Adding new lines**: `add_line` must update both `line_heights` and
//    `line_offsets` when a new line matches the filter. Forgetting this causes
//    new lines to not appear until a re-render is triggered elsewhere.
//
// 5. **Wrap mode complexity**: In wrap mode, line heights vary based on content
//    length and container width. Resizing the window invalidates all measured
//    heights (not currently handled - would need resize observer).
//
// 6. **Memory usage**: We store height/offset for every filtered line. For very
//    large logs (millions of lines), this could use significant memory.
//
// =============================================================================

const LINE_HEIGHT: f64 = 20.0;

const BASE_RENDER_THRESHOLD_MS: f64 = 50.0;
const MIN_RENDER_THRESHOLD_MS: f64 = 5.0;
const THRESHOLD_DECAY_FACTOR: f64 = 0.7;

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
    pub hide_error: Option<String>,
    pub filter_error: Option<String>,
    pub status_message: Option<String>,
    pub scroll_y: f64,
    pub scroll_x: f64,
    pub container_height: f64,
    pub container_width: f64,
    pub max_content_width: f64,
    pub version: u64,
    pub line_heights: Vec<f64>,
    pub line_offsets: Vec<f64>,
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
            hide_error: None,
            filter_error: None,
            status_message: None,
            scroll_y: 0.0,
            scroll_x: 0.0,
            container_height: 600.0,
            container_width: 800.0,
            max_content_width: 0.0,
            version: 0,
            line_heights: Vec::new(),
            line_offsets: Vec::new(),
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
                            search_start += full_match.end().max(1);
                        }
                        Ok(None) => break,
                        Err(e) => return Err(e.to_string()),
                    }
                }

                if ranges_to_remove.is_empty() {
                    return Ok(content.clone());
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
                Ok(result)
            }
            None => Ok(line.content.clone()),
        }
    }

    fn matches_filter(&self, line: &LogLine) -> bool {
        let content = self.get_display_content(line).unwrap_or_else(|_| line.content.clone());
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

    pub fn add_line(&mut self, content: String) {
        let line = LogLine {
            content: content
                .trim_end_matches('\n')
                .trim_end_matches('\r')
                .to_string(),
            timestamp: chrono::Local::now(),
        };
        let idx = self.lines.len();
        let matches = self.matches_filter(&line);
        let estimated_width = self.estimate_line_width(&line);
        if estimated_width > self.max_content_width {
            self.max_content_width = estimated_width;
        }
        self.lines.push(line);
        if matches {
            self.filtered_indices.push(idx);
            let current_total = self.line_offsets.last().copied().unwrap_or(0.0);
            self.line_heights.push(LINE_HEIGHT);
            self.line_offsets.push(current_total + LINE_HEIGHT);
        }
    }

    fn estimate_line_width(&self, line: &LogLine) -> f64 {
        let content = self.get_display_content(line).unwrap_or_else(|_| line.content.clone());
        let char_width = 7.2;
        let timestamp_width = if self.show_time { 80.0 } else { 0.0 };
        let line_num_width = 62.0;
        let padding = 24.0;
        timestamp_width + line_num_width + (content.len() as f64 * char_width) + padding
    }

    pub fn clear(&mut self) {
        self.lines.clear();
        self.filtered_indices.clear();
        self.scroll_y = 0.0;
        self.scroll_x = 0.0;
        self.max_content_width = 0.0;
        self.version += 1;
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

#[derive(Props, Clone, PartialEq)]
pub struct GuiAppProps {
    pub file: Option<PathBuf>,
    pub port: Option<u16>,
}
fn highlight_content(content: &str, highlight_expr: &Option<FilterExpr>) -> Vec<(String, HighlightStyle)> {
    let spans = highlight_line(content, highlight_expr.as_ref(), true, true);
    apply_highlights(content, &spans)
}

fn format_addr_display(ip: &std::net::IpAddr, port: u16, is_v6: bool, mode: ListenDisplayMode) -> String {
    match mode {
        ListenDisplayMode::AddrPort => {
            if is_v6 {
                format!("[{}]:{}", ip, port)
            } else {
                format!("{}:{}", ip, port)
            }
        }
        ListenDisplayMode::NcCommand => {
            if is_v6 {
                format!("nc -6 {} {}", ip, port)
            } else {
                format!("nc {} {}", ip, port)
            }
        }
    }
}

fn get_copy_text_from_interfaces(state: &ListenState) -> Option<String> {
    let port = state.port?;
    let mut addr_idx = 0usize;
    for iface in &state.network_interfaces {
        for addr_info in &iface.addresses {
            if addr_idx == state.selected_idx {
                let is_v6 = addr_info.ip.is_ipv6();
                return Some(format_addr_display(&addr_info.ip, port, is_v6, state.display_mode));
            }
            addr_idx += 1;
        }
    }
    None
}

#[component]
fn ListenPopup(listen_state: Signal<ListenState>) -> Element {
    let state = listen_state.read();
    let port = state.port.unwrap_or(0);
    let interfaces = state.network_interfaces.clone();
    let display_mode = state.display_mode;
    let selected_idx = state.selected_idx;
    drop(state);

    let mut addr_idx = 0usize;
    let mode_str = match display_mode {
        ListenDisplayMode::AddrPort => "[addr:port]  nc command",
        ListenDisplayMode::NcCommand => " addr:port  [nc command]",
    };

    rsx! {
        div { class: "popup-overlay",
            tabindex: "0",
            onclick: move |_| {},
            onkeydown: move |e| {
                match e.key() {
                    Key::Tab => {
                        listen_state.write().toggle_display_mode();
                    }
                    Key::ArrowUp => {
                        listen_state.write().select_prev();
                    }
                    Key::ArrowDown => {
                        listen_state.write().select_next();
                    }
                    Key::Enter => {
                        if let Some(text) = get_copy_text_from_interfaces(&listen_state.read()) {
                            #[cfg(target_os = "macos")]
                            {
                                let _ = std::process::Command::new("pbcopy")
                                    .stdin(std::process::Stdio::piped())
                                    .spawn()
                                    .and_then(|mut child| {
                                        use std::io::Write;
                                        if let Some(stdin) = child.stdin.as_mut() {
                                            stdin.write_all(text.as_bytes())?;
                                        }
                                        child.wait()
                                    });
                            }
                        }
                    }
                    _ => {}
                }
            },
            div { class: "popup",
                onclick: move |e| e.stop_propagation(),
                div { class: "popup-header",
                    span { "Listening on port " }
                    span { class: "popup-port", "{port}" }
                }
                div { class: "popup-mode",
                    span { class: "popup-label", "Mode (Tab): " }
                    span { class: "popup-mode-value", "{mode_str}" }
                }
                div { class: "popup-hint", "↑↓:Select  Enter/Click:Copy" }
                div { class: "popup-interfaces",
                    if interfaces.is_empty() {
                        div { class: "popup-error", "No network interfaces found" }
                    } else {
                        for iface in interfaces.iter() {
                            div { class: "popup-interface",
                                div {
                                    class: if iface.is_default { "popup-iface-name default" } else { "popup-iface-name" },
                                    "{iface.name}"
                                    if iface.is_default { " (default)" }
                                }
                                for addr_info in iface.addresses.iter() {
                                    {
                                        let current_idx = addr_idx;
                                        addr_idx += 1;
                                        let is_selected = current_idx == selected_idx;
                                        let ip = addr_info.ip;
                                        let is_v6 = ip.is_ipv6();
                                        let is_self_assigned = addr_info.is_self_assigned;
                                        let display_text = format_addr_display(&ip, port, is_v6, display_mode);
                                        rsx! {
                                            div {
                                                class: if is_selected { "popup-addr selected" } else if is_self_assigned { "popup-addr self-assigned" } else { "popup-addr" },
                                                onclick: move |_| {
                                                    listen_state.write().selected_idx = current_idx;
                                                    let mode = listen_state.read().display_mode;
                                                    let text = format_addr_display(&ip, port, is_v6, mode);
                                                    #[cfg(target_os = "macos")]
                                                    {
                                                        let _ = std::process::Command::new("pbcopy")
                                                            .stdin(std::process::Stdio::piped())
                                                            .spawn()
                                                            .and_then(|mut child| {
                                                                use std::io::Write;
                                                                if let Some(stdin) = child.stdin.as_mut() {
                                                                    stdin.write_all(text.as_bytes())?;
                                                                }
                                                                child.wait()
                                                            });
                                                    }
                                                },
                                                span { class: "popup-addr-indicator", if is_selected { "▶ " } else { "  " } }
                                                span { class: "popup-addr-text", "{display_text}" }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
pub fn GuiApp(props: GuiAppProps) -> Element {
    let mut app_state = use_signal(|| GuiAppState::new());
    let mut source_rx: Signal<Option<Receiver<SourceEvent>>> = use_signal(|| None);
    let mut container_element: Signal<Option<Rc<MountedData>>> = use_signal(|| None);
    let mut listen_state = use_signal(|| ListenState::new(props.port));
    let mut pending_scroll_to_bottom = use_signal(|| false);

    use_effect({
        let file = props.file.clone();
        let port = props.port;
        move || {
            let (sync_tx, sync_rx) = mpsc::channel::<SourceEvent>();
            let (async_tx, async_rx) = async_channel::unbounded::<SourceEvent>();

            std::thread::spawn(move || {
                while let Ok(event) = sync_rx.recv() {
                    if async_tx.send_blocking(event).is_err() {
                        break;
                    }
                }
            });

            let source = if let Some(port) = port {
                LogSource::Network(port)
            } else if let Some(ref path) = file {
                LogSource::File(path.clone())
            } else {
                LogSource::Stdin
            };

            if let Err(e) = start_source(source, sync_tx) {
                app_state.write().status_message = Some(format!("Failed to start source: {}", e));
            } else {
                source_rx.set(Some(async_rx));
            }
        }
    });

    use_future(move || async move {
        let rx = loop {
            let maybe_rx = source_rx.read().clone();
            if let Some(rx) = maybe_rx {
                break rx;
            }
            async_std::task::sleep(Duration::from_millis(10)).await;
        };

        let mut pending_lines: Vec<String> = Vec::new();
        let mut last_data_time: Option<Instant> = None;
        let mut current_threshold_ms: f64 = BASE_RENDER_THRESHOLD_MS;

        loop {
            if let Some(last_time) = last_data_time {
                let threshold = Duration::from_micros((current_threshold_ms * 1000.0) as u64);
                let wait_duration = threshold.saturating_sub(last_time.elapsed());

                match async_std::future::timeout(wait_duration, rx.recv()).await {
                    Ok(Ok(event)) => {
                        match event {
                            SourceEvent::Line(content) => {
                                pending_lines.push(content);
                                current_threshold_ms = (current_threshold_ms * THRESHOLD_DECAY_FACTOR)
                                    .max(MIN_RENDER_THRESHOLD_MS);
                            }
                            SourceEvent::Error(e) => {
                                app_state.write().status_message = Some(format!("Error: {}", e));
                            }
                            SourceEvent::Connected(peer) => {
                                listen_state.write().has_connection = true;
                                app_state.write().status_message =
                                    Some(format!("Connected: {}", peer));
                            }
                        }
                    }
                    Ok(Err(_)) => break,
                    Err(_) => {
                        let lines_to_add: Vec<String> = pending_lines.drain(..).collect();
                        let mut state = app_state.write();
                        let was_at_bottom = state.follow_tail;
                        for line in lines_to_add {
                            state.add_line(line);
                        }
                        if was_at_bottom {
                            state.scroll_to_bottom();
                            pending_scroll_to_bottom.set(true);
                        }
                        state.version += 1;
                        drop(state);

                        last_data_time = None;
                        current_threshold_ms = BASE_RENDER_THRESHOLD_MS;
                    }
                }
            } else {
                match rx.recv().await {
                    Ok(event) => {
                        match event {
                            SourceEvent::Line(content) => {
                                pending_lines.push(content);
                                last_data_time = Some(Instant::now());
                                current_threshold_ms = (current_threshold_ms * THRESHOLD_DECAY_FACTOR)
                                    .max(MIN_RENDER_THRESHOLD_MS);
                            }
                            SourceEvent::Error(e) => {
                                app_state.write().status_message = Some(format!("Error: {}", e));
                            }
                            SourceEvent::Connected(peer) => {
                                listen_state.write().has_connection = true;
                                app_state.write().status_message =
                                    Some(format!("Connected: {}", peer));
                            }
                        }
                    }
                    Err(_) => break,
                }
            }
        }
    });

    use_future(move || async move {
        loop {
            async_std::task::sleep(Duration::from_secs(1)).await;
            if app_state.read().show_time {
                app_state.write().version += 1;
            }
        }
    });

    use_future(move || async move {
        loop {
            if *pending_scroll_to_bottom.read() {
                pending_scroll_to_bottom.set(false);
                if let Some(ref el) = *container_element.read() {
                    let total = app_state.read().total_height();
                    let coords = dioxus::html::geometry::PixelsVector2D::new(0.0, total);
                    let _ = el.scroll(coords, ScrollBehavior::Instant).await;
                }
            }
            async_std::task::sleep(Duration::from_millis(16)).await;
        }
    });

    let state = app_state.read();
    let total_lines = state.lines.len();
    let filtered_count = state.filtered_indices.len();
    let scroll_y = state.scroll_y;
    let scroll_x = state.scroll_x;
    let container_height = state.container_height;
    let follow_tail = state.follow_tail;
    let show_time = state.show_time;
    let wrap_lines = state.wrap_lines;
    let hide_text = state.hide_text.clone();
    let filter_text = state.filter_text.clone();
    let highlight_text = state.highlight_text.clone();
    let hide_error = state.hide_error.clone();
    let filter_error = state.filter_error.clone();
    let status_message = state.status_message.clone();
    let highlight_expr = state.filter_state.highlight_expr.clone();
    let max_scroll = state.max_scroll();
    let total_height = state.total_height();
    let (start_idx, end_idx) = state.find_visible_range(scroll_y, container_height + LINE_HEIGHT * 3.0);
    let version = state.version;
    drop(state);

    let (visible_lines, runtime_hide_error): (Vec<(usize, usize, f64, LogLine, String)>, Option<String>) = {
        let state = app_state.read();
        let mut error: Option<String> = None;
        let lines: Vec<_> = (start_idx..end_idx)
            .filter_map(|filter_idx| {
                let offset = state.get_line_offset(filter_idx);
                state
                    .filtered_indices
                    .get(filter_idx)
                    .and_then(|&line_idx| {
                        state.lines.get(line_idx).map(|line| {
                            let content = match state.get_display_content(line) {
                                Ok(c) => c,
                                Err(e) => {
                                    if error.is_none() {
                                        error = Some(e);
                                    }
                                    line.content.clone()
                                }
                            };
                            (filter_idx, line_idx, offset, line.clone(), content)
                        })
                    })
            })
            .collect();
        (lines, error)
    };

    if let Some(err) = runtime_hide_error {
        app_state.write().hide_error = Some(format!("Runtime error: {}", err));
    }

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
                        class: if wrap_lines { "active" } else { "" },
                        onclick: move |_| {
                            let mut s = app_state.write();
                            s.wrap_lines = !s.wrap_lines;
                            if s.wrap_lines {
                                s.scroll_x = 0.0;
                            }
                            s.version += 1;
                            s.save_state();
                        },
                        "Wrap"
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
                div { class: "log-main",
                    div {
                        class: if wrap_lines { "log-container wrap-mode" } else { "log-container nowrap-mode" },
                        tabindex: "0",
                        onmounted: move |e| async move {
                            if let Ok(rect) = e.get_client_rect().await {
                                let mut s = app_state.write();
                                s.container_height = rect.size.height;
                                s.container_width = rect.size.width;
                                s.clamp_scroll();
                                s.clamp_scroll_x();
                                s.version += 1;
                            }
                            container_element.set(Some(e.data()));
                        },
                        onresize: move |_| async move {
                            if let Some(ref el) = *container_element.read() {
                                if let Ok(rect) = el.get_client_rect().await {
                                    let mut s = app_state.write();
                                    s.container_height = rect.size.height;
                                    s.container_width = rect.size.width;
                                    s.clamp_scroll();
                                    s.clamp_scroll_x();
                                    s.version += 1;
                                }
                            }
                        },
                        onscroll: move |e| {
                            let data = e.data();
                            let new_scroll_y = data.scroll_top() as f64;
                            let new_scroll_x = data.scroll_left() as f64;
                            let mut s = app_state.write();
                            let (old_start, old_end) = s.find_visible_range(s.scroll_y, s.container_height + LINE_HEIGHT * 3.0);
                            let (new_start, new_end) = s.find_visible_range(new_scroll_y, s.container_height + LINE_HEIGHT * 3.0);
                            s.scroll_y = new_scroll_y;
                            s.scroll_x = new_scroll_x;
                            s.follow_tail = s.is_at_bottom();
                            if old_start != new_start || old_end != new_end {
                                s.version += 1;
                            }
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
                                Key::ArrowLeft => {
                                    if !s.wrap_lines {
                                        s.scroll_x -= 40.0;
                                        s.clamp_scroll_x();
                                    }
                                }
                                Key::ArrowRight => {
                                    if !s.wrap_lines {
                                        s.scroll_x += 40.0;
                                        s.clamp_scroll_x();
                                    }
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
                                    s.scroll_x = 0.0;
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
                            style: "height: {total_height}px; position: relative;",
                            for (filter_idx, line_idx, offset, line, content) in visible_lines {
                                div {
                                    class: "log-line",
                                    key: "{line_idx}",
                                    style: if wrap_lines {
                                        format!("position: absolute; top: {offset}px; left: 0; right: 0;")
                                    } else {
                                        format!("position: absolute; top: {offset}px; left: -{scroll_x}px; right: 0;")
                                    },
                                    onmounted: {
                                        let filter_idx = filter_idx;
                                        move |e| async move {
                                            if let Ok(rect) = e.get_client_rect().await {
                                                app_state.write().set_line_height(filter_idx, rect.size.height);
                                            }
                                        }
                                    },
                                    if show_time {
                                        span { class: "timestamp", "{format_relative_time(line.timestamp)}" }
                                    }
                                    span { class: "line-num", "{line_idx + 1}" }
                                    span { class: "content",
                                        for (text, style) in highlight_content(&content, &highlight_expr) {
                                            {
                                                let class = style.css_class();
                                                if class.is_empty() {
                                                    rsx! { "{text}" }
                                                } else {
                                                    rsx! { span { class: "{class}", "{text}" } }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
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

            if listen_state.read().show_popup() {
                ListenPopup { listen_state }
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

html, body {
    overflow: hidden;
    overscroll-behavior: none;
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
    overflow: hidden;
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
    flex-direction: column;
    overflow: hidden;
    position: relative;
}

.log-main {
    flex: 1;
    display: flex;
    flex-direction: row;
    overflow: hidden;
}

.log-container {
    flex: 1;
    overflow-y: auto;
    overflow-x: hidden;
    background: light-dark(#ffffff, #1e1e1e);
    outline: none;
    position: relative;
    overscroll-behavior: contain;
}

.nowrap-mode.log-container {
    overflow-x: auto;
}

.log-container:focus {
    outline: none;
}

.log-list {
    position: relative;
    will-change: transform;
    padding-bottom: 4px;
}

.nowrap-mode .log-list {
    min-width: max-content;
}

.log-line {
    display: flex;
    padding: 1px 12px;
    font-family: 'SF Mono', Menlo, Monaco, 'Courier New', monospace;
    font-size: 12px;
    height: 20px;
    line-height: 18px;
}

.nowrap-mode .log-line {
    white-space: nowrap;
}

.wrap-mode .log-line {
    white-space: pre-wrap;
    word-break: break-all;
    height: auto;
    min-height: 20px;
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
}

.nowrap-mode .content {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
}

.wrap-mode .content {
    white-space: pre-wrap;
    word-break: break-all;
}

.hl-error {
    color: light-dark(#dc3545, #f85149);
    font-weight: bold;
}

.hl-warn {
    color: light-dark(#ffc107, #d29922);
    font-weight: bold;
}

.hl-info {
    color: light-dark(#28a745, #3fb950);
    font-weight: bold;
}

.hl-debug {
    color: light-dark(#17a2b8, #58a6ff);
}

.hl-bracket {
    color: light-dark(#0066cc, #79c0ff);
}

.hl-timestamp {
    color: light-dark(#6f42c1, #d2a8ff);
}

.hl-custom {
    background: light-dark(#ffff00, #ffcc00);
    color: light-dark(#000000, #000000);
    padding: 0 2px;
    border-radius: 2px;
    font-weight: bold;
}

.hl-json-key {
    color: light-dark(#17a2b8, #58a6ff);
}

.hl-json-string {
    color: light-dark(#28a745, #3fb950);
}

.hl-json-number {
    color: light-dark(#fd7e14, #d29922);
}

.hl-json-bool {
    color: light-dark(#6f42c1, #d2a8ff);
}

.hl-json-null {
    color: light-dark(#dc3545, #f85149);
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

.scrollbar-h {
    height: 14px;
    background: light-dark(#f0f0f0, #1e1e1e);
    border-top: 1px solid light-dark(#d4d4d4, #3c3c3c);
    position: relative;
}

.scrollbar-thumb-h {
    position: absolute;
    height: 10px;
    top: 2px;
    background: light-dark(#c4c4c4, #5a5a5a);
    border-radius: 5px;
    min-width: 30px;
}

.scrollbar-thumb-h:hover {
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

.popup-overlay {
    position: fixed;
    top: 0;
    left: 0;
    right: 0;
    bottom: 0;
    background: rgba(0, 0, 0, 0.5);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 1000;
}

.popup {
    background: light-dark(#ffffff, #252526);
    border: 1px solid light-dark(#d4d4d4, #454545);
    border-radius: 8px;
    padding: 16px 20px;
    min-width: 320px;
    max-width: 500px;
    box-shadow: 0 4px 20px rgba(0, 0, 0, 0.3);
}

.popup-header {
    font-size: 14px;
    margin-bottom: 12px;
    color: light-dark(#1e1e1e, #d4d4d4);
}

.popup-port {
    color: #e5c07b;
    font-weight: bold;
}

.popup-mode {
    font-size: 12px;
    margin-bottom: 4px;
}

.popup-label {
    color: light-dark(#858585, #858585);
}

.popup-mode-value {
    color: #e5c07b;
}

.popup-hint {
    font-size: 11px;
    color: light-dark(#858585, #858585);
    margin-bottom: 12px;
}

.popup-interfaces {
    max-height: 300px;
    overflow-y: auto;
}

.popup-error {
    color: #f44747;
    font-size: 12px;
}

.popup-interface {
    margin-bottom: 8px;
}

.popup-iface-name {
    font-size: 12px;
    font-weight: 500;
    color: #4ec9b0;
    margin-bottom: 4px;
}

.popup-iface-name.default {
    color: #6a9955;
}

.popup-addr {
    display: flex;
    align-items: center;
    padding: 4px 8px;
    font-family: 'SF Mono', Menlo, Monaco, 'Courier New', monospace;
    font-size: 12px;
    cursor: pointer;
    border-radius: 4px;
    margin-left: 8px;
}

.popup-addr:hover {
    background: light-dark(#e8e8e8, #3c3c3c);
}

.popup-addr.selected {
    background: light-dark(#007acc33, #007acc44);
}

.popup-addr.self-assigned {
    opacity: 0.5;
}

.popup-addr-indicator {
    color: #007acc;
    margin-right: 4px;
    width: 16px;
}

.popup-addr-text {
    color: light-dark(#1e1e1e, #d4d4d4);
}
"#;
