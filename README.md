<div align="center">
  <img src="meta/logo.avif" alt="Raccoon logo" width="250" />
  <h1>tmux-snaglord</h1>
  <p>Reign over your tmux scrollback.</p>
</div>

Stop fumbling with tmux copy mode. `tmux-snaglord` turns your tmux
scrollback into a structured, searchable list of commands and their outputs.

- **Find** any command or output instantly with fuzzy search
- **Copy** clean output without the prompt or surrounding noise
- **Extract** JSON (syntax highlighted) and file paths automatically
- **Paste** directly into another pane. Perfect for feeding context to LLM
  agents

[Install](#install) · [Quick start](#quick-start) · [Usage](#usage) ·
[Configuration](#configuration)

<video src="https://github.com/user-attachments/assets/a3e3db87-89da-4026-9733-fbdc588241b0" autoplay loop muted></video>

### Why not tmux copy-mode?

Tmux copy-mode requires you to scroll, visually locate boundaries, and manually
select text. Command and output boundaries blur together. `tmux-snaglord` solves
this by detecting prompts and treating each command + output as a single
selectable block.

## Features

- **Command blocks**: Each command and its output is a single selectable unit—no
  manual boundary selection
- **Cross-pane workflow**: View history from any pane, paste back to where you
  started. Switch source panes with `;` or search all visible panes at once
- **Three viewing modes** (Tab to switch):
  - **Commands**: Browse commands and their outputs
  - **JSON**: Auto-detects and extracts JSON with syntax highlighting
  - **Paths**: Extracts file paths and URLs for quick copying
- **Fuzzy search**: Filter through history with `/`
- **Multi-select**: Select multiple blocks with space, copy all at once
- **Zero-config for standard prompts**: Auto-detects bash, zsh, fish, starship,
  oh-my-zsh. Custom regex for others

## Install

### Quick install

```sh
curl -fsSL https://raw.githubusercontent.com/raine/tmux-snaglord/main/scripts/install.sh | bash
```

### Homebrew (macOS/Linux)

```sh
brew install raine/tmux-snaglord/tmux-snaglord
```

### Cargo

```sh
cargo install tmux-snaglord
```

## Quick start

1. Run `tmux-snaglord init` in a tmux pane with some command history
2. The tool auto-detects your shell prompt and saves it to config
3. Add a keybinding to `~/.tmux.conf`:

```tmux
# Open in a popup (tmux 3.2+)
bind-key C-y popup -E -w 60% -h 60% "tmux-snaglord"
```

See [Prompt detection](#prompt-detection) if auto-detection doesn't work.

### Use with CLI-based LLM agents

When working with Claude Code, Aider, or similar tools, you often need to share
command output from another pane:

1. Run commands in your working pane
2. Switch to the pane running your LLM agent
3. Open tmux-snaglord and press `;` to load history from your previous pane
4. Select the output and press `p` to paste directly into the conversation

## Usage

```sh
tmux-snaglord          # Run the TUI
tmux-snaglord init     # Auto-detect prompt and save to config
tmux-snaglord -t "%1"  # Target a specific pane
```

### CLI options

```
Usage: tmux-snaglord [OPTIONS]

Options:
  -p, --prompt <REGEX>    Regex pattern to identify command prompts
      --preset <NAME>     Preset pattern name (bash, zsh, fish, robbyrussell, starship, dollar, hash)
  -t, --target <PANE>     Target tmux pane (e.g., "%0", "session:window.pane", or "previous")
  -m, --mode <MODE>       Start in specific view mode [possible values: commands, json, paths]
  -a, --all               Start in all-panes mode (search across all visible panes)
```

The special target `previous` captures the last active pane, useful when you
want to run the tool from a different pane than the one you're inspecting.

### Key bindings

**Navigation**

| Key             | Action                            |
| --------------- | --------------------------------- |
| `j` / `↓`       | Next item                         |
| `k` / `↑`       | Previous item                     |
| `Ctrl+d`        | Scroll output down                |
| `Ctrl+u`        | Scroll output up                  |
| `1` / `2` / `3` | Switch to Commands / JSON / Paths |
| `Tab`           | Cycle to next mode                |
| `/`             | Enter search mode                 |

**Selection & copying**

| Key     | Action                               |
| ------- | ------------------------------------ |
| `Space` | Toggle selection (scratchpad)        |
| `Enter` | Copy full (command + output)         |
| `y`     | Copy output only                     |
| `Y`     | Copy full (command + output)         |
| `c`     | Copy command only                    |
| `p`     | Paste output to original pane        |
| `P`     | Paste full to original pane          |
| `D`     | Copy debug format (raw with escapes) |
| `Esc`   | Clear selection/search, or quit      |
| `q`     | Quit                                 |

**Pane navigation**

| Key | Action                                             |
| --- | -------------------------------------------------- |
| `;` | Cycle pane source (this → previous → all)          |
| `a` | Jump to all-panes mode                             |

Use `;` to cycle between viewing history from the current pane, the previous
pane, or all visible panes in the window. Press `a` to jump directly to
all-panes mode. Paste actions (`p`/`P`) always target the original pane where
the tool was launched.

**Search mode**

| Key                 | Action                   |
| ------------------- | ------------------------ |
| (type)              | Filter items             |
| `Enter`             | Exit search, keep filter |
| `Esc`               | Clear search and exit    |
| `Ctrl+n` / `Ctrl+p` | Navigate while searching |

## Prompt detection

`tmux-snaglord` captures the visible content of your tmux pane and uses a regex
pattern to find shell prompts. Each prompt marks the start of a new command, and
everything until the next prompt is that command's output:

```
~/code % ls           ← prompt detected, "ls" is the command
file1.txt
file2.txt             ← output
~/code % cat file1.txt   ← next prompt, new command starts
hello world           ← output
```

If no commands are found, run `tmux-snaglord init` to auto-detect the best
preset for your shell, or configure a custom pattern.

**Note:** Because this tool relies on regex pattern matching, detection accuracy
depends on your prompt configuration. Heavily customized prompts or command
output that resembles your prompt may cause incorrect parsing.

### Multiline prompts

Some prompts span multiple terminal lines — e.g. default Starship, which
puts a blank separator and a module line above the `❯` character:

```
                               ← blank separator (line 1, from add_newline)
~/code on  main                ← prompt decoration (line 2)
❯ git remote -v                ← actual prompt + command (line 3)
origin  git@github:foo/bar.git (fetch)
origin  git@github:foo/bar.git (push)
```

By default the parser treats each prompt as a single line, so the blank
line and `~/code on main` decoration get attached to the *previous*
command's output. Set `prompt_lines` in your config to tell the parser
how many terminal lines each prompt occupies:

```toml
preset = "starship"
prompt_lines = 3   # blank + modules + ❯ line (default Starship)
```

Set `prompt_lines = 2` if you've disabled Starship's `add_newline`, or
higher if your custom format adds more lines above `❯`.

The `prompt` regex (or `preset`) should still match the *last* line of
the prompt — the one with the command. The `prompt_lines - 1` decoration
lines directly above it are stripped from the preceding command's output.

## Configuration

You can persist preferences in `~/.config/tmux-snaglord/config.toml`:

```toml
# Use a built-in preset
preset = "starship"

# OR define a custom regex (takes precedence)
# prompt = "^[/~].* % "

# Enable Nerd Font icons and Powerline glyphs (default: false)
# nerd_fonts = true

# Number of terminal lines each prompt occupies (default: 1).
# See "Multiline prompts" above for details.
# prompt_lines = 2
```

### Presets

| Name           | Pattern                           | Description                     |
| -------------- | --------------------------------- | ------------------------------- |
| `bash`         | `^[\w.-]+@[\w.-]+:[~\w./-]+[#$] ` | Standard bash (user@host:path$) |
| `zsh`          | `^[\w.-]+% `                      | Default zsh (hostname%)         |
| `fish`         | `^.*?[\w./-]+> `                  | Fish default prompt             |
| `robbyrussell` | `^➜  `                            | Oh My Zsh robbyrussell theme    |
| `starship`     | `^❯ `                             | Starship default prompt         |
| `dollar`       | `^\$ `                            | Simple $ prompt                 |
| `hash`         | `^# `                             | Root shell prompt               |

## Related projects

- [workmux](https://github.com/raine/workmux) — Git worktrees + tmux windows for
  parallel AI agent workflows
- [claude-history](https://github.com/raine/claude-history) — Search and view
  Claude Code conversation history with fzf
- [tmux-agent-usage](https://github.com/raine/tmux-agent-usage) — Display AI
  agent rate limit usage in your tmux status bar
