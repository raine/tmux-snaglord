//! Application state management

use ratatui::widgets::ListState;

use crate::parser::CommandBlock;

/// Main application state
pub struct App {
    /// Parsed command blocks
    pub blocks: Vec<CommandBlock>,
    /// State for the command list widget
    pub list_state: ListState,
    /// Vertical scroll offset for the output pane
    pub scroll_offset: u16,
}

impl App {
    /// Create a new App with the given command blocks
    pub fn new(blocks: Vec<CommandBlock>) -> Self {
        let mut list_state = ListState::default();
        // Select first item if available
        if !blocks.is_empty() {
            list_state.select(Some(0));
        }

        Self {
            blocks,
            list_state,
            scroll_offset: 0,
        }
    }

    /// Move selection to the next item
    pub fn next(&mut self) {
        if self.blocks.is_empty() {
            return;
        }

        let i = match self.list_state.selected() {
            Some(i) => {
                if i >= self.blocks.len() - 1 {
                    0 // Wrap to beginning
                } else {
                    i + 1
                }
            }
            None => 0,
        };
        self.list_state.select(Some(i));
        self.scroll_offset = 0; // Reset scroll when changing selection
    }

    /// Move selection to the previous item
    pub fn previous(&mut self) {
        if self.blocks.is_empty() {
            return;
        }

        let i = match self.list_state.selected() {
            Some(i) => {
                if i == 0 {
                    self.blocks.len() - 1 // Wrap to end
                } else {
                    i - 1
                }
            }
            None => 0,
        };
        self.list_state.select(Some(i));
        self.scroll_offset = 0; // Reset scroll when changing selection
    }

    /// Scroll the output pane down
    pub fn scroll_down(&mut self) {
        self.scroll_offset = self.scroll_offset.saturating_add(1);
    }

    /// Scroll the output pane up
    pub fn scroll_up(&mut self) {
        self.scroll_offset = self.scroll_offset.saturating_sub(1);
    }

    /// Get the output of the currently selected block (ANSI stripped for copying)
    pub fn get_selected_output(&self) -> Option<String> {
        self.list_state
            .selected()
            .and_then(|i| self.blocks.get(i))
            .map(|b| strip_ansi(&b.output))
    }

    /// Get the command of the currently selected block (ANSI stripped for copying)
    pub fn get_selected_command(&self) -> Option<String> {
        self.list_state
            .selected()
            .and_then(|i| self.blocks.get(i))
            .map(|b| strip_ansi(&b.command))
    }

    /// Get the full content (command + output) of the currently selected block (ANSI stripped)
    pub fn get_selected_full(&self) -> Option<String> {
        self.list_state
            .selected()
            .and_then(|i| self.blocks.get(i))
            .map(|b| format!("{}\n{}", strip_ansi(&b.command), strip_ansi(&b.output)))
    }

    /// Get debug-formatted output for diagnosing parsing issues
    pub fn get_selected_debug(&self) -> Option<String> {
        self.list_state
            .selected()
            .and_then(|i| self.blocks.get(i))
            .map(|b| {
                let mut out = String::new();
                out.push_str("=== COMMAND (raw) ===\n");
                for (i, line) in b.command.lines().enumerate() {
                    out.push_str(&format!("{:3}| {}\n", i + 1, escape_debug(line)));
                }
                out.push_str("\n=== COMMAND (clean) ===\n");
                for (i, line) in b.clean_command.lines().enumerate() {
                    out.push_str(&format!("{:3}| {}\n", i + 1, line));
                }
                out.push_str("\n=== OUTPUT (raw) ===\n");
                if b.output.is_empty() {
                    out.push_str("(empty)\n");
                } else {
                    for (i, line) in b.output.lines().enumerate() {
                        out.push_str(&format!("{:3}| {}\n", i + 1, escape_debug(line)));
                    }
                }
                out
            })
    }
}

/// Strip ANSI escape codes from a string
fn strip_ansi(s: &str) -> String {
    let bytes = strip_ansi_escapes::strip(s);
    String::from_utf8_lossy(&bytes).into_owned()
}

/// Escape special characters for debug display
fn escape_debug(s: &str) -> String {
    s.replace('\x1b', "\\e")
        .replace('\t', "\\t")
        .replace('\r', "\\r")
}
