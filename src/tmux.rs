//! Interface for tmux commands

use anyhow::{Context, Result};
use std::io::Write;
use std::process::{Command, Stdio};

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
/// - `-S -5000`: capture last 5000 lines (not full history, which includes stale content)
///
/// The `pane_id` should be a resolved pane ID (e.g., "%0") from `resolve_pane_id`.
pub fn capture_pane(pane_id: &str) -> Result<String> {
    let args = vec![
        "capture-pane",
        "-e",
        "-J",
        "-p",
        "-S",
        "-5000",
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

/// Copy content to system clipboard (macOS pbcopy)
pub fn copy_to_clipboard(content: &str) -> Result<()> {
    let mut child = Command::new("pbcopy")
        .stdin(Stdio::piped())
        .spawn()
        .context("Failed to spawn pbcopy")?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(content.as_bytes())
            .context("Failed to write to pbcopy stdin")?;
    }

    let status = child.wait().context("Failed to wait for pbcopy")?;
    if !status.success() {
        anyhow::bail!("pbcopy exited with non-zero status");
    }

    Ok(())
}
