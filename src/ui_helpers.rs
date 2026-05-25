use std::path::Path;

/// Number of lines in the static help content — must stay in sync with the
/// help popup in `ui/mod.rs`.
pub const HELP_CONTENT_LINES: u16 = 30;

/// Compute the maximum scroll offset for the help popup given the terminal height.
pub fn help_max_scroll(terminal_height: u16) -> u16 {
    let popup_height = terminal_height * 80 / 100;
    let inner_height = popup_height.saturating_sub(2);
    HELP_CONTENT_LINES.saturating_sub(inner_height)
}

/// Convert an absolute path to a `~/…` representation when it falls under the
/// user's home directory.  Returns the path unchanged if it does not.
pub fn tilde_path(path: &Path) -> Option<String> {
    let home = dirs::home_dir()?;
    if let Ok(rel) = path.strip_prefix(&home) {
        Some(format!("~/{}", rel.display()))
    } else {
        Some(path.display().to_string())
    }
}

/// Build the hunk-counter string shown in the diff panel title bar.
/// Returns an empty string when there is nothing to display.
pub fn hunk_counter_text(
    is_conflicted: bool,
    conflict_cursor: usize,
    n_conflicts: usize,
    hunk_cursor: usize,
    total_hunks: usize,
) -> String {
    if is_conflicted && n_conflicts > 0 {
        format!(" [conflict {}/{}] ", conflict_cursor + 1, n_conflicts)
    } else if total_hunks > 0 {
        format!(" [{}/{}] ", hunk_cursor + 1, total_hunks)
    } else {
        String::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    // ── help_max_scroll ───────────────────────────────────────────────────

    #[test]
    fn help_max_scroll_zero_when_terminal_is_very_tall() {
        // popup_height = 200 * 80/100 = 160, inner = 158 > HELP_CONTENT_LINES (30)
        assert_eq!(help_max_scroll(200), 0);
    }

    #[test]
    fn help_max_scroll_nonzero_when_terminal_is_short() {
        // popup_height = 20 * 80/100 = 16, inner = 14, max_scroll = 30 - 14 = 16
        assert_eq!(help_max_scroll(20), 16);
    }

    #[test]
    fn help_max_scroll_does_not_underflow() {
        // even a 0-height terminal must not panic or underflow
        assert_eq!(help_max_scroll(0), HELP_CONTENT_LINES);
    }

    // ── tilde_path ────────────────────────────────────────────────────────

    #[test]
    fn tilde_path_replaces_home_prefix() {
        if let Some(home) = dirs::home_dir() {
            let path = home.join("projects/foo");
            let result = tilde_path(&path).unwrap();
            assert!(result.starts_with("~/"), "expected tilde prefix, got {result:?}");
            assert!(result.contains("projects/foo"));
        }
    }

    #[test]
    fn tilde_path_leaves_non_home_path_unchanged() {
        let path = PathBuf::from("/tmp/not/under/home");
        let result = tilde_path(&path).unwrap();
        assert_eq!(result, "/tmp/not/under/home");
    }

    // ── hunk_counter_text ─────────────────────────────────────────────────

    #[test]
    fn hunk_counter_empty_when_no_hunks_and_not_conflicted() {
        assert_eq!(hunk_counter_text(false, 0, 0, 0, 0), "");
    }

    #[test]
    fn hunk_counter_shows_hunk_position() {
        let text = hunk_counter_text(false, 0, 0, 1, 3);
        assert_eq!(text, " [2/3] ");
    }

    #[test]
    fn hunk_counter_shows_conflict_position() {
        let text = hunk_counter_text(true, 0, 2, 0, 0);
        assert_eq!(text, " [conflict 1/2] ");
    }

    #[test]
    fn hunk_counter_conflict_wins_over_hunk_count() {
        // when conflicted, show conflict counter even if total_hunks > 0
        let text = hunk_counter_text(true, 1, 3, 2, 5);
        assert!(text.contains("conflict"), "conflict marker must take precedence");
    }

    #[test]
    fn hunk_counter_empty_when_conflicted_but_no_conflict_blocks() {
        // is_conflicted but n_conflicts == 0 means markers were resolved
        assert_eq!(hunk_counter_text(true, 0, 0, 0, 0), "");
    }
}
