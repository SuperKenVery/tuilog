use anyhow::Result;
use gpui::{
    App, Application, Bounds, Context, FocusHandle, Focusable, KeyBinding, MouseButton,
    SharedString, Window, WindowBounds, WindowOptions, div, prelude::*, px, rgb, size,
};
use std::path::PathBuf;
use crate::gui::gpui_state::LogViewerState;
use crate::gui::gpui_text_input::{
    Backspace, Copy, Cut, Delete, End, Home, Left, Paste, Right, SelectAll, SelectLeft,
    SelectRight, ShowCharacterPalette, TextInput,
};
use gpui::Entity;

pub fn run_gpui(file: Option<PathBuf>, port: Option<u16>) -> Result<()> {
    Application::new().run(move |cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(960.), px(600.)), cx);
        cx.bind_keys([
            KeyBinding::new("backspace", Backspace, None),
            KeyBinding::new("delete", Delete, None),
            KeyBinding::new("left", Left, None),
            KeyBinding::new("right", Right, None),
            KeyBinding::new("shift-left", SelectLeft, None),
            KeyBinding::new("shift-right", SelectRight, None),
            KeyBinding::new("cmd-a", SelectAll, None),
            KeyBinding::new("cmd-v", Paste, None),
            KeyBinding::new("cmd-c", Copy, None),
            KeyBinding::new("cmd-x", Cut, None),
            KeyBinding::new("home", Home, None),
            KeyBinding::new("end", End, None),
            KeyBinding::new("ctrl-cmd-space", ShowCharacterPalette, None),
        ]);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |_, cx| {
                cx.new(|cx| {
                    let focus = cx.focus_handle();
                    let state = LogViewerState::new();
                    let hide_input = cx.new(|cx| TextInput::new(cx, "regex to hide...", "", None));
                    let filter_input = cx.new(|cx| TextInput::new(cx, "filter expression...", "", None));
                    let highlight_input =
                        cx.new(|cx| TextInput::new(cx, "highlight expression...", "", None));
                    LogViewer {
                        focus_handle: focus,
                        file,
                        port,
                        title: SharedString::from("LogViewer (GPUI)"),
                        state,
                        hide_input,
                        filter_input,
                        highlight_input,
                        popup_open: false,
                        pressed_apply: None,
                    }
                })
            },
        )
        .unwrap();
        cx.activate(true);
    });
    Ok(())
}

struct LogViewer {
    focus_handle: FocusHandle,
    file: Option<PathBuf>,
    port: Option<u16>,
    title: SharedString,
    state: LogViewerState,
    hide_input: Entity<TextInput>,
    filter_input: Entity<TextInput>,
    highlight_input: Entity<TextInput>,
    popup_open: bool,
    pressed_apply: Option<ApplyTarget>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ApplyTarget {
    Hide,
    Filter,
    Highlight,
}

impl Focusable for LogViewer {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for LogViewer {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let apply_bg = rgb(0xe5e7eb);
        let apply_border = rgb(0xd1d5db);
        let apply_hover_bg = rgb(0xf0f2f5);
        let apply_hover_border = rgb(0xcbd5e1);
        let apply_pressed_bg = rgb(0xe1e4e8);
        let apply_pressed_border = rgb(0xb6c0cd);
        let apply_text = rgb(0x111827);
        let header = div()
            .track_focus(&self.focus_handle(cx))
            .size_full()
            .flex()
            .flex_col()
            .child(
                div()
                    .h(px(44.))
                    .px_4()
                    .flex()
                    .items_center()
                    .justify_between()
                    .bg(rgb(0x1c1f24))
                    .text_color(rgb(0xe5e7eb))
                    .child(self.title.clone())
                    .child(
                        div()
                            .px_2()
                            .py_1()
                            .bg(rgb(0x2a2f36))
                            .text_color(rgb(0xe5e7eb))
                            .rounded_sm()
                            .child("Listen")
                            .on_mouse_up(MouseButton::Left, cx.listener(|this, _, _, cx| {
                                this.popup_open = !this.popup_open;
                                cx.notify();
                            })),
                    ),
            )
            .child(
                div()
                    .h(px(40.))
                    .px_3()
                    .flex()
                    .items_center()
                    .gap_2()
                    .bg(rgb(0xf6f7f9))
                    .text_color(rgb(0x111827))
                    .child(div().text_sm().child("Hide"))
                    .child(div().w(px(160.)).child(self.hide_input.clone()))
                    .child(
                        div()
                            .px_2()
                            .py_1()
                            .bg(if self.pressed_apply == Some(ApplyTarget::Hide) {
                                apply_pressed_bg
                            } else {
                                apply_bg
                            })
                            .text_color(apply_text)
                            .border_1()
                            .border_color(if self.pressed_apply == Some(ApplyTarget::Hide) {
                                apply_pressed_border
                            } else {
                                apply_border
                            })
                            .rounded_md()
                            .hover(|style| {
                                style.bg(apply_hover_bg).border_color(apply_hover_border).cursor_pointer()
                            })
                            .child("Apply")
                            .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| {
                                this.pressed_apply = Some(ApplyTarget::Hide);
                                cx.notify();
                            }))
                            .on_mouse_up(MouseButton::Left, cx.listener(|this, _, _, cx| {
                                let v = this.hide_input.read(cx).content().to_string();
                                this.state.hide_text = v;
                                this.state.apply_hide();
                                this.pressed_apply = None;
                                cx.notify();
                            }))
                            .on_mouse_up_out(MouseButton::Left, cx.listener(|this, _, _, cx| {
                                this.pressed_apply = None;
                                cx.notify();
                            })),
                    )
                    .child(div().pl_3().text_sm().child("Filter"))
                    .child(div().w(px(180.)).child(self.filter_input.clone()))
                    .child(
                        div()
                            .px_2()
                            .py_1()
                            .bg(if self.pressed_apply == Some(ApplyTarget::Filter) {
                                apply_pressed_bg
                            } else {
                                apply_bg
                            })
                            .text_color(apply_text)
                            .border_1()
                            .border_color(if self.pressed_apply == Some(ApplyTarget::Filter) {
                                apply_pressed_border
                            } else {
                                apply_border
                            })
                            .rounded_md()
                            .hover(|style| {
                                style.bg(apply_hover_bg).border_color(apply_hover_border).cursor_pointer()
                            })
                            .child("Apply")
                            .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| {
                                this.pressed_apply = Some(ApplyTarget::Filter);
                                cx.notify();
                            }))
                            .on_mouse_up(MouseButton::Left, cx.listener(|this, _, _, cx| {
                                let v = this.filter_input.read(cx).content().to_string();
                                this.state.filter_text = v;
                                this.state.apply_filter();
                                this.pressed_apply = None;
                                cx.notify();
                            }))
                            .on_mouse_up_out(MouseButton::Left, cx.listener(|this, _, _, cx| {
                                this.pressed_apply = None;
                                cx.notify();
                            })),
                    )
                    .child(div().pl_3().text_sm().child("Highlight"))
                    .child(div().w(px(180.)).child(self.highlight_input.clone()))
                    .child(
                        div()
                            .px_2()
                            .py_1()
                            .bg(if self.pressed_apply == Some(ApplyTarget::Highlight) {
                                apply_pressed_bg
                            } else {
                                apply_bg
                            })
                            .text_color(apply_text)
                            .border_1()
                            .border_color(if self.pressed_apply == Some(ApplyTarget::Highlight) {
                                apply_pressed_border
                            } else {
                                apply_border
                            })
                            .rounded_md()
                            .hover(|style| {
                                style.bg(apply_hover_bg).border_color(apply_hover_border).cursor_pointer()
                            })
                            .child("Apply")
                            .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| {
                                this.pressed_apply = Some(ApplyTarget::Highlight);
                                cx.notify();
                            }))
                            .on_mouse_up(MouseButton::Left, cx.listener(|this, _, _, cx| {
                                let v = this.highlight_input.read(cx).content().to_string();
                                this.state.highlight_text = v;
                                this.state.apply_highlight();
                                this.pressed_apply = None;
                                cx.notify();
                            }))
                            .on_mouse_up_out(MouseButton::Left, cx.listener(|this, _, _, cx| {
                                this.pressed_apply = None;
                                cx.notify();
                            })),
                    ),
            );
        let info = match (&self.file, &self.port) {
            (Some(f), _) => format!("File: {}", f.display()),
            (None, Some(p)) => format!("Listening on port {}", p),
            _ => "Reading from stdin".to_string(),
        };
        let filtered_count = self.state.filtered_indices.len();
        let total_lines = self.state.lines.len();
        let (start_idx, end_idx) =
            self.state.find_visible_range(self.state.scroll_y, self.state.container_height);
        let list = div()
            .flex_1()
            .bg(rgb(0xf8fafc))
            .text_color(rgb(0x111827))
            .px_3()
            .py_2()
            .child(
                div()
                    .py_2()
                    .text_sm()
                    .text_color(rgb(0x4b5563))
                    .border_b_1()
                    .border_color(rgb(0xe5e7eb))
                    .child(info),
            )
            .children(
                (start_idx..end_idx).map(|filter_ix| {
                    let ix = self.state.filtered_indices[filter_ix];
                    let line = &self.state.lines[ix];
                    div()
                        .h(px(20.))
                        .px_1()
                        .child(format!("{}", line.content))
                }),
            );
        let overlay = if self.popup_open {
            div()
                .absolute()
                .top(px(54.))
                .left(px(8.))
                .w(px(360.))
                .p_3()
                .rounded_md()
                .shadow_lg()
                .bg(gpui::white())
                .text_color(gpui::black())
                .border_1()
                .border_color(rgb(0xe5e7eb))
                .child("Listening: copy addresses here later")
        } else {
            div()
        };
        header
            .child(
                div()
                    .flex_1()
                    .px_3()
                    .py_2()
                    .child(
                        div()
                            .bg(gpui::white())
                            .border_1()
                            .border_color(rgb(0xe5e7eb))
                            .rounded_md()
                            .child(list),
                    ),
            )
            .child(
                div()
                    .h(px(28.))
                    .px_4()
                    .bg(rgb(0x1c1f24))
                    .text_color(rgb(0xcbd5e1))
                    .child(format!("{filtered_count} / {total_lines} lines")),
            )
            .child(overlay)
    }
}
