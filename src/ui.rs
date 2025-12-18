//! TUI rendering logic

use ansi_to_tui::IntoText;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout},
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
    render_footer(frame, vertical[1]);
}

/// Render the command list in the left pane
fn render_command_list(frame: &mut Frame, app: &mut App, area: ratatui::layout::Rect) {
    let items: Vec<ListItem> = app
        .blocks
        .iter()
        .enumerate()
        .map(|(i, block)| {
            // Use pre-computed clean command (ANSI stripped at parse time)
            let clean_cmd = &block.clean_command;

            // Truncate long commands
            let display = if clean_cmd.len() > 40 {
                format!("{}…", &clean_cmd[..39])
            } else {
                clean_cmd.clone()
            };

            ListItem::new(Line::from(format!("{:3} {}", i + 1, display)))
        })
        .collect();

    let title = if let Some(idx) = app.list_state.selected() {
        format!(" Commands ({}/{}) ", idx + 1, app.blocks.len())
    } else {
        " Commands ".to_string()
    };

    let list = List::new(items)
        .block(Block::default().borders(Borders::RIGHT).title(title))
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
    let content = if let Some(idx) = app.list_state.selected() {
        if let Some(block) = app.blocks.get(idx) {
            let full = format!("{}\n{}", block.command, block.output);
            let bytes = full.into_bytes();
            bytes
                .into_text()
                .unwrap_or_else(|_| "Error rendering".into())
        } else {
            "No selection".into()
        }
    } else {
        "Select a command with j/k...".into()
    };

    let paragraph = Paragraph::new(content).scroll((app.scroll_offset, 0));

    frame.render_widget(paragraph, area);
}

/// Render the help footer
fn render_footer(frame: &mut Frame, area: ratatui::layout::Rect) {
    let key_style = Style::default().fg(Color::Green);
    let desc_style = Style::default().fg(Color::White);
    let sep_style = Style::default().fg(Color::DarkGray);

    let help = Line::from(vec![
        Span::styled(" j/k ", key_style),
        Span::styled("navigate", desc_style),
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
