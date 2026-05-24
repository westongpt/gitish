use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, List, ListItem, ListState},
    Frame,
};

use crate::app::{App, Focus};
use crate::git::repo::FileStatus;

const ICON_NEW: &str = "󰈔";
const ICON_MODIFIED: &str = "󰏫";
const ICON_DELETED: &str = "󰆴";
const ICON_STAGED: &str = "";

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
            let icon = match file.status {
                FileStatus::New => ICON_NEW,
                FileStatus::Modified => ICON_MODIFIED,
                FileStatus::Deleted => ICON_DELETED,
            };
            let staged_icon = if file.staged { ICON_STAGED } else { " " };
            let icon_color = match file.status {
                FileStatus::New => theme.base0b,
                FileStatus::Modified => theme.base0d,
                FileStatus::Deleted => theme.base08,
            };
            // partial staging: staged + still has unstaged changes → yellow hint
            let staged_color = match (file.staged, file.unstaged) {
                (true, true) => theme.base0a,
                (true, false) => theme.base0e,
                _ => theme.base03,
            };

            let line = Line::from(vec![
                Span::styled(format!("{staged_icon} "), Style::default().fg(staged_color)),
                Span::styled(format!("{icon} "), Style::default().fg(icon_color)),
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
