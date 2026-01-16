use crate::app::App;
use crate::constants::{
    HELP_POPUP_HEIGHT, HELP_POPUP_WIDTH, INPUT_FIELD_HEIGHT, QUIT_POPUP_HEIGHT, QUIT_POPUP_WIDTH,
    STATUS_BAR_HEIGHT,
};
use crate::core::{format_relative_time, InputMode, ListenAddrEntry, ListenDisplayMode};
use crate::input::TextInput;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph},
    Frame,
};

pub fn draw(frame: &mut Frame, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(INPUT_FIELD_HEIGHT),
            Constraint::Length(INPUT_FIELD_HEIGHT),
            Constraint::Length(INPUT_FIELD_HEIGHT),
            Constraint::Length(INPUT_FIELD_HEIGHT),
            Constraint::Min(1),
            Constraint::Length(STATUS_BAR_HEIGHT),
        ])
        .split(frame.area());

    draw_text_input(
        frame,
        &app.input_fields.hide,
        chunks[0],
        " Hide (d) ",
        app.input_mode == InputMode::HideEdit,
    );
    draw_text_input(
        frame,
        &app.input_fields.filter,
        chunks[1],
        " Filter (f) ",
        app.input_mode == InputMode::FilterEdit,
    );
    draw_text_input(
        frame,
        &app.input_fields.highlight,
        chunks[2],
        " Highlight (h) ",
        app.input_mode == InputMode::HighlightEdit,
    );
    draw_text_input(
        frame,
        &app.input_fields.line_start,
        chunks[3],
        " Line Start (s) ",
        app.input_mode == InputMode::LineStartEdit,
    );
    draw_log_view(frame, app, chunks[4]);
    draw_status_bar(frame, app, chunks[5]);

    if app.input_mode != InputMode::Normal {
        draw_help_popup(frame);
    }

    if app.listen_state.show_popup() {
        draw_listen_popup(frame, app);
    }

    if app.show_quit_confirm {
        draw_quit_confirm(frame);
    }
}

fn draw_text_input(frame: &mut Frame, input: &TextInput, area: Rect, label: &str, is_active: bool) {
    let style = if is_active {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default()
    };

    let title = if let Some(err) = &input.error {
        format!("{} (Error: {}) ", label.trim(), err)
    } else {
        label.to_string()
    };

    let border_style = if input.has_error() {
        Style::default().fg(Color::Red)
    } else {
        style
    };

    let widget = Paragraph::new(input.text.as_str())
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(title)
                .border_style(border_style),
        )
        .style(style);
    frame.render_widget(widget, area);

    if is_active {
        frame.set_cursor_position((area.x + input.cursor as u16 + 1, area.y + 1));
    }
}

fn draw_log_view(frame: &mut Frame, app: &mut App, area: Rect) {
    let inner_height = area.height.saturating_sub(2) as usize;
    let inner_width = area.width.saturating_sub(2) as usize;

    let title = format!(
        " Logs [{}/{}] {}{} ",
        app.log_state.filtered_indices.len(),
        app.log_state.lines.len(),
        if app.log_state.follow_tail {
            "[FOLLOW]"
        } else {
            ""
        },
        if app.wrap_lines { "[WRAP]" } else { "" }
    );

    let block = Block::default()
        .borders(Borders::ALL)
        .title(title)
        .border_style(Style::default().fg(Color::Cyan));

    if app.log_state.filtered_indices.is_empty() {
        let list = List::new(Vec::<ListItem>::new()).block(block);
        frame.render_widget(list, area);
        return;
    }

    let prefix_width = app.prefix_width();
    let content_width = inner_width.saturating_sub(prefix_width);
    let bottom_idx = app.log_state.get_bottom_line_idx();

    let mut collected_lines: Vec<Line> = Vec::new();
    let mut current_filtered_idx = bottom_idx as i64;

    while collected_lines.len() < inner_height && current_filtered_idx >= 0 {
        let filtered_idx = current_filtered_idx as usize;
        if filtered_idx >= app.log_state.filtered_indices.len() {
            current_filtered_idx -= 1;
            continue;
        }
        let line_idx = app.log_state.filtered_indices[filtered_idx];
        let log_line = app.log_state.lines[line_idx].clone();

        let mut prefix_spans = Vec::new();
        if app.show_time {
            let time_age = crate::core::get_time_age(log_line.timestamp);
            let (time_color, is_bold) = match time_age {
                crate::core::TimeAge::VeryRecent => (Color::LightGreen, true),
                crate::core::TimeAge::Recent => (Color::Green, false),
                crate::core::TimeAge::Minutes => (Color::Rgb(136, 136, 136), false),
                crate::core::TimeAge::Hours => (Color::Rgb(102, 102, 102), false),
                crate::core::TimeAge::Days => (Color::Rgb(85, 85, 85), false),
            };
            let mut style = Style::default().fg(time_color);
            if is_bold {
                style = style.add_modifier(ratatui::style::Modifier::BOLD);
            }
            prefix_spans.push(Span::styled(
                format!("{:>6} ", format_relative_time(log_line.timestamp)),
                style,
            ));
        }
        prefix_spans.push(Span::styled(
            format!("{:>6} │ ", line_idx + 1),
            Style::default().fg(Color::DarkGray),
        ));

        let highlighted = app.render_line(&log_line);

        if app.wrap_lines && content_width > 0 {
            let wrapped = wrap_highlighted(&highlighted, content_width);
            let mut line_group: Vec<Line> = Vec::new();

            for (i, wrap_line) in wrapped.into_iter().enumerate() {
                let mut line_spans = Vec::new();
                if i == 0 {
                    line_spans.extend(prefix_spans.clone());
                } else {
                    line_spans.push(Span::styled(" ".repeat(prefix_width), Style::default()));
                }
                line_spans.extend(wrap_line);
                line_group.push(Line::from(line_spans));
            }

            for line in line_group.into_iter().rev() {
                collected_lines.push(line);
                if collected_lines.len() >= inner_height {
                    break;
                }
            }
        } else {
            let mut spans = prefix_spans;
            for (text, style) in highlighted {
                spans.push(Span::styled(text, style));
            }
            collected_lines.push(Line::from(spans));
        }

        current_filtered_idx -= 1;
    }

    collected_lines.reverse();

    let para = Paragraph::new(collected_lines).block(block);
    frame.render_widget(para, area);
}

fn wrap_highlighted(spans: &[(String, Style)], width: usize) -> Vec<Vec<Span<'static>>> {
    if width == 0 {
        return vec![spans
            .iter()
            .map(|(t, s)| Span::styled(t.clone(), *s))
            .collect()];
    }

    let mut result: Vec<Vec<Span<'static>>> = vec![Vec::new()];
    let mut current_width = 0;

    for (text, style) in spans {
        let mut remaining = text.as_str();
        while !remaining.is_empty() {
            let available = width.saturating_sub(current_width);
            if available == 0 {
                result.push(Vec::new());
                current_width = 0;
                continue;
            }

            let take_chars: usize = remaining.chars().take(available).count();
            let byte_end = remaining
                .char_indices()
                .nth(take_chars)
                .map(|(i, _)| i)
                .unwrap_or(remaining.len());

            let (chunk, rest) = remaining.split_at(byte_end);
            if !chunk.is_empty() {
                result
                    .last_mut()
                    .unwrap()
                    .push(Span::styled(chunk.to_string(), *style));
                current_width += chunk.chars().count();
            }
            remaining = rest;

            if !remaining.is_empty() {
                result.push(Vec::new());
                current_width = 0;
            }
        }
    }

    result
}

fn draw_status_bar(frame: &mut Frame, app: &App, area: Rect) {
    let status = if let Some(msg) = &app.status_message {
        msg.clone()
    } else {
        let last_update = if let Some(time) = app.log_state.last_update_time {
            format!(" | Last: {}", format_relative_time(time))
        } else {
            String::new()
        };
        format!(
            "q:Quit d:Hide f:Filter h:Highlight s:LineStart c:Clear t:Time({}) w:Wrap({}){}",
            if app.show_time { "ON" } else { "OFF" },
            if app.wrap_lines { "ON" } else { "OFF" },
            last_update
        )
    };

    let paragraph =
        Paragraph::new(status).style(Style::default().fg(Color::White).bg(Color::Blue));
    frame.render_widget(paragraph, area);
}

fn draw_help_popup(frame: &mut Frame) {
    let area = frame.area();
    let popup_area = Rect {
        x: area.width.saturating_sub(HELP_POPUP_WIDTH).max(area.x),
        y: area.y,
        width: HELP_POPUP_WIDTH.min(area.width),
        height: HELP_POPUP_HEIGHT.min(area.height),
    };

    let help_text = vec![
        Line::from("Enter: Apply | Esc: Cancel | ←→: Move cursor"),
        Line::from("Syntax: pattern && !pattern || pattern"),
        Line::from("Use quotes for special chars: \"a||b\""),
    ];

    let help = Paragraph::new(help_text)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Help ")
                .border_style(Style::default().fg(Color::Green)),
        )
        .style(Style::default().bg(Color::Black));

    frame.render_widget(Clear, popup_area);
    frame.render_widget(help, popup_area);
}

fn draw_listen_popup(frame: &mut Frame, app: &mut App) {
    let port = app.listen_state.port.unwrap_or(0);
    let interfaces = &app.listen_state.network_interfaces;
    let display_mode = app.listen_state.display_mode;

    let mut max_addr_width: usize = 0;
    for iface in interfaces {
        let iface_width = iface.name.len() + if iface.is_default { 10 } else { 0 };
        max_addr_width = max_addr_width.max(iface_width);

        for addr_info in &iface.addresses {
            let is_v6 = addr_info.ip.is_ipv6();
            let addr_width = calc_addr_line_width(&addr_info.ip, port, is_v6, display_mode);
            max_addr_width = max_addr_width.max(addr_width);
        }
    }

    let header_width = "Mode (Tab): [addr:port]  nc command ".len();
    let max_content_width = max_addr_width.max(header_width);

    let mut lines: Vec<Line> = Vec::new();
    let mut addr_entries: Vec<ListenAddrEntry> = Vec::new();

    lines.push(Line::from(vec![
        Span::styled("Listening on port ", Style::default().fg(Color::White)),
        Span::styled(format!("{}", port), Style::default().fg(Color::Yellow)),
    ]));
    lines.push(Line::from(""));

    let mode_str = match display_mode {
        ListenDisplayMode::AddrPort => "[addr:port]  nc command ",
        ListenDisplayMode::NcCommand => " addr:port  [nc command]",
    };
    lines.push(Line::from(vec![
        Span::styled("Mode (Tab): ", Style::default().fg(Color::Gray)),
        Span::styled(mode_str, Style::default().fg(Color::Yellow)),
    ]));
    lines.push(Line::from(Span::styled(
        "↑↓:Select  Enter/Click:Copy",
        Style::default().fg(Color::Gray),
    )));
    lines.push(Line::from(""));

    if interfaces.is_empty() {
        lines.push(Line::from(Span::styled(
            "No network interfaces found",
            Style::default().fg(Color::Red),
        )));
    } else {
        let mut addr_idx = 0;
        for iface in interfaces {
            let name_style = if iface.is_default {
                Style::default().fg(Color::Green)
            } else {
                Style::default().fg(Color::Cyan)
            };

            let suffix = if iface.is_default { " (default)" } else { "" };

            lines.push(Line::from(vec![Span::styled(
                format!("{}{}", iface.name, suffix),
                name_style,
            )]));

            for addr_info in &iface.addresses {
                let is_v6 = addr_info.ip.is_ipv6();
                let is_selected = addr_idx == app.listen_state.selected_idx;
                let current_row = lines.len() as u16 + 1;

                addr_entries.push(ListenAddrEntry {
                    ip: addr_info.ip,
                    is_v6,
                    is_self_assigned: addr_info.is_self_assigned,
                    row: current_row,
                });

                let line = build_addr_line(
                    &addr_info.ip,
                    port,
                    is_v6,
                    addr_info.is_self_assigned,
                    is_selected,
                    display_mode,
                );
                lines.push(line);
                addr_idx += 1;
            }
        }
    }

    let content_height = lines.len() as u16 + 2;
    let max_width = (max_content_width + 4) as u16;

    let area = frame.area();
    let popup_width = max_width.min(area.width.saturating_sub(4)).max(45);
    let popup_height = content_height.min(area.height.saturating_sub(4)).max(8);

    let popup_area = Rect {
        x: area.width.saturating_sub(popup_width) / 2,
        y: area.height.saturating_sub(popup_height) / 2,
        width: popup_width,
        height: popup_height,
    };

    let popup = Paragraph::new(lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Network Info ")
                .border_style(Style::default().fg(Color::Magenta)),
        )
        .style(Style::default().bg(Color::Black));

    frame.render_widget(Clear, popup_area);
    frame.render_widget(popup, popup_area);

    app.listen_state.addr_list = addr_entries;
    app.listen_state.popup_area = Some((
        popup_area.x,
        popup_area.y,
        popup_area.width,
        popup_area.height,
    ));
}

fn calc_addr_line_width(
    ip: &std::net::IpAddr,
    port: u16,
    is_v6: bool,
    display_mode: ListenDisplayMode,
) -> usize {
    let prefix_len = 2;
    let ip_str = ip.to_string();
    let port_str = port.to_string();

    match display_mode {
        ListenDisplayMode::AddrPort => {
            if is_v6 {
                prefix_len + 1 + ip_str.len() + 1 + 1 + port_str.len()
            } else {
                prefix_len + ip_str.len() + 1 + port_str.len()
            }
        }
        ListenDisplayMode::NcCommand => {
            if is_v6 {
                prefix_len + 3 + 3 + ip_str.len() + 1 + port_str.len()
            } else {
                prefix_len + 3 + ip_str.len() + 1 + port_str.len()
            }
        }
    }
}

fn build_addr_line<'a>(
    ip: &std::net::IpAddr,
    port: u16,
    is_v6: bool,
    is_self_assigned: bool,
    is_selected: bool,
    display_mode: ListenDisplayMode,
) -> Line<'a> {
    let base_addr_style = if is_self_assigned {
        Style::default().fg(Color::DarkGray)
    } else if is_selected {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default().fg(Color::White)
    };

    let dim_style = if is_self_assigned {
        Style::default().fg(Color::DarkGray)
    } else {
        Style::default().fg(Color::Gray)
    };

    let prefix = if is_selected { "▶ " } else { "  " };
    let prefix_style = if is_selected {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default()
    };

    match display_mode {
        ListenDisplayMode::AddrPort => {
            if is_v6 {
                Line::from(vec![
                    Span::styled(prefix, prefix_style),
                    Span::styled("[", dim_style),
                    Span::styled(ip.to_string(), base_addr_style),
                    Span::styled("]", dim_style),
                    Span::styled(format!(":{}", port), dim_style),
                ])
            } else {
                Line::from(vec![
                    Span::styled(prefix, prefix_style),
                    Span::styled(ip.to_string(), base_addr_style),
                    Span::styled(format!(":{}", port), dim_style),
                ])
            }
        }
        ListenDisplayMode::NcCommand => {
            if is_v6 {
                Line::from(vec![
                    Span::styled(prefix, prefix_style),
                    Span::styled("nc ", dim_style),
                    Span::styled("-6 ", dim_style),
                    Span::styled(ip.to_string(), base_addr_style),
                    Span::styled(format!(" {}", port), dim_style),
                ])
            } else {
                Line::from(vec![
                    Span::styled(prefix, prefix_style),
                    Span::styled("nc ", dim_style),
                    Span::styled(ip.to_string(), base_addr_style),
                    Span::styled(format!(" {}", port), dim_style),
                ])
            }
        }
    }
}

fn draw_quit_confirm(frame: &mut Frame) {
    let area = frame.area();
    let popup_width = QUIT_POPUP_WIDTH.min(area.width.saturating_sub(4));
    let popup_height = QUIT_POPUP_HEIGHT.min(area.height.saturating_sub(4));

    let popup_area = Rect {
        x: area.width.saturating_sub(popup_width) / 2,
        y: area.height.saturating_sub(popup_height) / 2,
        width: popup_width,
        height: popup_height,
    };

    let text = vec![
        Line::from(""),
        Line::from(Span::styled(
            "  Press 'y' to quit, any other key to cancel",
            Style::default().fg(Color::White),
        )),
    ];

    let popup = Paragraph::new(text)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Quit? ")
                .border_style(Style::default().fg(Color::Red)),
        )
        .style(Style::default().bg(Color::Black));

    frame.render_widget(Clear, popup_area);
    frame.render_widget(popup, popup_area);
}
