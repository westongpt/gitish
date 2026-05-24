use ratatui::{
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, Paragraph},
    Frame,
};

use crate::app::{App, Mode};

pub fn render(f: &mut Frame, app: &App) {
    let Mode::Loading(ref op) = app.mode else {
        return;
    };

    let theme = app.current_theme();
    let area = super::layout::centered_rect(30, 20, f.area());

    let label = Line::from(vec![
        Span::styled("  ", Style::default().fg(theme.base0a)),
        Span::styled(op.label(), Style::default().fg(theme.base05)),
    ]);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.base0a))
        .title(Span::styled(
            " Working ",
            Style::default()
                .fg(theme.base0a)
                .add_modifier(Modifier::BOLD),
        ))
        .style(Style::default().bg(app.bg_panel()));

    f.render_widget(Clear, area);
    f.render_widget(Paragraph::new(label).block(block).centered(), area);
}
