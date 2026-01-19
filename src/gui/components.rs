use crate::core::{ListenDisplayMode, ListenState};
use crate::filter::FilterExpr;
use super::state::highlight_content;
use dioxus::prelude::*;

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

#[derive(Props, Clone)]
pub struct LogLineContentProps {
    pub content: String,
    pub highlight_text: String,
    pub highlight_expr: Option<FilterExpr>,
}

impl PartialEq for LogLineContentProps {
    fn eq(&self, other: &Self) -> bool {
        self.content == other.content && self.highlight_text == other.highlight_text
    }
}

#[component]
pub fn LogLineContent(props: LogLineContentProps) -> Element {
    let parts = highlight_content(&props.content, &props.highlight_expr);
    rsx! {
        span { class: "content",
            for (text, style) in parts {
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

#[cfg(target_os = "macos")]
fn copy_to_clipboard(text: &str) {
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

#[cfg(not(target_os = "macos"))]
fn copy_to_clipboard(_text: &str) {}

#[component]
pub fn ListenPopup(listen_state: Signal<ListenState>) -> Element {
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
                            copy_to_clipboard(&text);
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
                                                    copy_to_clipboard(&text);
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
