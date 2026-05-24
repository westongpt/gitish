use git2::Repository;

use crate::error::AppError;

pub fn create_commit(repo: &Repository, title: &str, body: &str) -> Result<git2::Oid, AppError> {
    let sig = repo.signature()?;
    let mut index = repo.index()?;
    let tree_id = index.write_tree()?;
    let tree = repo.find_tree(tree_id)?;

    let parent_commit = repo
        .head()
        .ok()
        .and_then(|h| h.peel_to_commit().ok());

    let message = if body.trim().is_empty() {
        title.to_string()
    } else {
        format!("{title}\n\n{}", body.trim())
    };

    let parents: Vec<&git2::Commit> = parent_commit.iter().collect();
    let oid = repo.commit(Some("HEAD"), &sig, &sig, &message, &tree, &parents)?;
    Ok(oid)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::Path;
    use tempfile::TempDir;

    fn make_repo_with_staged_file() -> (TempDir, Repository) {
        let dir = TempDir::new().unwrap();
        let repo = Repository::init(dir.path()).unwrap();
        let mut cfg = repo.config().unwrap();
        cfg.set_str("user.name", "Test").unwrap();
        cfg.set_str("user.email", "test@test.com").unwrap();

        fs::write(dir.path().join("a.txt"), "hello\n").unwrap();
        let mut idx = repo.index().unwrap();
        idx.add_path(Path::new("a.txt")).unwrap();
        idx.write().unwrap();
        (dir, repo)
    }

    #[test]
    fn commit_with_title_only() {
        let (_dir, repo) = make_repo_with_staged_file();
        let oid = create_commit(&repo, "initial commit", "").unwrap();
        let commit = repo.find_commit(oid).unwrap();
        assert_eq!(commit.message().unwrap(), "initial commit");
    }

    #[test]
    fn commit_with_body() {
        let (_dir, repo) = make_repo_with_staged_file();
        let oid = create_commit(&repo, "feat: add thing", "longer description").unwrap();
        let commit = repo.find_commit(oid).unwrap();
        assert!(commit.message().unwrap().contains("longer description"));
    }
}
