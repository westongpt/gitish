mod commit_bar;
mod confirm_dialog;
mod diff_panel;
mod file_panel;
mod layout;
mod loading_overlay;
mod theme_picker;

use ratatui::{
    style::{Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
    Frame,
};

use crate::app::App;

/// Number of lines in the static help content — must stay in sync with the
/// `lines` vec in `render_help_popup`.
pub const HELP_CONTENT_LINES: u16 = 30;

/// Compute the maximum scroll offset for the help popup given the terminal height.
/// Mirrors the 80% / border math from `render_help_popup` so callers outside the
/// render path can clamp `help_scroll` without needing the actual frame area.
pub fn help_max_scroll(terminal_height: u16) -> u16 {
    let popup_height = terminal_height * 80 / 100;
    let inner_height = popup_height.saturating_sub(2);
    HELP_CONTENT_LINES.saturating_sub(inner_height)
}

pub fn draw(f: &mut Frame, app: &App) {
    let layout = layout::split_layout(f.area());

    file_panel::render(f, app, layout.file_panel);
    diff_panel::render(f, app, layout.diff_panel);
    commit_bar::render(f, app, layout.commit_bar);
    render_status_bar(f, app, layout.status_bar);

    if app.mode == crate::app::Mode::ThemePicker {
        theme_picker::render(f, app);
    }
    if app.mode == crate::app::Mode::Help {
        render_help_popup(f, app);
    }
    if matches!(app.mode, crate::app::Mode::Confirming(_)) {
        confirm_dialog::render(f, app);
    }
    if matches!(app.mode, crate::app::Mode::Loading(_)) {
        loading_overlay::render(f, app);
    }
}

fn render_help_popup(f: &mut Frame, app: &App) {
    let theme = app.current_theme();
    let area = layout::centered_rect(54, 80, f.area());

    let head = Style::default().fg(theme.base0d).add_modifier(Modifier::BOLD);
    let key = Style::default().fg(theme.base0a);
    let desc = Style::default().fg(theme.base05);

    let lines: Vec<Line> = vec![
        Line::from(Span::styled("Navigation", head)),
        Line::from(vec![Span::styled("  j / ↓       ", key), Span::styled("move down", desc)]),
        Line::from(vec![Span::styled("  k / ↑       ", key), Span::styled("move up", desc)]),
        Line::from(vec![Span::styled("  n           ", key), Span::styled("next hunk", desc)]),
        Line::from(vec![Span::styled("  p           ", key), Span::styled("prev hunk", desc)]),
        Line::from(vec![Span::styled("  Tab         ", key), Span::styled("switch panel", desc)]),
        Line::from(""),
        Line::from(Span::styled("Staging", head)),
        Line::from(vec![Span::styled("  s           ", key), Span::styled("stage file / hunk", desc)]),
        Line::from(vec![Span::styled("  u           ", key), Span::styled("unstage file / hunk", desc)]),
        Line::from(vec![Span::styled("  d           ", key), Span::styled("discard hunk", desc)]),
        Line::from(""),
        Line::from(Span::styled("Conflict Resolution", head)),
        Line::from(vec![Span::styled("  o           ", key), Span::styled("accept ours (HEAD)", desc)]),
        Line::from(vec![Span::styled("  i           ", key), Span::styled("accept incoming", desc)]),
        Line::from(vec![Span::styled("  b           ", key), Span::styled("accept both", desc)]),
        Line::from(""),
        Line::from(Span::styled("Commit", head)),
        Line::from(vec![Span::styled("  c           ", key), Span::styled("start commit", desc)]),
        Line::from(vec![Span::styled("  Enter       ", key), Span::styled("confirm", desc)]),
        Line::from(vec![Span::styled("  Esc         ", key), Span::styled("cancel", desc)]),
        Line::from(""),
        Line::from(Span::styled("Remote", head)),
        Line::from(vec![Span::styled("  P           ", key), Span::styled("push", desc)]),
        Line::from(vec![Span::styled("  L           ", key), Span::styled("pull", desc)]),
        Line::from(""),
        Line::from(Span::styled("Interface", head)),
        Line::from(vec![Span::styled("  t           ", key), Span::styled("theme picker", desc)]),
        Line::from(vec![Span::styled("  ? / Esc     ", key), Span::styled("close this help", desc)]),
        Line::from(vec![Span::styled("  q           ", key), Span::styled("quit", desc)]),
    ];

    let inner_height = area.height.saturating_sub(2);
    let max_scroll = HELP_CONTENT_LINES.saturating_sub(inner_height);
    let scroll = app.help_scroll.min(max_scroll);

    let paragraph = Paragraph::new(Text::from(lines))
        .wrap(Wrap { trim: false })
        .scroll((scroll, 0))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme.base0d))
                .title(Span::styled(
                    " Help ",
                    Style::default().fg(theme.base0d).add_modifier(Modifier::BOLD),
                ))
                .title_bottom(Span::styled(
                    " j/k to scroll ",
                    Style::default().fg(theme.base03),
                ))
                .style(Style::default().bg(app.bg_panel())),
        );

    f.render_widget(Clear, area);
    f.render_widget(paragraph, area);
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
