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
use unicode_width::UnicodeWidthStr;

use crate::app::{App, Mode, ViewSource};
use crate::parser::PathType;

/// Powerline glyphs for capsule-style tabs
const PL_LEFT_CAP: &str = "\u{e0b6}"; //
const PL_RIGHT_CAP: &str = "\u{e0b4}"; //

/// Truncate a string to fit within max_width display columns, adding ellipsis if needed.
/// Uses unicode width to handle multi-byte characters correctly.
fn truncate_to_width(s: &str, max_width: usize) -> String {
    let width = s.width();
    if width <= max_width {
        return s.to_string();
    }

    // Need to truncate - leave room for ellipsis
    let target_width = max_width.saturating_sub(1);
    let mut current_width = 0;
    let mut end_idx = 0;

    for (idx, ch) in s.char_indices() {
        let ch_width = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0);
        if current_width + ch_width > target_width {
            break;
        }
        current_width += ch_width;
        end_idx = idx + ch.len_utf8();
    }

    format!("{}…", &s[..end_idx])
}

/// Highlight matched characters in text and truncate to fit width.
/// Returns a vector of styled spans with matched characters highlighted.
fn highlight_text(
    text: &str,
    matches: Option<&Vec<usize>>,
    base_style: Style,
    highlight_style: Style,
    max_width: usize,
) -> Vec<Span<'static>> {
    let width = text.width();

    // If no matches and fits in width, return single span
    if matches.is_none() && width <= max_width {
        return vec![Span::styled(text.to_string(), base_style)];
    }

    let mut spans = Vec::new();
    let mut current_width = 0;
    // Reserve space for ellipsis if truncation is needed
    let target_width = if width > max_width {
        max_width.saturating_sub(1)
    } else {
        max_width
    };

    // Buffer for adjacent characters with same style
    let mut pending_chars = String::new();
    let mut pending_is_highlight = false;

    // Helper closure to flush buffer
    let flush =
        |chars: &mut String, is_highlight: bool, output: &mut Vec<Span<'static>>, hl, base| {
            if !chars.is_empty() {
                let style = if is_highlight { hl } else { base };
                output.push(Span::styled(std::mem::take(chars), style));
            }
        };

    for (byte_idx, ch) in text.char_indices() {
        let ch_width = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0);

        // Check for truncation
        if current_width + ch_width > target_width {
            flush(
                &mut pending_chars,
                pending_is_highlight,
                &mut spans,
                highlight_style,
                base_style,
            );
            spans.push(Span::styled("…", base_style));
            return spans;
        }

        // Binary search is O(log n), faster than HashSet build O(n) in render loop
        let is_match = matches.is_some_and(|m| m.binary_search(&byte_idx).is_ok());

        // State change: flush buffer
        if is_match != pending_is_highlight {
            flush(
                &mut pending_chars,
                pending_is_highlight,
                &mut spans,
                highlight_style,
                base_style,
            );
            pending_is_highlight = is_match;
        }

        pending_chars.push(ch);
        current_width += ch_width;
    }

    // Flush remaining
    flush(
        &mut pending_chars,
        pending_is_highlight,
        &mut spans,
        highlight_style,
        base_style,
    );
    spans
}

/// Build the standard list block with mode tabs and count indicator
fn build_list_block(
    mode: Mode,
    use_nerd_fonts: bool,
    view_source: ViewSource,
    count_title: Line<'static>,
) -> Block<'static> {
    let mode_tabs = build_mode_tabs(mode, use_nerd_fonts, view_source);
    Block::default()
        .borders(Borders::RIGHT)
        .border_style(Style::default().fg(Color::DarkGray))
        .title_top(mode_tabs)
        .title_top(count_title.alignment(Alignment::Right))
}

/// Build the count title "(X/Y)" for the list header
fn build_count_title(selected_idx: Option<usize>, total: usize) -> Line<'static> {
    if let Some(idx) = selected_idx {
        Line::from(vec![Span::styled(
            format!(" ({}/{}) ", idx + 1, total),
            Style::default().fg(Color::DarkGray),
        )])
    } else {
        Line::from(vec![])
    }
}

/// Create a styled List widget with standard highlight settings
fn styled_list(items: Vec<ListItem<'static>>, block: Block<'static>) -> List<'static> {
    List::new(items)
        .block(block)
        .highlight_style(
            Style::default()
                .add_modifier(Modifier::BOLD)
                .bg(Color::DarkGray),
        )
        .highlight_symbol("▶ ")
}

/// Build mode tabs (powerline capsules or plain brackets based on nerd_fonts setting)
fn build_mode_tabs(
    active_mode: Mode,
    use_nerd_fonts: bool,
    view_source: ViewSource,
) -> Line<'static> {
    let tabs = [
        ("Commands", Color::Green, Mode::Commands),
        ("Paths", Color::Magenta, Mode::Paths),
        ("JSON", Color::Blue, Mode::Json),
    ];

    let mut spans = Vec::new();

    for (i, (label, color, mode)) in tabs.iter().enumerate() {
        let is_active = *mode == active_mode;

        if i > 0 {
            // Add spacing between tabs
            spans.push(Span::raw("  "));
        }

        if is_active {
            if use_nerd_fonts {
                // Powerline style: colored caps + inverted text
                spans.push(Span::styled(PL_LEFT_CAP, Style::default().fg(*color)));
                spans.push(Span::styled(
                    *label,
                    Style::default()
                        .fg(Color::Black)
                        .bg(*color)
                        .add_modifier(Modifier::BOLD),
                ));
                spans.push(Span::styled(PL_RIGHT_CAP, Style::default().fg(*color)));
            } else {
                // Plain ASCII: [Label]
                spans.push(Span::styled("[", Style::default().fg(*color)));
                spans.push(Span::styled(
                    *label,
                    Style::default().fg(*color).add_modifier(Modifier::BOLD),
                ));
                spans.push(Span::styled("]", Style::default().fg(*color)));
            }
        } else {
            // Inactive tab: gray text with spacing to align
            spans.push(Span::raw(" "));
            spans.push(Span::styled(*label, Style::default().fg(Color::DarkGray)));
            spans.push(Span::raw(" "));
        }
    }

    // Add view source indicator
    let (source_label, source_color) = match view_source {
        ViewSource::Original => ("THIS", Color::Green),
        ViewSource::Previous => ("PREV", Color::Yellow),
        ViewSource::All => ("ALL", Color::Cyan),
    };
    spans.push(Span::raw("  "));
    // Simple tag style: muted bg with colored text
    spans.push(Span::styled(
        format!(" {} ", source_label),
        Style::default()
            .fg(source_color)
            .bg(Color::Rgb(50, 50, 50))
            .add_modifier(Modifier::BOLD),
    ));

    Line::from(spans)
}

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
        Mode::Paths => render_paths_list(frame, app, area),
    }
}

/// Render the command list in the left pane
fn render_command_list(frame: &mut Frame, app: &mut App, area: ratatui::layout::Rect) {
    let selected_idx = app.commands.state.selected();
    // Available width: area - border (1) - highlight symbol (2) - number (3) - marker (2)
    let max_width = area.width.saturating_sub(8) as usize;

    let items: Vec<ListItem> = app
        .commands
        .filtered_indices
        .iter()
        .enumerate()
        .map(|(visual_idx, &real_idx)| {
            let block = &app.commands.items[real_idx];
            let is_focused = selected_idx == Some(visual_idx);
            let is_pinned = app.selection.contains(&real_idx);
            let matches = app.match_indices.get(&real_idx);
            format_list_item(
                visual_idx,
                &block.clean_command,
                is_focused,
                is_pinned,
                max_width,
                matches,
            )
        })
        .collect();

    // Commands mode: show selection count if items are pinned, otherwise standard count
    let count_title = if !app.selection.is_empty() {
        Line::from(vec![Span::styled(
            format!(" {} selected ", app.selection.len()),
            Style::default().fg(Color::Yellow),
        )])
    } else {
        build_count_title(selected_idx, app.commands.filtered_indices.len())
    };

    let block = build_list_block(Mode::Commands, app.nerd_fonts, app.view_source, count_title);
    let list = styled_list(items, block);
    frame.render_stateful_widget(list, area, &mut app.commands.state);
}

/// Render the JSON list in the left pane
fn render_json_list(frame: &mut Frame, app: &mut App, area: ratatui::layout::Rect) {
    let selected_idx = app.jsons.state.selected();
    // Available width: area - border (1) - highlight symbol (2) - number (4)
    let max_width = area.width.saturating_sub(7) as usize;

    let items: Vec<ListItem> = app
        .jsons
        .filtered_indices
        .iter()
        .enumerate()
        .map(|(visual_idx, &real_idx)| {
            let json_block = &app.jsons.items[real_idx];
            let is_focused = selected_idx == Some(visual_idx);
            format_json_list_item(visual_idx, &json_block.name, is_focused, max_width)
        })
        .collect();

    let count_title = build_count_title(selected_idx, app.jsons.filtered_indices.len());
    let block = build_list_block(Mode::Json, app.nerd_fonts, app.view_source, count_title);
    let list = styled_list(items, block);
    frame.render_stateful_widget(list, area, &mut app.jsons.state);
}

/// Format a JSON list item
fn format_json_list_item(
    index: usize,
    name: &str,
    is_focused: bool,
    max_width: usize,
) -> ListItem<'static> {
    // Truncate long names to fit available width
    let display = truncate_to_width(name, max_width);

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

/// Render the paths/URLs list in the left pane
fn render_paths_list(frame: &mut Frame, app: &mut App, area: ratatui::layout::Rect) {
    let selected_idx = app.paths.state.selected();
    // Available width: area - border (1) - highlight symbol (2) - number (4) - icon (2)
    let max_width = area.width.saturating_sub(9) as usize;

    let use_nerd_fonts = app.nerd_fonts;
    let items: Vec<ListItem> = app
        .paths
        .filtered_indices
        .iter()
        .enumerate()
        .map(|(visual_idx, &real_idx)| {
            let path_block = &app.paths.items[real_idx];
            let is_focused = selected_idx == Some(visual_idx);
            format_path_list_item(visual_idx, path_block, is_focused, max_width, use_nerd_fonts)
        })
        .collect();

    let count_title = build_count_title(selected_idx, app.paths.filtered_indices.len());
    let block = build_list_block(Mode::Paths, app.nerd_fonts, app.view_source, count_title);
    let list = styled_list(items, block);
    frame.render_stateful_widget(list, area, &mut app.paths.state);
}

/// Format a path list item with type indicator
fn format_path_list_item(
    index: usize,
    block: &crate::parser::PathBlock,
    is_focused: bool,
    max_width: usize,
    use_nerd_fonts: bool,
) -> ListItem<'static> {
    // Type indicator
    let type_icon = if use_nerd_fonts {
        match block.kind {
            PathType::Url => "\u{f0ac} ",  // nf-fa-globe
            PathType::File => "\u{f4a5} ", // nf-oct-file
        }
    } else {
        match block.kind {
            PathType::Url => "@ ",
            PathType::File => "~ ",
        }
    };

    // Truncate long paths to fit available width
    let display = truncate_to_width(&block.raw, max_width);

    let num_style = if is_focused {
        Style::default().fg(Color::Magenta)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let icon_style = match block.kind {
        PathType::Url => Style::default().fg(Color::Cyan),
        PathType::File => Style::default().fg(Color::Yellow),
    };

    let path_style = Style::default().fg(Color::White);

    ListItem::new(Line::from(vec![
        Span::styled(format!("{:3} ", index + 1), num_style),
        Span::styled(type_icon, icon_style),
        Span::styled(display, path_style),
    ]))
}

/// Render the output pane on the right
fn render_output_pane(frame: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    let content = match app.mode {
        Mode::Commands => {
            // Convert ANSI escape codes to ratatui styled Text
            // Show command + output together
            if let Some(block) = app.commands.selected() {
                let full = format!("{}\n{}", block.command, block.output);
                let bytes = full.into_bytes();
                bytes
                    .into_text()
                    .unwrap_or_else(|_| "Error rendering".into())
            } else if app.commands.items.is_empty() {
                // No commands found - show diagnostic info
                Text::from(vec![
                    Line::from(Span::styled(
                        "No commands found.",
                        Style::default().fg(Color::Yellow),
                    )),
                    Line::from(""),
                    Line::from(vec![
                        Span::styled("Pattern: ", Style::default().fg(Color::DarkGray)),
                        Span::styled(
                            app.prompt_pattern.clone(),
                            Style::default().fg(Color::White),
                        ),
                    ]),
                    Line::from(""),
                    Line::from(Span::styled(
                        "Try 'tmux-snaglord init' to auto-detect your prompt.",
                        Style::default().fg(Color::DarkGray),
                    )),
                ])
            } else if app.commands.filtered_indices.is_empty() && !app.search_query.is_empty() {
                "No matching commands".into()
            } else {
                "Select a command with j/k...".into()
            }
        }
        Mode::Json => {
            if let Some(block) = app.jsons.selected() {
                json_to_text(&block.value, 2)
            } else if app.jsons.items.is_empty() {
                "No JSON objects found in history.".into()
            } else if app.jsons.filtered_indices.is_empty() && !app.search_query.is_empty() {
                "No matching JSON objects".into()
            } else {
                "Select a JSON object with j/k...".into()
            }
        }
        Mode::Paths => {
            if let Some(block) = app.paths.selected() {
                path_to_text(block)
            } else if app.paths.items.is_empty() {
                "No paths or URLs found in history.".into()
            } else if app.paths.filtered_indices.is_empty() && !app.search_query.is_empty() {
                "No matching paths".into()
            } else {
                "Select a path with j/k...".into()
            }
        }
    };

    let paragraph = Paragraph::new(content).scroll((app.scroll_offset, 0));

    frame.render_widget(paragraph, area);
}

/// Render the footer (help or search bar)
fn render_footer(frame: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    if let Some(ref err) = app.error_msg {
        render_error_bar(frame, err, area);
    } else if app.is_searching {
        render_search_bar(frame, app, area);
    } else {
        render_help_bar(frame, app, area);
    }
}

/// Render an error message bar
fn render_error_bar(frame: &mut Frame, error: &str, area: ratatui::layout::Rect) {
    let error_text = Line::from(vec![
        Span::styled(
            " ✗ ",
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        ),
        Span::styled(error, Style::default().fg(Color::Red)),
    ]);

    let paragraph = Paragraph::new(error_text).block(
        Block::default()
            .borders(Borders::TOP)
            .border_style(Style::default().fg(Color::Red)),
    );

    frame.render_widget(paragraph, area);
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
            Span::styled("1-3 ", key_style),
            Span::styled("mode", desc_style),
            Span::styled("  ·  ", sep_style),
            Span::styled("; ", key_style),
            Span::styled("prev pane", desc_style),
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
            Span::styled("P ", key_style),
            Span::styled("paste cmd+out", desc_style),
            Span::styled("  ·  ", sep_style),
            Span::styled("p ", key_style),
            Span::styled("paste out", desc_style),
            Span::styled("  ·  ", sep_style),
            Span::styled("q ", key_style),
            Span::styled("quit", desc_style),
        ]),
        Mode::Json => Line::from(vec![
            Span::styled("1-3 ", key_style),
            Span::styled("mode", desc_style),
            Span::styled("  ·  ", sep_style),
            Span::styled("; ", key_style),
            Span::styled("prev pane", desc_style),
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
            Span::styled("P ", key_style),
            Span::styled("paste pretty", desc_style),
            Span::styled("  ·  ", sep_style),
            Span::styled("p ", key_style),
            Span::styled("paste raw", desc_style),
            Span::styled("  ·  ", sep_style),
            Span::styled("q ", key_style),
            Span::styled("quit", desc_style),
        ]),
        Mode::Paths => Line::from(vec![
            Span::styled("1-3 ", key_style),
            Span::styled("mode", desc_style),
            Span::styled("  ·  ", sep_style),
            Span::styled("; ", key_style),
            Span::styled("prev pane", desc_style),
            Span::styled("  ·  ", sep_style),
            Span::styled("/ ", key_style),
            Span::styled("search", desc_style),
            Span::styled("  ·  ", sep_style),
            Span::styled("j/k ", key_style),
            Span::styled("nav", desc_style),
            Span::styled("  ·  ", sep_style),
            Span::styled("y ", key_style),
            Span::styled("full", desc_style),
            Span::styled("  ·  ", sep_style),
            Span::styled("Y ", key_style),
            Span::styled("path", desc_style),
            Span::styled("  ·  ", sep_style),
            Span::styled("P ", key_style),
            Span::styled("paste path", desc_style),
            Span::styled("  ·  ", sep_style),
            Span::styled("p ", key_style),
            Span::styled("paste full", desc_style),
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

/// Format a single list item with consistent styling and optional match highlighting
fn format_list_item(
    index: usize,
    command: &str,
    is_focused: bool,
    is_pinned: bool,
    max_width: usize,
    matches: Option<&Vec<usize>>,
) -> ListItem<'static> {
    let num_style = if is_focused {
        Style::default().fg(Color::Green)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let base_cmd_style = if is_pinned {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default().fg(Color::White)
    };

    // Highlight style: light green (similar to fzf) for matched characters
    let highlight_style = Style::default()
        .fg(Color::LightGreen)
        .add_modifier(Modifier::BOLD);

    let marker = if is_pinned { "* " } else { "  " };

    // Generate highlighted spans for the command text
    let command_spans = highlight_text(command, matches, base_cmd_style, highlight_style, max_width);

    let mut line_spans = vec![
        Span::styled(format!("{:3}", index + 1), num_style),
        Span::styled(marker, Style::default().fg(Color::Yellow)),
    ];

    // Append the highlighted text spans
    line_spans.extend(command_spans);

    ListItem::new(Line::from(line_spans))
}

// === Path Display ===

/// Convert a PathBlock to styled Text for preview pane
fn path_to_text(block: &crate::parser::PathBlock) -> Text<'static> {
    let mut lines = Vec::new();

    // Type label
    let type_label = match block.kind {
        PathType::Url => ("URL", Color::Cyan),
        PathType::File => ("File", Color::Yellow),
    };

    lines.push(Line::from(vec![
        Span::styled("Type: ", Style::default().fg(Color::DarkGray)),
        Span::styled(type_label.0, Style::default().fg(type_label.1)),
    ]));

    lines.push(Line::from(""));

    // Full path/URL
    lines.push(Line::from(vec![
        Span::styled("Full: ", Style::default().fg(Color::DarkGray)),
        Span::styled(block.raw.clone(), Style::default().fg(Color::White)),
    ]));

    // If path has line/col info, show separately
    if block.line.is_some() || block.col.is_some() {
        lines.push(Line::from(""));

        lines.push(Line::from(vec![
            Span::styled("Path: ", Style::default().fg(Color::DarkGray)),
            Span::styled(block.path.clone(), Style::default().fg(Color::Green)),
        ]));

        if let Some(line) = block.line {
            lines.push(Line::from(vec![
                Span::styled("Line: ", Style::default().fg(Color::DarkGray)),
                Span::styled(line.to_string(), Style::default().fg(Color::Yellow)),
            ]));
        }

        if let Some(col) = block.col {
            lines.push(Line::from(vec![
                Span::styled("Col:  ", Style::default().fg(Color::DarkGray)),
                Span::styled(col.to_string(), Style::default().fg(Color::Yellow)),
            ]));
        }
    }

    lines.push(Line::from(""));
    lines.push(Line::from(""));

    // Copy hints
    lines.push(Line::from(vec![Span::styled(
        "Copy options:",
        Style::default()
            .fg(Color::DarkGray)
            .add_modifier(Modifier::BOLD),
    )]));
    lines.push(Line::from(vec![
        Span::styled("  y ", Style::default().fg(Color::Magenta)),
        Span::styled("full path with line:col", Style::default().fg(Color::White)),
    ]));
    lines.push(Line::from(vec![
        Span::styled("  Y ", Style::default().fg(Color::Magenta)),
        Span::styled("path only (no line:col)", Style::default().fg(Color::White)),
    ]));

    Text::from(lines)
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
                render_json_item(
                    item,
                    indent_level + 1,
                    indent_size,
                    i < arr.len() - 1,
                    lines,
                );
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
        line.spans
            .push(Span::styled(",", json_style::punctuation()));
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
                    spans.push(Span::styled(
                        format!("\"{}\"", escaped),
                        json_style::string(),
                    ));
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
                render_json_item(
                    item,
                    indent_level + 1,
                    indent_size,
                    i < arr.len() - 1,
                    lines,
                );
            }

            // Closing bracket
            let mut closing = vec![Span::raw(indent), Span::styled("]", json_style::bracket())];
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
            let mut closing = vec![Span::raw(indent), Span::styled("}", json_style::bracket())];
            if trailing_comma {
                closing.push(Span::styled(",", json_style::punctuation()));
            }
            lines.push(Line::from(closing));
        }
    }
}
