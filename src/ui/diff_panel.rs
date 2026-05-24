use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, List, ListItem, ListState},
    Frame,
};

use crate::app::{App, Focus};
use crate::git::repo::{ConflictBlock, Hunk, LineKind};
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

    let no_file_selected = file_name.is_empty();

    let total = app.total_hunks();
    let hunk_counter = if app.is_file_conflicted() && !app.conflict_blocks.is_empty() {
        format!(
            " [conflict {}/{}] ",
            app.conflict_cursor + 1,
            app.conflict_blocks.len()
        )
    } else if total > 0 {
        format!(" [{}/{}] ", app.hunk_cursor + 1, total)
    } else {
        String::new()
    };

    let title = if no_file_selected {
        Line::from(Span::styled(
            " Diff",
            Style::default().fg(theme.base0d).add_modifier(Modifier::BOLD),
        ))
    } else {
        Line::from(vec![
            Span::styled(
                format!(" {file_name}"),
                Style::default().fg(theme.base0d).add_modifier(Modifier::BOLD),
            ),
            Span::styled(hunk_counter, Style::default().fg(theme.base03)),
        ])
    };

    // Build flat list of items
    let mut items: Vec<ListItem> = Vec::new();
    let mut selected_row: usize = 0;

    if no_file_selected {
        items.push(ListItem::new(Line::from(Span::styled(
            " No File Selected",
            Style::default().fg(theme.base03),
        ))));
    } else if app.is_file_conflicted() {
        // Conflict view
        if app.conflict_blocks.is_empty() {
            items.push(ListItem::new(Line::from(Span::styled(
                " No conflict markers found",
                Style::default().fg(theme.base03),
            ))));
        } else {
            for (i, block) in app.conflict_blocks.iter().enumerate() {
                let is_selected = app.conflict_cursor == i;
                if is_selected {
                    selected_row = items.len();
                }
                push_conflict_items(&mut items, block, i, is_selected, theme, app);
            }
        }
    } else {
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
    }

    let mut state = ListState::default();
    let has_content = !app.conflict_blocks.is_empty() || total > 0;
    if has_content {
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

fn push_conflict_items(
    items: &mut Vec<ListItem>,
    block: &ConflictBlock,
    idx: usize,
    is_selected: bool,
    theme: &Theme,
    app: &App,
) {
    let sel_bg = theme.base02;
    let normal_bg = app.bg_main();
    let bg = if is_selected { sel_bg } else { normal_bg };
    let cursor_glyph = if is_selected { "▶ " } else { "  " };
    let n_conflicts = app.conflict_blocks.len();

    // Conflict block header
    items.push(ListItem::new(Line::from(vec![
        Span::styled(
            cursor_glyph,
            Style::default().fg(theme.base09).bg(bg),
        ),
        Span::styled(
            format!("Conflict {}/{}", idx + 1, n_conflicts),
            Style::default()
                .fg(theme.base09)
                .bg(bg)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            "  o: ours  i: incoming  b: both",
            Style::default().fg(theme.base03).bg(bg),
        ),
    ])));

    // Ours section header
    items.push(ListItem::new(Line::from(Span::styled(
        "  ◀ Ours (HEAD)",
        Style::default()
            .fg(theme.base0b)
            .bg(bg)
            .add_modifier(Modifier::ITALIC),
    ))));
    for line in &block.ours {
        items.push(ListItem::new(Line::from(Span::styled(
            format!("  +{line}"),
            Style::default().fg(theme.base0b).bg(bg),
        ))));
    }

    // Separator
    items.push(ListItem::new(Line::from(Span::styled(
        "  ═══════════════",
        Style::default().fg(theme.base03).bg(bg),
    ))));

    // Theirs section header
    items.push(ListItem::new(Line::from(Span::styled(
        "  ▶ Incoming",
        Style::default()
            .fg(theme.base08)
            .bg(bg)
            .add_modifier(Modifier::ITALIC),
    ))));
    for line in &block.theirs {
        items.push(ListItem::new(Line::from(Span::styled(
            format!("  -{line}"),
            Style::default().fg(theme.base08).bg(bg),
        ))));
    }
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

#[cfg(test)]
mod tests {
    use ratatui::{backend::TestBackend, Terminal};

    use crate::app::App;

    fn make_app() -> (tempfile::TempDir, tempfile::TempDir, App) {
        let repo_dir = tempfile::TempDir::new().unwrap();
        let config_dir = tempfile::TempDir::new().unwrap();
        let repo = git2::Repository::init(repo_dir.path()).unwrap();
        let app = App::new(repo, config_dir.path().to_path_buf(), None).unwrap();
        (repo_dir, config_dir, app)
    }

    fn render_to_string(app: &App, width: u16, height: u16) -> String {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|f| {
                super::render(f, app, f.area());
            })
            .unwrap();
        let buffer = terminal.backend().buffer().clone();
        buffer
            .content
            .iter()
            .map(|c| c.symbol().to_string())
            .collect::<Vec<_>>()
            .join("")
    }

    #[test]
    fn empty_state_shows_no_file_selected_message() {
        let (_repo, _cfg, app) = make_app();
        // A new empty repo has no files, so no file is selected.
        assert!(app.files.is_empty());
        let rendered = render_to_string(&app, 60, 10);
        assert!(
            rendered.contains("No File Selected"),
            "expected 'No File Selected' in diff pane when no file is selected, got: {rendered:?}"
        );
    }

    #[test]
    fn empty_state_title_does_not_show_space_only() {
        let (_repo, _cfg, app) = make_app();
        assert!(app.files.is_empty());
        let rendered = render_to_string(&app, 60, 10);
        // Title should show "Diff", not just a bare space.
        assert!(
            rendered.contains("Diff"),
            "expected 'Diff' in title when no file is selected, got: {rendered:?}"
        );
    }
}
