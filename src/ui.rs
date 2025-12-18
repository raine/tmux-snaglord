//! TUI rendering logic

use ansi_to_tui::IntoText;
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph},
};

use crate::app::App;

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

    render_command_list(frame, app, chunks[0]);
    render_output_pane(frame, app, chunks[1]);
    render_footer(frame, app, vertical[1]);
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
            // Use pre-computed clean command (ANSI stripped at parse time)
            let clean_cmd = &block.clean_command;

            // Truncate long commands
            let display = if clean_cmd.len() > 40 {
                format!("{}…", &clean_cmd[..39])
            } else {
                clean_cmd.clone()
            };

            // Style: dim line numbers, white commands
            let is_selected = selected_idx == Some(visual_idx);
            let num_style = if is_selected {
                Style::default().fg(Color::Green)
            } else {
                Style::default().fg(Color::DarkGray)
            };
            let cmd_style = Style::default().fg(Color::White);

            ListItem::new(Line::from(vec![
                Span::styled(format!("{:3} ", visual_idx + 1), num_style),
                Span::styled(display, cmd_style),
            ]))
        })
        .collect();

    // Left title: always shows "Commands (X/Y)"
    let left_title = if let Some(idx) = selected_idx {
        Line::from(vec![Span::styled(
            format!(" Commands ({}/{}) ", idx + 1, app.filtered_indices.len()),
            Style::default().fg(Color::Green),
        )])
    } else {
        Line::from(vec![Span::styled(
            " Commands ",
            Style::default().fg(Color::Green),
        )])
    };

    // Build block with left title
    let mut block = Block::default()
        .borders(Borders::RIGHT)
        .border_style(Style::default().fg(Color::DarkGray))
        .title_top(left_title);

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

/// Render the output pane on the right
fn render_output_pane(frame: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    // Convert ANSI escape codes to ratatui styled Text
    // Show command + output together
    let content = if let Some(block) = app.get_selected_block() {
        let full = format!("{}\n{}", block.command, block.output);
        let bytes = full.into_bytes();
        bytes
            .into_text()
            .unwrap_or_else(|_| "Error rendering".into())
    } else if app.filtered_indices.is_empty() && !app.search_query.is_empty() {
        "No matching commands".into()
    } else {
        "Select a command with j/k...".into()
    };

    let paragraph = Paragraph::new(content).scroll((app.scroll_offset, 0));

    frame.render_widget(paragraph, area);
}

/// Render the footer (help or search bar)
fn render_footer(frame: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    if app.is_searching {
        render_search_bar(frame, app, area);
    } else {
        render_help_bar(frame, area);
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
fn render_help_bar(frame: &mut Frame, area: ratatui::layout::Rect) {
    let key_style = Style::default().fg(Color::Green);
    let desc_style = Style::default().fg(Color::White);
    let sep_style = Style::default().fg(Color::DarkGray);

    let help = Line::from(vec![
        Span::styled(" / ", key_style),
        Span::styled("search", desc_style),
        Span::styled("  ·  ", sep_style),
        Span::styled("j/k ", key_style),
        Span::styled("nav", desc_style),
        Span::styled("  ·  ", sep_style),
        Span::styled("y ", key_style),
        Span::styled("output", desc_style),
        Span::styled("  ·  ", sep_style),
        Span::styled("Y ", key_style),
        Span::styled("all", desc_style),
        Span::styled("  ·  ", sep_style),
        Span::styled("c ", key_style),
        Span::styled("cmd", desc_style),
        Span::styled("  ·  ", sep_style),
        Span::styled("q ", key_style),
        Span::styled("quit", desc_style),
    ]);

    let paragraph = Paragraph::new(help).block(
        Block::default()
            .borders(Borders::TOP)
            .border_style(Style::default().fg(Color::DarkGray)),
    );

    frame.render_widget(paragraph, area);
}
