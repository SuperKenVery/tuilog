use crate::app::{App, InputMode};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph},
    Frame,
};

pub fn draw(frame: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Min(1),
            Constraint::Length(1),
        ])
        .split(frame.area());

    draw_hide_input(frame, app, chunks[0]);
    draw_filter_input(frame, app, chunks[1]);
    draw_highlight_input(frame, app, chunks[2]);
    draw_log_view(frame, app, chunks[3]);
    draw_status_bar(frame, app, chunks[4]);

    if app.input_mode != InputMode::Normal {
        draw_help_popup(frame);
    }
}

fn draw_hide_input(frame: &mut Frame, app: &App, area: Rect) {
    let style = if app.input_mode == InputMode::HideEdit {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default()
    };

    let title = if app.hide_error.is_some() {
        format!(" Hide (Error: {}) ", app.hide_error.as_ref().unwrap())
    } else {
        " Hide (d) ".to_string()
    };

    let border_style = if app.hide_error.is_some() {
        Style::default().fg(Color::Red)
    } else {
        style
    };

    let input = Paragraph::new(app.hide_input.as_str())
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(title)
                .border_style(border_style),
        )
        .style(style);
    frame.render_widget(input, area);

    if app.input_mode == InputMode::HideEdit {
        frame.set_cursor_position((area.x + app.hide_input.len() as u16 + 1, area.y + 1));
    }
}

fn draw_filter_input(frame: &mut Frame, app: &App, area: Rect) {
    let style = if app.input_mode == InputMode::FilterEdit {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default()
    };

    let title = if app.filter_error.is_some() {
        format!(" Filter (Error: {}) ", app.filter_error.as_ref().unwrap())
    } else {
        " Filter (f) ".to_string()
    };

    let border_style = if app.filter_error.is_some() {
        Style::default().fg(Color::Red)
    } else {
        style
    };

    let input = Paragraph::new(app.filter_input.as_str())
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(title)
                .border_style(border_style),
        )
        .style(style);
    frame.render_widget(input, area);

    if app.input_mode == InputMode::FilterEdit {
        frame.set_cursor_position((area.x + app.filter_input.len() as u16 + 1, area.y + 1));
    }
}

fn draw_highlight_input(frame: &mut Frame, app: &App, area: Rect) {
    let style = if app.input_mode == InputMode::HighlightEdit {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default()
    };

    let title = if app.highlight_error.is_some() {
        format!(
            " Highlight (Error: {}) ",
            app.highlight_error.as_ref().unwrap()
        )
    } else {
        " Highlight (h) ".to_string()
    };

    let border_style = if app.highlight_error.is_some() {
        Style::default().fg(Color::Red)
    } else {
        style
    };

    let input = Paragraph::new(app.highlight_input.as_str())
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(title)
                .border_style(border_style),
        )
        .style(style);
    frame.render_widget(input, area);

    if app.input_mode == InputMode::HighlightEdit {
        frame.set_cursor_position((area.x + app.highlight_input.len() as u16 + 1, area.y + 1));
    }
}

fn draw_log_view(frame: &mut Frame, app: &App, area: Rect) {
    let inner_height = area.height.saturating_sub(2) as usize;
    let inner_width = area.width.saturating_sub(2) as usize;

    let title = format!(
        " Logs [{}/{}] {}{} ",
        app.filtered_indices.len(),
        app.lines.len(),
        if app.follow_tail { "[FOLLOW]" } else { "" },
        if app.wrap_lines { "[WRAP]" } else { "" }
    );

    let block = Block::default()
        .borders(Borders::ALL)
        .title(title)
        .border_style(Style::default().fg(Color::Cyan));

    if app.wrap_lines {
        let prefix_width = if app.show_time { 9 + 9 } else { 9 };
        let content_width = inner_width.saturating_sub(prefix_width);
        let visible_lines = app.get_visible_lines(inner_height);

        let mut lines: Vec<Line> = Vec::new();
        for (idx, line) in visible_lines {
            let mut prefix_spans = Vec::new();
            if app.show_time {
                prefix_spans.push(Span::styled(
                    format!("{} ", line.timestamp),
                    Style::default().fg(Color::DarkGray),
                ));
            }
            prefix_spans.push(Span::styled(
                format!("{:>6} │ ", idx + 1),
                Style::default().fg(Color::DarkGray),
            ));

            let highlighted = app.render_line(line);
            let wrapped = wrap_highlighted(&highlighted, content_width);

            for (i, wrap_line) in wrapped.into_iter().enumerate() {
                let mut line_spans = Vec::new();
                if i == 0 {
                    line_spans.extend(prefix_spans.clone());
                } else {
                    line_spans.push(Span::styled(
                        " ".repeat(prefix_width),
                        Style::default(),
                    ));
                }
                line_spans.extend(wrap_line);
                lines.push(Line::from(line_spans));
            }
        }

        let para = Paragraph::new(lines).block(block);
        frame.render_widget(para, area);
    } else {
        let visible_lines = app.get_visible_lines(inner_height);
        let items: Vec<ListItem> = visible_lines
            .iter()
            .map(|(idx, line)| {
                let mut spans = Vec::new();

                if app.show_time {
                    spans.push(Span::styled(
                        format!("{} ", line.timestamp),
                        Style::default().fg(Color::DarkGray),
                    ));
                }

                spans.push(Span::styled(
                    format!("{:>6} │ ", idx + 1),
                    Style::default().fg(Color::DarkGray),
                ));

                let highlighted = app.render_line(line);
                for (text, style) in highlighted {
                    spans.push(Span::styled(text, style));
                }

                ListItem::new(Line::from(spans))
            })
            .collect();

        let list = List::new(items).block(block);
        frame.render_widget(list, area);
    }
}

fn wrap_highlighted(spans: &[(String, Style)], width: usize) -> Vec<Vec<Span<'static>>> {
    if width == 0 {
        return vec![spans.iter().map(|(t, s)| Span::styled(t.clone(), *s)).collect()];
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
                result.last_mut().unwrap().push(Span::styled(chunk.to_string(), *style));
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
        format!(
            "q:Quit d:Hide f:Filter h:Highlight c:Clear t:Time({}) s:Syntax({}) w:Wrap({})",
            if app.show_time { "ON" } else { "OFF" },
            if app.heuristic_highlight { "ON" } else { "OFF" },
            if app.wrap_lines { "ON" } else { "OFF" }
        )
    };

    let paragraph = Paragraph::new(status).style(Style::default().fg(Color::White).bg(Color::Blue));
    frame.render_widget(paragraph, area);
}

fn draw_help_popup(frame: &mut Frame) {
    let area = frame.area();
    let popup_area = Rect {
        x: area.width.saturating_sub(40).max(area.x),
        y: area.y,
        width: 40.min(area.width),
        height: 5.min(area.height),
    };

    let help_text = vec![
        Line::from("Enter: Apply | Esc: Cancel"),
        Line::from("Syntax: pattern && pattern || pattern"),
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
