//! Interface for tmux commands

use anyhow::{Context, Result};
use std::process::Command;

/// Special target identifier for the previous (last active) pane
const PREVIOUS_PANE_TARGET: &str = "previous";

/// Get the pane ID of the previous (last active) pane in the current window.
///
/// Uses tmux's `pane_last` format variable to find the pane that was active
/// before the current one.
fn get_previous_pane_id() -> Result<String> {
    let output = Command::new("tmux")
        .args(["list-panes", "-f", "#{pane_last}", "-F", "#{pane_id}"])
        .output()
        .context("Failed to execute tmux list-panes")?;

    if !output.status.success() {
        anyhow::bail!(
            "tmux list-panes failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let pane_id = String::from_utf8(output.stdout)
        .context("tmux output contained invalid UTF-8")?
        .trim()
        .to_string();

    if pane_id.is_empty() {
        anyhow::bail!(
            "No previous pane found. Make sure you have multiple panes in the current window."
        );
    }

    Ok(pane_id)
}

/// Resolve the target string (e.g., "previous", "%1") to a concrete pane ID
///
/// Returns the resolved pane ID that can be used with tmux commands.
/// - `Some("previous")`: resolves to the previous (last active) pane
/// - `Some(id)`: returns the id as-is
/// - `None`: returns the current pane's ID
pub fn resolve_pane_id(target: Option<&str>) -> Result<String> {
    match target {
        Some(PREVIOUS_PANE_TARGET) => get_previous_pane_id(),
        Some(id) => Ok(id.to_string()),
        None => {
            // Default to current pane if none specified
            let output = Command::new("tmux")
                .args(["display-message", "-p", "#{pane_id}"])
                .output()
                .context("Failed to get current pane ID")?;

            Ok(String::from_utf8(output.stdout)
                .context("Invalid UTF-8 in pane ID")?
                .trim()
                .to_string())
        }
    }
}

/// Capture the content of a tmux pane
///
/// Uses `tmux capture-pane` with:
/// - `-e`: preserve escape sequences (ANSI colors)
/// - `-J`: join wrapped lines
/// - `-p`: output to stdout
/// - `-S -`: start from the beginning of scrollback history
/// - `-E -`: end at the last line (ensures we capture everything including content below cursor)
///
/// The `pane_id` should be a resolved pane ID (e.g., "%0") from `resolve_pane_id`.
pub fn capture_pane(pane_id: &str) -> Result<String> {
    let args = vec![
        "capture-pane",
        "-e",
        "-J",
        "-p",
        "-S",
        "-",
        "-E",
        "-",
        "-t",
        pane_id,
    ];

    let output = Command::new("tmux")
        .args(&args)
        .output()
        .context("Failed to execute tmux capture-pane. Are you running inside tmux?")?;

    if !output.status.success() {
        anyhow::bail!(
            "tmux capture-pane failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    String::from_utf8(output.stdout).context("tmux output contained invalid UTF-8")
}

/// Send content to a target pane as literal keys
///
/// Uses `tmux send-keys` with `-l` flag to send text literally without interpreting
/// special characters as key names.
pub fn send_keys(pane_id: &str, content: &str) -> Result<()> {
    let output = Command::new("tmux")
        .args(["send-keys", "-t", pane_id, "-l", content])
        .output()
        .context("Failed to execute tmux send-keys")?;

    if !output.status.success() {
        anyhow::bail!(
            "tmux send-keys failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(())
}

/// List all pane IDs in the current tmux window
///
/// Returns a vector of pane IDs (e.g., ["%0", "%1", "%2"])
pub fn list_panes() -> Result<Vec<String>> {
    let output = Command::new("tmux")
        .args(["list-panes", "-F", "#{pane_id}"])
        .output()
        .context("Failed to execute tmux list-panes")?;

    if !output.status.success() {
        anyhow::bail!(
            "tmux list-panes failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let out = String::from_utf8(output.stdout).context("tmux output contained invalid UTF-8")?;
    Ok(out
        .lines()
        .filter(|s| !s.is_empty())
        .map(|s| s.trim().to_string())
        .collect())
}

/// Copy content to system clipboard (cross-platform)
pub fn copy_to_clipboard(content: &str) -> Result<()> {
    let mut clipboard = arboard::Clipboard::new().context("Failed to access clipboard")?;
    clipboard
        .set_text(content)
        .context("Failed to copy to clipboard")?;
    Ok(())
}
