use anyhow::{Context, Result};
use clap::Parser;
use crossterm::{
    event::{self, Event, KeyCode, KeyEvent, KeyModifiers},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};
use regex::Regex;
use std::io;

mod action;
mod app;
mod parser;
mod tmux;
mod ui;
mod utils;

use action::Action;
use app::{App, UpdateResult};

#[derive(Parser)]
#[command(name = "tmux-copy-tool")]
#[command(about = "A TUI for copying terminal history from tmux")]
struct Cli {
    /// Regex pattern to identify command prompts
    #[arg(short, long, default_value = parser::DEFAULT_PROMPT_REGEX)]
    prompt: String,

    /// Target tmux pane (e.g., "%0" or "session:window.pane")
    #[arg(short = 't', long)]
    target: Option<String>,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    // Validate regex early (before potentially slow tmux capture)
    let prompt_re = Regex::new(&cli.prompt).context("Invalid prompt regex pattern")?;

    // Capture tmux pane content
    let content = tmux::capture_pane(cli.target.as_deref())?;

    // Parse into command blocks
    let blocks = parser::parse_history(&content, &prompt_re);

    if blocks.is_empty() {
        eprintln!("No commands found. Try adjusting the --prompt regex.");
        eprintln!("Current pattern: {}", cli.prompt);
        return Ok(());
    }

    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Run the app
    let mut app = App::new(blocks);
    let res = run_app(&mut terminal, &mut app);

    // Restore terminal
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    res
}

fn run_app(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>, app: &mut App) -> Result<()> {
    loop {
        terminal.draw(|f| ui::render(f, app))?;

        if let Event::Key(key) = event::read()?
            && let Some(action) = get_action(key, app)
        {
            match app.update(action)? {
                UpdateResult::Quit => return Ok(()),
                UpdateResult::Continue => {}
            }
        }
    }
}

/// Map a key event to an action based on current application state
fn get_action(key: KeyEvent, app: &App) -> Option<Action> {
    // Search mode key mappings
    if app.is_searching {
        return match key.code {
            KeyCode::Enter => Some(Action::ExitSearch),
            KeyCode::Esc => Some(Action::ClearSearch),
            KeyCode::Backspace => Some(Action::SearchBackspace),
            KeyCode::Up => Some(Action::Previous),
            KeyCode::Down => Some(Action::Next),
            // Ctrl+c should quit even in search mode
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                Some(Action::Quit)
            }
            KeyCode::Char('n') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                Some(Action::Next)
            }
            KeyCode::Char('p') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                Some(Action::Previous)
            }
            KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                Some(Action::ScrollDown)
            }
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                Some(Action::ScrollUp)
            }
            // Only accept unmodified characters as search input
            KeyCode::Char(c)
                if key.modifiers.is_empty() || key.modifiers == KeyModifiers::SHIFT =>
            {
                Some(Action::SearchInput(c))
            }
            _ => None,
        };
    }

    // Normal mode key mappings
    match key.code {
        KeyCode::Char('q') => Some(Action::Quit),
        KeyCode::Esc => {
            if !app.search_query.is_empty() {
                Some(Action::ClearSearch)
            } else if !app.selection.is_empty() {
                Some(Action::ClearSelection)
            } else {
                Some(Action::Quit)
            }
        }
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => Some(Action::Quit),

        KeyCode::Char('/') => Some(Action::EnterSearch),

        KeyCode::Char('j') | KeyCode::Down => Some(Action::Next),
        KeyCode::Char('k') | KeyCode::Up => Some(Action::Previous),

        KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            Some(Action::ScrollDown)
        }
        KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            Some(Action::ScrollUp)
        }

        // Scratchpad: toggle selection and submit
        KeyCode::Char(' ') => Some(Action::ToggleSelection),
        KeyCode::Enter => Some(Action::Submit),

        KeyCode::Char('y') => Some(Action::CopyOutput),
        KeyCode::Char('Y') => Some(Action::CopyFull),
        KeyCode::Char('c') => Some(Action::CopyCommand),
        KeyCode::Char('D') => Some(Action::CopyDebug),

        _ => None,
    }
}
