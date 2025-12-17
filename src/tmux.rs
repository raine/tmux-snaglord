//! Interface for tmux commands

use anyhow::{Context, Result};
use std::io::Write;
use std::process::{Command, Stdio};

/// Capture the content of a tmux pane
///
/// Uses `tmux capture-pane` with:
/// - `-e`: preserve escape sequences (ANSI colors)
/// - `-J`: join wrapped lines
/// - `-p`: output to stdout
/// - `-S -1000`: capture last 1000 lines (not full history, which includes stale content)
pub fn capture_pane(pane_id: Option<&str>) -> Result<String> {
    let mut args = vec!["capture-pane", "-e", "-J", "-p", "-S", "-1000"];

    if let Some(id) = pane_id {
        args.push("-t");
        args.push(id);
    }

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
