use git2::Repository;

use crate::config::Preferences;
use crate::error::AppError;
use crate::events::{next_event, AppEvent};
use crate::git::repo::{diff_for_file, list_changed_files, staged_diff_for_file, ChangedFile, Hunk};
use crate::git::{commit::create_commit, remote, stage};
use crate::git::repo::FileStatus;
use crate::theme::{all_themes, load_theme_by_name, seed_themes, NamedTheme};

#[derive(Debug, Clone, PartialEq)]
pub enum Focus {
    FileList,
    DiffView,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Mode {
    Normal,
    CommitTitle,
    CommitBody,
    ThemePicker,
    Quitting,
}

pub struct App {
    pub repo: Repository,
    pub files: Vec<ChangedFile>,
    pub file_cursor: usize,
    /// Hunks already in the index (staged)
    pub staged_hunks: Vec<Hunk>,
    /// Hunks only in the workdir (not yet staged)
    pub unstaged_hunks: Vec<Hunk>,
    /// Index into [staged_hunks..., unstaged_hunks...] — staged come first
    pub hunk_cursor: usize,
    pub focus: Focus,
    pub mode: Mode,
    pub commit_title: String,
    pub commit_body: String,
    pub status_msg: Option<String>,
    pub themes: Vec<NamedTheme>,
    pub theme_idx: usize,
    pub theme_picker_cursor: usize,
    pub transparent: bool,
    config_dir: std::path::PathBuf,
}

impl App {
    pub fn new(
        repo: Repository,
        config_dir: std::path::PathBuf,
        initial_theme: Option<&str>,
    ) -> Result<Self, AppError> {
        seed_themes(&config_dir)?;
        let themes = all_themes(&config_dir);
        let prefs = Preferences::load(&config_dir);

        let theme_name = initial_theme.or(prefs.theme.as_deref());
        let theme_idx = theme_name
            .and_then(|name| load_theme_by_name(&themes, name))
            .and_then(|nt| themes.iter().position(|t| t.name == nt.name))
            .unwrap_or(0);

        let files = list_changed_files(&repo)?;
        let (staged_hunks, unstaged_hunks) = load_hunks_for(&repo, files.first());

        Ok(Self {
            repo,
            files,
            file_cursor: 0,
            staged_hunks,
            unstaged_hunks,
            hunk_cursor: 0,
            focus: Focus::FileList,
            mode: Mode::Normal,
            commit_title: String::new(),
            commit_body: String::new(),
            status_msg: None,
            themes,
            theme_idx,
            theme_picker_cursor: theme_idx,
            transparent: prefs.transparent,
            config_dir,
        })
    }

    pub fn current_theme(&self) -> &crate::theme::Theme {
        &self.themes[self.theme_idx].theme
    }

    /// Main content background — transparent if the user opted in.
    pub fn bg_main(&self) -> ratatui::style::Color {
        if self.transparent {
            ratatui::style::Color::Reset
        } else {
            self.current_theme().base00
        }
    }

    /// Panel / bar background (one shade lighter than main).
    pub fn bg_panel(&self) -> ratatui::style::Color {
        if self.transparent {
            ratatui::style::Color::Reset
        } else {
            self.current_theme().base01
        }
    }

    pub fn total_hunks(&self) -> usize {
        self.staged_hunks.len() + self.unstaged_hunks.len()
    }

    /// Returns the selected hunk and whether it is staged.
    pub fn selected_hunk(&self) -> Option<(&Hunk, bool)> {
        let n = self.staged_hunks.len();
        if self.hunk_cursor < n {
            self.staged_hunks.get(self.hunk_cursor).map(|h| (h, true))
        } else {
            self.unstaged_hunks
                .get(self.hunk_cursor - n)
                .map(|h| (h, false))
        }
    }

    // ── file list navigation ──────────────────────────────────────────────

    pub fn move_file_up(&mut self) {
        if self.file_cursor > 0 {
            self.file_cursor -= 1;
            self.reload_hunks();
        }
    }

    pub fn move_file_down(&mut self) {
        if self.file_cursor + 1 < self.files.len() {
            self.file_cursor += 1;
            self.reload_hunks();
        }
    }

    // ── hunk navigation ───────────────────────────────────────────────────

    pub fn move_hunk_up(&mut self) {
        if self.hunk_cursor > 0 {
            self.hunk_cursor -= 1;
        }
    }

    pub fn move_hunk_down(&mut self) {
        if self.hunk_cursor + 1 < self.total_hunks() {
            self.hunk_cursor += 1;
        }
    }

    // ── staging ───────────────────────────────────────────────────────────

    pub fn stage_current(&mut self) -> Result<(), AppError> {
        let Some(file) = self.files.get(self.file_cursor) else {
            return Ok(());
        };
        let path = file.path.clone();

        match self.focus {
            Focus::FileList => {
                stage::stage_file(&self.repo, &path)?;
                self.status_msg = Some(format!("Staged {path}"));
            }
            Focus::DiffView => match self.selected_hunk() {
                Some((hunk, false)) => {
                    let hunk = hunk.clone();
                    stage::stage_hunk(&self.repo, &path, &hunk)?;
                    self.status_msg = Some("Hunk staged".into());
                }
                Some((_, true)) => {
                    self.status_msg = Some("Hunk is already staged".into());
                    return Ok(());
                }
                None => return Ok(()),
            },
        }
        self.refresh()?;
        Ok(())
    }

    pub fn unstage_current(&mut self) -> Result<(), AppError> {
        let Some(file) = self.files.get(self.file_cursor) else {
            return Ok(());
        };
        let path = file.path.clone();

        match self.focus {
            Focus::FileList => {
                stage::unstage_file(&self.repo, &path)?;
                self.status_msg = Some(format!("Unstaged {path}"));
            }
            Focus::DiffView => match self.selected_hunk() {
                Some((hunk, true)) => {
                    // pass the staged hunk so build_patch reverses the right diff
                    let hunk = hunk.clone();
                    stage::unstage_hunk(&self.repo, &path, &hunk)?;
                    self.status_msg = Some("Hunk unstaged".into());
                }
                Some((_, false)) => {
                    self.status_msg = Some("Hunk is not staged".into());
                    return Ok(());
                }
                None => return Ok(()),
            },
        }
        self.refresh()?;
        Ok(())
    }

    pub fn delete_untracked_current(&mut self) -> Result<(), AppError> {
        let Some(file) = self.files.get(self.file_cursor) else {
            return Ok(());
        };
        if file.status != FileStatus::Untracked {
            self.status_msg = Some("Not an untracked file".into());
            return Ok(());
        }
        let path = file.path.clone();
        stage::delete_untracked_file(&self.repo, &path)?;
        self.status_msg = Some(format!("Deleted {path}"));
        self.refresh()?;
        Ok(())
    }

    pub fn discard_current(&mut self) -> Result<(), AppError> {
        let Some(file) = self.files.get(self.file_cursor) else {
            return Ok(());
        };
        let path = file.path.clone();
        match self.selected_hunk() {
            Some((hunk, false)) => {
                let hunk = hunk.clone();
                stage::discard_hunk(&self.repo, &path, &hunk)?;
                self.status_msg = Some("Hunk discarded".into());
                self.refresh()?;
            }
            Some((_, true)) => {
                self.status_msg = Some("Unstage the hunk first to discard it".into());
            }
            None => {}
        }
        Ok(())
    }

    // ── commit ────────────────────────────────────────────────────────────

    pub fn do_commit(&mut self) -> Result<(), AppError> {
        if self.commit_title.trim().is_empty() {
            self.status_msg = Some("Commit title cannot be empty".into());
            return Ok(());
        }
        let oid = create_commit(&self.repo, &self.commit_title, &self.commit_body)?;
        self.commit_title.clear();
        self.commit_body.clear();
        self.mode = Mode::Normal;
        self.status_msg = Some(format!("Committed {}", &oid.to_string()[..7]));
        self.refresh()?;
        Ok(())
    }

    // ── remote ────────────────────────────────────────────────────────────

    pub fn do_push(&mut self) -> Result<(), AppError> {
        self.status_msg = Some("Pushing…".into());
        let result = remote::push(&self.repo)?;
        self.status_msg = Some(if result.success {
            format!("Push: {}", result.output)
        } else {
            format!("Push failed: {}", result.output)
        });
        Ok(())
    }

    pub fn do_pull(&mut self) -> Result<(), AppError> {
        self.status_msg = Some("Pulling…".into());
        let result = remote::pull(&self.repo)?;
        if result.success {
            self.status_msg = Some(format!("Pull: {}", result.output));
            self.refresh()?;
        } else {
            self.status_msg = Some(format!("Pull failed: {}", result.output));
        }
        Ok(())
    }

    // ── theme picker ──────────────────────────────────────────────────────

    pub fn picker_up(&mut self) {
        if self.theme_picker_cursor > 0 {
            self.theme_picker_cursor -= 1;
        }
    }

    pub fn picker_down(&mut self) {
        if self.theme_picker_cursor + 1 < self.themes.len() {
            self.theme_picker_cursor += 1;
        }
    }

    pub fn apply_theme(&mut self) -> Result<(), AppError> {
        self.theme_idx = self.theme_picker_cursor;
        self.mode = Mode::Normal;
        let name = self.themes[self.theme_idx].name.clone();
        let prefs = Preferences { theme: Some(name.clone()), transparent: self.transparent };
        prefs.save(&self.config_dir)?;
        self.status_msg = Some(format!("Theme: {name}"));
        Ok(())
    }

    // ── internal helpers ──────────────────────────────────────────────────

    fn reload_hunks(&mut self) {
        self.hunk_cursor = 0;
        let file = self.files.get(self.file_cursor);
        let (staged, unstaged) = load_hunks_for(&self.repo, file);
        self.staged_hunks = staged;
        self.unstaged_hunks = unstaged;
    }

    fn refresh(&mut self) -> Result<(), AppError> {
        self.files = list_changed_files(&self.repo)?;
        if self.file_cursor >= self.files.len() {
            self.file_cursor = self.files.len().saturating_sub(1);
        }
        let prev_cursor = self.hunk_cursor;
        self.reload_hunks();
        // keep cursor in bounds after staging/unstaging shifts the list
        let total = self.total_hunks();
        if total > 0 && self.hunk_cursor >= total {
            self.hunk_cursor = total - 1;
        }
        // if cursor was pointing at a staged hunk and we just staged something,
        // try to land on the same logical position
        let _ = prev_cursor; // currently unused; could use for smarter repositioning
        Ok(())
    }

    // ── main event loop ───────────────────────────────────────────────────

    pub fn run(
        &mut self,
        terminal: &mut ratatui::Terminal<ratatui::backend::CrosstermBackend<std::io::Stdout>>,
    ) -> Result<(), AppError> {
        loop {
            terminal.draw(|f| crate::ui::draw(f, self))?;

            let text_input = matches!(self.mode, Mode::CommitTitle | Mode::CommitBody);
            let Some(event) = next_event(text_input)? else {
                continue;
            };

            self.status_msg = None;

            match self.mode.clone() {
                Mode::Normal => self.handle_normal(event)?,
                Mode::CommitTitle => self.handle_commit_title(event)?,
                Mode::CommitBody => self.handle_commit_body(event)?,
                Mode::ThemePicker => self.handle_theme_picker(event)?,
                Mode::Quitting => break,
            }

            if self.mode == Mode::Quitting {
                break;
            }
        }
        Ok(())
    }

    fn handle_normal(&mut self, event: AppEvent) -> Result<(), AppError> {
        match event {
            AppEvent::Quit => self.mode = Mode::Quitting,
            AppEvent::ToggleFocus => {
                self.focus = match self.focus {
                    Focus::FileList => Focus::DiffView,
                    Focus::DiffView => Focus::FileList,
                };
            }
            AppEvent::MoveUp => match self.focus {
                Focus::FileList => self.move_file_up(),
                Focus::DiffView => self.move_hunk_up(),
            },
            AppEvent::MoveDown => match self.focus {
                Focus::FileList => self.move_file_down(),
                Focus::DiffView => self.move_hunk_down(),
            },
            AppEvent::NextHunk => self.move_hunk_down(),
            AppEvent::PrevHunk => self.move_hunk_up(),
            AppEvent::Stage => self.stage_current()?,
            AppEvent::Unstage => self.unstage_current()?,
            AppEvent::Discard => self.discard_current()?,
            AppEvent::DeleteUntracked => self.delete_untracked_current()?,
            AppEvent::Push => self.do_push()?,
            AppEvent::Pull => self.do_pull()?,
            AppEvent::Commit => self.mode = Mode::CommitTitle,
            AppEvent::OpenThemePicker => {
                self.theme_picker_cursor = self.theme_idx;
                self.mode = Mode::ThemePicker;
            }
            _ => {}
        }
        Ok(())
    }

    fn handle_commit_title(&mut self, event: AppEvent) -> Result<(), AppError> {
        match event {
            AppEvent::Confirm => self.mode = Mode::CommitBody,
            AppEvent::Cancel => {
                self.commit_title.clear();
                self.mode = Mode::Normal;
            }
            AppEvent::Char(ch) => self.commit_title.push(ch),
            AppEvent::Backspace => {
                self.commit_title.pop();
            }
            _ => {}
        }
        Ok(())
    }

    fn handle_commit_body(&mut self, event: AppEvent) -> Result<(), AppError> {
        match event {
            AppEvent::Confirm => self.do_commit()?,
            AppEvent::Cancel => self.mode = Mode::Normal,
            AppEvent::Char(ch) => self.commit_body.push(ch),
            AppEvent::Backspace => {
                self.commit_body.pop();
            }
            _ => {}
        }
        Ok(())
    }

    fn handle_theme_picker(&mut self, event: AppEvent) -> Result<(), AppError> {
        match event {
            AppEvent::MoveUp | AppEvent::PrevHunk => self.picker_up(),
            AppEvent::MoveDown | AppEvent::NextHunk => self.picker_down(),
            AppEvent::Confirm => self.apply_theme()?,
            AppEvent::Cancel | AppEvent::Quit => self.mode = Mode::Normal,
            _ => {}
        }
        Ok(())
    }
}

fn load_hunks_for(repo: &Repository, file: Option<&ChangedFile>) -> (Vec<Hunk>, Vec<Hunk>) {
    let Some(f) = file else {
        return (vec![], vec![]);
    };
    let staged = staged_diff_for_file(repo, &f.path).unwrap_or_default();
    let unstaged = diff_for_file(repo, &f.path).unwrap_or_default();
    (staged, unstaged)
}
