use std::path::Path;

use git2::{Delta, DiffOptions, Repository, Status, StatusOptions};

use crate::error::AppError;

#[derive(Debug, Clone, PartialEq)]
pub enum FileStatus {
    Untracked, // WT_NEW only — git has never seen this file
    New,       // INDEX_NEW  — staged for the first time
    Modified,
    Deleted,
    Conflicted, // merge conflict — file contains conflict markers
}

/// One conflict region parsed from a file with merge markers.
#[derive(Debug, Clone)]
pub struct ConflictBlock {
    /// Lines from HEAD (our side), without the leading conflict marker line.
    pub ours: Vec<String>,
    /// Lines from the incoming branch (their side).
    pub theirs: Vec<String>,
    /// 0-based index of the `<<<<<<<` line in the file.
    pub start_line: usize,
    /// 0-based index of the `>>>>>>>` line in the file (inclusive).
    pub end_line: usize,
}

/// Parse standard conflict markers from file content.
/// Handles both 2-way (`<<<`/`===`/`>>>`) and diff3 (`<<<`/`|||`/`===`/`>>>`) styles.
pub fn parse_conflict_markers(content: &str) -> Vec<ConflictBlock> {
    let lines: Vec<&str> = content.lines().collect();
    let mut blocks = Vec::new();
    let mut i = 0;

    while i < lines.len() {
        if lines[i].starts_with("<<<<<<<") {
            let start = i;
            let mut ours: Vec<String> = Vec::new();
            let mut theirs: Vec<String> = Vec::new();
            let mut in_base = false; // diff3 base section (||||||| ... =======)
            let mut in_theirs = false;
            i += 1;

            while i < lines.len() {
                if lines[i].starts_with(">>>>>>>") {
                    blocks.push(ConflictBlock {
                        ours,
                        theirs,
                        start_line: start,
                        end_line: i,
                    });
                    i += 1;
                    break;
                } else if lines[i].starts_with("|||||||") {
                    // diff3 base marker — skip base lines
                    in_base = true;
                    i += 1;
                } else if lines[i].starts_with("=======") {
                    in_base = false;
                    in_theirs = true;
                    i += 1;
                } else {
                    if in_theirs {
                        theirs.push(lines[i].to_string());
                    } else if !in_base {
                        ours.push(lines[i].to_string());
                    }
                    i += 1;
                }
            }
        } else {
            i += 1;
        }
    }

    blocks
}

/// Read conflict blocks from a file in the working directory.
pub fn detect_conflicts(repo: &Repository, path: &str) -> Result<Vec<ConflictBlock>, AppError> {
    let workdir = repo
        .workdir()
        .ok_or_else(|| AppError::Invalid("bare repository".into()))?;
    let content = std::fs::read_to_string(workdir.join(path))?;
    Ok(parse_conflict_markers(&content))
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

        let conflicted = s.contains(Status::CONFLICTED);
        if !staged && !unstaged && !conflicted {
            continue;
        }

        let file_status = if s.contains(Status::CONFLICTED) {
            FileStatus::Conflicted
        } else if s.contains(Status::WT_NEW) && !s.intersects(
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

    // ── conflict marker parsing ───────────────────────────────────────────

    #[test]
    fn parse_basic_conflict_two_way() {
        let content = "before\n<<<<<<< HEAD\nours_line\n=======\ntheirs_line\n>>>>>>> branch\nafter\n";
        let blocks = parse_conflict_markers(content);
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].ours, vec!["ours_line"]);
        assert_eq!(blocks[0].theirs, vec!["theirs_line"]);
        assert_eq!(blocks[0].start_line, 1); // 0-indexed "<<<<<<< HEAD" line
        assert_eq!(blocks[0].end_line, 5);   // 0-indexed ">>>>>>> branch" line
    }

    #[test]
    fn parse_conflict_multiple_blocks() {
        let content = concat!(
            "<<<<<<< HEAD\na\n=======\nb\n>>>>>>> x\n",
            "middle\n",
            "<<<<<<< HEAD\nc\n=======\nd\n>>>>>>> x\n",
        );
        let blocks = parse_conflict_markers(content);
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0].ours, vec!["a"]);
        assert_eq!(blocks[0].theirs, vec!["b"]);
        assert_eq!(blocks[1].ours, vec!["c"]);
        assert_eq!(blocks[1].theirs, vec!["d"]);
    }

    #[test]
    fn parse_conflict_diff3_style_skips_base() {
        let content = "<<<<<<< HEAD\nours\n||||||| parent\nbase\n=======\ntheirs\n>>>>>>> x\n";
        let blocks = parse_conflict_markers(content);
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].ours, vec!["ours"]);
        assert_eq!(blocks[0].theirs, vec!["theirs"]);
    }

    #[test]
    fn parse_conflict_no_markers_returns_empty() {
        let content = "just normal text\nno conflicts here\n";
        let blocks = parse_conflict_markers(content);
        assert!(blocks.is_empty());
    }

    #[test]
    fn parse_conflict_multiline_sides() {
        let content = "<<<<<<< HEAD\nline1\nline2\n=======\nother1\nother2\nother3\n>>>>>>> x\n";
        let blocks = parse_conflict_markers(content);
        assert_eq!(blocks[0].ours, vec!["line1", "line2"]);
        assert_eq!(blocks[0].theirs, vec!["other1", "other2", "other3"]);
    }

    #[test]
    fn list_changed_files_detects_conflicted() {
        use std::process::Command;
        let dir = TempDir::new().unwrap();
        let p = dir.path();

        let run = |args: &[&str]| {
            Command::new("git").args(args).current_dir(p).output().unwrap()
        };

        run(&["init", "-q"]);
        run(&["config", "user.email", "test@test.com"]);
        run(&["config", "user.name", "Test"]);
        fs::write(p.join("f.txt"), "base\n").unwrap();
        run(&["add", "f.txt"]);
        run(&["commit", "-qm", "base"]);
        run(&["checkout", "-qb", "feat"]);
        fs::write(p.join("f.txt"), "feat side\n").unwrap();
        run(&["add", "f.txt"]);
        run(&["commit", "-qm", "feat"]);
        run(&["checkout", "-q", "main"]);
        fs::write(p.join("f.txt"), "main side\n").unwrap();
        run(&["add", "f.txt"]);
        run(&["commit", "-qm", "main"]);
        run(&["merge", "--no-commit", "feat"]); // exits non-zero — expected

        let repo = Repository::open(p).unwrap();
        let files = list_changed_files(&repo).unwrap();
        let conflict_entry = files.iter().find(|f| f.path == "f.txt");
        assert!(
            conflict_entry.is_some(),
            "conflicted file must appear in list_changed_files"
        );
        assert_eq!(
            conflict_entry.unwrap().status,
            FileStatus::Conflicted,
            "status of a conflicted file must be FileStatus::Conflicted"
        );
    }

    #[test]
    fn detect_conflicts_reads_workdir_file() {
        let (dir, repo) = make_repo();
        let conflict_content = "preamble\n<<<<<<< HEAD\nours\n=======\ntheirs\n>>>>>>> x\npostamble\n";
        fs::write(dir.path().join("conflict.txt"), conflict_content).unwrap();
        let blocks = detect_conflicts(&repo, "conflict.txt").unwrap();
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].ours, vec!["ours"]);
        assert_eq!(blocks[0].theirs, vec!["theirs"]);
    }

    // ── existing tests ────────────────────────────────────────────────────

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
