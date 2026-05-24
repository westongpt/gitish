use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, List, ListItem, ListState},
    Frame,
};

use crate::app::{App, Focus};
use crate::git::repo::FileStatus;

// File-type glyphs (nf-md / Material Design Icons)
const ICON_UNTRACKED: &str  = "\u{F128}";  // nf-fa-question_circle  — unknown to git
const ICON_NEW: &str        = "\u{F0214}"; // nf-md-file_plus        — newly staged file
const ICON_MODIFIED: &str   = "\u{F03EB}"; // nf-md-pencil           — changed file
const ICON_DELETED: &str    = "\u{F01B4}"; // nf-md-delete           — removed file
const ICON_CONFLICTED: &str = "\u{F0E7A}"; // nf-md-alert_circle     — merge conflict

// Staging-state glyphs (nf-fa / Font Awesome circle family)
const ICON_STAGED: &str   = "\u{F058}"; // nf-fa-check_circle   — fully in index
const ICON_PARTIAL: &str  = "\u{F192}"; // nf-fa-dot_circle_o   — partially staged
const ICON_UNSTAGED: &str = "\u{F10C}"; // nf-fa-circle_o       — not staged

pub fn render(f: &mut Frame, app: &App, area: Rect) {
    let theme = app.current_theme();
    let focused = app.focus == Focus::FileList;

    let border_style = if focused {
        Style::default().fg(theme.base0e)
    } else {
        Style::default().fg(theme.base03)
    };

    let items: Vec<ListItem> = app
        .files
        .iter()
        .map(|file| {
            let (stage_icon, stage_color) = match (file.staged, file.unstaged) {
                (true, false) => (ICON_STAGED,   theme.base0e), // fully staged   — mauve
                (true, true)  => (ICON_PARTIAL,  theme.base0a), // partially staged — yellow
                _             => (ICON_UNSTAGED, theme.base03), // not staged      — dim
            };

            let (type_icon, type_color) = match file.status {
                FileStatus::Untracked  => (ICON_UNTRACKED,  theme.base03),
                FileStatus::New        => (ICON_NEW,         theme.base0b),
                FileStatus::Modified   => (ICON_MODIFIED,    theme.base0d),
                FileStatus::Deleted    => (ICON_DELETED,     theme.base08),
                FileStatus::Conflicted => (ICON_CONFLICTED,  theme.base09),
            };

            let line = Line::from(vec![
                Span::styled(format!("{stage_icon} "), Style::default().fg(stage_color)),
                Span::styled(format!("{type_icon} "), Style::default().fg(type_color)),
                Span::styled(&file.path, Style::default().fg(theme.base05)),
            ]);
            ListItem::new(line)
        })
        .collect();

    let mut state = ListState::default();
    state.select(if app.files.is_empty() {
        None
    } else {
        Some(app.file_cursor)
    });

    let title = Span::styled(
        " Files ",
        Style::default().fg(theme.base0d).add_modifier(Modifier::BOLD),
    );

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(border_style)
                .title(title)
                .style(Style::default().bg(app.bg_main())),
        )
        .highlight_style(
            Style::default()
                .bg(theme.base02)
                .fg(theme.base07)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("▶ ");

    f.render_stateful_widget(list, area, &mut state);
}
