use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, List, ListItem, ListState},
    Frame,
};

use crate::app::{App, Focus};
use crate::git::repo::{Hunk, LineKind};
use crate::theme::Theme;

pub fn render(f: &mut Frame, app: &App, area: Rect) {
    let theme = app.current_theme();
    let focused = app.focus == Focus::DiffView;

    let border_style = if focused {
        Style::default().fg(theme.base0e)
    } else {
        Style::default().fg(theme.base03)
    };

    let file_name = app
        .files
        .get(app.file_cursor)
        .map(|f| f.path.as_str())
        .unwrap_or("");

    let total = app.total_hunks();
    let hunk_counter = if total > 0 {
        format!(" [{}/{}] ", app.hunk_cursor + 1, total)
    } else {
        String::new()
    };

    let title = Line::from(vec![
        Span::styled(
            format!(" {file_name}"),
            Style::default().fg(theme.base0d).add_modifier(Modifier::BOLD),
        ),
        Span::styled(hunk_counter, Style::default().fg(theme.base03)),
    ]);

    // Build flat list of items tracking which row = which hunk
    let mut items: Vec<ListItem> = Vec::new();
    // row_to_hunk[row] = Some(hunk_cursor_index) if that row belongs to a hunk
    let mut selected_row: usize = 0;

    let n_staged = app.staged_hunks.len();

    if n_staged > 0 {
        items.push(section_header("  Staged", theme.base0e, theme.base01));
        for (i, hunk) in app.staged_hunks.iter().enumerate() {
            let cursor_idx = i;
            let is_selected = app.hunk_cursor == cursor_idx;
            if is_selected {
                selected_row = items.len();
            }
            push_hunk_items(&mut items, hunk, is_selected, HunkKind::Staged, theme, app);
        }
    }

    if !app.unstaged_hunks.is_empty() {
        items.push(section_header("  Unstaged", theme.base08, theme.base01));
        for (i, hunk) in app.unstaged_hunks.iter().enumerate() {
            let cursor_idx = n_staged + i;
            let is_selected = app.hunk_cursor == cursor_idx;
            if is_selected {
                selected_row = items.len();
            }
            push_hunk_items(&mut items, hunk, is_selected, HunkKind::Unstaged, theme, app);
        }
    }

    if items.is_empty() {
        items.push(ListItem::new(Line::from(Span::styled(
            " No changes",
            Style::default().fg(theme.base03),
        ))));
    }

    let mut state = ListState::default();
    if total > 0 {
        state.select(Some(selected_row));
    }

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(border_style)
                .title(title)
                .style(Style::default().bg(app.bg_main())),
        )
        // No automatic highlight — we set backgrounds manually per line
        .highlight_style(Style::default());

    f.render_stateful_widget(list, area, &mut state);
}

enum HunkKind {
    Staged,
    Unstaged,
}

fn section_header<'a>(label: &'a str, fg: Color, bg: Color) -> ListItem<'a> {
    ListItem::new(Line::from(Span::styled(
        label,
        Style::default()
            .fg(fg)
            .bg(bg)
            .add_modifier(Modifier::BOLD | Modifier::ITALIC),
    )))
}

fn push_hunk_items(
    items: &mut Vec<ListItem>,
    hunk: &Hunk,
    is_selected: bool,
    kind: HunkKind,
    theme: &Theme,
    app: &App,
) {
    let sel_bg = theme.base02;
    let normal_bg = app.bg_main();
    let bg = if is_selected { sel_bg } else { normal_bg };

    // hunk header line
    let header_fg = match kind {
        HunkKind::Staged => theme.base0e,
        HunkKind::Unstaged => theme.base0c,
    };
    let cursor_glyph = if is_selected { "▶ " } else { "  " };
    items.push(ListItem::new(Line::from(vec![
        Span::styled(
            cursor_glyph,
            Style::default().fg(header_fg).bg(bg),
        ),
        Span::styled(
            hunk.header.clone(),
            Style::default()
                .fg(header_fg)
                .bg(bg)
                .add_modifier(Modifier::BOLD),
        ),
    ])));

    // diff lines
    for line in &hunk.lines {
        let (prefix, fg) = match line.kind {
            LineKind::Added => ('+', theme.base0b),
            LineKind::Removed => ('-', theme.base08),
            LineKind::Context => (' ', theme.base05),
        };
        // dim context lines when not selected so the +/- lines pop
        let fg = if !is_selected && line.kind == LineKind::Context {
            theme.base03
        } else {
            fg
        };
        items.push(ListItem::new(Line::from(Span::styled(
            format!("  {prefix}{}", line.content),
            Style::default().fg(fg).bg(bg),
        ))));
    }
}
