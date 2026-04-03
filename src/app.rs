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
use crate::utils::escape_debug;

/// Trait for items that can be fuzzy searched
pub trait FuzzySearchable {
    /// Returns (score, optional match indices) if the item matches the query
    fn fuzzy_match(
        &self,
        query: &str,
        matcher: &SkimMatcherV2,
    ) -> Option<(i64, Option<Vec<usize>>)>;
}

impl FuzzySearchable for CommandBlock {
    fn fuzzy_match(
        &self,
        query: &str,
        matcher: &SkimMatcherV2,
    ) -> Option<(i64, Option<Vec<usize>>)> {
        let cmd_result = matcher.fuzzy_indices(&self.command_text, query);
        let out_score = matcher.fuzzy_match(&self.clean_output, query);

        match (cmd_result, out_score) {
            (Some((c_score, indices)), Some(o_score)) => {
                Some((c_score.max(o_score), Some(indices)))
            }
            (Some((c_score, indices)), None) => Some((c_score, Some(indices))),
            (None, Some(o_score)) => Some((o_score, None)),
            (None, None) => None,
        }
    }
}

impl FuzzySearchable for JsonBlock {
    fn fuzzy_match(
        &self,
        query: &str,
        matcher: &SkimMatcherV2,
    ) -> Option<(i64, Option<Vec<usize>>)> {
        let name_score = matcher.fuzzy_match(&self.name, query);
        let raw_score = matcher.fuzzy_match(&self.raw, query);

        match (name_score, raw_score) {
            (Some(n), Some(r)) => Some((n.max(r), None)),
            (Some(n), None) => Some((n, None)),
            (None, Some(r)) => Some((r, None)),
            (None, None) => None,
        }
    }
}

impl FuzzySearchable for PathBlock {
    fn fuzzy_match(
        &self,
        query: &str,
        matcher: &SkimMatcherV2,
    ) -> Option<(i64, Option<Vec<usize>>)> {
        let path_score = matcher.fuzzy_match(&self.path, query);
        let raw_score = matcher.fuzzy_match(&self.raw, query);

        match (path_score, raw_score) {
            (Some(p), Some(r)) => Some((p.max(r), None)),
            (Some(p), None) => Some((p, None)),
            (None, Some(r)) => Some((r, None)),
            (None, None) => None,
        }
    }
}

/// Generic wrapper for a filterable list with selection state
pub struct StatefulList<T> {
    /// All items in the list
    pub items: Vec<T>,
    /// Widget state for ratatui
    pub state: ListState,
    /// Indices of items matching current filter (into `items`)
    pub filtered_indices: Vec<usize>,
}

impl<T> Default for StatefulList<T> {
    fn default() -> Self {
        Self {
            items: Vec::new(),
            state: ListState::default(),
            filtered_indices: Vec::new(),
        }
    }
}

impl<T> StatefulList<T> {
    /// Create a new list with items, selecting the first one
    pub fn with_items(items: Vec<T>) -> Self {
        let indices: Vec<usize> = (0..items.len()).collect();
        let mut state = ListState::default();
        if !items.is_empty() {
            state.select(Some(0));
        }
        Self {
            items,
            state,
            filtered_indices: indices,
        }
    }

    /// Move selection to next item (wraps around)
    pub fn next(&mut self) {
        if self.filtered_indices.is_empty() {
            return;
        }
        let i = self
            .state
            .selected()
            .map_or(0, |i| (i + 1) % self.filtered_indices.len());
        self.state.select(Some(i));
    }

    /// Move selection to previous item (wraps around)
    pub fn previous(&mut self) {
        if self.filtered_indices.is_empty() {
            return;
        }
        let len = self.filtered_indices.len();
        let i = self.state.selected().map_or(0, |i| (i + len - 1) % len);
        self.state.select(Some(i));
    }

    /// Get the currently selected item
    pub fn selected(&self) -> Option<&T> {
        self.state
            .selected()
            .and_then(|i| self.filtered_indices.get(i))
            .and_then(|&real_idx| self.items.get(real_idx))
    }

    /// Get the real index of the currently selected item
    pub fn selected_index(&self) -> Option<usize> {
        self.state
            .selected()
            .and_then(|i| self.filtered_indices.get(i).copied())
    }

    /// Reset filter to show all items
    pub fn reset_filter(&mut self) {
        self.filtered_indices = (0..self.items.len()).collect();
        if !self.filtered_indices.is_empty() {
            self.state.select(Some(0));
        } else {
            self.state.select(None);
        }
    }

    /// Update filtered indices and reset selection
    pub fn set_filtered(&mut self, indices: Vec<usize>) {
        self.filtered_indices = indices;
        if !self.filtered_indices.is_empty() {
            self.state.select(Some(0));
        } else {
            self.state.select(None);
        }
    }
}

impl<T: FuzzySearchable> StatefulList<T> {
    /// Filter items by fuzzy query, returning a map of index -> match indices
    pub fn filter_by_query(&mut self, query: &str) -> HashMap<usize, Vec<usize>> {
        let matcher = SkimMatcherV2::default();
        let mut match_indices = HashMap::new();

        let mut matches: Vec<(i64, usize)> = self
            .items
            .iter()
            .enumerate()
            .filter_map(|(idx, item)| {
                item.fuzzy_match(query, &matcher).map(|(score, indices)| {
                    if let Some(indices) = indices {
                        match_indices.insert(idx, indices);
                    }
                    (score, idx)
                })
            })
            .collect();

        matches.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
        let indices = matches.into_iter().map(|(_, idx)| idx).collect();
        self.set_filtered(indices);

        match_indices
    }
}

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

/// Type of content to retrieve from current selection
#[derive(Clone, Copy)]
pub enum ContentType {
    /// Raw/minimal content (output for Commands, raw JSON, raw path with line:col)
    Raw,
    /// Full/enhanced content (cmd+output for Commands, pretty JSON, path only)
    Full,
}

/// Main application state
pub struct App {
    /// Current view mode
    pub mode: Mode,
    /// Use Nerd Fonts/Powerline glyphs
    pub nerd_fonts: bool,
    /// Prompt pattern used for parsing (for diagnostics)
    pub prompt_pattern: String,

    // Mode-specific lists (using StatefulList for unified state management)
    /// Command blocks with selection state
    pub commands: StatefulList<CommandBlock>,
    /// Indices of blocks selected for scratchpad (insertion order)
    pub selection: Vec<usize>,

    /// JSON blocks with selection state
    pub jsons: StatefulList<JsonBlock>,

    /// Path/URL blocks with selection state
    pub paths: StatefulList<PathBlock>,

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

    /// Whether to show the help overlay
    pub show_help: bool,

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
            commands: StatefulList::default(),
            selection: Vec::new(),
            jsons: StatefulList::default(),
            paths: StatefulList::default(),
            match_indices: HashMap::new(),
            scroll_offset: 0,
            search_query: String::new(),
            is_searching: false,
            prompt_re,
            error_msg: None,
            show_help: false,
            view_source: ViewSource::Original,
            original_pane_id,
        };

        // Load initial content from original pane
        let _ = app.load_content();
        app
    }

    /// Load content based on current view_source
    pub fn load_content(&mut self) -> Result<()> {
        self.commands.items.clear();

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

        self.commands.items.append(&mut new_blocks);
        Ok(())
    }

    /// Finalize state after loading blocks (indices, JSON, paths, etc.)
    fn finalize_ingestion(&mut self) {
        // Reset commands list state
        self.commands.reset_filter();
        self.match_indices.clear();
        self.selection.clear();

        // Parse JSONs from command outputs
        self.jsons = StatefulList::with_items(find_json_candidates(&self.commands.items));

        // Parse paths/URLs from command outputs
        self.paths = StatefulList::with_items(find_path_candidates(&self.commands.items));

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
            Action::DismissHelp => {
                self.show_help = false;
                return Ok(UpdateResult::Continue);
            }
            Action::ShowHelp => {
                self.show_help = !self.show_help;
                return Ok(UpdateResult::Continue);
            }
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
                if let Some(content) = self.get_content(ContentType::Raw) {
                    tmux::copy_to_clipboard(&content)?;
                    return Ok(UpdateResult::Quit);
                }
            }
            Action::CopyFull => {
                if let Some(content) = self.get_content(ContentType::Full) {
                    tmux::copy_to_clipboard(&content)?;
                    return Ok(UpdateResult::Quit);
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
                // Submit copies: Raw for Commands/Paths, Full (pretty) for JSON
                let content_type = match self.mode {
                    Mode::Json => ContentType::Full,
                    _ => ContentType::Raw,
                };
                if let Some(content) = self.get_content(content_type) {
                    tmux::copy_to_clipboard(&content)?;
                    return Ok(UpdateResult::Quit);
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
                // Paste raw content to original pane (where tool was launched)
                if let Some(content) = self.get_content(ContentType::Raw) {
                    tmux::send_keys(&self.original_pane_id, &content)?;
                    return Ok(UpdateResult::Quit);
                }
            }
            Action::PasteFull => {
                // Paste full content to original pane (where tool was launched)
                if let Some(content) = self.get_content(ContentType::Full) {
                    tmux::send_keys(&self.original_pane_id, &content)?;
                    return Ok(UpdateResult::Quit);
                }
            }
        }
        Ok(UpdateResult::Continue)
    }

    /// Toggle the selection state of the current item
    fn toggle_selection(&mut self) {
        if let Some(idx) = self.commands.selected_index() {
            if let Some(pos) = self.selection.iter().position(|&i| i == idx) {
                self.selection.remove(pos);
            } else {
                self.selection.push(idx);
            }
        }
    }

    /// Move selection to the next item
    fn next(&mut self) {
        match self.mode {
            Mode::Commands => self.commands.next(),
            Mode::Json => self.jsons.next(),
            Mode::Paths => self.paths.next(),
        }
        self.scroll_offset = 0;
    }

    /// Move selection to the previous item
    fn previous(&mut self) {
        match self.mode {
            Mode::Commands => self.commands.previous(),
            Mode::Json => self.jsons.previous(),
            Mode::Paths => self.paths.previous(),
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
        self.match_indices.clear();

        if self.search_query.is_empty() {
            match self.mode {
                Mode::Commands => self.commands.reset_filter(),
                Mode::Json => self.jsons.reset_filter(),
                Mode::Paths => self.paths.reset_filter(),
            }
        } else {
            match self.mode {
                Mode::Commands => {
                    self.match_indices = self.commands.filter_by_query(&self.search_query);
                }
                Mode::Json => {
                    self.jsons.filter_by_query(&self.search_query);
                }
                Mode::Paths => {
                    self.paths.filter_by_query(&self.search_query);
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
                .filter_map(|&i| self.commands.items.get(i))
                .map(&extractor)
                .collect();
            if combined.is_empty() {
                None
            } else {
                Some(combined.join("\n"))
            }
        } else {
            // Single mode: get current item
            self.commands.selected().map(extractor)
        }
    }

    /// Get output payload (handles both single and batch selection)
    fn get_output_payload(&self) -> Option<String> {
        self.resolve_payload(|b| b.clean_output.clone())
    }

    /// Get command payload (handles both single and batch selection)
    fn get_command_payload(&self) -> Option<String> {
        self.resolve_payload(|b| b.command_text.clone())
    }

    /// Get full payload (handles both single and batch selection)
    fn get_full_payload(&self) -> Option<String> {
        self.resolve_payload(|b| format!("{}\n{}", b.clean_command, b.clean_output))
    }

    /// Get debug-formatted output for diagnosing parsing issues
    fn get_selected_debug(&self) -> Option<String> {
        self.commands.selected().map(|b| {
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

    /// Get content from current selection based on mode and content type
    fn get_content(&self, content_type: ContentType) -> Option<String> {
        match self.mode {
            Mode::Commands => match content_type {
                ContentType::Raw => self.get_output_payload(),
                ContentType::Full => self.get_full_payload(),
            },
            Mode::Json => {
                let block = self.jsons.selected()?;
                Some(match content_type {
                    ContentType::Raw => block.raw.clone(),
                    ContentType::Full => block.pretty.clone(),
                })
            }
            Mode::Paths => {
                let block = self.paths.selected()?;
                Some(match content_type {
                    ContentType::Raw => block.raw.clone(),
                    ContentType::Full => block.path.clone(),
                })
            }
        }
    }
}
