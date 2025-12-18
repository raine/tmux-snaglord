//! Application state management

use anyhow::Result;
use fuzzy_matcher::FuzzyMatcher;
use fuzzy_matcher::skim::SkimMatcherV2;
use ratatui::widgets::ListState;

use crate::action::Action;
use crate::parser::CommandBlock;
use crate::tmux;
use crate::utils::{escape_debug, strip_ansi};

/// Result of processing an action
pub enum UpdateResult {
    /// Continue running the application
    Continue,
    /// Quit the application
    Quit,
}

/// Main application state
pub struct App {
    /// Parsed command blocks
    pub blocks: Vec<CommandBlock>,
    /// State for the command list widget
    pub list_state: ListState,
    /// Vertical scroll offset for the output pane
    pub scroll_offset: u16,
    /// Current search query
    pub search_query: String,
    /// Whether we're in search mode
    pub is_searching: bool,
    /// Indices of blocks that match the current filter
    pub filtered_indices: Vec<usize>,
    /// Indices of blocks selected for scratchpad (insertion order)
    pub selection: Vec<usize>,
}

impl App {
    /// Create a new App with the given command blocks
    pub fn new(blocks: Vec<CommandBlock>) -> Self {
        let mut list_state = ListState::default();
        if !blocks.is_empty() {
            list_state.select(Some(0));
        }

        let filtered_indices = (0..blocks.len()).collect();

        Self {
            blocks,
            list_state,
            scroll_offset: 0,
            search_query: String::new(),
            is_searching: false,
            filtered_indices,
            selection: Vec::new(),
        }
    }

    /// Process an action and update application state
    pub fn update(&mut self, action: Action) -> Result<UpdateResult> {
        match action {
            Action::Quit => return Ok(UpdateResult::Quit),

            Action::Next => self.next(),
            Action::Previous => self.previous(),

            Action::ScrollDown => {
                self.scroll_offset = self.scroll_offset.saturating_add(10);
            }
            Action::ScrollUp => {
                self.scroll_offset = self.scroll_offset.saturating_sub(10);
            }

            Action::EnterSearch => {
                self.is_searching = true;
                self.search_query.clear();
            }
            Action::ExitSearch => {
                self.is_searching = false;
            }
            Action::ClearSearch => {
                self.clear_search();
            }

            Action::SearchInput(c) => {
                self.on_search_input(c);
            }
            Action::SearchBackspace => {
                self.on_search_backspace();
            }

            Action::CopyOutput => {
                if let Some(output) = self.get_output_payload() {
                    tmux::copy_to_clipboard(&output)?;
                    return Ok(UpdateResult::Quit);
                }
            }
            Action::CopyFull => {
                if let Some(full) = self.get_full_payload() {
                    tmux::copy_to_clipboard(&full)?;
                    return Ok(UpdateResult::Quit);
                }
            }
            Action::CopyCommand => {
                if let Some(cmd) = self.get_command_payload() {
                    tmux::copy_to_clipboard(&cmd)?;
                    return Ok(UpdateResult::Quit);
                }
            }
            Action::CopyDebug => {
                if let Some(debug) = self.get_selected_debug() {
                    tmux::copy_to_clipboard(&debug)?;
                    return Ok(UpdateResult::Quit);
                }
            }

            Action::ToggleSelection => {
                self.toggle_selection();
            }
            Action::ClearSelection => {
                self.selection.clear();
            }
            Action::Submit => {
                // Submit copies full content (command + output), same as Y
                if let Some(full) = self.get_full_payload() {
                    tmux::copy_to_clipboard(&full)?;
                    return Ok(UpdateResult::Quit);
                }
            }
        }
        Ok(UpdateResult::Continue)
    }

    /// Toggle the selection state of the current item
    fn toggle_selection(&mut self) {
        if let Some(idx) = self.get_current_data_index() {
            if let Some(pos) = self.selection.iter().position(|&i| i == idx) {
                self.selection.remove(pos);
            } else {
                self.selection.push(idx);
            }
        }
    }

    /// Get the actual data index from the visual list selection
    fn get_current_data_index(&self) -> Option<usize> {
        self.list_state
            .selected()
            .and_then(|i| self.filtered_indices.get(i).copied())
    }

    /// Move selection to the next item
    fn next(&mut self) {
        if self.filtered_indices.is_empty() {
            return;
        }

        let i = match self.list_state.selected() {
            Some(i) => {
                if i >= self.filtered_indices.len() - 1 {
                    0
                } else {
                    i + 1
                }
            }
            None => 0,
        };
        self.list_state.select(Some(i));
        self.scroll_offset = 0;
    }

    /// Move selection to the previous item
    fn previous(&mut self) {
        if self.filtered_indices.is_empty() {
            return;
        }

        let i = match self.list_state.selected() {
            Some(i) => {
                if i == 0 {
                    self.filtered_indices.len() - 1
                } else {
                    i - 1
                }
            }
            None => 0,
        };
        self.list_state.select(Some(i));
        self.scroll_offset = 0;
    }

    /// Handle character input during search
    fn on_search_input(&mut self, c: char) {
        self.search_query.push(c);
        self.update_search_results();
    }

    /// Handle backspace during search
    fn on_search_backspace(&mut self) {
        self.search_query.pop();
        self.update_search_results();
    }

    /// Update filtered results based on current search query
    fn update_search_results(&mut self) {
        if self.search_query.is_empty() {
            self.filtered_indices = (0..self.blocks.len()).collect();
        } else {
            let matcher = SkimMatcherV2::default();
            let mut matches: Vec<(i64, usize)> = self
                .blocks
                .iter()
                .enumerate()
                .filter_map(|(idx, block)| {
                    let cmd_score = matcher.fuzzy_match(&block.clean_command, &self.search_query);
                    let clean_output = strip_ansi(&block.output);
                    let out_score = matcher.fuzzy_match(&clean_output, &self.search_query);
                    match (cmd_score, out_score) {
                        (Some(c), Some(o)) => Some((c.max(o), idx)),
                        (Some(c), None) => Some((c, idx)),
                        (None, Some(o)) => Some((o, idx)),
                        (None, None) => None,
                    }
                })
                .collect();

            matches.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
            self.filtered_indices = matches.into_iter().map(|(_, idx)| idx).collect();
        }

        if !self.filtered_indices.is_empty() {
            self.list_state.select(Some(0));
        } else {
            self.list_state.select(None);
        }
        self.scroll_offset = 0;
    }

    /// Clear search and restore full list
    fn clear_search(&mut self) {
        self.search_query.clear();
        self.is_searching = false;
        self.update_search_results();
    }

    /// Helper to resolve payload based on selection state
    /// If items are selected in scratchpad, returns joined content; otherwise single item
    fn resolve_payload<F>(&self, extractor: F) -> Option<String>
    where
        F: Fn(&CommandBlock) -> String,
    {
        if !self.selection.is_empty() {
            // Batch mode: join all selected blocks (insertion order)
            let combined: Vec<String> = self
                .selection
                .iter()
                .filter_map(|&i| self.blocks.get(i))
                .map(&extractor)
                .collect();
            if combined.is_empty() {
                None
            } else {
                Some(combined.join("\n"))
            }
        } else {
            // Single mode: get current item
            self.get_current_data_index()
                .and_then(|i| self.blocks.get(i))
                .map(extractor)
        }
    }

    /// Get output payload (handles both single and batch selection)
    fn get_output_payload(&self) -> Option<String> {
        self.resolve_payload(|b| strip_ansi(&b.output))
    }

    /// Get command payload (handles both single and batch selection)
    fn get_command_payload(&self) -> Option<String> {
        self.resolve_payload(|b| b.command_text.clone())
    }

    /// Get full payload (handles both single and batch selection)
    fn get_full_payload(&self) -> Option<String> {
        self.resolve_payload(|b| format!("{}\n{}", strip_ansi(&b.command), strip_ansi(&b.output)))
    }

    /// Get debug-formatted output for diagnosing parsing issues
    fn get_selected_debug(&self) -> Option<String> {
        self.get_current_data_index()
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

    /// Get the currently selected block for display
    pub fn get_selected_block(&self) -> Option<&CommandBlock> {
        self.get_current_data_index()
            .and_then(|i| self.blocks.get(i))
    }
}
