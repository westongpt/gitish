use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, List, ListItem, ListState},
    Frame,
};

use crate::app::{App, Focus};
use crate::git::repo::FileStatus;
use crate::glyphs::{Glyphs, StageState};

pub fn render(f: &mut Frame, app: &App, area: Rect) {
    let theme = app.current_theme();
    let glyphs = Glyphs::new(app.use_nerd_fonts);
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
            let (stage_state, stage_color) = match (file.staged, file.unstaged) {
                (true, false) => (StageState::Staged,   theme.base0e), // fully staged   — mauve
                (true, true)  => (StageState::Partial,  theme.base0a), // partially staged — yellow
                _             => (StageState::Unstaged, theme.base03), // not staged      — dim
            };
            let stage_icon = glyphs.stage_state(stage_state);

            let type_color = match file.status {
                FileStatus::Untracked  => theme.base03,
                FileStatus::New        => theme.base0b,
                FileStatus::Modified   => theme.base0d,
                FileStatus::Deleted    => theme.base08,
                FileStatus::Conflicted => theme.base09,
            };
            let type_icon = glyphs.file_status(file.status);

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
