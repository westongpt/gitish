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

pub fn next_event() -> Result<Option<AppEvent>, AppError> {
    if !event::poll(std::time::Duration::from_millis(100))? {
        return Ok(None);
    }
    let ev = match event::read()? {
        Event::Key(k) => translate_key(k),
        _ => None,
    };
    Ok(ev)
}

fn translate_key(key: KeyEvent) -> Option<AppEvent> {
    // ctrl-c / ctrl-q always quit
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
