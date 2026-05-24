use std::path::Path;

use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Paragraph},
    Frame,
};

use crate::app::{App, Mode};

pub fn render(f: &mut Frame, app: &App, area: Rect) {
    let theme = app.current_theme();

    let (title_label, content, hint, border_color) = match app.mode {
        Mode::CommitTitle => (
            " Commit Title ",
            app.commit_title.as_str(),
            "Enter: next  Esc: cancel",
            theme.base0e,
        ),
        Mode::CommitBody => (
            " Commit Body (optional) ",
            app.commit_body.as_str(),
            "Enter: commit  Esc: back",
            theme.base0e,
        ),
        _ => {
            let hint = if app.is_file_conflicted() && app.focus == crate::app::Focus::DiffView {
                "j/k: nav  o: accept ours  i: accept incoming  b: accept both"
            } else {
                match app.focus {
                    crate::app::Focus::FileList => "s: stage  u: unstage  c: commit  ? help  q: quit",
                    crate::app::Focus::DiffView => "j/k: nav  s: stage  u: unstage  d: discard  ? help",
                }
            };
            (" Commit ", "", hint, theme.base03)
        }
    };

    let display = if matches!(app.mode, Mode::CommitTitle | Mode::CommitBody) {
        Line::from(vec![
            Span::styled(content, Style::default().fg(theme.base05)),
            Span::styled("█", Style::default().fg(theme.base0e)),
        ])
    } else if !app.commit_title.is_empty() {
        Line::from(vec![Span::styled(
            format!("  {}", app.commit_title),
            Style::default().fg(theme.base04),
        )])
    } else {
        Line::from(vec![Span::styled(
            hint,
            Style::default().fg(theme.base03),
        )])
    };

    let title = Span::styled(
        title_label,
        Style::default()
            .fg(theme.base0e)
            .add_modifier(Modifier::BOLD),
    );

    let workdir_label = app
        .repo
        .workdir()
        .and_then(|p| tilde_path(p))
        .unwrap_or_default();
    let workdir_span = Span::styled(
        format!(" {} ", workdir_label),
        Style::default().fg(theme.base04),
    );

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(border_color))
        .title(title)
        .title(Line::from(workdir_span).right_aligned())
        .style(Style::default().bg(app.bg_panel()));

    let paragraph = Paragraph::new(display).block(block);
    f.render_widget(paragraph, area);
}

fn tilde_path(path: &Path) -> Option<String> {
    let home = dirs::home_dir()?;
    if let Ok(rel) = path.strip_prefix(&home) {
        Some(format!("~/{}", rel.display()))
    } else {
        Some(path.display().to_string())
    }
}
