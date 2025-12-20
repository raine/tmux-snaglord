//! Application state management

use std::collections::HashMap;

use anyhow::Result;
use fuzzy_matcher::FuzzyMatcher;
use fuzzy_matcher::skim::SkimMatcherV2;
use ratatui::widgets::ListState;
use regex::Regex;

use clap::ValueEnum;

use crate::action::Action;
use crate::parser::{
    CommandBlock, JsonBlock, PathBlock, find_json_candidates, find_path_candidates, parse_history,
};
use crate::tmux;
use crate::utils::{escape_debug, strip_ansi};

/// Application mode (Commands, JSON, or Paths view)
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Mode {
    Commands,
    Json,
    Paths,
}

/// Source of pane content being viewed
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewSource {
    /// Viewing the original pane where the tool was launched
    Original,
    /// Viewing the previous (last active) pane
    Previous,
    /// Viewing aggregated content from all visible panes
    All,
}

/// Result of processing an action
pub enum UpdateResult {
    /// Continue running the application
    Continue,
    /// Quit the application
    Quit,
}

/// Main application state
pub struct App {
    /// Current view mode
    pub mode: Mode,
    /// Use Nerd Fonts/Powerline glyphs
    pub nerd_fonts: bool,
    /// Prompt pattern used for parsing (for diagnostics)
    pub prompt_pattern: String,

    // Command state
    /// Parsed command blocks
    pub blocks: Vec<CommandBlock>,
    /// State for the command list widget
    pub list_state: ListState,
    /// Indices of blocks that match the current filter
    pub filtered_indices: Vec<usize>,
    /// Indices of blocks selected for scratchpad (insertion order)
    pub selection: Vec<usize>,

    // JSON state
    /// Parsed JSON blocks
    pub json_blocks: Vec<JsonBlock>,
    /// State for the JSON list widget
    pub json_list_state: ListState,
    /// Indices of JSON blocks that match the current filter
    pub json_filtered_indices: Vec<usize>,

    // Paths state
    /// Parsed path/URL blocks
    pub path_blocks: Vec<PathBlock>,
    /// State for the paths list widget
    pub path_list_state: ListState,
    /// Indices of path blocks that match the current filter
    pub path_filtered_indices: Vec<usize>,

    // Search highlighting
    /// Maps block index -> byte indices of matched characters (for Commands mode)
    pub match_indices: HashMap<usize, Vec<usize>>,

    // Shared state
    /// Vertical scroll offset for the output pane
    pub scroll_offset: u16,
    /// Current search query
    pub search_query: String,
    /// Whether we're in search mode
    pub is_searching: bool,

    // Parsing state
    /// Prompt pattern regex for re-parsing on reload
    prompt_re: Regex,

    // Error state
    /// Transient error message to display in UI
    pub error_msg: Option<String>,

    /// Current view source (original, previous, or all panes)
    pub view_source: ViewSource,
    /// ID of the original pane where the app started (paste target)
    pub original_pane_id: String,
}

impl App {
    /// Create a new App with the given prompt regex and pattern string
    ///
    /// The app will load content from the original pane on creation.
    pub fn new(
        prompt_re: Regex,
        nerd_fonts: bool,
        prompt_pattern: String,
        original_pane_id: String,
    ) -> Self {
        let mut app = Self {
            mode: Mode::Commands,
            nerd_fonts,
            prompt_pattern,
            blocks: Vec::new(),
            list_state: ListState::default(),
            filtered_indices: Vec::new(),
            selection: Vec::new(),
            json_blocks: Vec::new(),
            json_list_state: ListState::default(),
            json_filtered_indices: Vec::new(),
            path_blocks: Vec::new(),
            path_list_state: ListState::default(),
            path_filtered_indices: Vec::new(),
            match_indices: HashMap::new(),
            scroll_offset: 0,
            search_query: String::new(),
            is_searching: false,
            prompt_re,
            error_msg: None,
            view_source: ViewSource::Original,
            original_pane_id,
        };

        // Load initial content from original pane
        let _ = app.load_content();
        app
    }

    /// Load content based on current view_source
    pub fn load_content(&mut self) -> Result<()> {
        self.blocks.clear();

        match self.view_source {
            ViewSource::Original => {
                self.ingest_pane(&self.original_pane_id.clone())?;
            }
            ViewSource::Previous => {
                let prev = tmux::resolve_pane_id(Some("previous"))?;
                self.ingest_pane(&prev)?;
            }
            ViewSource::All => {
                let panes = tmux::list_panes()?;
                for pane_id in panes {
                    // Ignore errors for individual panes so one failure doesn't break the app
                    let _ = self.ingest_pane(&pane_id);
                }
            }
        }

        self.finalize_ingestion();
        Ok(())
    }

    /// Capture and parse a specific pane, appending to blocks
    fn ingest_pane(&mut self, pane_id: &str) -> Result<()> {
        let content = tmux::capture_pane(pane_id)?;
        let mut new_blocks = parse_history(&content, &self.prompt_re);

        // Tag each block with its source pane
        for block in &mut new_blocks {
            block.pane_id = pane_id.to_string();
        }

        self.blocks.append(&mut new_blocks);
        Ok(())
    }

    /// Finalize state after loading blocks (indices, JSON, paths, etc.)
    fn finalize_ingestion(&mut self) {
        self.list_state = ListState::default();
        if !self.blocks.is_empty() {
            self.list_state.select(Some(0));
        }
        self.filtered_indices = (0..self.blocks.len()).collect();
        self.match_indices.clear();
        self.selection.clear();

        // Parse JSONs from command outputs
        self.json_blocks = find_json_candidates(&self.blocks);
        self.json_list_state = ListState::default();
        if !self.json_blocks.is_empty() {
            self.json_list_state.select(Some(0));
        }
        self.json_filtered_indices = (0..self.json_blocks.len()).collect();

        // Parse paths/URLs from command outputs
        self.path_blocks = find_path_candidates(&self.blocks);
        self.path_list_state = ListState::default();
        if !self.path_blocks.is_empty() {
            self.path_list_state.select(Some(0));
        }
        self.path_filtered_indices = (0..self.path_blocks.len()).collect();

        // Reset view state
        self.scroll_offset = 0;

        // Re-run search if active
        if !self.search_query.is_empty() {
            self.update_search_results();
        }
    }

    /// Process an action and update application state
    pub fn update(&mut self, action: Action) -> Result<UpdateResult> {
        // Clear any previous error on new action
        self.error_msg = None;

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
                match self.mode {
                    Mode::Commands => {
                        if let Some(output) = self.get_output_payload() {
                            tmux::copy_to_clipboard(&output)?;
                            return Ok(UpdateResult::Quit);
                        }
                    }
                    Mode::Json => {
                        // In JSON mode, y copies the raw (minified) JSON
                        if let Some(block) = self.get_selected_json_block() {
                            tmux::copy_to_clipboard(&block.raw)?;
                            return Ok(UpdateResult::Quit);
                        }
                    }
                    Mode::Paths => {
                        // In Paths mode, y copies the raw match (path with line:col if present)
                        if let Some(block) = self.get_selected_path_block() {
                            tmux::copy_to_clipboard(&block.raw)?;
                            return Ok(UpdateResult::Quit);
                        }
                    }
                }
            }
            Action::CopyFull => {
                match self.mode {
                    Mode::Commands => {
                        if let Some(full) = self.get_full_payload() {
                            tmux::copy_to_clipboard(&full)?;
                            return Ok(UpdateResult::Quit);
                        }
                    }
                    Mode::Json => {
                        // In JSON mode, Y copies the pretty-printed JSON
                        if let Some(block) = self.get_selected_json_block() {
                            tmux::copy_to_clipboard(&block.pretty)?;
                            return Ok(UpdateResult::Quit);
                        }
                    }
                    Mode::Paths => {
                        // In Paths mode, Y copies just the path (without line:col)
                        if let Some(block) = self.get_selected_path_block() {
                            tmux::copy_to_clipboard(&block.path)?;
                            return Ok(UpdateResult::Quit);
                        }
                    }
                }
            }
            Action::CopyCommand => {
                // CopyCommand only applies in Commands mode
                if self.mode == Mode::Commands
                    && let Some(cmd) = self.get_command_payload()
                {
                    tmux::copy_to_clipboard(&cmd)?;
                    return Ok(UpdateResult::Quit);
                }
            }
            Action::CopyDebug => {
                // CopyDebug only applies in Commands mode
                if self.mode == Mode::Commands
                    && let Some(debug) = self.get_selected_debug()
                {
                    tmux::copy_to_clipboard(&debug)?;
                    return Ok(UpdateResult::Quit);
                }
            }

            Action::ToggleSelection => {
                // Selection only applies in Commands mode
                if self.mode == Mode::Commands {
                    self.toggle_selection();
                }
            }
            Action::ClearSelection => {
                // Selection only applies in Commands mode
                if self.mode == Mode::Commands {
                    self.selection.clear();
                }
            }
            Action::Submit => {
                match self.mode {
                    Mode::Commands => {
                        // Submit copies output only (same as y)
                        if let Some(output) = self.get_output_payload() {
                            tmux::copy_to_clipboard(&output)?;
                            return Ok(UpdateResult::Quit);
                        }
                    }
                    Mode::Json => {
                        if let Some(block) = self.get_selected_json_block() {
                            tmux::copy_to_clipboard(&block.pretty)?;
                            return Ok(UpdateResult::Quit);
                        }
                    }
                    Mode::Paths => {
                        // Submit copies the raw match (same as y)
                        if let Some(block) = self.get_selected_path_block() {
                            tmux::copy_to_clipboard(&block.raw)?;
                            return Ok(UpdateResult::Quit);
                        }
                    }
                }
            }
            Action::SwitchMode => {
                self.mode = match self.mode {
                    Mode::Commands => Mode::Paths,
                    Mode::Paths => Mode::Json,
                    Mode::Json => Mode::Commands,
                };
                self.scroll_offset = 0;
                self.update_search_results();
            }
            Action::SwitchModePrev => {
                self.mode = match self.mode {
                    Mode::Commands => Mode::Json,
                    Mode::Json => Mode::Paths,
                    Mode::Paths => Mode::Commands,
                };
                self.scroll_offset = 0;
                self.update_search_results();
            }
            Action::SwitchToCommands => {
                self.mode = Mode::Commands;
                self.scroll_offset = 0;
                self.update_search_results();
            }
            Action::SwitchToJson => {
                self.mode = Mode::Json;
                self.scroll_offset = 0;
                self.update_search_results();
            }
            Action::SwitchToPaths => {
                self.mode = Mode::Paths;
                self.scroll_offset = 0;
                self.update_search_results();
            }
            Action::TogglePreviousPane => {
                // Cycle: Original -> Previous -> All -> Original
                self.view_source = match self.view_source {
                    ViewSource::Original => ViewSource::Previous,
                    ViewSource::Previous => ViewSource::All,
                    ViewSource::All => ViewSource::Original,
                };

                if let Err(e) = self.load_content() {
                    self.error_msg = Some(format!("Failed to load content: {}", e));
                    // Fallback to original on error
                    self.view_source = ViewSource::Original;
                    let _ = self.load_content();
                }
            }
            Action::SwitchToAllPanes => {
                if self.view_source != ViewSource::All {
                    self.view_source = ViewSource::All;
                    if let Err(e) = self.load_content() {
                        self.error_msg = Some(format!("Failed to load all panes: {}", e));
                        self.view_source = ViewSource::Original;
                        let _ = self.load_content();
                    }
                }
            }
            Action::PasteOutput => {
                // Paste output only (mirrors 'y' copy behavior)
                // Always paste to original pane (where tool was launched), not the viewed pane
                let payload = match self.mode {
                    Mode::Commands => self.get_output_payload(),
                    Mode::Json => self.get_selected_json_block().map(|b| b.raw.clone()),
                    Mode::Paths => self.get_selected_path_block().map(|b| b.raw.clone()),
                };

                if let Some(content) = payload {
                    tmux::send_keys(&self.original_pane_id, &content)?;
                    return Ok(UpdateResult::Quit);
                }
            }
            Action::PasteFull => {
                // Paste command+output (mirrors 'Y' copy behavior)
                // Always paste to original pane (where tool was launched), not the viewed pane
                let payload = match self.mode {
                    Mode::Commands => self.get_full_payload(),
                    Mode::Json => self.get_selected_json_block().map(|b| b.pretty.clone()),
                    Mode::Paths => self.get_selected_path_block().map(|b| b.path.clone()),
                };

                if let Some(content) = payload {
                    tmux::send_keys(&self.original_pane_id, &content)?;
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
        match self.mode {
            Mode::Commands => {
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
            }
            Mode::Json => {
                if self.json_filtered_indices.is_empty() {
                    return;
                }
                let i = match self.json_list_state.selected() {
                    Some(i) => {
                        if i >= self.json_filtered_indices.len() - 1 {
                            0
                        } else {
                            i + 1
                        }
                    }
                    None => 0,
                };
                self.json_list_state.select(Some(i));
            }
            Mode::Paths => {
                if self.path_filtered_indices.is_empty() {
                    return;
                }
                let i = match self.path_list_state.selected() {
                    Some(i) => {
                        if i >= self.path_filtered_indices.len() - 1 {
                            0
                        } else {
                            i + 1
                        }
                    }
                    None => 0,
                };
                self.path_list_state.select(Some(i));
            }
        }
        self.scroll_offset = 0;
    }

    /// Move selection to the previous item
    fn previous(&mut self) {
        match self.mode {
            Mode::Commands => {
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
            }
            Mode::Json => {
                if self.json_filtered_indices.is_empty() {
                    return;
                }
                let i = match self.json_list_state.selected() {
                    Some(i) => {
                        if i == 0 {
                            self.json_filtered_indices.len() - 1
                        } else {
                            i - 1
                        }
                    }
                    None => 0,
                };
                self.json_list_state.select(Some(i));
            }
            Mode::Paths => {
                if self.path_filtered_indices.is_empty() {
                    return;
                }
                let i = match self.path_list_state.selected() {
                    Some(i) => {
                        if i == 0 {
                            self.path_filtered_indices.len() - 1
                        } else {
                            i - 1
                        }
                    }
                    None => 0,
                };
                self.path_list_state.select(Some(i));
            }
        }
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
        // Clear previous match indices
        self.match_indices.clear();

        match self.mode {
            Mode::Commands => {
                if self.search_query.is_empty() {
                    self.filtered_indices = (0..self.blocks.len()).collect();
                } else {
                    let matcher = SkimMatcherV2::default();
                    let mut matches: Vec<(i64, usize)> = self
                        .blocks
                        .iter()
                        .enumerate()
                        .filter_map(|(idx, block)| {
                            // Use fuzzy_indices to get both score and match positions
                            let cmd_result =
                                matcher.fuzzy_indices(&block.clean_command, &self.search_query);
                            let clean_output = strip_ansi(&block.output);
                            let out_score = matcher.fuzzy_match(&clean_output, &self.search_query);

                            match (cmd_result, out_score) {
                                (Some((c_score, c_indices)), Some(o_score)) => {
                                    self.match_indices.insert(idx, c_indices);
                                    Some((c_score.max(o_score), idx))
                                }
                                (Some((c_score, c_indices)), None) => {
                                    self.match_indices.insert(idx, c_indices);
                                    Some((c_score, idx))
                                }
                                (None, Some(o_score)) => {
                                    // Output matched but not command - no highlighting
                                    Some((o_score, idx))
                                }
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
            }
            Mode::Json => {
                if self.search_query.is_empty() {
                    self.json_filtered_indices = (0..self.json_blocks.len()).collect();
                } else {
                    let matcher = SkimMatcherV2::default();
                    let mut matches: Vec<(i64, usize)> = self
                        .json_blocks
                        .iter()
                        .enumerate()
                        .filter_map(|(idx, block)| {
                            // Search in both name and raw JSON content
                            let name_score = matcher.fuzzy_match(&block.name, &self.search_query);
                            let raw_score = matcher.fuzzy_match(&block.raw, &self.search_query);
                            match (name_score, raw_score) {
                                (Some(n), Some(r)) => Some((n.max(r), idx)),
                                (Some(n), None) => Some((n, idx)),
                                (None, Some(r)) => Some((r, idx)),
                                (None, None) => None,
                            }
                        })
                        .collect();

                    matches.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
                    self.json_filtered_indices = matches.into_iter().map(|(_, idx)| idx).collect();
                }

                if !self.json_filtered_indices.is_empty() {
                    self.json_list_state.select(Some(0));
                } else {
                    self.json_list_state.select(None);
                }
            }
            Mode::Paths => {
                if self.search_query.is_empty() {
                    self.path_filtered_indices = (0..self.path_blocks.len()).collect();
                } else {
                    let matcher = SkimMatcherV2::default();
                    let mut matches: Vec<(i64, usize)> = self
                        .path_blocks
                        .iter()
                        .enumerate()
                        .filter_map(|(idx, block)| {
                            // Search in path and raw string
                            let path_score = matcher.fuzzy_match(&block.path, &self.search_query);
                            let raw_score = matcher.fuzzy_match(&block.raw, &self.search_query);
                            match (path_score, raw_score) {
                                (Some(p), Some(r)) => Some((p.max(r), idx)),
                                (Some(p), None) => Some((p, idx)),
                                (None, Some(r)) => Some((r, idx)),
                                (None, None) => None,
                            }
                        })
                        .collect();

                    matches.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
                    self.path_filtered_indices = matches.into_iter().map(|(_, idx)| idx).collect();
                }

                if !self.path_filtered_indices.is_empty() {
                    self.path_list_state.select(Some(0));
                } else {
                    self.path_list_state.select(None);
                }
            }
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

    /// Get the currently selected JSON block for display
    pub fn get_selected_json_block(&self) -> Option<&JsonBlock> {
        self.json_list_state
            .selected()
            .and_then(|i| self.json_filtered_indices.get(i).copied())
            .and_then(|real_idx| self.json_blocks.get(real_idx))
    }

    /// Get the currently selected path block for display
    pub fn get_selected_path_block(&self) -> Option<&PathBlock> {
        self.path_list_state
            .selected()
            .and_then(|i| self.path_filtered_indices.get(i).copied())
            .and_then(|real_idx| self.path_blocks.get(real_idx))
    }
}
