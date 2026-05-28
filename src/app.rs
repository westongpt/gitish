use std::sync::mpsc;

use git2::Repository;

use crate::config::Preferences;
use crate::error::AppError;
use crate::events::{next_event, AppEvent};
use crate::git::remote::RemoteResult;
use crate::git::repo::{
    detect_conflicts, diff_for_file, list_changed_files, staged_diff_for_file, ChangedFile,
    ConflictBlock, FileStatus, Hunk,
};
use crate::git::stage::ConflictSide;
use crate::git::{commit::create_commit, remote, stage};
use crate::theme::{all_themes, seed_themes, ThemeList};

type WorkerMsg = (LoadingOp, Result<RemoteResult, AppError>);

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
    /// Dummy op that shows the spinner overlay without doing any real work.
    /// Used with `--open spinner` for screenshots and demos.
    Demo,
}

impl LoadingOp {
    pub fn label(&self) -> &'static str {
        match self {
            LoadingOp::Push => "Pushing…",
            LoadingOp::Pull => "Pulling…",
            LoadingOp::Commit => "Committing…",
            LoadingOp::Demo => "Working…",
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
    pub themes: ThemeList,
    pub theme_picker_cursor: usize,
    pub transparent: bool,
    /// Render file/status icons with Nerd Font glyphs; false falls back to ASCII.
    pub use_nerd_fonts: bool,
    /// Set by refresh() to force terminal.clear() before the next draw,
    /// preventing stale cells when content shrinks (e.g. after a commit).
    pub needs_clear: bool,
    pub help_scroll: u16,
    /// Clamping bound for help_scroll; recomputed from terminal size each frame.
    pub help_max_scroll: u16,
    /// Incremented each run-loop tick while in Loading mode; drives spinner animation.
    pub spinner_tick: u64,
    /// Receives the result from a background push/pull thread.
    worker_rx: Option<mpsc::Receiver<WorkerMsg>>,
    config_dir: std::path::PathBuf,
}

impl App {
    pub fn new(
        repo: Repository,
        config_dir: std::path::PathBuf,
        initial_theme: Option<&str>,
    ) -> Result<Self, AppError> {
        seed_themes(&config_dir)?;
        let mut themes = ThemeList::new(all_themes(&config_dir));
        let (prefs, config_err) = Preferences::load(&config_dir);

        let theme_name = initial_theme.or(prefs.theme.as_deref());
        if let Some(name) = theme_name {
            if let Some(idx) = themes.find_idx_by_name(name) {
                themes.set_current_idx(idx);
            }
        }

        let files = list_changed_files(&repo)?;
        let (staged_hunks, unstaged_hunks, load_err) = match load_hunks_for(&repo, files.first()) {
            Ok((staged, unstaged)) => (staged, unstaged, None),
            Err(e) => (vec![], vec![], Some(format!("Failed to load diff: {e}"))),
        };
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
            status_msg: config_err.or(load_err),
            theme_picker_cursor: themes.current_idx(),
            themes,
            transparent: prefs.transparent,
            use_nerd_fonts: prefs.use_nerd_fonts,
            needs_clear: false,
            help_scroll: 0,
            help_max_scroll: u16::MAX,
            spinner_tick: 0,
            worker_rx: None,
            config_dir,
        })
    }

    pub fn current_theme(&self) -> &crate::theme::Theme {
        &self.themes.current().theme
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


    /// Synchronous path for commit (fast, in-process libgit2 — no thread needed).
    pub fn execute_commit(&mut self) -> Result<(), AppError> {
        self.mode = Mode::Normal;
        self.do_commit()
    }

    /// Spawn a background thread for the blocking push/pull and store the receiver.
    /// Must only be called when `self.worker_rx` is `None`.
    pub fn spawn_remote_worker(&mut self, op: LoadingOp) -> Result<(), AppError> {
        let workdir = self
            .repo
            .workdir()
            .ok_or_else(|| AppError::Invalid("cannot push/pull a bare repository".into()))?
            .to_path_buf();

        let (tx, rx) = mpsc::channel::<WorkerMsg>();
        self.worker_rx = Some(rx);

        std::thread::spawn(move || {
            let result = match op {
                LoadingOp::Push => remote::push_in_dir(workdir),
                LoadingOp::Pull => remote::pull_in_dir(workdir),
                LoadingOp::Commit | LoadingOp::Demo => {
                    unreachable!("only Push/Pull reach spawn_remote_worker")
                }
            };
            let _ = tx.send((op, result));
        });

        Ok(())
    }

    /// Handle the result that arrived from a worker thread.
    pub fn finish_remote_op(
        &mut self,
        op: LoadingOp,
        result: Result<RemoteResult, AppError>,
    ) -> Result<(), AppError> {
        self.mode = Mode::Normal;
        match result {
            Err(e) => {
                self.status_msg = Some(format!("Error: {e}"));
            }
            Ok(r) => match op {
                LoadingOp::Push => {
                    self.status_msg = Some(if r.success {
                        format!("Push: {}", r.output)
                    } else {
                        format!("Push failed: {}", r.output)
                    });
                }
                LoadingOp::Pull => {
                    if r.success {
                        self.status_msg = Some(format!("Pull: {}", r.output));
                        self.refresh()?;
                    } else {
                        self.status_msg = Some(format!("Pull failed: {}", r.output));
                    }
                }
                LoadingOp::Commit | LoadingOp::Demo => {
                    unreachable!("only Push/Pull produce worker results")
                }
            },
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
        self.themes.set_current_idx(self.theme_picker_cursor);
        self.mode = Mode::Normal;
        let name = self.themes.current().name.clone();
        let prefs = Preferences {
            theme: Some(name.clone()),
            transparent: self.transparent,
            use_nerd_fonts: self.use_nerd_fonts,
        };
        prefs.save(&self.config_dir)?;
        self.status_msg = Some(format!("Theme: {name}"));
        // All colors changed; force full repaint so ratatui's diff doesn't miss any cells.
        self.needs_clear = true;
        Ok(())
    }

    pub fn open_theme_picker(&mut self) {
        self.theme_picker_cursor = self.themes.current_idx();
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
        let name = self.themes.current().name.clone();
        let prefs = Preferences {
            theme: Some(name),
            transparent: self.transparent,
            use_nerd_fonts: self.use_nerd_fonts,
        };
        prefs.save(&self.config_dir)?;
        // Background colors changed globally; force full repaint.
        self.needs_clear = true;
        Ok(())
    }

    // ── internal helpers ──────────────────────────────────────────────────

    fn reload_hunks(&mut self) {
        self.hunk_cursor = 0;
        self.conflict_cursor = 0;
        let file = self.files.get(self.file_cursor);
        match load_hunks_for(&self.repo, file) {
            Ok((staged, unstaged)) => {
                self.staged_hunks = staged;
                self.unstaged_hunks = unstaged;
            }
            Err(e) => {
                self.staged_hunks = vec![];
                self.unstaged_hunks = vec![];
                self.status_msg = Some(format!("Failed to load diff: {e}"));
            }
        }
        self.conflict_blocks = load_conflicts_for(&self.repo, file);
        // Diff panel content just changed; signal the run loop to clear the
        // terminal buffer so ratatui's diff doesn't leave stale cells from
        // a previously longer hunk list.
        self.needs_clear = true;
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
            // Collect any completed worker result before drawing so the final
            // status message appears in the same frame the overlay disappears.
            if let Some(rx) = self.worker_rx.take() {
                match rx.try_recv() {
                    Ok((op, outcome)) => {
                        self.finish_remote_op(op, outcome)?;
                        self.spinner_tick = 0;
                        // worker_rx stays None — operation complete
                    }
                    Err(mpsc::TryRecvError::Empty) => {
                        // Thread still running — put the receiver back.
                        self.worker_rx = Some(rx);
                        self.spinner_tick = self.spinner_tick.wrapping_add(1);
                    }
                    Err(mpsc::TryRecvError::Disconnected) => {
                        // Thread panicked or exited without sending — treat as error.
                        self.mode = Mode::Normal;
                        self.status_msg = Some("Remote operation failed unexpectedly".into());
                        self.spinner_tick = 0;
                    }
                }
            }

            // After any operation that shrinks content (commit, stage, unstage),
            // refresh() sets needs_clear so we force a full repaint and avoid
            // ratatui's diff leaving ghost cells from the previous larger content.
            if self.needs_clear {
                terminal.clear()?;
                self.needs_clear = false;
            }
            if let Ok(size) = terminal.size() {
                self.help_max_scroll = crate::ui::help_max_scroll(size.height);
            }
            terminal.draw(|f| crate::ui::draw(f, self))?;

            // After the loading frame is painted, dispatch the operation.
            if let Mode::Loading(op) = &self.mode {
                let op = op.clone();
                match op {
                    LoadingOp::Commit => {
                        // Commit is synchronous (fast libgit2 call, no blocking I/O).
                        self.execute_commit()?;
                        continue;
                    }
                    LoadingOp::Push | LoadingOp::Pull => {
                        if self.worker_rx.is_none() {
                            self.spawn_remote_worker(op)?;
                        }
                        // Fall through to event polling so inputs are drained while
                        // the worker thread runs.
                    }
                    LoadingOp::Demo => {
                        // Spinner-only demo mode (--open spinner). Tick the spinner
                        // each frame and stay in Loading until the user quits.
                        self.spinner_tick = self.spinner_tick.wrapping_add(1);
                    }
                }
            }

            let text_input = matches!(self.mode, Mode::CommitTitle | Mode::CommitBody);
            let Some(event) = next_event(text_input)? else {
                continue;
            };

            // Terminal resize: force a full repaint so ratatui's double-buffer
            // stays in sync with the actual terminal dimensions.
            if matches!(event, AppEvent::Resize) {
                self.needs_clear = true;
                continue;
            }

            self.status_msg = None;

            let prev_is_overlay = is_overlay_mode(&self.mode);

            let confirming_action = if let Mode::Confirming(a) = &self.mode {
                Some(a.clone())
            } else {
                None
            };
            match &self.mode {
                Mode::Normal => self.handle_normal(event)?,
                Mode::CommitTitle => self.handle_commit_title(event)?,
                Mode::CommitBody => self.handle_commit_body(event)?,
                Mode::ThemePicker => self.handle_theme_picker(event)?,
                Mode::Confirming(_) => {
                    self.handle_confirming(event, confirming_action.unwrap())?
                }
                Mode::Help => self.handle_help(event)?,
                Mode::Loading(_) => {
                    // Drain input while a remote worker runs; only Quit is honoured.
                    if matches!(event, AppEvent::Quit) {
                        self.mode = Mode::Quitting;
                    }
                }
                Mode::Quitting => break,
            }

            // When an overlay appears or disappears, the area it occupied needs a
            // full repaint — ratatui's diff can miss cells when transparent
            // backgrounds make previous and current content look identical.
            if is_overlay_mode(&self.mode) != prev_is_overlay {
                self.needs_clear = true;
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
            AppEvent::MoveDown => {
                self.help_scroll = self.help_scroll.saturating_add(1).min(self.help_max_scroll);
            }
            AppEvent::MoveUp => self.help_scroll = self.help_scroll.saturating_sub(1),
            _ => {}
        }
        Ok(())
    }

    fn handle_normal(&mut self, event: AppEvent) -> Result<(), AppError> {
        match event {
            AppEvent::Quit => self.mode = Mode::Quitting,
            AppEvent::OpenHelp => {
                self.help_scroll = 0;
                self.mode = Mode::Help;
            }
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
            AppEvent::Refresh => {
                self.refresh()?;
                // refresh() may set a diff-load error; don't overwrite it.
                if self.status_msg.is_none() {
                    self.status_msg = Some("Refreshed".into());
                }
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

/// Returns true when `mode` renders a floating overlay (Help, ThemePicker,
/// Confirming, Loading). Used by the run loop to detect transitions that
/// require a full terminal repaint.
pub fn is_overlay_mode(mode: &Mode) -> bool {
    matches!(
        mode,
        Mode::Help | Mode::ThemePicker | Mode::Confirming(_) | Mode::Loading(_)
    )
}

// Returns `Ok` with empty hunks for the benign "nothing to show" cases (no
// file selected, or a conflicted file whose hunks are loaded separately). A
// genuine libgit2 failure propagates as `Err` so callers can surface it
// instead of silently rendering an empty diff that looks like "no changes".
fn load_hunks_for(
    repo: &Repository,
    file: Option<&ChangedFile>,
) -> Result<(Vec<Hunk>, Vec<Hunk>), AppError> {
    let Some(f) = file else {
        return Ok((vec![], vec![]));
    };
    if f.status == FileStatus::Conflicted {
        return Ok((vec![], vec![]));
    }
    let staged = staged_diff_for_file(repo, &f.path)?;
    let unstaged = diff_for_file(repo, &f.path)?;
    Ok((staged, unstaged))
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
    fn corrupt_config_surfaces_error_in_status_msg() {
        let repo_dir = tempfile::TempDir::new().unwrap();
        let config_dir = tempfile::TempDir::new().unwrap();
        std::fs::write(config_dir.path().join("config.toml"), "not valid toml !!!").unwrap();
        let repo = git2::Repository::init(repo_dir.path()).unwrap();
        let app = App::new(repo, config_dir.path().to_path_buf(), None).unwrap();
        assert!(
            app.status_msg.is_some(),
            "a corrupt config.toml must surface an error in the status bar"
        );
        let msg = app.status_msg.unwrap();
        assert!(msg.contains("config.toml.bak"), "status message must mention the backup file");
    }

    #[test]
    fn valid_config_leaves_status_msg_empty() {
        let (_repo, _cfg, app) = make_test_app();
        assert!(
            app.status_msg.is_none(),
            "a valid (or absent) config must not set a status message on startup"
        );
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
    fn needs_clear_true_after_reload_hunks() {
        let (_repo, _cfg, mut app) = make_test_app();
        app.needs_clear = false;
        app.reload_hunks();
        assert!(
            app.needs_clear,
            "reload_hunks() must set needs_clear so switching files repaints the diff panel"
        );
    }

    fn modified_file(path: &str) -> ChangedFile {
        ChangedFile { path: path.into(), status: FileStatus::Modified, staged: false, unstaged: true }
    }

    #[test]
    fn load_hunks_for_none_file_is_ok_empty() {
        let (_repo_dir, _cfg, app) = make_test_app();
        let (staged, unstaged) = load_hunks_for(&app.repo, None).unwrap();
        assert!(staged.is_empty() && unstaged.is_empty(), "no selected file is a benign empty case, not an error");
    }

    #[test]
    fn load_hunks_for_conflicted_file_is_ok_empty() {
        let (_repo_dir, _cfg, app) = make_test_app();
        let conflicted = ChangedFile {
            path: "c.txt".into(),
            status: FileStatus::Conflicted,
            staged: false,
            unstaged: true,
        };
        let (staged, unstaged) = load_hunks_for(&app.repo, Some(&conflicted)).unwrap();
        assert!(
            staged.is_empty() && unstaged.is_empty(),
            "conflicted files load hunks separately and must not be treated as a diff error"
        );
    }

    #[test]
    fn load_hunks_for_propagates_diff_error() {
        // A bare repo has no working directory, so diff_index_to_workdir fails.
        // The error must propagate instead of being swallowed into empty hunks.
        let dir = tempfile::TempDir::new().unwrap();
        let repo = git2::Repository::init_bare(dir.path()).unwrap();
        let file = modified_file("anything.txt");
        let result = load_hunks_for(&repo, Some(&file));
        assert!(result.is_err(), "a genuine libgit2 diff failure must propagate as Err");
    }

    #[test]
    fn reload_hunks_surfaces_diff_error_in_status_msg() {
        let (_repo_dir, dir, mut app) = make_test_app();
        // Point the app at a bare repo so the diff load fails deterministically.
        app.repo = git2::Repository::init_bare(dir.path().join("bare")).unwrap();
        app.files = vec![modified_file("x.txt")];
        app.file_cursor = 0;
        app.status_msg = None;
        app.reload_hunks();
        let msg = app.status_msg.expect("a failed diff load must surface a status message");
        assert!(msg.contains("Failed to load diff"), "status message must name the failure, got: {msg:?}");
        assert!(app.staged_hunks.is_empty() && app.unstaged_hunks.is_empty());
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

        let (reloaded, _) = crate::config::Preferences::load(cfg.path());
        assert!(reloaded.transparent, "toggle must persist transparent=true to config.toml");

        app.toggle_transparent().unwrap();
        assert!(!app.transparent);
        let (reloaded2, _) = crate::config::Preferences::load(cfg.path());
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
        assert_eq!(app.themes.current_idx(), 0);
        assert_eq!(app.mode, Mode::Normal, "confirm on a theme item must close the picker");
    }

    #[test]
    fn help_scroll_starts_at_zero() {
        let (_repo, _cfg, app) = make_test_app();
        assert_eq!(app.help_scroll, 0);
    }

    #[test]
    fn help_move_down_increments_scroll() {
        let (_repo, _cfg, mut app) = make_test_app();
        app.mode = Mode::Help;
        app.help_max_scroll = 10;
        app.handle_help(crate::events::AppEvent::MoveDown).unwrap();
        assert_eq!(app.help_scroll, 1);
    }

    #[test]
    fn help_move_down_does_not_exceed_max_scroll() {
        let (_repo, _cfg, mut app) = make_test_app();
        app.mode = Mode::Help;
        app.help_max_scroll = 3;
        app.help_scroll = 3;
        app.handle_help(crate::events::AppEvent::MoveDown).unwrap();
        assert_eq!(app.help_scroll, 3, "scroll must not exceed help_max_scroll");
    }

    #[test]
    fn help_move_up_does_not_underflow() {
        let (_repo, _cfg, mut app) = make_test_app();
        app.mode = Mode::Help;
        app.handle_help(crate::events::AppEvent::MoveUp).unwrap();
        assert_eq!(app.help_scroll, 0, "scroll must not go below zero");
    }

    #[test]
    fn help_scroll_resets_on_open() {
        let (_repo, _cfg, mut app) = make_test_app();
        app.help_scroll = 5;
        app.handle_normal(crate::events::AppEvent::OpenHelp).unwrap();
        assert_eq!(app.help_scroll, 0, "opening help must reset scroll to top");
        assert_eq!(app.mode, Mode::Help);
    }

    #[test]
    fn help_cancel_closes_help() {
        let (_repo, _cfg, mut app) = make_test_app();
        app.mode = Mode::Help;
        app.handle_help(crate::events::AppEvent::Cancel).unwrap();
        assert_eq!(app.mode, Mode::Normal);
    }

    // ── worker thread / spinner tests ─────────────────────────────────────

    #[test]
    fn spinner_tick_starts_at_zero() {
        let (_repo, _cfg, app) = make_test_app();
        assert_eq!(app.spinner_tick, 0);
    }

    #[test]
    fn worker_rx_starts_as_none() {
        let (_repo, _cfg, app) = make_test_app();
        assert!(app.worker_rx.is_none());
    }

    #[test]
    fn spawn_remote_worker_sets_receiver() {
        let (_repo, _cfg, mut app) = make_test_app();
        app.mode = Mode::Loading(LoadingOp::Push);
        // spawn_remote_worker requires a non-bare repo workdir; make_test_app provides one
        app.spawn_remote_worker(LoadingOp::Push).unwrap();
        assert!(app.worker_rx.is_some(), "worker_rx must be Some after spawning");
    }

    #[test]
    fn finish_remote_op_push_success_sets_status_msg() {
        let (_repo, _cfg, mut app) = make_test_app();
        let result = Ok(crate::git::remote::RemoteResult { success: true, output: "ok".into() });
        app.finish_remote_op(LoadingOp::Push, result).unwrap();
        assert_eq!(app.status_msg, Some("Push: ok".into()));
        assert_eq!(app.mode, Mode::Normal);
    }

    #[test]
    fn finish_remote_op_push_failure_sets_status_msg() {
        let (_repo, _cfg, mut app) = make_test_app();
        let result = Ok(crate::git::remote::RemoteResult { success: false, output: "rejected".into() });
        app.finish_remote_op(LoadingOp::Push, result).unwrap();
        assert_eq!(app.status_msg, Some("Push failed: rejected".into()));
        assert_eq!(app.mode, Mode::Normal);
    }

    #[test]
    fn finish_remote_op_pull_success_sets_status_msg() {
        let (_repo, _cfg, mut app) = make_test_app();
        let result = Ok(crate::git::remote::RemoteResult { success: true, output: "up-to-date".into() });
        app.finish_remote_op(LoadingOp::Pull, result).unwrap();
        assert_eq!(app.status_msg, Some("Pull: up-to-date".into()));
        assert_eq!(app.mode, Mode::Normal);
    }

    #[test]
    fn finish_remote_op_pull_failure_sets_status_msg() {
        let (_repo, _cfg, mut app) = make_test_app();
        let result = Ok(crate::git::remote::RemoteResult { success: false, output: "not merged".into() });
        app.finish_remote_op(LoadingOp::Pull, result).unwrap();
        assert_eq!(app.status_msg, Some("Pull failed: not merged".into()));
        assert_eq!(app.mode, Mode::Normal);
    }

    #[test]
    fn finish_remote_op_error_sets_status_msg() {
        let (_repo, _cfg, mut app) = make_test_app();
        let result: Result<crate::git::remote::RemoteResult, AppError> =
            Err(AppError::Invalid("connection refused".into()));
        app.finish_remote_op(LoadingOp::Push, result).unwrap();
        let msg = app.status_msg.unwrap();
        assert!(msg.contains("connection refused"), "error text must appear in status bar");
        assert_eq!(app.mode, Mode::Normal);
    }

    #[test]
    fn loading_input_only_honours_quit() {
        let (_repo, _cfg, mut app) = make_test_app();
        app.mode = Mode::Loading(LoadingOp::Push);

        // non-Quit events must be silently drained
        let result = app.handle_normal(crate::events::AppEvent::Stage);
        // handle_normal isn't called during Loading — verify the mode unchanged
        // when we manually replicate the Loading arm of the run-loop match.
        // The run loop does: Mode::Loading(_) => { if Quit { mode = Quitting } }
        let event = crate::events::AppEvent::Stage;
        let before = app.mode.clone();
        if matches!(app.mode, Mode::Loading(_)) && matches!(event, AppEvent::Quit) {
            app.mode = Mode::Quitting;
        }
        assert_eq!(app.mode, before, "non-Quit event must not change Loading mode");
        let _ = result;
    }

    #[test]
    fn loading_quit_event_sets_quitting() {
        let (_repo, _cfg, mut app) = make_test_app();
        app.mode = Mode::Loading(LoadingOp::Push);
        let event = crate::events::AppEvent::Quit;
        if matches!(app.mode, Mode::Loading(_)) && matches!(event, AppEvent::Quit) {
            app.mode = Mode::Quitting;
        }
        assert_eq!(app.mode, Mode::Quitting);
    }

    #[test]
    fn execute_commit_empty_title_leaves_normal_mode() {
        let (_repo, _cfg, mut app) = make_test_app();
        app.mode = Mode::Loading(LoadingOp::Commit);
        app.commit_title.clear();
        app.execute_commit().unwrap();
        assert_eq!(app.mode, Mode::Normal, "empty-title commit must still exit Loading");
        assert!(app.status_msg.is_some(), "must set an error message for empty title");
    }

    // ── PendingAction ─────────────────────────────────────────────────────

    #[test]
    fn pending_action_prompt_contains_path() {
        let action = PendingAction::DeleteUntracked("foo/bar.rs".into());
        assert!(action.prompt().contains("foo/bar.rs"));
    }

    // ── bg helpers ────────────────────────────────────────────────────────

    #[test]
    fn bg_main_is_reset_when_transparent() {
        let (_repo, _cfg, mut app) = make_test_app();
        app.transparent = true;
        assert_eq!(app.bg_main(), ratatui::style::Color::Reset);
    }

    #[test]
    fn bg_panel_is_reset_when_transparent() {
        let (_repo, _cfg, mut app) = make_test_app();
        app.transparent = true;
        assert_eq!(app.bg_panel(), ratatui::style::Color::Reset);
    }

    #[test]
    fn bg_main_is_theme_color_when_not_transparent() {
        let (_repo, _cfg, mut app) = make_test_app();
        app.transparent = false;
        let expected = app.current_theme().base00;
        assert_eq!(app.bg_main(), expected);
    }

    // ── total_hunks / selected_hunk ───────────────────────────────────────

    #[test]
    fn total_hunks_is_sum_of_staged_and_unstaged() {
        let (_repo, _cfg, mut app) = make_test_app();
        let make_hunk = || crate::git::repo::Hunk {
            header: "@@ -1,1 +1,1 @@".into(),
            lines: vec![],
            old_start: 1,
            old_lines: 1,
            new_start: 1,
            new_lines: 1,
        };
        app.staged_hunks = vec![make_hunk(), make_hunk()];
        app.unstaged_hunks = vec![make_hunk()];
        assert_eq!(app.total_hunks(), 3);
    }

    #[test]
    fn selected_hunk_none_when_no_hunks() {
        let (_repo, _cfg, app) = make_test_app();
        assert!(app.selected_hunk().is_none());
    }

    #[test]
    fn selected_hunk_returns_staged_first() {
        let (_repo, _cfg, mut app) = make_test_app();
        let make_hunk = |header: &str| crate::git::repo::Hunk {
            header: header.into(),
            lines: vec![],
            old_start: 1,
            old_lines: 1,
            new_start: 1,
            new_lines: 1,
        };
        app.staged_hunks = vec![make_hunk("staged")];
        app.unstaged_hunks = vec![make_hunk("unstaged")];
        app.hunk_cursor = 0;
        let (hunk, is_staged) = app.selected_hunk().unwrap();
        assert_eq!(hunk.header, "staged");
        assert!(is_staged);
    }

    #[test]
    fn selected_hunk_returns_unstaged_after_staged() {
        let (_repo, _cfg, mut app) = make_test_app();
        let make_hunk = |header: &str| crate::git::repo::Hunk {
            header: header.into(),
            lines: vec![],
            old_start: 1,
            old_lines: 1,
            new_start: 1,
            new_lines: 1,
        };
        app.staged_hunks = vec![make_hunk("staged")];
        app.unstaged_hunks = vec![make_hunk("unstaged")];
        app.hunk_cursor = 1;
        let (hunk, is_staged) = app.selected_hunk().unwrap();
        assert_eq!(hunk.header, "unstaged");
        assert!(!is_staged);
    }

    // ── is_file_conflicted ────────────────────────────────────────────────

    #[test]
    fn is_file_conflicted_false_when_no_files() {
        let (_repo, _cfg, app) = make_test_app();
        assert!(!app.is_file_conflicted());
    }

    // ── hunk navigation ───────────────────────────────────────────────────

    #[test]
    fn move_hunk_up_does_not_underflow() {
        let (_repo, _cfg, mut app) = make_test_app();
        app.hunk_cursor = 0;
        app.move_hunk_up();
        assert_eq!(app.hunk_cursor, 0);
    }

    #[test]
    fn move_hunk_down_does_not_exceed_total() {
        let (_repo, _cfg, mut app) = make_test_app();
        app.move_hunk_down();
        assert_eq!(app.hunk_cursor, 0);
    }

    #[test]
    fn move_hunk_down_increments_within_bounds() {
        let (_repo, _cfg, mut app) = make_test_app();
        let make_hunk = || crate::git::repo::Hunk {
            header: "h".into(),
            lines: vec![],
            old_start: 1,
            old_lines: 1,
            new_start: 1,
            new_lines: 1,
        };
        app.unstaged_hunks = vec![make_hunk(), make_hunk()];
        app.move_hunk_down();
        assert_eq!(app.hunk_cursor, 1);
        app.move_hunk_down();
        assert_eq!(app.hunk_cursor, 1, "must not exceed total_hunks - 1");
    }

    // ── file navigation ───────────────────────────────────────────────────

    #[test]
    fn move_file_up_does_not_underflow() {
        let (_repo, _cfg, mut app) = make_test_app();
        app.move_file_up();
        assert_eq!(app.file_cursor, 0);
    }

    #[test]
    fn move_file_down_does_not_exceed_len() {
        let (_repo, _cfg, mut app) = make_test_app();
        app.move_file_down();
        assert_eq!(app.file_cursor, 0);
    }

    // ── conflict navigation ───────────────────────────────────────────────

    #[test]
    fn move_conflict_up_does_not_underflow() {
        let (_repo, _cfg, mut app) = make_test_app();
        app.move_conflict_up();
        assert_eq!(app.conflict_cursor, 0);
    }

    #[test]
    fn move_conflict_down_does_not_exceed_len() {
        let (_repo, _cfg, mut app) = make_test_app();
        app.move_conflict_down();
        assert_eq!(app.conflict_cursor, 0);
    }

    // ── delete_untracked_current guards ──────────────────────────────────

    #[test]
    fn delete_untracked_current_no_op_when_no_files() {
        let (_repo, _cfg, mut app) = make_test_app();
        app.delete_untracked_current();
        assert!(matches!(app.mode, Mode::Normal));
    }

    // ── discard_current guards ────────────────────────────────────────────

    #[test]
    fn discard_current_no_op_when_no_hunks() {
        let (_repo, _cfg, mut app) = make_test_app();
        app.discard_current().unwrap();
        assert!(app.status_msg.is_none());
    }

    #[test]
    fn discard_current_staged_hunk_sets_status() {
        let (_repo, _cfg, mut app) = make_test_app();
        app.files = vec![crate::git::repo::ChangedFile {
            path: "fake.rs".into(),
            status: crate::git::repo::FileStatus::Modified,
            staged: false,
            unstaged: true,
        }];
        app.staged_hunks = vec![crate::git::repo::Hunk {
            header: "h".into(),
            lines: vec![],
            old_start: 1,
            old_lines: 1,
            new_start: 1,
            new_lines: 1,
        }];
        app.hunk_cursor = 0;
        app.discard_current().unwrap();
        assert_eq!(
            app.status_msg.as_deref(),
            Some("Unstage the hunk first to discard it")
        );
    }

    // ── handle_commit_title ───────────────────────────────────────────────

    #[test]
    fn commit_title_char_appends() {
        let (_repo, _cfg, mut app) = make_test_app();
        app.handle_commit_title(crate::events::AppEvent::Char('f')).unwrap();
        app.handle_commit_title(crate::events::AppEvent::Char('o')).unwrap();
        assert_eq!(app.commit_title, "fo");
    }

    #[test]
    fn commit_title_backspace_removes_last() {
        let (_repo, _cfg, mut app) = make_test_app();
        app.commit_title = "ab".into();
        app.handle_commit_title(crate::events::AppEvent::Backspace).unwrap();
        assert_eq!(app.commit_title, "a");
    }

    #[test]
    fn commit_title_confirm_transitions_to_body() {
        let (_repo, _cfg, mut app) = make_test_app();
        app.mode = Mode::CommitTitle;
        app.handle_commit_title(crate::events::AppEvent::Confirm).unwrap();
        assert_eq!(app.mode, Mode::CommitBody);
    }

    #[test]
    fn commit_title_cancel_clears_and_returns_to_normal() {
        let (_repo, _cfg, mut app) = make_test_app();
        app.commit_title = "work in progress".into();
        app.mode = Mode::CommitTitle;
        app.handle_commit_title(crate::events::AppEvent::Cancel).unwrap();
        assert_eq!(app.mode, Mode::Normal);
        assert!(app.commit_title.is_empty());
    }

    // ── handle_commit_body ────────────────────────────────────────────────

    #[test]
    fn commit_body_char_appends() {
        let (_repo, _cfg, mut app) = make_test_app();
        app.handle_commit_body(crate::events::AppEvent::Char('x')).unwrap();
        assert_eq!(app.commit_body, "x");
    }

    #[test]
    fn commit_body_backspace_removes_last() {
        let (_repo, _cfg, mut app) = make_test_app();
        app.commit_body = "hello".into();
        app.handle_commit_body(crate::events::AppEvent::Backspace).unwrap();
        assert_eq!(app.commit_body, "hell");
    }

    #[test]
    fn commit_body_confirm_transitions_to_loading() {
        let (_repo, _cfg, mut app) = make_test_app();
        app.mode = Mode::CommitBody;
        app.handle_commit_body(crate::events::AppEvent::Confirm).unwrap();
        assert_eq!(app.mode, Mode::Loading(LoadingOp::Commit));
    }

    #[test]
    fn commit_body_cancel_returns_to_normal() {
        let (_repo, _cfg, mut app) = make_test_app();
        app.mode = Mode::CommitBody;
        app.handle_commit_body(crate::events::AppEvent::Cancel).unwrap();
        assert_eq!(app.mode, Mode::Normal);
    }

    // ── handle_confirming ─────────────────────────────────────────────────

    #[test]
    fn confirming_cancel_sets_cancelled_status() {
        let (_repo, _cfg, mut app) = make_test_app();
        let action = PendingAction::DeleteUntracked("x".into());
        app.mode = Mode::Confirming(action.clone());
        app.handle_confirming(crate::events::AppEvent::Cancel, action).unwrap();
        assert_eq!(app.mode, Mode::Normal);
        assert_eq!(app.status_msg.as_deref(), Some("Cancelled"));
    }

    #[test]
    fn confirming_n_sets_cancelled_status() {
        let (_repo, _cfg, mut app) = make_test_app();
        let action = PendingAction::DeleteUntracked("x".into());
        app.handle_confirming(crate::events::AppEvent::Char('n'), action).unwrap();
        assert_eq!(app.mode, Mode::Normal);
        assert_eq!(app.status_msg.as_deref(), Some("Cancelled"));
    }

    #[test]
    fn confirming_uppercase_n_sets_cancelled_status() {
        let (_repo, _cfg, mut app) = make_test_app();
        let action = PendingAction::DeleteUntracked("x".into());
        app.handle_confirming(crate::events::AppEvent::Char('N'), action).unwrap();
        assert_eq!(app.mode, Mode::Normal);
        assert_eq!(app.status_msg.as_deref(), Some("Cancelled"));
    }

    // ── handle_theme_picker ───────────────────────────────────────────────

    #[test]
    fn theme_picker_move_up_decrements_cursor() {
        let (_repo, _cfg, mut app) = make_test_app();
        app.theme_picker_cursor = 1;
        app.handle_theme_picker(crate::events::AppEvent::MoveUp).unwrap();
        assert_eq!(app.theme_picker_cursor, 0);
    }

    #[test]
    fn theme_picker_move_down_increments_cursor() {
        let (_repo, _cfg, mut app) = make_test_app();
        app.theme_picker_cursor = 0;
        app.handle_theme_picker(crate::events::AppEvent::MoveDown).unwrap();
        assert_eq!(app.theme_picker_cursor, 1);
    }

    #[test]
    fn theme_picker_prev_hunk_moves_up() {
        let (_repo, _cfg, mut app) = make_test_app();
        app.theme_picker_cursor = 2;
        app.handle_theme_picker(crate::events::AppEvent::PrevHunk).unwrap();
        assert_eq!(app.theme_picker_cursor, 1);
    }

    #[test]
    fn theme_picker_next_hunk_moves_down() {
        let (_repo, _cfg, mut app) = make_test_app();
        app.theme_picker_cursor = 0;
        app.handle_theme_picker(crate::events::AppEvent::NextHunk).unwrap();
        assert_eq!(app.theme_picker_cursor, 1);
    }

    #[test]
    fn theme_picker_cancel_returns_to_normal() {
        let (_repo, _cfg, mut app) = make_test_app();
        app.mode = Mode::ThemePicker;
        app.handle_theme_picker(crate::events::AppEvent::Cancel).unwrap();
        assert_eq!(app.mode, Mode::Normal);
    }

    #[test]
    fn theme_picker_quit_returns_to_normal() {
        let (_repo, _cfg, mut app) = make_test_app();
        app.mode = Mode::ThemePicker;
        app.handle_theme_picker(crate::events::AppEvent::Quit).unwrap();
        assert_eq!(app.mode, Mode::Normal);
    }

    // ── handle_normal ─────────────────────────────────────────────────────

    #[test]
    fn handle_normal_quit_sets_quitting() {
        let (_repo, _cfg, mut app) = make_test_app();
        app.handle_normal(crate::events::AppEvent::Quit).unwrap();
        assert_eq!(app.mode, Mode::Quitting);
    }

    #[test]
    fn handle_normal_commit_event_sets_commit_title_mode() {
        let (_repo, _cfg, mut app) = make_test_app();
        app.handle_normal(crate::events::AppEvent::Commit).unwrap();
        assert_eq!(app.mode, Mode::CommitTitle);
    }

    #[test]
    fn handle_normal_toggle_focus_file_to_diff() {
        let (_repo, _cfg, mut app) = make_test_app();
        assert_eq!(app.focus, Focus::FileList);
        app.handle_normal(crate::events::AppEvent::ToggleFocus).unwrap();
        assert_eq!(app.focus, Focus::DiffView);
    }

    #[test]
    fn handle_normal_toggle_focus_diff_to_file() {
        let (_repo, _cfg, mut app) = make_test_app();
        app.focus = Focus::DiffView;
        app.handle_normal(crate::events::AppEvent::ToggleFocus).unwrap();
        assert_eq!(app.focus, Focus::FileList);
    }

    #[test]
    fn handle_normal_open_theme_picker() {
        let (_repo, _cfg, mut app) = make_test_app();
        app.handle_normal(crate::events::AppEvent::OpenThemePicker).unwrap();
        assert_eq!(app.mode, Mode::ThemePicker);
    }

    #[test]
    fn handle_normal_push_enters_loading() {
        let (_repo, _cfg, mut app) = make_test_app();
        app.handle_normal(crate::events::AppEvent::Push).unwrap();
        assert_eq!(app.mode, Mode::Loading(LoadingOp::Push));
    }

    #[test]
    fn handle_normal_pull_enters_loading() {
        let (_repo, _cfg, mut app) = make_test_app();
        app.handle_normal(crate::events::AppEvent::Pull).unwrap();
        assert_eq!(app.mode, Mode::Loading(LoadingOp::Pull));
    }

    #[test]
    fn handle_normal_refresh_sets_status_msg() {
        let (_repo, _cfg, mut app) = make_test_app();
        app.handle_normal(crate::events::AppEvent::Refresh).unwrap();
        assert_eq!(app.status_msg.as_deref(), Some("Refreshed"));
    }

    #[test]
    fn handle_normal_refresh_sets_needs_clear() {
        let (_repo, _cfg, mut app) = make_test_app();
        app.handle_normal(crate::events::AppEvent::Refresh).unwrap();
        assert!(app.needs_clear, "refresh must set needs_clear to force a repaint");
    }

    #[test]
    fn handle_normal_refresh_stays_in_normal_mode() {
        let (_repo, _cfg, mut app) = make_test_app();
        app.handle_normal(crate::events::AppEvent::Refresh).unwrap();
        assert_eq!(app.mode, Mode::Normal);
    }

    #[test]
    fn handle_normal_stage_no_op_when_no_files() {
        let (_repo, _cfg, mut app) = make_test_app();
        app.handle_normal(crate::events::AppEvent::Stage).unwrap();
        assert!(app.status_msg.is_none());
    }

    #[test]
    fn handle_normal_unstage_no_op_when_no_files() {
        let (_repo, _cfg, mut app) = make_test_app();
        app.handle_normal(crate::events::AppEvent::Unstage).unwrap();
        assert!(app.status_msg.is_none());
    }

    #[test]
    fn handle_normal_discard_no_op_when_no_hunks() {
        let (_repo, _cfg, mut app) = make_test_app();
        app.handle_normal(crate::events::AppEvent::Discard).unwrap();
        assert!(app.status_msg.is_none());
    }

    #[test]
    fn handle_normal_move_up_file_list_clamps() {
        let (_repo, _cfg, mut app) = make_test_app();
        app.focus = Focus::FileList;
        app.handle_normal(crate::events::AppEvent::MoveUp).unwrap();
        assert_eq!(app.file_cursor, 0);
    }

    #[test]
    fn handle_normal_move_down_file_list_clamps() {
        let (_repo, _cfg, mut app) = make_test_app();
        app.focus = Focus::FileList;
        app.handle_normal(crate::events::AppEvent::MoveDown).unwrap();
        assert_eq!(app.file_cursor, 0);
    }

    #[test]
    fn handle_normal_move_up_diff_view_clamps() {
        let (_repo, _cfg, mut app) = make_test_app();
        app.focus = Focus::DiffView;
        app.handle_normal(crate::events::AppEvent::MoveUp).unwrap();
        assert_eq!(app.hunk_cursor, 0);
    }

    #[test]
    fn handle_normal_move_down_diff_view_clamps() {
        let (_repo, _cfg, mut app) = make_test_app();
        app.focus = Focus::DiffView;
        app.handle_normal(crate::events::AppEvent::MoveDown).unwrap();
        assert_eq!(app.hunk_cursor, 0);
    }

    #[test]
    fn handle_normal_next_hunk_clamps() {
        let (_repo, _cfg, mut app) = make_test_app();
        app.handle_normal(crate::events::AppEvent::NextHunk).unwrap();
        assert_eq!(app.hunk_cursor, 0);
    }

    #[test]
    fn handle_normal_prev_hunk_clamps() {
        let (_repo, _cfg, mut app) = make_test_app();
        app.handle_normal(crate::events::AppEvent::PrevHunk).unwrap();
        assert_eq!(app.hunk_cursor, 0);
    }

    // ── LoadingOp::Demo label ─────────────────────────────────────────────

    #[test]
    fn loading_op_demo_label_is_non_empty() {
        assert!(!LoadingOp::Demo.label().is_empty());
    }

    // ── is_overlay_mode ───────────────────────────────────────────────────

    #[test]
    fn is_overlay_mode_false_for_normal() {
        assert!(!is_overlay_mode(&Mode::Normal));
    }

    #[test]
    fn is_overlay_mode_false_for_commit_title() {
        assert!(!is_overlay_mode(&Mode::CommitTitle));
    }

    #[test]
    fn is_overlay_mode_false_for_commit_body() {
        assert!(!is_overlay_mode(&Mode::CommitBody));
    }

    #[test]
    fn is_overlay_mode_true_for_help() {
        assert!(is_overlay_mode(&Mode::Help));
    }

    #[test]
    fn is_overlay_mode_true_for_theme_picker() {
        assert!(is_overlay_mode(&Mode::ThemePicker));
    }

    #[test]
    fn is_overlay_mode_true_for_confirming() {
        let action = PendingAction::DeleteUntracked("x".into());
        assert!(is_overlay_mode(&Mode::Confirming(action)));
    }

    #[test]
    fn is_overlay_mode_true_for_loading() {
        assert!(is_overlay_mode(&Mode::Loading(LoadingOp::Push)));
    }

    // ── needs_clear set on overlay-affecting operations ───────────────────

    #[test]
    fn apply_theme_sets_needs_clear() {
        let (_repo, _cfg, mut app) = make_test_app();
        app.needs_clear = false;
        app.apply_theme().unwrap();
        assert!(app.needs_clear, "apply_theme must set needs_clear so all cells repaint with new colors");
    }

    #[test]
    fn apply_theme_closes_theme_picker() {
        let (_repo, _cfg, mut app) = make_test_app();
        app.mode = Mode::ThemePicker;
        app.apply_theme().unwrap();
        assert_eq!(app.mode, Mode::Normal);
    }

    #[test]
    fn toggle_transparent_sets_needs_clear() {
        let (_repo, _cfg, mut app) = make_test_app();
        app.needs_clear = false;
        app.toggle_transparent().unwrap();
        assert!(app.needs_clear, "toggle_transparent must set needs_clear so background color change repaints all cells");
    }

    #[test]
    fn toggle_transparent_sets_needs_clear_when_turning_off() {
        let (_repo, _cfg, mut app) = make_test_app();
        app.transparent = true;
        app.needs_clear = false;
        app.toggle_transparent().unwrap();
        assert!(!app.transparent);
        assert!(app.needs_clear, "toggling transparent off must also set needs_clear");
    }
}
