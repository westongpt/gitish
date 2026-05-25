use std::io::Write as _;
use std::path::Path;
use std::process::{Command, Stdio};

use git2::{ApplyLocation, Repository};

use crate::error::AppError;
use crate::git::repo::{ConflictBlock, Hunk};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ConflictSide {
    Ours,
    Theirs,
    Both,
}

/// Resolve a single conflict block in-place and write the file back.
/// Replaces the conflict block at `block_idx` with the chosen side's lines.
/// If no more conflict markers remain after the edit, the file is staged.
pub fn resolve_conflict_block(
    repo: &Repository,
    path: &str,
    blocks: &[ConflictBlock],
    block_idx: usize,
    side: ConflictSide,
) -> Result<(), AppError> {
    let workdir = repo
        .workdir()
        .ok_or_else(|| AppError::Invalid("bare repository".into()))?;
    let full_path = workdir.join(path);
    let content = std::fs::read_to_string(&full_path)?;
    let lines: Vec<&str> = content.lines().collect();

    let block = blocks
        .get(block_idx)
        .ok_or_else(|| AppError::Invalid("conflict block index out of range".into()))?;

    let replacement: Vec<String> = match side {
        ConflictSide::Ours => block.ours.clone(),
        ConflictSide::Theirs => block.theirs.clone(),
        ConflictSide::Both => {
            let mut both = block.ours.clone();
            both.extend(block.theirs.clone());
            both
        }
    };

    let mut output: Vec<&str> = lines[..block.start_line].to_vec();
    for r in &replacement {
        output.push(r.as_str());
    }
    if block.end_line + 1 < lines.len() {
        output.extend_from_slice(&lines[block.end_line + 1..]);
    }

    let mut new_content = output.join("\n");
    // Preserve trailing newline if original had one
    if content.ends_with('\n') {
        new_content.push('\n');
    }

    std::fs::write(&full_path, &new_content)?;

    // If no more conflict markers remain, stage the file automatically
    if !new_content.contains("<<<<<<<") {
        stage_file(repo, path)?;
    }

    Ok(())
}

pub fn stage_file(repo: &Repository, path: &str) -> Result<(), AppError> {
    let mut index = repo.index()?;
    let workdir = repo
        .workdir()
        .ok_or_else(|| AppError::Invalid("bare repository".into()))?;
    if workdir.join(path).exists() {
        index.add_path(Path::new(path))?;
    } else {
        index.remove_path(Path::new(path))?;
    }
    index.write()?;
    Ok(())
}

pub fn unstage_file(repo: &Repository, path: &str) -> Result<(), AppError> {
    let head = repo.head().ok().and_then(|h| h.peel_to_commit().ok());
    match head {
        Some(commit) => {
            repo.reset_default(Some(commit.as_object()), [path])?;
        }
        None => {
            let mut index = repo.index()?;
            index.remove_path(Path::new(path))?;
            index.write()?;
        }
    }
    Ok(())
}

pub fn stage_hunk(repo: &Repository, path: &str, hunk: &Hunk) -> Result<(), AppError> {
    // Forward patch of the workdir diff, applied to the index.
    let patch = build_patch(path, hunk);
    let diff = git2::Diff::from_buffer(patch.as_bytes())?;
    repo.apply(&diff, ApplyLocation::Index, None)?;
    Ok(())
}

/// Unstage a single hunk by piping the forward staged-diff patch to
/// `git apply --cached -R`. We let git handle the reversal rather than
/// computing it manually, which avoids line-number drift across multi-hunk files.
pub fn unstage_hunk(repo: &Repository, path: &str, hunk: &Hunk) -> Result<(), AppError> {
    let workdir = repo
        .workdir()
        .ok_or_else(|| AppError::Invalid("bare repository".into()))?;

    let patch = build_patch(path, hunk);

    let mut child = Command::new("git")
        .args(["apply", "--cached", "-R"])
        .current_dir(workdir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(patch.as_bytes())?;
    }

    let out = child.wait_with_output()?;
    if !out.status.success() {
        let msg = String::from_utf8_lossy(&out.stderr);
        return Err(AppError::Invalid(format!("unstage failed: {}", msg.trim())));
    }
    Ok(())
}

pub fn delete_untracked_file(repo: &Repository, path: &str) -> Result<(), AppError> {
    let workdir = repo
        .workdir()
        .ok_or_else(|| AppError::Invalid("bare repository".into()))?;
    let full = workdir.join(path);
    std::fs::remove_file(&full)?;
    Ok(())
}

pub fn discard_hunk(repo: &Repository, path: &str, hunk: &Hunk) -> Result<(), AppError> {
    let patch = build_patch(path, hunk);
    let diff = git2::Diff::from_buffer(patch.as_bytes())?;
    repo.apply(&diff, ApplyLocation::WorkDir, None)?;
    Ok(())
}

fn build_patch(path: &str, hunk: &Hunk) -> String {
    use crate::git::repo::LineKind;

    let mut out = String::new();
    out.push_str(&format!("diff --git a/{path} b/{path}\n"));
    out.push_str(&format!("--- a/{path}\n"));
    out.push_str(&format!("+++ b/{path}\n"));
    out.push_str(&format!(
        "@@ -{},{} +{},{} @@\n",
        hunk.old_start, hunk.old_lines, hunk.new_start, hunk.new_lines
    ));
    for line in &hunk.lines {
        let prefix = match line.kind {
            LineKind::Added => '+',
            LineKind::Removed => '-',
            LineKind::Context => ' ',
        };
        out.push(prefix);
        out.push_str(&line.content);
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::repo::{
        diff_for_file, list_changed_files, parse_conflict_markers, staged_diff_for_file,
    };
    use std::fs;
    use std::path::Path;
    use tempfile::TempDir;

    fn make_repo_with_commit(content: &str) -> (TempDir, Repository) {
        let dir = TempDir::new().unwrap();
        let repo = Repository::init(dir.path()).unwrap();
        let mut cfg = repo.config().unwrap();
        cfg.set_str("user.name", "Test").unwrap();
        cfg.set_str("user.email", "test@test.com").unwrap();

        let fpath = dir.path().join("file.txt");
        fs::write(&fpath, content).unwrap();
        let mut idx = repo.index().unwrap();
        idx.add_path(Path::new("file.txt")).unwrap();
        idx.write().unwrap();

        {
            let sig = repo.signature().unwrap();
            let tree_id = {
                let mut idx = repo.index().unwrap();
                idx.write_tree().unwrap()
            };
            let tree = repo.find_tree(tree_id).unwrap();
            repo.commit(Some("HEAD"), &sig, &sig, "init", &tree, &[])
                .unwrap();
        }

        (dir, repo)
    }

    #[test]
    fn stage_and_unstage_file() {
        let (dir, repo) = make_repo_with_commit("original\n");
        fs::write(dir.path().join("file.txt"), "modified\n").unwrap();

        stage_file(&repo, "file.txt").unwrap();
        let files = list_changed_files(&repo).unwrap();
        assert!(files.iter().any(|f| f.path == "file.txt" && f.staged));

        unstage_file(&repo, "file.txt").unwrap();
        let files = list_changed_files(&repo).unwrap();
        assert!(files.iter().any(|f| f.path == "file.txt" && !f.staged));
    }

    #[test]
    fn stage_hunk_applies_to_index() {
        let (dir, repo) = make_repo_with_commit("line1\nline2\nline3\n");
        fs::write(dir.path().join("file.txt"), "line1\nchanged\nline3\n").unwrap();

        let hunks = diff_for_file(&repo, "file.txt").unwrap();
        assert!(!hunks.is_empty());
        stage_hunk(&repo, "file.txt", &hunks[0]).unwrap();

        let files = list_changed_files(&repo).unwrap();
        assert!(files.iter().any(|f| f.path == "file.txt" && f.staged));
    }

    #[test]
    fn stage_deleted_file_removes_from_index() {
        let (dir, repo) = make_repo_with_commit("content\n");
        let fpath = dir.path().join("file.txt");

        // Delete the file from the working tree
        fs::remove_file(&fpath).unwrap();

        let files = list_changed_files(&repo).unwrap();
        assert!(
            files.iter().any(|f| f.path == "file.txt" && f.unstaged),
            "deleted file should appear as unstaged before staging"
        );

        // stage_file should not crash and should mark the deletion in the index
        stage_file(&repo, "file.txt").unwrap();

        let files = list_changed_files(&repo).unwrap();
        assert!(
            files.iter().any(|f| f.path == "file.txt" && f.staged),
            "deleted file should be staged after stage_file"
        );
    }

    #[test]
    fn stage_deleted_file_unstage_restores_index() {
        let (dir, repo) = make_repo_with_commit("content\n");
        let fpath = dir.path().join("file.txt");

        fs::remove_file(&fpath).unwrap();
        stage_file(&repo, "file.txt").unwrap();

        let files = list_changed_files(&repo).unwrap();
        assert!(
            files.iter().any(|f| f.path == "file.txt" && f.staged),
            "deletion should be staged"
        );

        unstage_file(&repo, "file.txt").unwrap();

        let files = list_changed_files(&repo).unwrap();
        let entry = files.iter().find(|f| f.path == "file.txt");
        assert!(
            entry.map_or(true, |f| !f.staged),
            "file should not be staged after unstaging"
        );
    }

    #[test]
    fn delete_untracked_removes_file() {
        let (dir, repo) = make_repo_with_commit("original\n");
        let untracked = dir.path().join("new_file.txt");
        fs::write(&untracked, "hello\n").unwrap();

        let files = list_changed_files(&repo).unwrap();
        assert!(files
            .iter()
            .any(|f| f.path == "new_file.txt"
                && f.status == crate::git::repo::FileStatus::Untracked));

        delete_untracked_file(&repo, "new_file.txt").unwrap();

        assert!(!untracked.exists());
        let files = list_changed_files(&repo).unwrap();
        assert!(!files.iter().any(|f| f.path == "new_file.txt"));
    }

    #[test]
    fn unstage_hunk_restores_index() {
        let (dir, repo) = make_repo_with_commit("line1\nline2\nline3\n");
        fs::write(dir.path().join("file.txt"), "line1\nchanged\nline3\n").unwrap();

        // Stage the hunk first
        let hunks = diff_for_file(&repo, "file.txt").unwrap();
        stage_hunk(&repo, "file.txt", &hunks[0]).unwrap();
        let files = list_changed_files(&repo).unwrap();
        assert!(files.iter().any(|f| f.path == "file.txt" && f.staged));

        // Now unstage it using the staged hunk
        let staged = staged_diff_for_file(&repo, "file.txt").unwrap();
        assert!(!staged.is_empty());
        unstage_hunk(&repo, "file.txt", &staged[0]).unwrap();

        let files = list_changed_files(&repo).unwrap();
        assert!(files.iter().any(|f| f.path == "file.txt" && !f.staged));
    }

    // ── conflict resolution ───────────────────────────────────────────────

    fn write_conflict_file(dir: &TempDir) -> (String, Vec<ConflictBlock>) {
        let content = "before\n<<<<<<< HEAD\nours_line\n=======\ntheirs_line\n>>>>>>> x\nafter\n";
        fs::write(dir.path().join("conflict.txt"), content).unwrap();
        let blocks = parse_conflict_markers(content);
        (content.to_string(), blocks)
    }

    #[test]
    fn resolve_conflict_ours_writes_correct_content() {
        let (dir, repo) = make_repo_with_commit("original\n");
        let (_, blocks) = write_conflict_file(&dir);

        resolve_conflict_block(&repo, "conflict.txt", &blocks, 0, ConflictSide::Ours).unwrap();

        let result = fs::read_to_string(dir.path().join("conflict.txt")).unwrap();
        assert!(result.contains("ours_line"), "should contain ours line");
        assert!(!result.contains("theirs_line"), "should not contain theirs line");
        assert!(!result.contains("<<<<<<<"), "conflict markers should be gone");
        assert!(result.contains("before"), "context before conflict should remain");
        assert!(result.contains("after"), "context after conflict should remain");
    }

    #[test]
    fn resolve_conflict_theirs_writes_correct_content() {
        let (dir, repo) = make_repo_with_commit("original\n");
        let (_, blocks) = write_conflict_file(&dir);

        resolve_conflict_block(&repo, "conflict.txt", &blocks, 0, ConflictSide::Theirs).unwrap();

        let result = fs::read_to_string(dir.path().join("conflict.txt")).unwrap();
        assert!(!result.contains("ours_line"), "should not contain ours line");
        assert!(result.contains("theirs_line"), "should contain theirs line");
        assert!(!result.contains("<<<<<<<"), "conflict markers should be gone");
    }

    #[test]
    fn resolve_conflict_both_includes_all_lines() {
        let (dir, repo) = make_repo_with_commit("original\n");
        let (_, blocks) = write_conflict_file(&dir);

        resolve_conflict_block(&repo, "conflict.txt", &blocks, 0, ConflictSide::Both).unwrap();

        let result = fs::read_to_string(dir.path().join("conflict.txt")).unwrap();
        assert!(result.contains("ours_line"), "should contain ours line");
        assert!(result.contains("theirs_line"), "should contain theirs line");
        assert!(!result.contains("<<<<<<<"), "conflict markers should be gone");
    }

    #[test]
    fn resolve_conflict_second_block() {
        let (dir, repo) = make_repo_with_commit("original\n");
        let content = concat!(
            "<<<<<<< HEAD\na\n=======\nb\n>>>>>>> x\n",
            "middle\n",
            "<<<<<<< HEAD\nc\n=======\nd\n>>>>>>> x\n"
        );
        fs::write(dir.path().join("conflict.txt"), content).unwrap();
        let blocks = parse_conflict_markers(content);
        assert_eq!(blocks.len(), 2);

        // Resolve only the second block with "theirs"
        resolve_conflict_block(&repo, "conflict.txt", &blocks, 1, ConflictSide::Theirs).unwrap();

        let result = fs::read_to_string(dir.path().join("conflict.txt")).unwrap();
        assert!(result.contains("d"), "should contain theirs for second block");
        // First block still has conflict markers
        assert!(result.contains("<<<<<<<"), "first block conflict markers should remain");
    }

    #[test]
    fn resolve_conflict_out_of_range_returns_error() {
        let (dir, repo) = make_repo_with_commit("original\n");
        let (_, blocks) = write_conflict_file(&dir);

        let result = resolve_conflict_block(&repo, "conflict.txt", &blocks, 99, ConflictSide::Ours);
        assert!(result.is_err(), "out-of-range index should return an error");
    }
}
