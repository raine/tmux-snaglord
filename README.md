<div align="center">
  <img src="meta/logo.avif" alt="Raccoon logo" width="250" />
  <h1>tmux-snaglord</h1>
  <p>Reign over your tmux scrollback.</p>
</div>

`tmux-snaglord` parses your current tmux pane's scrollback history, separates
commands from their output using regex prompt detection, and presents them in a
structured interface. Fuzzy search history, extract JSON blobs, find file paths,
and copy content to your clipboard. Copying in tmux has never been so easy.

[Install](#install) · [Quick start](#quick-start) · [Usage](#usage) ·
[Configuration](#configuration)

## Demo

```
┌─ Commands ──────────────────────────┐┌─ Output ──────────────────────────────┐
│  git status                         ││{                                      │
│  curl api.local/users               ││  "id": 12,                            │
│> cat config.json                    ││  "name": "tmux-snaglord",             │
│  ls -la                             ││  "features": [                        │
│                                     ││    "parsing",                         │
│                                     ││    "tui",                             │
│                                     ││    "json-highlighting"                │
│                                     ││  ]                                    │
│                                     ││}                                      │
└─────────────────────────────────────┘└───────────────────────────────────────┘
 Tab: switch mode  Space: select  /: search  y: copy
```

## Features

- **Smart parsing**: Splits history into command/output blocks using regex
  prompt detection
- **Three viewing modes** (Tab to switch):
  - **Commands**: Browse commands and their outputs
  - **JSON**: Detects and extracts JSON from output with syntax highlighting
  - **Paths**: Extracts file paths and URLs for quick copying
- **Fuzzy search**: Filter through history with `/`
- **Multi-select**: Use space to select multiple blocks, copy all at once
- **Shell presets**: Built-in support for bash, zsh, oh-my-zsh, starship, fish

## Install

### Homebrew (macOS/Linux)

```sh
brew install raine/tap/tmux-snaglord
```

### Cargo

```sh
cargo install tmux-snaglord
```

## Quick start

1. Run `tmux-snaglord init` in a tmux pane with some command history
2. The tool auto-detects your shell prompt and saves it to config
3. Run `tmux-snaglord` — it just works now

```sh
$ tmux-snaglord init
Detecting prompt pattern...

  starship     12 commands
  zsh          12 commands
  fish         8 commands

Detected 'starship' (12 commands)
Saved to /Users/you/.config/tmux-snaglord/config.toml
```

See [Prompt detection](#prompt-detection) if auto-detection doesn't work.

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
```

The special target `previous` captures the last active pane, useful when you want
to run the tool from a different pane than the one you're inspecting.

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

| Key | Action                                        |
| --- | --------------------------------------------- |
| `;` | Toggle between original and previous pane     |

Use `;` to switch between viewing history from different panes without
restarting. Paste actions (`p`/`P`) always target the original pane where the
tool was launched.

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

## Configuration

You can persist preferences in `~/.config/tmux-snaglord/config.toml`:

```toml
# Use a built-in preset
preset = "starship"

# OR define a custom regex (takes precedence)
# prompt = "^[/~].* % "

# Enable Nerd Font icons and Powerline glyphs (default: false)
# nerd_fonts = true
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

## tmux integration

Bind to a key in `~/.tmux.conf`:

```tmux
# Open in a popup (tmux 3.2+)
bind-key C-y popup -E -w 60% -h 60% "tmux-snaglord"
```

## Related projects

- [workmux](https://github.com/raine/workmux) — Git worktrees + tmux windows for
  parallel AI agent workflows
- [claude-history](https://github.com/raine/claude-history) — Search and view
  Claude Code conversation history with fzf
