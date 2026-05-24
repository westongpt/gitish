use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};

use crate::error::AppError;

#[derive(Debug, Clone, PartialEq)]
pub enum AppEvent {
    // navigation
    MoveUp,
    MoveDown,
    NextHunk,
    PrevHunk,
    // actions
    Stage,
    Unstage,
    Discard,
    Commit,
    // modes
    ToggleFocus,
    OpenThemePicker,
    // text input
    Char(char),
    Backspace,
    // universal
    Confirm,
    Cancel,
    Quit,
}

/// When `text_input` is true, all printable characters are passed through as
/// `AppEvent::Char` instead of being interpreted as keybindings. Only
/// structural keys (Enter, Esc, Backspace, Ctrl-C) are still translated.
pub fn next_event(text_input: bool) -> Result<Option<AppEvent>, AppError> {
    if !event::poll(std::time::Duration::from_millis(100))? {
        return Ok(None);
    }
    let ev = match event::read()? {
        Event::Key(k) => {
            if text_input {
                translate_key_input(k)
            } else {
                translate_key(k)
            }
        }
        _ => None,
    };
    Ok(ev)
}

fn translate_key(key: KeyEvent) -> Option<AppEvent> {
    if key.modifiers == KeyModifiers::CONTROL {
        return match key.code {
            KeyCode::Char('c') | KeyCode::Char('q') => Some(AppEvent::Quit),
            _ => None,
        };
    }

    match key.code {
        KeyCode::Char('q') => Some(AppEvent::Quit),
        KeyCode::Char('j') | KeyCode::Down => Some(AppEvent::MoveDown),
        KeyCode::Char('k') | KeyCode::Up => Some(AppEvent::MoveUp),
        KeyCode::Char('n') => Some(AppEvent::NextHunk),
        KeyCode::Char('p') => Some(AppEvent::PrevHunk),
        KeyCode::Char('s') => Some(AppEvent::Stage),
        KeyCode::Char('u') => Some(AppEvent::Unstage),
        KeyCode::Char('d') => Some(AppEvent::Discard),
        KeyCode::Char('c') => Some(AppEvent::Commit),
        KeyCode::Char('t') => Some(AppEvent::OpenThemePicker),
        KeyCode::Tab => Some(AppEvent::ToggleFocus),
        KeyCode::Enter => Some(AppEvent::Confirm),
        KeyCode::Esc => Some(AppEvent::Cancel),
        KeyCode::Backspace => Some(AppEvent::Backspace),
        KeyCode::Char(ch) => Some(AppEvent::Char(ch)),
        _ => None,
    }
}

fn translate_key_input(key: KeyEvent) -> Option<AppEvent> {
    if key.modifiers == KeyModifiers::CONTROL {
        return match key.code {
            KeyCode::Char('c') => Some(AppEvent::Quit),
            _ => None,
        };
    }

    match key.code {
        KeyCode::Enter => Some(AppEvent::Confirm),
        KeyCode::Esc => Some(AppEvent::Cancel),
        KeyCode::Backspace => Some(AppEvent::Backspace),
        KeyCode::Char(ch) => Some(AppEvent::Char(ch)),
        _ => None,
    }
}
