use crate::core::{format_relative_time, get_time_age, ListenState, LogLine, TimeAge};
use crate::filter::FilterExpr;
use crate::highlight::{apply_highlights, highlight_line, HighlightStyle};
use crate::source::{start_source, LogSource, SourceEvent};
use crate::state::AppState;
use async_channel::Receiver;
use dioxus::html::MountedData;
use dioxus::prelude::*;
use std::rc::Rc;
use std::sync::Arc;
use fancy_regex::Regex;
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use super::components::ListenPopup;
use super::state::GuiAppState;
use super::style::CSS;

const LINE_HEIGHT: f64 = 20.0;
const BASE_RENDER_THRESHOLD_MS: f64 = 50.0;
const MIN_RENDER_THRESHOLD_MS: f64 = 5.0;
const THRESHOLD_DECAY_FACTOR: f64 = 0.7;

#[derive(Props, Clone, PartialEq)]
pub struct GuiAppProps {
    pub file: Option<PathBuf>,
    pub port: Option<u16>,
}

fn highlight_content(content: &str, highlight_expr: &Option<FilterExpr>) -> Vec<(String, HighlightStyle)> {
    let spans = highlight_line(content, highlight_expr.as_ref(), true, true);
    apply_highlights(content, &spans)
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

            let state = AppState::load();
            let line_start_regex = if state.line_start_regex.trim().is_empty() {
                None
            } else {
                match Regex::new(&state.line_start_regex) {
                    Ok(re) => Some(Arc::new(re)),
                    Err(_) => None,
                }
            };

            if let Err(e) = start_source(source, sync_tx, line_start_regex) {
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
                                let mut state = app_state.write();
                                state.is_connected = true;
                                state.status_message = Some(format!("Connected: {}", peer));
                            }
                            SourceEvent::Disconnected(peer) => {
                                let mut state = app_state.write();
                                state.is_connected = false;
                                state.status_message = Some(format!("Disconnected: {}", peer));
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
                                let mut state = app_state.write();
                                state.is_connected = true;
                                state.status_message = Some(format!("Connected: {}", peer));
                            }
                            SourceEvent::Disconnected(peer) => {
                                let mut state = app_state.write();
                                state.is_connected = false;
                                state.status_message = Some(format!("Disconnected: {}", peer));
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
    let line_start_text = state.line_start_text.clone();
    let line_start_error = state.line_start_error.clone();
    let status_message = state.status_message.clone();
    let is_connected = state.is_connected;
    let highlight_expr = state.filter_state.highlight_expr.clone();
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
                div { class: "filter-group",
                    label { "Line Start:" }
                    input {
                        r#type: "text",
                        class: if line_start_error.is_some() { "error" } else { "" },
                        placeholder: "regex for log line start...",
                        value: "{line_start_text}",
                        oninput: move |e| app_state.write().line_start_text = e.value(),
                        onkeydown: move |e| {
                            if e.key() == Key::Enter {
                                app_state.write().apply_line_start();
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
                                pending_scroll_to_bottom.set(true);
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
                            if !s.is_at_bottom() {
                                s.follow_tail = false;
                            }
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
                                    key: "{line_idx}-{wrap_lines}",
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
                                        {
                                            let time_age = get_time_age(line.timestamp);
                                            let age_class = match time_age {
                                                TimeAge::VeryRecent => "timestamp very-recent",
                                                TimeAge::Recent => "timestamp recent",
                                                TimeAge::Minutes => "timestamp minutes",
                                                TimeAge::Hours => "timestamp hours",
                                                TimeAge::Days => "timestamp days",
                                            };
                                            rsx! { span { class: "{age_class}", "{format_relative_time(line.timestamp)}" } }
                                        }
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

            div {
                class: if status_message.is_some() && !is_connected { "statusbar disconnected" } else { "statusbar" },
                span { class: "status-info",
                    "{filtered_count} / {total_lines} lines"
                    if follow_tail { " • Following" }
                }
                if let Some(ref msg) = status_message {
                    span { class: "status-msg", "{msg}" }
                }
            }

            if listen_state.read().show_popup() {
                ListenPopup { listen_state }
            }
        }
    }
}
