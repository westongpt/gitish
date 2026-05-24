use std::path::Path;

use git2::{Delta, DiffOptions, Repository, Status, StatusOptions};

use crate::error::AppError;

#[derive(Debug, Clone, PartialEq)]
pub enum FileStatus {
    Untracked, // WT_NEW only — git has never seen this file
    New,       // INDEX_NEW  — staged for the first time
    Modified,
    Deleted,
}

#[derive(Debug, Clone)]
pub struct ChangedFile {
    pub path: String,
    pub status: FileStatus,
    pub staged: bool,
    pub unstaged: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum LineKind {
    Added,
    Removed,
    Context,
}

#[derive(Debug, Clone)]
pub struct DiffLine {
    pub kind: LineKind,
    pub content: String,
}

#[derive(Debug, Clone)]
pub struct Hunk {
    pub header: String,
    pub lines: Vec<DiffLine>,
    pub old_start: u32,
    pub old_lines: u32,
    pub new_start: u32,
    pub new_lines: u32,
}

pub fn open_repo(path: &Path) -> Result<Repository, AppError> {
    Ok(Repository::discover(path)?)
}

pub fn list_changed_files(repo: &Repository) -> Result<Vec<ChangedFile>, AppError> {
    let mut opts = StatusOptions::new();
    opts.include_untracked(true)
        .recurse_untracked_dirs(true)
        .include_ignored(false);

    let statuses = repo.statuses(Some(&mut opts))?;
    let mut files = Vec::new();

    for entry in statuses.iter() {
        let s = entry.status();
        let path = entry.path().unwrap_or("").to_string();

        let staged = s.intersects(
            Status::INDEX_NEW
                | Status::INDEX_MODIFIED
                | Status::INDEX_DELETED
                | Status::INDEX_RENAMED,
        );
        let unstaged = s.intersects(
            Status::WT_NEW
                | Status::WT_MODIFIED
                | Status::WT_DELETED
                | Status::WT_RENAMED,
        );

        if !staged && !unstaged {
            continue;
        }

        let file_status = if s.contains(Status::WT_NEW) && !s.intersects(
            Status::INDEX_NEW | Status::INDEX_MODIFIED | Status::INDEX_DELETED,
        ) {
            FileStatus::Untracked
        } else if s.intersects(Status::INDEX_NEW) {
            FileStatus::New
        } else if s.intersects(Status::INDEX_DELETED | Status::WT_DELETED) {
            FileStatus::Deleted
        } else {
            FileStatus::Modified
        };

        files.push(ChangedFile {
            path,
            status: file_status,
            staged,
            unstaged,
        });
    }

    files.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(files)
}

pub fn diff_for_file(repo: &Repository, path: &str) -> Result<Vec<Hunk>, AppError> {
    let mut opts = DiffOptions::new();
    opts.pathspec(path).context_lines(3);

    let diff = repo.diff_index_to_workdir(None, Some(&mut opts))?;
    parse_diff_hunks(&diff)
}

pub fn staged_diff_for_file(repo: &Repository, path: &str) -> Result<Vec<Hunk>, AppError> {
    let head = repo.head().ok().and_then(|h| h.peel_to_tree().ok());
    let mut opts = DiffOptions::new();
    opts.pathspec(path).context_lines(3);

    let diff = repo.diff_tree_to_index(head.as_ref(), None, Some(&mut opts))?;
    parse_diff_hunks(&diff)
}

fn parse_diff_hunks(diff: &git2::Diff) -> Result<Vec<Hunk>, AppError> {
    use std::cell::RefCell;

    let hunks: RefCell<Vec<Hunk>> = RefCell::new(Vec::new());

    diff.foreach(
        &mut |_, _| true,
        None,
        Some(&mut |delta, hunk| {
            if matches!(
                delta.status(),
                Delta::Unmodified | Delta::Ignored | Delta::Unreadable
            ) {
                return true;
            }
            hunks.borrow_mut().push(Hunk {
                header: String::from_utf8_lossy(hunk.header()).trim_end().to_string(),
                lines: Vec::new(),
                old_start: hunk.old_start(),
                old_lines: hunk.old_lines(),
                new_start: hunk.new_start(),
                new_lines: hunk.new_lines(),
            });
            true
        }),
        Some(&mut |_, _, line| {
            let kind = match line.origin() {
                '+' => LineKind::Added,
                '-' => LineKind::Removed,
                ' ' => LineKind::Context,
                _ => return true,
            };
            let content = String::from_utf8_lossy(line.content())
                .trim_end_matches('\n')
                .trim_end_matches('\r')
                .to_string();
            if let Some(hunk) = hunks.borrow_mut().last_mut() {
                hunk.lines.push(DiffLine { kind, content });
            }
            true
        }),
    )?;

    Ok(hunks.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn make_repo() -> (TempDir, Repository) {
        let dir = TempDir::new().unwrap();
        let repo = Repository::init(dir.path()).unwrap();
        // configure minimal identity so commits work
        let mut cfg = repo.config().unwrap();
        cfg.set_str("user.name", "Test").unwrap();
        cfg.set_str("user.email", "test@test.com").unwrap();
        (dir, repo)
    }

    fn initial_commit(repo: &Repository) {
        let sig = repo.signature().unwrap();
        let tree_id = {
            let mut idx = repo.index().unwrap();
            idx.write_tree().unwrap()
        };
        let tree = repo.find_tree(tree_id).unwrap();
        repo.commit(Some("HEAD"), &sig, &sig, "init", &tree, &[])
            .unwrap();
    }

    #[test]
    fn detects_new_untracked_file() {
        let (dir, repo) = make_repo();
        initial_commit(&repo);
        fs::write(dir.path().join("foo.txt"), "hello\n").unwrap();
        let files = list_changed_files(&repo).unwrap();
        assert!(files.iter().any(|f| f.path == "foo.txt" && f.status == FileStatus::Untracked));
    }

    #[test]
    fn detects_modified_file() {
        let (dir, repo) = make_repo();
        let fpath = dir.path().join("bar.txt");
        fs::write(&fpath, "original\n").unwrap();
        let mut idx = repo.index().unwrap();
        idx.add_path(Path::new("bar.txt")).unwrap();
        idx.write().unwrap();
        initial_commit(&repo);

        fs::write(&fpath, "modified\n").unwrap();
        let files = list_changed_files(&repo).unwrap();
        assert!(files
            .iter()
            .any(|f| f.path == "bar.txt" && f.status == FileStatus::Modified));
    }

    #[test]
    fn diff_for_file_returns_hunks() {
        let (dir, repo) = make_repo();
        let fpath = dir.path().join("baz.txt");
        fs::write(&fpath, "line1\nline2\nline3\n").unwrap();
        let mut idx = repo.index().unwrap();
        idx.add_path(Path::new("baz.txt")).unwrap();
        idx.write().unwrap();
        initial_commit(&repo);

        fs::write(&fpath, "line1\nchanged\nline3\n").unwrap();
        let hunks = diff_for_file(&repo, "baz.txt").unwrap();
        assert!(!hunks.is_empty());
        assert!(hunks[0]
            .lines
            .iter()
            .any(|l| l.kind == LineKind::Removed && l.content.contains("line2")));
        assert!(hunks[0]
            .lines
            .iter()
            .any(|l| l.kind == LineKind::Added && l.content.contains("changed")));
    }
}
