use anyhow::{Context, Result};
use clap::Parser;
use crossterm::{
    event::{self, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};
use regex::Regex;
use std::io;

mod app;
mod parser;
mod tmux;
mod ui;

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
    let mut app = app::App::new(blocks);
    let res = run_app(&mut terminal, &mut app);

    // Restore terminal
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    // Handle any error from the app loop
    res
}

fn run_app(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut app::App,
) -> Result<()> {
    loop {
        terminal.draw(|f| ui::render(f, app))?;

        if let Event::Key(key) = event::read()? {
            match key.code {
                // Quit
                KeyCode::Char('q') => return Ok(()),
                KeyCode::Esc => return Ok(()),
                KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    return Ok(());
                }

                // Navigation
                KeyCode::Char('j') | KeyCode::Down => app.next(),
                KeyCode::Char('k') | KeyCode::Up => app.previous(),

                // Scroll output pane
                KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    for _ in 0..10 {
                        app.scroll_down();
                    }
                }
                KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    for _ in 0..10 {
                        app.scroll_up();
                    }
                }

                // Copy output only
                KeyCode::Char('y') => {
                    if let Some(output) = app.get_selected_output() {
                        tmux::copy_to_clipboard(output)?;
                        return Ok(()); // Exit after copying
                    }
                }

                // Copy full block (command + output)
                KeyCode::Char('Y') => {
                    if let Some(full) = app.get_selected_full() {
                        tmux::copy_to_clipboard(&full)?;
                        return Ok(()); // Exit after copying
                    }
                }

                // Copy debug output to system clipboard (for diagnosing parsing issues)
                KeyCode::Char('d') => {
                    if let Some(debug) = app.get_selected_debug() {
                        tmux::copy_to_clipboard(&debug)?;
                        return Ok(()); // Exit after copying
                    }
                }

                _ => {}
            }
        }
    }
}
