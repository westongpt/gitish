use ratatui::{
    style::{Modifier, Style},
    text::Span,
    widgets::{Block, BorderType, Borders, Clear, List, ListItem, ListState},
    Frame,
};

use crate::app::App;
use crate::ui::layout::centered_rect;

pub fn render(f: &mut Frame, app: &App) {
    let theme = app.current_theme();
    let area = centered_rect(50, 60, f.area());

    let items: Vec<ListItem> = app
        .themes
        .iter()
        .map(|t| ListItem::new(Span::styled(&t.name, Style::default().fg(theme.base05))))
        .collect();

    let mut state = ListState::default();
    state.select(Some(app.theme_picker_cursor));

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(theme.base0e))
                .title(Span::styled(
                    " Select Theme ",
                    Style::default()
                        .fg(theme.base0e)
                        .add_modifier(Modifier::BOLD),
                ))
                .style(Style::default().bg(theme.base01)),
        )
        .highlight_style(
            Style::default()
                .bg(theme.base02)
                .fg(theme.base07)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("▶ ");

    f.render_widget(Clear, area);
    f.render_stateful_widget(list, area, &mut state);
}
