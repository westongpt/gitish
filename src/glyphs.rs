//! Central icon set with Nerd Font glyphs and ASCII fallbacks.
//!
//! When `use_nerd_fonts` is false (config flag), every icon resolves to a plain
//! ASCII character so the UI stays legible on terminals without a Nerd Font
//! installed (no tofu boxes / replacement glyphs).

use crate::git::repo::FileStatus;

/// How much of a file is staged, used to pick a staging-state marker.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StageState {
    Staged,
    Partial,
    Unstaged,
}

/// Resolves icons to either Nerd Font glyphs or ASCII fallbacks.
#[derive(Clone, Copy, Debug)]
pub struct Glyphs {
    nerd: bool,
}

impl Glyphs {
    pub fn new(use_nerd_fonts: bool) -> Self {
        Glyphs { nerd: use_nerd_fonts }
    }

    /// Icon for a file's git status (left-hand type marker).
    pub fn file_status(&self, status: FileStatus) -> &'static str {
        match (self.nerd, status) {
            (true, FileStatus::Untracked) => "\u{F128}",  // nf-fa-question_circle
            (true, FileStatus::New) => "\u{F0214}",       // nf-md-file_plus
            (true, FileStatus::Modified) => "\u{F03EB}",  // nf-md-pencil
            (true, FileStatus::Deleted) => "\u{F01B4}",   // nf-md-delete
            (true, FileStatus::Conflicted) => "\u{F0E7A}", // nf-md-alert_circle

            (false, FileStatus::Untracked) => "?",
            (false, FileStatus::New) => "A",
            (false, FileStatus::Modified) => "M",
            (false, FileStatus::Deleted) => "D",
            (false, FileStatus::Conflicted) => "!",
        }
    }

    /// Icon for a file's staging state (right-hand status marker).
    pub fn stage_state(&self, state: StageState) -> &'static str {
        match (self.nerd, state) {
            (true, StageState::Staged) => "\u{F058}",   // nf-fa-check_circle
            (true, StageState::Partial) => "\u{F192}",  // nf-fa-dot_circle_o
            (true, StageState::Unstaged) => "\u{F10C}", // nf-fa-circle_o

            (false, StageState::Staged) => "+",
            (false, StageState::Partial) => "~",
            (false, StageState::Unstaged) => " ",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nerd_file_status_returns_glyphs() {
        let g = Glyphs::new(true);
        assert_eq!(g.file_status(FileStatus::Untracked), "\u{F128}");
        assert_eq!(g.file_status(FileStatus::New), "\u{F0214}");
        assert_eq!(g.file_status(FileStatus::Modified), "\u{F03EB}");
        assert_eq!(g.file_status(FileStatus::Deleted), "\u{F01B4}");
        assert_eq!(g.file_status(FileStatus::Conflicted), "\u{F0E7A}");
    }

    #[test]
    fn ascii_file_status_returns_plain_chars() {
        let g = Glyphs::new(false);
        assert_eq!(g.file_status(FileStatus::Untracked), "?");
        assert_eq!(g.file_status(FileStatus::New), "A");
        assert_eq!(g.file_status(FileStatus::Modified), "M");
        assert_eq!(g.file_status(FileStatus::Deleted), "D");
        assert_eq!(g.file_status(FileStatus::Conflicted), "!");
    }

    #[test]
    fn nerd_stage_state_returns_glyphs() {
        let g = Glyphs::new(true);
        assert_eq!(g.stage_state(StageState::Staged), "\u{F058}");
        assert_eq!(g.stage_state(StageState::Partial), "\u{F192}");
        assert_eq!(g.stage_state(StageState::Unstaged), "\u{F10C}");
    }

    #[test]
    fn ascii_stage_state_returns_plain_chars() {
        let g = Glyphs::new(false);
        assert_eq!(g.stage_state(StageState::Staged), "+");
        assert_eq!(g.stage_state(StageState::Partial), "~");
        assert_eq!(g.stage_state(StageState::Unstaged), " ");
    }

    #[test]
    fn ascii_icons_are_single_byte_no_tofu() {
        let g = Glyphs::new(false);
        for s in [
            g.file_status(FileStatus::Untracked),
            g.file_status(FileStatus::New),
            g.file_status(FileStatus::Modified),
            g.file_status(FileStatus::Deleted),
            g.file_status(FileStatus::Conflicted),
            g.stage_state(StageState::Staged),
            g.stage_state(StageState::Partial),
            g.stage_state(StageState::Unstaged),
        ] {
            assert!(s.is_ascii(), "ASCII fallback {s:?} must contain only ASCII");
            assert_eq!(s.chars().count(), 1, "ASCII fallback {s:?} must be one column wide");
        }
    }
}
