//! TUI rendering logic

use ansi_to_tui::IntoText;
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, List, ListItem, Paragraph},
};
use serde_json::Value;

use crate::app::{App, Mode};

/// Render the application UI
pub fn render(frame: &mut Frame, app: &mut App) {
    // Split into main area and footer (2 lines for border + text)
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(2)])
        .split(frame.area());

    // Split main area into left (30%) and right (70%) panes
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(30), Constraint::Percentage(70)])
        .split(vertical[0]);

    render_list_pane(frame, app, chunks[0]);
    render_output_pane(frame, app, chunks[1]);
    render_footer(frame, app, vertical[1]);
}

/// Render the left list pane based on current mode
fn render_list_pane(frame: &mut Frame, app: &mut App, area: ratatui::layout::Rect) {
    match app.mode {
        Mode::Commands => render_command_list(frame, app, area),
        Mode::Json => render_json_list(frame, app, area),
    }
}

/// Render the command list in the left pane
fn render_command_list(frame: &mut Frame, app: &mut App, area: ratatui::layout::Rect) {
    let selected_idx = app.list_state.selected();

    let items: Vec<ListItem> = app
        .filtered_indices
        .iter()
        .enumerate()
        .map(|(visual_idx, &real_idx)| {
            let block = &app.blocks[real_idx];
            let is_focused = selected_idx == Some(visual_idx);
            let is_pinned = app.selection.contains(&real_idx);
            format_list_item(visual_idx, &block.clean_command, is_focused, is_pinned)
        })
        .collect();

    // Build title with mode tabs and count info
    let mode_tabs = Line::from(vec![
        Span::styled(
            "[Commands]",
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" ", Style::default()),
        Span::styled("JSON", Style::default().fg(Color::DarkGray)),
    ]);

    // Left title: shows selection count if items are pinned, otherwise "Commands (X/Y)"
    let left_title = if !app.selection.is_empty() {
        Line::from(vec![Span::styled(
            format!(" {} selected ", app.selection.len()),
            Style::default().fg(Color::Yellow),
        )])
    } else if let Some(idx) = selected_idx {
        Line::from(vec![Span::styled(
            format!(" ({}/{}) ", idx + 1, app.filtered_indices.len()),
            Style::default().fg(Color::DarkGray),
        )])
    } else {
        Line::from(vec![])
    };

    // Build block with mode tabs and count
    let mut block = Block::default()
        .borders(Borders::RIGHT)
        .border_style(Style::default().fg(Color::DarkGray))
        .title_top(mode_tabs)
        .title_top(left_title.alignment(Alignment::Right));

    // Add right-aligned filter indicator when search is active
    if !app.search_query.is_empty() {
        let filter_title = Line::from(vec![Span::styled(
            format!(
                " \"{}\" ({} of {}) ",
                app.search_query,
                app.filtered_indices.len(),
                app.blocks.len()
            ),
            Style::default().fg(Color::Yellow),
        )]);
        block = block.title_top(filter_title.alignment(Alignment::Right));
    }

    let list = List::new(items)
        .block(block)
        .highlight_style(
            Style::default()
                .add_modifier(Modifier::BOLD)
                .bg(Color::DarkGray),
        )
        .highlight_symbol("▶ ");

    frame.render_stateful_widget(list, area, &mut app.list_state);
}

/// Render the JSON list in the left pane
fn render_json_list(frame: &mut Frame, app: &mut App, area: ratatui::layout::Rect) {
    let selected_idx = app.json_list_state.selected();

    let items: Vec<ListItem> = app
        .json_filtered_indices
        .iter()
        .enumerate()
        .map(|(visual_idx, &real_idx)| {
            let block = &app.json_blocks[real_idx];
            let is_focused = selected_idx == Some(visual_idx);
            format_json_list_item(visual_idx, &block.name, is_focused)
        })
        .collect();

    // Build title with mode tabs
    let mode_tabs = Line::from(vec![
        Span::styled("Commands", Style::default().fg(Color::DarkGray)),
        Span::styled(" ", Style::default()),
        Span::styled(
            "[JSON]",
            Style::default()
                .fg(Color::Blue)
                .add_modifier(Modifier::BOLD),
        ),
    ]);

    // Count info
    let count_title = if let Some(idx) = selected_idx {
        Line::from(vec![Span::styled(
            format!(" ({}/{}) ", idx + 1, app.json_filtered_indices.len()),
            Style::default().fg(Color::DarkGray),
        )])
    } else {
        Line::from(vec![])
    };

    let mut block = Block::default()
        .borders(Borders::RIGHT)
        .border_style(Style::default().fg(Color::DarkGray))
        .title_top(mode_tabs)
        .title_top(count_title.alignment(Alignment::Right));

    // Add search filter indicator
    if !app.search_query.is_empty() {
        let filter_title = Line::from(vec![Span::styled(
            format!(
                " \"{}\" ({} of {}) ",
                app.search_query,
                app.json_filtered_indices.len(),
                app.json_blocks.len()
            ),
            Style::default().fg(Color::Yellow),
        )]);
        block = block.title_bottom(filter_title);
    }

    let list = List::new(items)
        .block(block)
        .highlight_style(
            Style::default()
                .add_modifier(Modifier::BOLD)
                .bg(Color::DarkGray),
        )
        .highlight_symbol("▶ ");

    frame.render_stateful_widget(list, area, &mut app.json_list_state);
}

/// Format a JSON list item
fn format_json_list_item(index: usize, name: &str, is_focused: bool) -> ListItem<'static> {
    // Truncate long names
    let display = if name.len() > 38 {
        format!("{}…", &name[..37])
    } else {
        name.to_string()
    };

    let num_style = if is_focused {
        Style::default().fg(Color::Blue)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let name_style = Style::default().fg(Color::White);

    ListItem::new(Line::from(vec![
        Span::styled(format!("{:3} ", index + 1), num_style),
        Span::styled(display, name_style),
    ]))
}

/// Render the output pane on the right
fn render_output_pane(frame: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    let content = match app.mode {
        Mode::Commands => {
            // Convert ANSI escape codes to ratatui styled Text
            // Show command + output together
            if let Some(block) = app.get_selected_block() {
                let full = format!("{}\n{}", block.command, block.output);
                let bytes = full.into_bytes();
                bytes
                    .into_text()
                    .unwrap_or_else(|_| "Error rendering".into())
            } else if app.filtered_indices.is_empty() && !app.search_query.is_empty() {
                "No matching commands".into()
            } else {
                "Select a command with j/k...".into()
            }
        }
        Mode::Json => {
            if let Some(block) = app.get_selected_json_block() {
                json_to_text(&block.value, 2)
            } else if app.json_blocks.is_empty() {
                "No JSON objects found in history.".into()
            } else if app.json_filtered_indices.is_empty() && !app.search_query.is_empty() {
                "No matching JSON objects".into()
            } else {
                "Select a JSON object with j/k...".into()
            }
        }
    };

    let paragraph = Paragraph::new(content).scroll((app.scroll_offset, 0));

    frame.render_widget(paragraph, area);
}

/// Render the footer (help or search bar)
fn render_footer(frame: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    if app.is_searching {
        render_search_bar(frame, app, area);
    } else {
        render_help_bar(frame, app, area);
    }
}

/// Render the search bar
fn render_search_bar(frame: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    let search_text = Line::from(vec![
        Span::styled(" / ", Style::default().fg(Color::Yellow)),
        Span::styled(&app.search_query, Style::default().fg(Color::White)),
        Span::styled("▏", Style::default().fg(Color::Yellow)), // Cursor
    ]);

    let paragraph = Paragraph::new(search_text).block(
        Block::default()
            .borders(Borders::TOP)
            .border_style(Style::default().fg(Color::Yellow)),
    );

    frame.render_widget(paragraph, area);
}

/// Render the help bar
fn render_help_bar(frame: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    let key_style = Style::default().fg(Color::Green);
    let desc_style = Style::default().fg(Color::White);
    let sep_style = Style::default().fg(Color::DarkGray);

    let help = match app.mode {
        Mode::Commands => Line::from(vec![
            Span::styled("TAB ", key_style),
            Span::styled("mode", desc_style),
            Span::styled("  ·  ", sep_style),
            Span::styled("/ ", key_style),
            Span::styled("search", desc_style),
            Span::styled("  ·  ", sep_style),
            Span::styled("j/k ", key_style),
            Span::styled("nav", desc_style),
            Span::styled("  ·  ", sep_style),
            Span::styled("spc ", key_style),
            Span::styled("pin", desc_style),
            Span::styled("  ·  ", sep_style),
            Span::styled("Y ", key_style),
            Span::styled("cmd+out", desc_style),
            Span::styled("  ·  ", sep_style),
            Span::styled("y ", key_style),
            Span::styled("out", desc_style),
            Span::styled("  ·  ", sep_style),
            Span::styled("c ", key_style),
            Span::styled("cmd", desc_style),
            Span::styled("  ·  ", sep_style),
            Span::styled("q ", key_style),
            Span::styled("quit", desc_style),
        ]),
        Mode::Json => Line::from(vec![
            Span::styled("TAB ", key_style),
            Span::styled("mode", desc_style),
            Span::styled("  ·  ", sep_style),
            Span::styled("/ ", key_style),
            Span::styled("search", desc_style),
            Span::styled("  ·  ", sep_style),
            Span::styled("j/k ", key_style),
            Span::styled("nav", desc_style),
            Span::styled("  ·  ", sep_style),
            Span::styled("y ", key_style),
            Span::styled("raw", desc_style),
            Span::styled("  ·  ", sep_style),
            Span::styled("Y ", key_style),
            Span::styled("pretty", desc_style),
            Span::styled("  ·  ", sep_style),
            Span::styled("q ", key_style),
            Span::styled("quit", desc_style),
        ]),
    };

    let paragraph = Paragraph::new(help).block(
        Block::default()
            .borders(Borders::TOP)
            .border_style(Style::default().fg(Color::DarkGray)),
    );

    frame.render_widget(paragraph, area);
}

/// Format a single list item with consistent styling
fn format_list_item(
    index: usize,
    command: &str,
    is_focused: bool,
    is_pinned: bool,
) -> ListItem<'static> {
    // Truncate long commands (reduced to make room for pin marker)
    let display = if command.len() > 38 {
        format!("{}…", &command[..37])
    } else {
        command.to_string()
    };

    let num_style = if is_focused {
        Style::default().fg(Color::Green)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let cmd_style = if is_pinned {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default().fg(Color::White)
    };

    let marker = if is_pinned { "* " } else { "  " };

    ListItem::new(Line::from(vec![
        Span::styled(format!("{:3}", index + 1), num_style),
        Span::styled(marker, Style::default().fg(Color::Yellow)),
        Span::styled(display, cmd_style),
    ]))
}

// === JSON Syntax Highlighting ===

/// Convert a JSON value to syntax-highlighted ratatui Text
fn json_to_text(value: &Value, indent_size: usize) -> Text<'static> {
    let mut lines = Vec::new();
    render_json_value(value, 0, indent_size, &mut lines);
    Text::from(lines)
}

/// Styles for JSON syntax highlighting
mod json_style {
    use ratatui::style::{Color, Style};

    pub fn key() -> Style {
        Style::default().fg(Color::Cyan)
    }
    pub fn string() -> Style {
        Style::default().fg(Color::Green)
    }
    pub fn number() -> Style {
        Style::default().fg(Color::Yellow)
    }
    pub fn boolean() -> Style {
        Style::default().fg(Color::Magenta)
    }
    pub fn null() -> Style {
        Style::default().fg(Color::Red)
    }
    pub fn bracket() -> Style {
        Style::default().fg(Color::White)
    }
    pub fn punctuation() -> Style {
        Style::default().fg(Color::DarkGray)
    }
}

/// Recursively render a JSON value with syntax highlighting
fn render_json_value(
    value: &Value,
    indent_level: usize,
    indent_size: usize,
    lines: &mut Vec<Line<'static>>,
) {
    let indent = " ".repeat(indent_level * indent_size);

    match value {
        Value::Null => {
            lines.push(Line::from(vec![
                Span::raw(indent),
                Span::styled("null", json_style::null()),
            ]));
        }
        Value::Bool(b) => {
            lines.push(Line::from(vec![
                Span::raw(indent),
                Span::styled(b.to_string(), json_style::boolean()),
            ]));
        }
        Value::Number(n) => {
            lines.push(Line::from(vec![
                Span::raw(indent),
                Span::styled(n.to_string(), json_style::number()),
            ]));
        }
        Value::String(s) => {
            // Escape special characters for display
            let escaped = s.replace('\\', "\\\\").replace('"', "\\\"");
            lines.push(Line::from(vec![
                Span::raw(indent),
                Span::styled(format!("\"{}\"", escaped), json_style::string()),
            ]));
        }
        Value::Array(arr) => {
            if arr.is_empty() {
                lines.push(Line::from(vec![
                    Span::raw(indent),
                    Span::styled("[]", json_style::bracket()),
                ]));
                return;
            }

            lines.push(Line::from(vec![
                Span::raw(indent.clone()),
                Span::styled("[", json_style::bracket()),
            ]));

            for (i, item) in arr.iter().enumerate() {
                render_json_item(item, indent_level + 1, indent_size, i < arr.len() - 1, lines);
            }

            lines.push(Line::from(vec![
                Span::raw(indent),
                Span::styled("]", json_style::bracket()),
            ]));
        }
        Value::Object(obj) => {
            if obj.is_empty() {
                lines.push(Line::from(vec![
                    Span::raw(indent),
                    Span::styled("{}", json_style::bracket()),
                ]));
                return;
            }

            lines.push(Line::from(vec![
                Span::raw(indent.clone()),
                Span::styled("{", json_style::bracket()),
            ]));

            let len = obj.len();
            for (i, (key, val)) in obj.iter().enumerate() {
                render_json_key_value(key, val, indent_level + 1, indent_size, i < len - 1, lines);
            }

            lines.push(Line::from(vec![
                Span::raw(indent),
                Span::styled("}", json_style::bracket()),
            ]));
        }
    }
}

/// Render an array item with proper comma handling
fn render_json_item(
    value: &Value,
    indent_level: usize,
    indent_size: usize,
    trailing_comma: bool,
    lines: &mut Vec<Line<'static>>,
) {
    let start_idx = lines.len();
    render_json_value(value, indent_level, indent_size, lines);

    // Add trailing comma to the last line of this item
    if trailing_comma
        && let Some(last) = lines.get_mut(start_idx..)
        && let Some(line) = last.last_mut()
    {
        line.spans.push(Span::styled(",", json_style::punctuation()));
    }
}

/// Render a key-value pair in an object
fn render_json_key_value(
    key: &str,
    value: &Value,
    indent_level: usize,
    indent_size: usize,
    trailing_comma: bool,
    lines: &mut Vec<Line<'static>>,
) {
    let indent = " ".repeat(indent_level * indent_size);

    match value {
        // Primitives: key and value on same line
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {
            let mut spans = vec![
                Span::raw(indent),
                Span::styled(format!("\"{}\"", key), json_style::key()),
                Span::styled(": ", json_style::punctuation()),
            ];

            // Add the value inline
            match value {
                Value::Null => spans.push(Span::styled("null", json_style::null())),
                Value::Bool(b) => spans.push(Span::styled(b.to_string(), json_style::boolean())),
                Value::Number(n) => spans.push(Span::styled(n.to_string(), json_style::number())),
                Value::String(s) => {
                    let escaped = s.replace('\\', "\\\\").replace('"', "\\\"");
                    spans.push(Span::styled(format!("\"{}\"", escaped), json_style::string()));
                }
                _ => unreachable!(),
            }

            if trailing_comma {
                spans.push(Span::styled(",", json_style::punctuation()));
            }

            lines.push(Line::from(spans));
        }
        // Complex types: opening bracket on same line as key
        Value::Array(arr) => {
            if arr.is_empty() {
                let mut spans = vec![
                    Span::raw(indent),
                    Span::styled(format!("\"{}\"", key), json_style::key()),
                    Span::styled(": ", json_style::punctuation()),
                    Span::styled("[]", json_style::bracket()),
                ];
                if trailing_comma {
                    spans.push(Span::styled(",", json_style::punctuation()));
                }
                lines.push(Line::from(spans));
                return;
            }

            // Key with opening bracket
            lines.push(Line::from(vec![
                Span::raw(indent.clone()),
                Span::styled(format!("\"{}\"", key), json_style::key()),
                Span::styled(": ", json_style::punctuation()),
                Span::styled("[", json_style::bracket()),
            ]));

            // Array contents
            for (i, item) in arr.iter().enumerate() {
                render_json_item(item, indent_level + 1, indent_size, i < arr.len() - 1, lines);
            }

            // Closing bracket
            let mut closing = vec![
                Span::raw(indent),
                Span::styled("]", json_style::bracket()),
            ];
            if trailing_comma {
                closing.push(Span::styled(",", json_style::punctuation()));
            }
            lines.push(Line::from(closing));
        }
        Value::Object(obj) => {
            if obj.is_empty() {
                let mut spans = vec![
                    Span::raw(indent),
                    Span::styled(format!("\"{}\"", key), json_style::key()),
                    Span::styled(": ", json_style::punctuation()),
                    Span::styled("{}", json_style::bracket()),
                ];
                if trailing_comma {
                    spans.push(Span::styled(",", json_style::punctuation()));
                }
                lines.push(Line::from(spans));
                return;
            }

            // Key with opening brace
            lines.push(Line::from(vec![
                Span::raw(indent.clone()),
                Span::styled(format!("\"{}\"", key), json_style::key()),
                Span::styled(": ", json_style::punctuation()),
                Span::styled("{", json_style::bracket()),
            ]));

            // Object contents
            let len = obj.len();
            for (i, (k, v)) in obj.iter().enumerate() {
                render_json_key_value(k, v, indent_level + 1, indent_size, i < len - 1, lines);
            }

            // Closing brace
            let mut closing = vec![
                Span::raw(indent),
                Span::styled("}", json_style::bracket()),
            ];
            if trailing_comma {
                closing.push(Span::styled(",", json_style::punctuation()));
            }
            lines.push(Line::from(closing));
        }
    }
}
