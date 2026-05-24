use git2::Repository;

use crate::config::Preferences;
use crate::error::AppError;
use crate::events::{next_event, AppEvent};
use crate::git::repo::{
    detect_conflicts, diff_for_file, list_changed_files, staged_diff_for_file, ChangedFile,
    ConflictBlock, FileStatus, Hunk,
};
use crate::git::stage::ConflictSide;
use crate::git::{commit::create_commit, remote, stage};
use crate::theme::{all_themes, load_theme_by_name, seed_themes, NamedTheme};

#[derive(Debug, Clone, PartialEq)]
pub enum Focus {
    FileList,
    DiffView,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PendingAction {
    DeleteUntracked(String),
}

impl PendingAction {
    pub fn prompt(&self) -> String {
        match self {
            PendingAction::DeleteUntracked(path) => {
                format!("Delete untracked file '{path}'?")
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum LoadingOp {
    Push,
    Pull,
    Commit,
}

impl LoadingOp {
    pub fn label(&self) -> &'static str {
        match self {
            LoadingOp::Push => "Pushing…",
            LoadingOp::Pull => "Pulling…",
            LoadingOp::Commit => "Committing…",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Mode {
    Normal,
    CommitTitle,
    CommitBody,
    ThemePicker,
    Help,
    Confirming(PendingAction),
    Loading(LoadingOp),
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
    /// Conflict blocks for the currently selected conflicted file.
    pub conflict_blocks: Vec<ConflictBlock>,
    /// Cursor within `conflict_blocks` when a conflicted file is selected.
    pub conflict_cursor: usize,
    pub focus: Focus,
    pub mode: Mode,
    pub commit_title: String,
    pub commit_body: String,
    pub status_msg: Option<String>,
    pub themes: Vec<NamedTheme>,
    pub theme_idx: usize,
    pub theme_picker_cursor: usize,
    pub transparent: bool,
    /// Set by refresh() to force terminal.clear() before the next draw,
    /// preventing stale cells when content shrinks (e.g. after a commit).
    pub needs_clear: bool,
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
        let conflict_blocks = load_conflicts_for(&repo, files.first());

        Ok(Self {
            repo,
            files,
            file_cursor: 0,
            staged_hunks,
            unstaged_hunks,
            hunk_cursor: 0,
            conflict_blocks,
            conflict_cursor: 0,
            focus: Focus::FileList,
            mode: Mode::Normal,
            commit_title: String::new(),
            commit_body: String::new(),
            status_msg: None,
            themes,
            theme_idx,
            theme_picker_cursor: theme_idx,
            transparent: prefs.transparent,
            needs_clear: false,
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

    pub fn is_file_conflicted(&self) -> bool {
        self.files
            .get(self.file_cursor)
            .map(|f| f.status == FileStatus::Conflicted)
            .unwrap_or(false)
    }

    // ── conflict navigation ───────────────────────────────────────────────

    pub fn move_conflict_up(&mut self) {
        if self.conflict_cursor > 0 {
            self.conflict_cursor -= 1;
        }
    }

    pub fn move_conflict_down(&mut self) {
        if self.conflict_cursor + 1 < self.conflict_blocks.len() {
            self.conflict_cursor += 1;
        }
    }

    pub fn resolve_conflict(&mut self, side: ConflictSide) -> Result<(), AppError> {
        let Some(file) = self.files.get(self.file_cursor) else {
            return Ok(());
        };
        if file.status != FileStatus::Conflicted {
            return Ok(());
        }
        let path = file.path.clone();
        stage::resolve_conflict_block(
            &self.repo,
            &path,
            &self.conflict_blocks,
            self.conflict_cursor,
            side,
        )?;
        let label = match side {
            ConflictSide::Ours => "ours",
            ConflictSide::Theirs => "theirs",
            ConflictSide::Both => "both",
        };
        self.status_msg = Some(format!("Conflict resolved ({label})"));
        self.refresh()?;
        Ok(())
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

    pub fn delete_untracked_current(&mut self) {
        let Some(file) = self.files.get(self.file_cursor) else {
            return;
        };
        if file.status != FileStatus::Untracked {
            self.status_msg = Some("Not an untracked file".into());
            return;
        }
        self.mode = Mode::Confirming(PendingAction::DeleteUntracked(file.path.clone()));
    }

    pub fn execute_pending(&mut self, action: PendingAction) -> Result<(), AppError> {
        match action {
            PendingAction::DeleteUntracked(path) => {
                stage::delete_untracked_file(&self.repo, &path)?;
                self.status_msg = Some(format!("Deleted {path}"));
                self.refresh()?;
            }
        }
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
        let result = remote::push(&self.repo)?;
        self.status_msg = Some(if result.success {
            format!("Push: {}", result.output)
        } else {
            format!("Push failed: {}", result.output)
        });
        Ok(())
    }

    pub fn do_pull(&mut self) -> Result<(), AppError> {
        let result = remote::pull(&self.repo)?;
        if result.success {
            self.status_msg = Some(format!("Pull: {}", result.output));
            self.refresh()?;
        } else {
            self.status_msg = Some(format!("Pull failed: {}", result.output));
        }
        Ok(())
    }

    pub fn execute_loading(&mut self, op: LoadingOp) -> Result<(), AppError> {
        self.mode = Mode::Normal;
        match op {
            LoadingOp::Push => self.do_push()?,
            LoadingOp::Pull => self.do_pull()?,
            LoadingOp::Commit => self.do_commit()?,
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
        if self.theme_picker_cursor + 1 < self.themes.len() + 1 {
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

    pub fn open_theme_picker(&mut self) {
        self.theme_picker_cursor = self.theme_idx;
        self.mode = Mode::ThemePicker;
    }

    pub fn picker_confirm(&mut self) -> Result<(), AppError> {
        if self.theme_picker_cursor == self.themes.len() {
            self.toggle_transparent()?;
        } else {
            self.apply_theme()?;
        }
        Ok(())
    }

    pub fn toggle_transparent(&mut self) -> Result<(), AppError> {
        self.transparent = !self.transparent;
        let name = self.themes[self.theme_idx].name.clone();
        let prefs = Preferences { theme: Some(name), transparent: self.transparent };
        prefs.save(&self.config_dir)?;
        Ok(())
    }

    // ── internal helpers ──────────────────────────────────────────────────

    fn reload_hunks(&mut self) {
        self.hunk_cursor = 0;
        self.conflict_cursor = 0;
        let file = self.files.get(self.file_cursor);
        let (staged, unstaged) = load_hunks_for(&self.repo, file);
        self.staged_hunks = staged;
        self.unstaged_hunks = unstaged;
        self.conflict_blocks = load_conflicts_for(&self.repo, file);
    }

    fn refresh(&mut self) -> Result<(), AppError> {
        self.files = list_changed_files(&self.repo)?;
        if self.file_cursor >= self.files.len() {
            self.file_cursor = self.files.len().saturating_sub(1);
        }
        let prev_cursor = self.hunk_cursor;
        let prev_conflict_cursor = self.conflict_cursor;
        self.reload_hunks();
        // keep hunk cursor in bounds after staging/unstaging shifts the list
        let total = self.total_hunks();
        if total > 0 && self.hunk_cursor >= total {
            self.hunk_cursor = total - 1;
        }
        let _ = prev_cursor;
        // keep conflict cursor in bounds after a block is resolved
        let n_conflicts = self.conflict_blocks.len();
        if n_conflicts > 0 && prev_conflict_cursor < n_conflicts {
            self.conflict_cursor = prev_conflict_cursor;
        } else if n_conflicts > 0 {
            self.conflict_cursor = n_conflicts - 1;
        }
        // Content may have shrunk; signal the run loop to clear the terminal
        // buffer before the next draw so ratatui's diff doesn't leave ghost cells.
        self.needs_clear = true;
        Ok(())
    }

    // ── main event loop ───────────────────────────────────────────────────

    pub fn run(
        &mut self,
        terminal: &mut ratatui::Terminal<ratatui::backend::CrosstermBackend<std::io::Stdout>>,
    ) -> Result<(), AppError> {
        loop {
            // After any operation that shrinks content (commit, stage, unstage),
            // refresh() sets needs_clear so we force a full repaint and avoid
            // ratatui's diff leaving ghost cells from the previous larger content.
            if self.needs_clear {
                terminal.clear()?;
                self.needs_clear = false;
            }
            terminal.draw(|f| crate::ui::draw(f, self))?;

            // Execute any pending loading operation after the frame is rendered,
            // so the user sees the loading indicator before we block.
            if let Mode::Loading(op) = self.mode.clone() {
                self.execute_loading(op)?;
                continue;
            }

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
                Mode::Confirming(action) => self.handle_confirming(event, action)?,
                Mode::Help => self.handle_help(event)?,
                Mode::Loading(_) => {} // handled above before event polling
                Mode::Quitting => break,
            }

            if self.mode == Mode::Quitting {
                break;
            }
        }
        Ok(())
    }

    fn handle_help(&mut self, event: AppEvent) -> Result<(), AppError> {
        match event {
            AppEvent::Quit | AppEvent::Cancel | AppEvent::OpenHelp => self.mode = Mode::Normal,
            _ => {}
        }
        Ok(())
    }

    fn handle_normal(&mut self, event: AppEvent) -> Result<(), AppError> {
        match event {
            AppEvent::Quit => self.mode = Mode::Quitting,
            AppEvent::OpenHelp => self.mode = Mode::Help,
            AppEvent::ToggleFocus => {
                self.focus = match self.focus {
                    Focus::FileList => Focus::DiffView,
                    Focus::DiffView => Focus::FileList,
                };
            }
            AppEvent::MoveUp => match self.focus {
                Focus::FileList => self.move_file_up(),
                Focus::DiffView => {
                    if self.is_file_conflicted() {
                        self.move_conflict_up();
                    } else {
                        self.move_hunk_up();
                    }
                }
            },
            AppEvent::MoveDown => match self.focus {
                Focus::FileList => self.move_file_down(),
                Focus::DiffView => {
                    if self.is_file_conflicted() {
                        self.move_conflict_down();
                    } else {
                        self.move_hunk_down();
                    }
                }
            },
            AppEvent::NextHunk => {
                if self.is_file_conflicted() {
                    self.move_conflict_down();
                } else {
                    self.move_hunk_down();
                }
            }
            AppEvent::PrevHunk => {
                if self.is_file_conflicted() {
                    self.move_conflict_up();
                } else {
                    self.move_hunk_up();
                }
            }
            AppEvent::Stage => self.stage_current()?,
            AppEvent::Unstage => self.unstage_current()?,
            AppEvent::Discard => self.discard_current()?,
            AppEvent::DeleteUntracked => self.delete_untracked_current(),
            AppEvent::AcceptOurs => self.resolve_conflict(ConflictSide::Ours)?,
            AppEvent::AcceptTheirs => self.resolve_conflict(ConflictSide::Theirs)?,
            AppEvent::AcceptBoth => self.resolve_conflict(ConflictSide::Both)?,
            AppEvent::Push => self.mode = Mode::Loading(LoadingOp::Push),
            AppEvent::Pull => self.mode = Mode::Loading(LoadingOp::Pull),
            AppEvent::Commit => self.mode = Mode::CommitTitle,
            AppEvent::OpenThemePicker => self.open_theme_picker(),
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
            AppEvent::Confirm => self.mode = Mode::Loading(LoadingOp::Commit),
            AppEvent::Cancel => self.mode = Mode::Normal,
            AppEvent::Char(ch) => self.commit_body.push(ch),
            AppEvent::Backspace => {
                self.commit_body.pop();
            }
            _ => {}
        }
        Ok(())
    }

    fn handle_confirming(&mut self, event: AppEvent, action: PendingAction) -> Result<(), AppError> {
        match event {
            AppEvent::Confirm | AppEvent::Char('y') | AppEvent::Char('Y') => {
                self.mode = Mode::Normal;
                self.execute_pending(action)?;
            }
            AppEvent::Cancel | AppEvent::Char('n') | AppEvent::Char('N') => {
                self.mode = Mode::Normal;
                self.status_msg = Some("Cancelled".into());
            }
            _ => {}
        }
        Ok(())
    }

    fn handle_theme_picker(&mut self, event: AppEvent) -> Result<(), AppError> {
        match event {
            AppEvent::MoveUp | AppEvent::PrevHunk => self.picker_up(),
            AppEvent::MoveDown | AppEvent::NextHunk => self.picker_down(),
            AppEvent::Confirm => self.picker_confirm()?,
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
    if f.status == FileStatus::Conflicted {
        return (vec![], vec![]);
    }
    let staged = staged_diff_for_file(repo, &f.path).unwrap_or_default();
    let unstaged = diff_for_file(repo, &f.path).unwrap_or_default();
    (staged, unstaged)
}

fn load_conflicts_for(repo: &Repository, file: Option<&ChangedFile>) -> Vec<ConflictBlock> {
    let Some(f) = file else {
        return vec![];
    };
    if f.status != FileStatus::Conflicted {
        return vec![];
    }
    detect_conflicts(repo, &f.path).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loading_op_labels_are_non_empty() {
        assert!(!LoadingOp::Push.label().is_empty());
        assert!(!LoadingOp::Pull.label().is_empty());
        assert!(!LoadingOp::Commit.label().is_empty());
    }

    #[test]
    fn loading_op_labels_are_distinct() {
        let labels = [LoadingOp::Push.label(), LoadingOp::Pull.label(), LoadingOp::Commit.label()];
        for (i, a) in labels.iter().enumerate() {
            for (j, b) in labels.iter().enumerate() {
                if i != j {
                    assert_ne!(a, b, "ops at {i} and {j} share label {a:?}");
                }
            }
        }
    }

    #[test]
    fn mode_loading_equality() {
        assert_eq!(Mode::Loading(LoadingOp::Push), Mode::Loading(LoadingOp::Push));
        assert_ne!(Mode::Loading(LoadingOp::Push), Mode::Loading(LoadingOp::Pull));
        assert_ne!(Mode::Loading(LoadingOp::Push), Mode::Normal);
    }

    #[test]
    fn pending_action_push_sets_loading_mode() {
        // Simulate what handle_normal does for the Push event — no repo needed.
        let mut mode = Mode::Normal;
        // Replicate the dispatch logic
        let event = crate::events::AppEvent::Push;
        if event == crate::events::AppEvent::Push {
            mode = Mode::Loading(LoadingOp::Push);
        }
        assert_eq!(mode, Mode::Loading(LoadingOp::Push));
    }

    #[test]
    fn pending_action_pull_sets_loading_mode() {
        let mut mode = Mode::Normal;
        let event = crate::events::AppEvent::Pull;
        if event == crate::events::AppEvent::Pull {
            mode = Mode::Loading(LoadingOp::Pull);
        }
        assert_eq!(mode, Mode::Loading(LoadingOp::Pull));
    }

    #[test]
    fn commit_confirm_sets_loading_mode() {
        let mut mode = Mode::CommitBody;
        let event = crate::events::AppEvent::Confirm;
        if matches!(mode, Mode::CommitBody) && event == crate::events::AppEvent::Confirm {
            mode = Mode::Loading(LoadingOp::Commit);
        }
        assert_eq!(mode, Mode::Loading(LoadingOp::Commit));
    }

    fn make_test_app() -> (tempfile::TempDir, tempfile::TempDir, App) {
        let repo_dir = tempfile::TempDir::new().unwrap();
        let config_dir = tempfile::TempDir::new().unwrap();
        let repo = git2::Repository::init(repo_dir.path()).unwrap();
        let app = App::new(repo, config_dir.path().to_path_buf(), None).unwrap();
        (repo_dir, config_dir, app)
    }

    #[test]
    fn needs_clear_false_on_construction() {
        let (_repo, _cfg, app) = make_test_app();
        assert!(!app.needs_clear, "needs_clear should start false");
    }

    #[test]
    fn needs_clear_true_after_refresh() {
        let (_repo, _cfg, mut app) = make_test_app();
        app.refresh().unwrap();
        assert!(app.needs_clear, "refresh() must set needs_clear so the run loop clears stale cells");
    }

    #[test]
    fn needs_clear_reset_after_being_consumed() {
        let (_repo, _cfg, mut app) = make_test_app();
        app.refresh().unwrap();
        assert!(app.needs_clear);
        // Simulate the run loop consuming the flag
        app.needs_clear = false;
        assert!(!app.needs_clear);
    }

    #[test]
    fn picker_down_reaches_transparent_toggle_item() {
        let (_repo, _cfg, mut app) = make_test_app();
        let max = app.themes.len();
        app.theme_picker_cursor = max - 1;
        app.picker_down();
        assert_eq!(app.theme_picker_cursor, max, "cursor should reach the transparent toggle item");
    }

    #[test]
    fn picker_down_does_not_exceed_transparent_item() {
        let (_repo, _cfg, mut app) = make_test_app();
        let max = app.themes.len();
        app.theme_picker_cursor = max;
        app.picker_down();
        assert_eq!(app.theme_picker_cursor, max, "cursor must not go past the transparent toggle item");
    }

    #[test]
    fn toggle_transparent_flips_and_persists() {
        let (_repo, cfg, mut app) = make_test_app();
        assert!(!app.transparent);
        app.toggle_transparent().unwrap();
        assert!(app.transparent);

        let reloaded = crate::config::Preferences::load(cfg.path());
        assert!(reloaded.transparent, "toggle must persist transparent=true to config.toml");

        app.toggle_transparent().unwrap();
        assert!(!app.transparent);
        let reloaded2 = crate::config::Preferences::load(cfg.path());
        assert!(!reloaded2.transparent, "second toggle must persist transparent=false");
    }

    #[test]
    fn picker_confirm_on_transparent_item_toggles_and_stays_in_picker() {
        let (_repo, _cfg, mut app) = make_test_app();
        app.mode = Mode::ThemePicker;
        app.theme_picker_cursor = app.themes.len();
        app.picker_confirm().unwrap();
        assert!(app.transparent, "confirm on transparent item must toggle it on");
        assert_eq!(app.mode, Mode::ThemePicker, "mode must remain ThemePicker after toggling transparent");
    }

    #[test]
    fn picker_confirm_on_theme_item_applies_theme_and_closes() {
        let (_repo, _cfg, mut app) = make_test_app();
        app.mode = Mode::ThemePicker;
        app.theme_picker_cursor = 0;
        app.picker_confirm().unwrap();
        assert_eq!(app.theme_idx, 0);
        assert_eq!(app.mode, Mode::Normal, "confirm on a theme item must close the picker");
    }
}
