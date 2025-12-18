//! User actions that can be performed in the application

/// Represents all possible user actions
#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    /// Quit the application
    Quit,
    /// Move selection to next item
    Next,
    /// Move selection to previous item
    Previous,
    /// Scroll output pane down
    ScrollDown,
    /// Scroll output pane up
    ScrollUp,
    /// Enter search mode
    EnterSearch,
    /// Exit search mode (keep current selection)
    ExitSearch,
    /// Clear search filter and restore full list
    ClearSearch,
    /// Input a character during search
    SearchInput(char),
    /// Delete last character in search query
    SearchBackspace,
    /// Copy output to clipboard
    CopyOutput,
    /// Copy command + output to clipboard
    CopyFull,
    /// Copy command only to clipboard
    CopyCommand,
    /// Copy debug info to clipboard
    CopyDebug,
    /// Toggle selection of the current item for scratchpad
    ToggleSelection,
    /// Clear all selections in the scratchpad
    ClearSelection,
    /// Submit/copy the current selection (or single item if none selected)
    Submit,
    /// Switch to next mode
    SwitchMode,
    /// Switch to previous mode
    SwitchModePrev,
    /// Switch directly to Commands mode
    SwitchToCommands,
    /// Switch directly to JSON mode
    SwitchToJson,
    /// Switch directly to Paths mode
    SwitchToPaths,
    /// Reload content from the previous tmux pane
    LoadPreviousPane,
    /// Paste output to target pane (send-keys)
    PasteOutput,
    /// Paste command+output to target pane (send-keys)
    PasteFull,
}
