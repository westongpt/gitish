use ratatui::{
    layout::{Constraint, Direction, Layout},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, List, ListItem, ListState, Paragraph},
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

    let items: Vec<ListItem> = app
        .themes
        .iter()
        .map(|t| ListItem::new(Span::styled(&t.name, Style::default().fg(theme.base05))))
        .collect();

    let selected_in_list = (app.theme_picker_cursor < app.themes.len())
        .then_some(app.theme_picker_cursor);

    let mut state = ListState::default();
    state.select(selected_in_list);

    let list = List::new(items)
        .highlight_style(
            Style::default()
                .bg(theme.base02)
                .fg(theme.base07)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("▶ ");

    f.render_stateful_widget(list, chunks[0], &mut state);

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
