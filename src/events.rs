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
    DeleteUntracked,
    Commit,
    // conflict resolution
    AcceptOurs,
    AcceptTheirs,
    AcceptBoth,
    // remote
    Push,
    Pull,
    // modes
    ToggleFocus,
    OpenThemePicker,
    OpenHelp,
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
        KeyCode::Char('X') => Some(AppEvent::DeleteUntracked),
        KeyCode::Char('c') => Some(AppEvent::Commit),
        KeyCode::Char('P') => Some(AppEvent::Push),
        KeyCode::Char('L') => Some(AppEvent::Pull),
        KeyCode::Char('t') => Some(AppEvent::OpenThemePicker),
        KeyCode::Char('o') => Some(AppEvent::AcceptOurs),
        KeyCode::Char('i') => Some(AppEvent::AcceptTheirs),
        KeyCode::Char('b') => Some(AppEvent::AcceptBoth),
        KeyCode::Char('?') => Some(AppEvent::OpenHelp),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn ctrl(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::CONTROL)
    }

    #[test]
    fn translate_key_quit() {
        assert_eq!(translate_key(key(KeyCode::Char('q'))), Some(AppEvent::Quit));
    }

    #[test]
    fn translate_key_ctrl_c_quits() {
        assert_eq!(translate_key(ctrl(KeyCode::Char('c'))), Some(AppEvent::Quit));
    }

    #[test]
    fn translate_key_ctrl_q_quits() {
        assert_eq!(translate_key(ctrl(KeyCode::Char('q'))), Some(AppEvent::Quit));
    }

    #[test]
    fn translate_key_ctrl_other_is_none() {
        assert_eq!(translate_key(ctrl(KeyCode::Char('x'))), None);
    }

    #[test]
    fn translate_key_move_down() {
        assert_eq!(translate_key(key(KeyCode::Char('j'))), Some(AppEvent::MoveDown));
        assert_eq!(translate_key(key(KeyCode::Down)), Some(AppEvent::MoveDown));
    }

    #[test]
    fn translate_key_move_up() {
        assert_eq!(translate_key(key(KeyCode::Char('k'))), Some(AppEvent::MoveUp));
        assert_eq!(translate_key(key(KeyCode::Up)), Some(AppEvent::MoveUp));
    }

    #[test]
    fn translate_key_hunk_navigation() {
        assert_eq!(translate_key(key(KeyCode::Char('n'))), Some(AppEvent::NextHunk));
        assert_eq!(translate_key(key(KeyCode::Char('p'))), Some(AppEvent::PrevHunk));
    }

    #[test]
    fn translate_key_staging() {
        assert_eq!(translate_key(key(KeyCode::Char('s'))), Some(AppEvent::Stage));
        assert_eq!(translate_key(key(KeyCode::Char('u'))), Some(AppEvent::Unstage));
        assert_eq!(translate_key(key(KeyCode::Char('d'))), Some(AppEvent::Discard));
    }

    #[test]
    fn translate_key_delete_untracked() {
        assert_eq!(translate_key(key(KeyCode::Char('X'))), Some(AppEvent::DeleteUntracked));
    }

    #[test]
    fn translate_key_commit_and_remote() {
        assert_eq!(translate_key(key(KeyCode::Char('c'))), Some(AppEvent::Commit));
        assert_eq!(translate_key(key(KeyCode::Char('P'))), Some(AppEvent::Push));
        assert_eq!(translate_key(key(KeyCode::Char('L'))), Some(AppEvent::Pull));
    }

    #[test]
    fn translate_key_conflict_resolution() {
        assert_eq!(translate_key(key(KeyCode::Char('o'))), Some(AppEvent::AcceptOurs));
        assert_eq!(translate_key(key(KeyCode::Char('i'))), Some(AppEvent::AcceptTheirs));
        assert_eq!(translate_key(key(KeyCode::Char('b'))), Some(AppEvent::AcceptBoth));
    }

    #[test]
    fn translate_key_structural() {
        assert_eq!(translate_key(key(KeyCode::Tab)), Some(AppEvent::ToggleFocus));
        assert_eq!(translate_key(key(KeyCode::Enter)), Some(AppEvent::Confirm));
        assert_eq!(translate_key(key(KeyCode::Esc)), Some(AppEvent::Cancel));
        assert_eq!(translate_key(key(KeyCode::Backspace)), Some(AppEvent::Backspace));
    }

    #[test]
    fn translate_key_theme_and_help() {
        assert_eq!(translate_key(key(KeyCode::Char('t'))), Some(AppEvent::OpenThemePicker));
        assert_eq!(translate_key(key(KeyCode::Char('?'))), Some(AppEvent::OpenHelp));
    }

    #[test]
    fn translate_key_unknown_is_none() {
        assert_eq!(translate_key(key(KeyCode::F(1))), None);
    }

    #[test]
    fn translate_key_input_passthrough_char() {
        assert_eq!(translate_key_input(key(KeyCode::Char('a'))), Some(AppEvent::Char('a')));
        assert_eq!(translate_key_input(key(KeyCode::Char('z'))), Some(AppEvent::Char('z')));
    }

    #[test]
    fn translate_key_input_structural_keys() {
        assert_eq!(translate_key_input(key(KeyCode::Enter)), Some(AppEvent::Confirm));
        assert_eq!(translate_key_input(key(KeyCode::Esc)), Some(AppEvent::Cancel));
        assert_eq!(translate_key_input(key(KeyCode::Backspace)), Some(AppEvent::Backspace));
    }

    #[test]
    fn translate_key_input_ctrl_c_quits() {
        assert_eq!(translate_key_input(ctrl(KeyCode::Char('c'))), Some(AppEvent::Quit));
    }

    #[test]
    fn translate_key_input_ctrl_other_is_none() {
        assert_eq!(translate_key_input(ctrl(KeyCode::Char('s'))), None);
    }

    #[test]
    fn translate_key_input_unknown_is_none() {
        assert_eq!(translate_key_input(key(KeyCode::F(5))), None);
    }
}
