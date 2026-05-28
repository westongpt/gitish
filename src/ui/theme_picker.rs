use ratatui::{
    layout::{Constraint, Direction, Layout},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, List, ListItem, Paragraph},
    Frame,
};

use crate::app::App;
use crate::ui::layout::centered_rect;

pub fn render(f: &mut Frame, app: &App) {
    let theme = app.current_theme();
    let area = centered_rect(50, 60, f.area());

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.base0e))
        .title(Span::styled(
            " Select Theme ",
            Style::default()
                .fg(theme.base0e)
                .add_modifier(Modifier::BOLD),
        ))
        .style(Style::default().bg(app.bg_panel()));

    let inner = block.inner(area);
    f.render_widget(Clear, area);
    f.render_widget(block, area);

    // Split inner: theme list fills available space, toggle pinned to bottom.
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(1)])
        .split(inner);

    // Build items with the cursor prefix baked into the text so the left
    // margin stays fixed regardless of whether the transparent-toggle row
    // is selected (ratatui's highlight_symbol only adds padding when the
    // list itself has a selection, causing items to snap left otherwise).
    let items: Vec<ListItem> = app
        .themes
        .iter()
        .enumerate()
        .map(|(i, t)| {
            let is_selected = app.theme_picker_cursor == i;
            let prefix = if is_selected { "▶ " } else { "  " };
            let style = if is_selected {
                Style::default()
                    .bg(theme.base02)
                    .fg(theme.base07)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme.base05)
            };
            ListItem::new(Span::styled(format!("{prefix}{}", t.name), style))
        })
        .collect();

    f.render_widget(List::new(items), chunks[0]);

    let on_toggle = app.theme_picker_cursor == app.themes.len();
    let check = if app.transparent { "󰄵 " } else { "󰄱 " };
    let prefix = if on_toggle { "▶ " } else { "  " };
    let toggle_style = if on_toggle {
        Style::default()
            .bg(theme.base02)
            .fg(theme.base07)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme.base0d)
    };
    let toggle = Paragraph::new(Line::from(Span::styled(
        format!("{prefix}{check}Use transparent background"),
        toggle_style,
    )));
    f.render_widget(toggle, chunks[1]);
}

#[cfg(test)]
mod tests {
    use ratatui::{backend::TestBackend, Terminal};

    use crate::app::{App, Mode};

    fn make_app() -> (tempfile::TempDir, tempfile::TempDir, App) {
        let repo_dir = tempfile::TempDir::new().unwrap();
        let config_dir = tempfile::TempDir::new().unwrap();
        let repo = git2::Repository::init(repo_dir.path()).unwrap();
        let app = App::new(repo, config_dir.path().to_path_buf(), None).unwrap();
        (repo_dir, config_dir, app)
    }

    /// Render the picker and return the buffer as a Vec of per-row cell symbols.
    fn render_rows(app: &App, width: u16, height: u16) -> Vec<Vec<String>> {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| super::render(f, app)).unwrap();
        let buf = terminal.backend().buffer().clone();
        (0..height)
            .map(|row| {
                (0..width)
                    .map(|col| buf[(col, row)].symbol().to_string())
                    .collect()
            })
            .collect()
    }

    /// Find the column index of `needle` in any rendered row, searching by
    /// joining each row's cells into a string and looking for the substring.
    fn find_col_of(rows: &[Vec<String>], needle: &str) -> Option<usize> {
        for row in rows {
            let line: String = row.join("");
            if let Some(byte_pos) = line.find(needle) {
                // Convert byte position to cell (column) index.
                let col = line[..byte_pos].chars().count();
                return Some(col);
            }
        }
        None
    }

    #[test]
    fn theme_name_column_unchanged_when_toggle_selected() {
        let (_repo, _cfg, mut app) = make_app();
        app.mode = Mode::ThemePicker;

        let name = app.themes.iter().next().unwrap().name.clone();

        // Cursor on first theme — baked-in "▶ " prefix.
        app.theme_picker_cursor = 0;
        let rows_theme = render_rows(&app, 80, 30);
        let col_theme = find_col_of(&rows_theme, &name);

        // Cursor on transparent-toggle row — previously caused snap-left.
        app.theme_picker_cursor = app.themes.len();
        let rows_toggle = render_rows(&app, 80, 30);
        let col_toggle = find_col_of(&rows_toggle, &name);

        assert!(col_theme.is_some(), "theme name must appear when a theme is selected");
        assert!(col_toggle.is_some(), "theme name must still appear when toggle is selected");
        assert_eq!(
            col_theme, col_toggle,
            "theme name column must not shift when moving selection to the transparent toggle"
        );
    }

    #[test]
    fn selected_theme_shows_cursor_glyph() {
        let (_repo, _cfg, mut app) = make_app();
        app.mode = Mode::ThemePicker;
        app.theme_picker_cursor = 0;
        let rows = render_rows(&app, 80, 30);
        let full: String = rows.into_iter().flatten().collect();
        assert!(full.contains('▶'), "selected theme row must show the ▶ cursor glyph");
    }

    #[test]
    fn unselected_themes_do_not_show_cursor_glyph() {
        let (_repo, _cfg, mut app) = make_app();
        app.mode = Mode::ThemePicker;

        // Need at least two themes to select index 1 and check that index 0 has no glyph.
        if app.themes.len() < 2 {
            return;
        }
        app.theme_picker_cursor = 1;
        let first_name = app.themes.iter().next().unwrap().name.clone();
        let rows = render_rows(&app, 80, 30);

        // Find the row that contains the first theme name.
        let row_line = rows
            .iter()
            .find(|r| r.join("").contains(&first_name))
            .expect("first theme name must appear");

        let line = row_line.join("");
        let name_pos = line.find(&first_name).unwrap();
        // The two cells before the name must not contain "▶".
        let before: String = line.chars().take(name_pos).collect();
        assert!(
            !before.ends_with('▶'),
            "non-selected theme row must not have ▶ glyph, prefix was: {before:?}"
        );
    }
}
