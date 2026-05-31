# Changelog

## v0.1.8 (2026-05-31)

- Add shortcuts to save command output or full command text to a temp file and copy the file path.

## v0.1.7 (2026-04-20)

- Add support for multi-line shell prompts via the `prompt_lines` config option
  ([#5](https://github.com/raine/tmux-snaglord/pull/5))
- Fix `init` command silently overwriting unrelated user config settings when
  re-run

## v0.1.6 (2026-04-06)

- Fix identical commands run multiple times being collapsed into one entry

## v0.1.5 (2026-04-03)

- Add help overlay showing keybindings, triggered by `?` key
- Restyle bottom bar to a compact single-line footer
- Strip shell prompt from command list, showing just the command itself
- Fix fuzzy search matching against prompt/directory path instead of the command

## v0.1.4 (2025-12-27)

- Now captures full scrollback history instead of only the last 5000 lines

<!-- skipped: v0.1.3 -->

## v0.1.2 (2025-12-20)

- Added all-panes search mode to search across all visible panes in the current
  tmux window (press `a` or cycle with `;`, or start with `--all` flag)
- Matched characters are now highlighted during fuzzy search
- Improved clipboard support across platforms (macOS, Linux with Wayland/X11,
  Windows)

## v0.1.1 (2025-12-20)

- Changed Enter key to copy only the command output instead of command + output

## v0.1.0 (2025-12-19)

Initial release of tmux-snaglord, a TUI tool for browsing and copying from your
tmux scrollback history.
