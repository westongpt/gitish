mod commit_bar;
mod diff_panel;
mod file_panel;
mod layout;
mod theme_picker;

use ratatui::{
    style::Style,
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use crate::app::App;

pub fn draw(f: &mut Frame, app: &App) {
    let layout = layout::split_layout(f.area());

    file_panel::render(f, app, layout.file_panel);
    diff_panel::render(f, app, layout.diff_panel);
    commit_bar::render(f, app, layout.commit_bar);
    render_status_bar(f, app, layout.status_bar);

    if app.mode == crate::app::Mode::ThemePicker {
        theme_picker::render(f, app);
    }
}

fn render_status_bar(f: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    let theme = app.current_theme();
    let msg = app.status_msg.as_deref().unwrap_or("");
    let line = Line::from(vec![Span::styled(
        format!(" {msg}"),
        Style::default().fg(theme.base0c),
    )]);
    f.render_widget(
        Paragraph::new(line).style(Style::default().bg(app.bg_panel())),
        area,
    );
}
