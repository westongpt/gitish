mod app;
mod args;
mod config;
mod error;
mod events;
mod git;
mod seeds;
mod theme;
mod ui;

use std::io;

use crossterm::{
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};

use crate::app::App;
use crate::args::parse_args;
use crate::config::config_dir;
use crate::error::AppError;
use crate::git::repo::open_repo;

struct TerminalGuard;

impl TerminalGuard {
    fn enter() -> io::Result<Self> {
        enable_raw_mode()?;
        execute!(io::stdout(), EnterAlternateScreen)?;
        Ok(Self)
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen);
    }
}

fn main() -> Result<(), AppError> {
    let parsed = parse_args()?;
    let Some(cli) = parsed else {
        return Ok(());
    };

    let search_path = match cli.path {
        Some(p) => p,
        None => std::env::current_dir()?,
    };

    let repo = open_repo(&search_path).map_err(|e| match &e {
        AppError::Git(g) if g.code() == git2::ErrorCode::NotFound => {
            AppError::Invalid(format!(
                "No git repository found in '{}'.\nRun 'git init' or navigate to a repo first.",
                search_path.display()
            ))
        }
        _ => e,
    })?;
    let config_dir = config_dir();

    let _guard = TerminalGuard::enter()?;
    let backend = CrosstermBackend::new(io::stdout());
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new(repo, config_dir, None)?;
    app.run(&mut terminal)?;

    Ok(())
}
