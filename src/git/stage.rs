use std::path::Path;

use git2::{ApplyLocation, Repository};

use crate::error::AppError;
use crate::git::repo::Hunk;

pub fn stage_file(repo: &Repository, path: &str) -> Result<(), AppError> {
    let mut index = repo.index()?;
    index.add_path(Path::new(path))?;
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
            // No HEAD yet — remove from index entirely
            let mut index = repo.index()?;
            index.remove_path(Path::new(path))?;
            index.write()?;
        }
    }
    Ok(())
}

pub fn stage_hunk(repo: &Repository, path: &str, hunk: &Hunk) -> Result<(), AppError> {
    let patch = build_patch(path, hunk, false);
    let diff = git2::Diff::from_buffer(patch.as_bytes())?;
    repo.apply(&diff, ApplyLocation::Index, None)?;
    Ok(())
}

pub fn unstage_hunk(repo: &Repository, path: &str, hunk: &Hunk) -> Result<(), AppError> {
    let patch = build_patch(path, hunk, true);
    let diff = git2::Diff::from_buffer(patch.as_bytes())?;
    repo.apply(&diff, ApplyLocation::Index, None)?;
    Ok(())
}

pub fn discard_hunk(repo: &Repository, path: &str, hunk: &Hunk) -> Result<(), AppError> {
    let patch = build_patch(path, hunk, false);
    let diff = git2::Diff::from_buffer(patch.as_bytes())?;
    repo.apply(&diff, ApplyLocation::WorkDir, None)?;
    Ok(())
}

fn build_patch(path: &str, hunk: &Hunk, reverse: bool) -> String {
    use crate::git::repo::LineKind;

    let mut out = String::new();
    out.push_str(&format!("diff --git a/{path} b/{path}\n"));
    out.push_str(&format!("--- a/{path}\n"));
    out.push_str(&format!("+++ b/{path}\n"));

    if reverse {
        out.push_str(&format!(
            "@@ -{},{} +{},{} @@\n",
            hunk.new_start, hunk.new_lines, hunk.old_start, hunk.old_lines
        ));
        for line in &hunk.lines {
            let prefix = match line.kind {
                LineKind::Added => '-',
                LineKind::Removed => '+',
                LineKind::Context => ' ',
            };
            out.push(prefix);
            out.push_str(&line.content);
            out.push('\n');
        }
    } else {
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
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::repo::{diff_for_file, list_changed_files};
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
        let f = files.iter().find(|f| f.path == "file.txt").unwrap();
        assert!(f.staged);

        unstage_file(&repo, "file.txt").unwrap();
        let files = list_changed_files(&repo).unwrap();
        let f = files.iter().find(|f| f.path == "file.txt").unwrap();
        assert!(!f.staged);
    }

    #[test]
    fn stage_hunk_applies_to_index() {
        let (dir, repo) = make_repo_with_commit("line1\nline2\nline3\n");
        fs::write(dir.path().join("file.txt"), "line1\nchanged\nline3\n").unwrap();

        let hunks = diff_for_file(&repo, "file.txt").unwrap();
        assert!(!hunks.is_empty());
        stage_hunk(&repo, "file.txt", &hunks[0]).unwrap();

        let files = list_changed_files(&repo).unwrap();
        let f = files.iter().find(|f| f.path == "file.txt").unwrap();
        assert!(f.staged);
    }
}
