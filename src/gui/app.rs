use crate::core::{format_relative_time, FilterState, LogLine};
use crate::filter::parse_filter;
use crate::source::{start_source, LogSource, SourceEvent};
use crate::state::AppState;
use fancy_regex::Regex;
use gpui::{
    actions, div, prelude::*, px, rgb, rgba, size, App, Application, Context, FocusHandle,
    Focusable, InteractiveElement, KeyBinding, ParentElement, Pixels, Render, ScrollStrategy,
    SharedString, Size, StatefulInteractiveElement, Styled, TitlebarOptions, Window, WindowBounds,
    WindowOptions,
};
use gpui_component::{v_virtual_list, VirtualListScrollHandle};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;

actions!(
    log_viewer,
    [
        ScrollUp,
        ScrollDown,
        ScrollPageUp,
        ScrollPageDown,
        ScrollToTop,
        ScrollToBottom,
        ToggleFollow,
        ToggleTime,
        ClearLogs,
        Quit,
    ]
);

const LINE_HEIGHT: f32 = 20.0;

pub struct LogViewer {
    lines: Vec<LogLine>,
    filtered_indices: Vec<usize>,
    filter_state: FilterState,

    hide_input: String,
    filter_input: String,
    highlight_input: String,

    scroll_handle: VirtualListScrollHandle,
    item_sizes: Rc<Vec<Size<Pixels>>>,
    follow_tail: bool,
    show_time: bool,
    status_message: Option<String>,

    focus_handle: FocusHandle,
}

impl LogViewer {
    pub fn new(
        file: Option<PathBuf>,
        port: Option<u16>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let persisted = AppState::load();

        let mut viewer = Self {
            lines: Vec::new(),
            filtered_indices: Vec::new(),
            filter_state: FilterState::default(),
            hide_input: persisted.hide_input.clone(),
            filter_input: persisted.filter_input.clone(),
            highlight_input: persisted.highlight_input.clone(),
            scroll_handle: VirtualListScrollHandle::new(),
            item_sizes: Rc::new(Vec::new()),
            follow_tail: true,
            show_time: true,
            status_message: None,
            focus_handle: cx.focus_handle(),
        };

        if !persisted.hide_input.trim().is_empty() {
            if let Ok(re) = Regex::new(&persisted.hide_input) {
                viewer.filter_state.hide_regex = Some(re);
            }
        }
        if !persisted.filter_input.trim().is_empty() {
            if let Ok(expr) = parse_filter(&persisted.filter_input) {
                viewer.filter_state.filter_expr = Some(expr);
            }
        }
        if !persisted.highlight_input.trim().is_empty() {
            if let Ok(expr) = parse_filter(&persisted.highlight_input) {
                viewer.filter_state.highlight_expr = Some(expr);
            }
        }

        let (tx, rx) = mpsc::channel::<SourceEvent>();
        let source = if let Some(port) = port {
            LogSource::Network(port)
        } else if let Some(ref path) = file {
            LogSource::File(path.clone())
        } else {
            LogSource::Stdin
        };

        if let Err(e) = start_source(source, tx) {
            viewer.status_message = Some(format!("Failed to start: {}", e));
        }

        let rx = Arc::new(Mutex::new(Some(rx)));
        cx.spawn_in(window, async move |this, cx| {
            loop {
                let rx_clone = rx.clone();
                let event = cx
                    .background_executor()
                    .spawn(async move {
                        let guard = rx_clone.lock().ok()?;
                        let rx = guard.as_ref()?;
                        rx.recv_timeout(Duration::from_millis(50)).ok()
                    })
                    .await;

                if let Some(event) = event {
                    cx.update(|_, cx| {
                        this.update(cx, |v, cx| v.handle_event(event, cx)).ok();
                    })
                    .ok();
                }
            }
        })
        .detach();

        cx.spawn_in(window, async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_secs(1))
                    .await;
                cx.update(|_, cx| {
                    this.update(cx, |v, cx| {
                        if v.show_time {
                            cx.notify();
                        }
                    })
                    .ok();
                })
                .ok();
            }
        })
        .detach();

        viewer
    }

    fn handle_event(&mut self, event: SourceEvent, cx: &mut Context<Self>) {
        let was_following = self.follow_tail;
        match event {
            SourceEvent::Line(content) => {
                let line = LogLine {
                    content: content.trim_end().to_string(),
                    timestamp: chrono::Local::now(),
                };
                let idx = self.lines.len();
                if self.matches_filter(&line) {
                    self.filtered_indices.push(idx);
                    self.update_item_sizes();
                }
                self.lines.push(line);
            }
            SourceEvent::Error(e) => {
                self.status_message = Some(format!("Error: {}", e));
            }
            SourceEvent::Connected(peer) => {
                self.status_message = Some(format!("Connected: {}", peer));
            }
        }
        if was_following {
            self.scroll_handle.scroll_to_bottom();
        }
        cx.notify();
    }

    fn update_item_sizes(&mut self) {
        let sizes: Vec<Size<Pixels>> = self
            .filtered_indices
            .iter()
            .map(|_| size(px(10000.), px(LINE_HEIGHT)))
            .collect();
        self.item_sizes = Rc::new(sizes);
    }

    fn get_display_content(&self, line: &LogLine) -> String {
        match &self.filter_state.hide_regex {
            Some(re) => match re.replace_all(&line.content, "") {
                std::borrow::Cow::Borrowed(_) => line.content.clone(),
                std::borrow::Cow::Owned(s) => s,
            },
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

    fn rebuild_filtered(&mut self) {
        self.filtered_indices.clear();
        for (i, line) in self.lines.iter().enumerate() {
            if self.matches_filter(line) {
                self.filtered_indices.push(i);
            }
        }
        self.update_item_sizes();
    }

    fn save_state(&self) {
        AppState {
            hide_input: self.hide_input.clone(),
            filter_input: self.filter_input.clone(),
            highlight_input: self.highlight_input.clone(),
            wrap_lines: false,
        }
        .save();
    }

    fn scroll_up(&mut self, _: &ScrollUp, _: &mut Window, cx: &mut Context<Self>) {
        let current = self.scroll_handle.offset();
        self.scroll_handle
            .set_offset(gpui::point(current.x, current.y - px(LINE_HEIGHT)));
        self.follow_tail = false;
        cx.notify();
    }

    fn scroll_down(&mut self, _: &ScrollDown, _: &mut Window, cx: &mut Context<Self>) {
        let current = self.scroll_handle.offset();
        self.scroll_handle
            .set_offset(gpui::point(current.x, current.y + px(LINE_HEIGHT)));
        self.follow_tail = false;
        cx.notify();
    }

    fn scroll_page_up(&mut self, _: &ScrollPageUp, _: &mut Window, cx: &mut Context<Self>) {
        let current = self.scroll_handle.offset();
        self.scroll_handle
            .set_offset(gpui::point(current.x, current.y - px(LINE_HEIGHT * 20.0)));
        self.follow_tail = false;
        cx.notify();
    }

    fn scroll_page_down(&mut self, _: &ScrollPageDown, _: &mut Window, cx: &mut Context<Self>) {
        let current = self.scroll_handle.offset();
        self.scroll_handle
            .set_offset(gpui::point(current.x, current.y + px(LINE_HEIGHT * 20.0)));
        self.follow_tail = false;
        cx.notify();
    }

    fn scroll_to_top(&mut self, _: &ScrollToTop, _: &mut Window, cx: &mut Context<Self>) {
        self.scroll_handle.scroll_to_item(0, ScrollStrategy::Top);
        self.follow_tail = false;
        cx.notify();
    }

    fn scroll_to_bottom_action(
        &mut self,
        _: &ScrollToBottom,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.scroll_handle.scroll_to_bottom();
        self.follow_tail = true;
        cx.notify();
    }

    fn toggle_follow(&mut self, _: &ToggleFollow, _: &mut Window, cx: &mut Context<Self>) {
        self.follow_tail = !self.follow_tail;
        if self.follow_tail {
            self.scroll_handle.scroll_to_bottom();
        }
        cx.notify();
    }

    fn toggle_time(&mut self, _: &ToggleTime, _: &mut Window, cx: &mut Context<Self>) {
        self.show_time = !self.show_time;
        cx.notify();
    }

    fn clear_logs(&mut self, _: &ClearLogs, _: &mut Window, cx: &mut Context<Self>) {
        self.lines.clear();
        self.filtered_indices.clear();
        self.update_item_sizes();
        cx.notify();
    }
}

impl Focusable for LogViewer {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

fn render_highlighted_line(content: &str, highlight_expr: &Option<crate::filter::FilterExpr>) -> impl IntoElement {
    let highlight_expr = match highlight_expr {
        Some(expr) => expr,
        None => return div().child(content.to_string()),
    };

    let matches = highlight_expr.find_all_matches(content);
    if matches.is_empty() {
        return div().child(content.to_string());
    }

    let mut children: Vec<gpui::AnyElement> = Vec::new();
    let mut last_end = 0;

    for (start, end) in matches {
        if start > last_end {
            children.push(
                div()
                    .child(content[last_end..start].to_string())
                    .into_any_element(),
            );
        }
        children.push(
            div()
                .bg(rgba(0xffcc0066))
                .child(content[start..end].to_string())
                .into_any_element(),
        );
        last_end = end;
    }

    if last_end < content.len() {
        children.push(
            div()
                .child(content[last_end..].to_string())
                .into_any_element(),
        );
    }

    div().flex().children(children)
}

impl Render for LogViewer {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let total = self.lines.len();
        let filtered = self.filtered_indices.len();
        let follow = self.follow_tail;
        let show_time = self.show_time;

        let time_bg = if show_time {
            rgb(0x007acc)
        } else {
            rgb(0x3c3c3c)
        };
        let follow_bg = if follow {
            rgb(0x007acc)
        } else {
            rgb(0x3c3c3c)
        };

        let hide_display: SharedString = if self.hide_input.is_empty() {
            "...".into()
        } else {
            self.hide_input.clone().into()
        };
        let filter_display: SharedString = if self.filter_input.is_empty() {
            "...".into()
        } else {
            self.filter_input.clone().into()
        };
        let highlight_display: SharedString = if self.highlight_input.is_empty() {
            "...".into()
        } else {
            self.highlight_input.clone().into()
        };

        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(0x1e1e1e))
            .text_color(rgb(0xd4d4d4))
            .text_size(px(13.))
            .key_context("LogViewer")
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::scroll_up))
            .on_action(cx.listener(Self::scroll_down))
            .on_action(cx.listener(Self::scroll_page_up))
            .on_action(cx.listener(Self::scroll_page_down))
            .on_action(cx.listener(Self::scroll_to_top))
            .on_action(cx.listener(Self::scroll_to_bottom_action))
            .on_action(cx.listener(Self::toggle_follow))
            .on_action(cx.listener(Self::toggle_time))
            .on_action(cx.listener(Self::clear_logs))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .gap_4()
                    .p_2()
                    .bg(rgb(0x252526))
                    .border_b_1()
                    .border_color(rgb(0x3c3c3c))
                    .child(
                        div()
                            .flex()
                            .gap_2()
                            .items_center()
                            .child(
                                div()
                                    .text_size(px(12.))
                                    .text_color(rgb(0x858585))
                                    .child("Hide"),
                            )
                            .child(
                                div()
                                    .w(px(120.))
                                    .px_2()
                                    .py_1()
                                    .bg(rgb(0x3c3c3c))
                                    .border_1()
                                    .border_color(rgb(0x4c4c4c))
                                    .rounded(px(3.))
                                    .text_size(px(12.))
                                    .child(hide_display),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .gap_2()
                            .items_center()
                            .child(
                                div()
                                    .text_size(px(12.))
                                    .text_color(rgb(0x858585))
                                    .child("Filter"),
                            )
                            .child(
                                div()
                                    .w(px(120.))
                                    .px_2()
                                    .py_1()
                                    .bg(rgb(0x3c3c3c))
                                    .border_1()
                                    .border_color(rgb(0x4c4c4c))
                                    .rounded(px(3.))
                                    .text_size(px(12.))
                                    .child(filter_display),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .gap_2()
                            .items_center()
                            .child(
                                div()
                                    .text_size(px(12.))
                                    .text_color(rgb(0x858585))
                                    .child("Highlight"),
                            )
                            .child(
                                div()
                                    .w(px(120.))
                                    .px_2()
                                    .py_1()
                                    .bg(rgb(0x3c3c3c))
                                    .border_1()
                                    .border_color(rgb(0x4c4c4c))
                                    .rounded(px(3.))
                                    .text_size(px(12.))
                                    .child(highlight_display),
                            ),
                    )
                    .child(div().flex_grow())
                    .child(
                        div()
                            .id("time-btn")
                            .px_2()
                            .py_1()
                            .bg(time_bg)
                            .rounded(px(3.))
                            .text_size(px(12.))
                            .cursor_pointer()
                            .child("Time")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.show_time = !this.show_time;
                                cx.notify();
                            })),
                    )
                    .child(
                        div()
                            .id("follow-btn")
                            .px_2()
                            .py_1()
                            .bg(follow_bg)
                            .rounded(px(3.))
                            .text_size(px(12.))
                            .cursor_pointer()
                            .child("Follow")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.follow_tail = !this.follow_tail;
                                if this.follow_tail {
                                    this.scroll_handle.scroll_to_bottom();
                                }
                                cx.notify();
                            })),
                    )
                    .child(
                        div()
                            .id("clear-btn")
                            .px_2()
                            .py_1()
                            .bg(rgb(0x3c3c3c))
                            .rounded(px(3.))
                            .text_size(px(12.))
                            .cursor_pointer()
                            .child("Clear")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.lines.clear();
                                this.filtered_indices.clear();
                                this.update_item_sizes();
                                cx.notify();
                            })),
                    ),
            )
            .child(
                v_virtual_list(
                    cx.entity().clone(),
                    "log-list",
                    self.item_sizes.clone(),
                    {
                        let highlight_expr = self.filter_state.highlight_expr.clone();
                        move |view, visible_range, _, _cx| {
                            visible_range
                                .filter_map(|ix| {
                                    let line_idx = *view.filtered_indices.get(ix)?;
                                    let line = view.lines.get(line_idx)?;
                                    let content = view.get_display_content(line);

                                    let time_str = if view.show_time {
                                        format!("{:>6} ", format_relative_time(line.timestamp))
                                    } else {
                                        String::new()
                                    };
                                    let line_num = format!("{:>5} ", line_idx + 1);

                                    Some(
                                        div()
                                            .flex()
                                            .h(px(LINE_HEIGHT))
                                            .text_size(px(12.))
                                            .font_family("monospace")
                                            .hover(|s| s.bg(rgb(0x2a2d2e)))
                                            .when(!time_str.is_empty(), |el| {
                                                el.child(
                                                    div()
                                                        .text_color(rgb(0x6a9955))
                                                        .child(time_str),
                                                )
                                            })
                                            .child(
                                                div().text_color(rgb(0x858585)).child(line_num),
                                            )
                                            .child(
                                                div()
                                                    .flex_grow()
                                                    .overflow_hidden()
                                                    .whitespace_nowrap()
                                                    .child(render_highlighted_line(
                                                        &content,
                                                        &highlight_expr,
                                                    )),
                                            ),
                                    )
                                })
                                .collect()
                        }
                    },
                )
                .track_scroll(&self.scroll_handle)
                .flex_1()
                .p_2(),
            )
            .child(
                div()
                    .flex()
                    .justify_between()
                    .px_2()
                    .py_1()
                    .bg(rgb(0x007acc))
                    .text_color(rgb(0xffffff))
                    .text_size(px(12.))
                    .child(format!(
                        "{} / {} lines{}",
                        filtered,
                        total,
                        if follow { " • Following" } else { "" }
                    ))
                    .when_some(self.status_message.clone(), |el, msg| el.child(msg)),
            )
    }
}

pub fn run_gui(file: Option<PathBuf>, port: Option<u16>) {
    Application::new().run(move |cx: &mut App| {
        cx.bind_keys([
            KeyBinding::new("up", ScrollUp, Some("LogViewer")),
            KeyBinding::new("down", ScrollDown, Some("LogViewer")),
            KeyBinding::new("pageup", ScrollPageUp, Some("LogViewer")),
            KeyBinding::new("pagedown", ScrollPageDown, Some("LogViewer")),
            KeyBinding::new("cmd-up", ScrollToTop, Some("LogViewer")),
            KeyBinding::new("cmd-down", ScrollToBottom, Some("LogViewer")),
            KeyBinding::new("cmd-f", ToggleFollow, Some("LogViewer")),
            KeyBinding::new("cmd-t", ToggleTime, Some("LogViewer")),
            KeyBinding::new("cmd-k", ClearLogs, Some("LogViewer")),
            KeyBinding::new("cmd-q", Quit, None),
        ]);

        cx.on_action(|_: &Quit, cx| cx.quit());

        let bounds =
            gpui::bounds(gpui::point(px(0.0), px(0.0)), gpui::size(px(1000.), px(700.)));
        let window = cx
            .open_window(
                WindowOptions {
                    titlebar: Some(TitlebarOptions {
                        title: Some("Log Viewer".into()),
                        ..Default::default()
                    }),
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    ..Default::default()
                },
                |window, cx| cx.new(|cx| LogViewer::new(file, port, window, cx)),
            )
            .unwrap();

        window
            .update(cx, |view, window, cx| {
                window.focus(&view.focus_handle(cx));
                cx.activate(true);
            })
            .unwrap();
    });
}
