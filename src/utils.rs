//! Shared utility functions

/// Escape special characters for debug display
pub fn escape_debug(s: &str) -> String {
    s.replace('\x1b', "\\e")
        .replace('\t', "\\t")
        .replace('\r', "\\r")
}
