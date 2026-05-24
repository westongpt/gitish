use ratatui::{
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, Paragraph},
    Frame,
};

use crate::app::{App, Mode, PendingAction};
use crate::ui::layout::centered_rect;

pub fn render(f: &mut Frame, app: &App) {
    let Mode::Confirming(action) = &app.mode else {
        return;
    };
    let theme = app.current_theme();
    let area = centered_rect(50, 20, f.area());

    let prompt = match action {
        PendingAction::DeleteUntracked(_) => action.prompt(),
    };

    let content = Line::from(vec![
        Span::styled("  ", Style::default()),
        Span::styled(prompt, Style::default().fg(theme.base05)),
    ]);
    let hint = Line::from(vec![Span::styled(
        "  y / Enter: confirm   n / Esc: cancel",
        Style::default().fg(theme.base03),
    )]);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.base08))
        .title(Span::styled(
            " Confirm ",
            Style::default()
                .fg(theme.base08)
                .add_modifier(Modifier::BOLD),
        ))
        .style(Style::default().bg(app.bg_panel()));

    f.render_widget(Clear, area);
    f.render_widget(
        Paragraph::new(vec![content, Line::from(""), hint]).block(block),
        area,
    );
}
