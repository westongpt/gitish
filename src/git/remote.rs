use std::process::Command;

use git2::Repository;

use crate::error::AppError;

pub struct RemoteResult {
    pub success: bool,
    pub output: String,
}

pub fn push(repo: &Repository) -> Result<RemoteResult, AppError> {
    run_git(repo, &["push"])
}

pub fn pull(repo: &Repository) -> Result<RemoteResult, AppError> {
    run_git(repo, &["pull"])
}

fn run_git(repo: &Repository, args: &[&str]) -> Result<RemoteResult, AppError> {
    let workdir = repo
        .workdir()
        .ok_or_else(|| AppError::Invalid("cannot push/pull a bare repository".into()))?;

    let out = Command::new("git")
        .args(args)
        .current_dir(workdir)
        .output()?;

    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);

    // git writes progress to stderr and results to stdout; combine both
    let combined = match (stdout.trim(), stderr.trim()) {
        ("", s) => s.to_string(),
        (o, "") => o.to_string(),
        (o, s) => format!("{o} {s}"),
    };

    // collapse to a single status line for the status bar
    let summary = combined
        .lines()
        .filter(|l| !l.trim().is_empty())
        .last()
        .unwrap_or(if out.status.success() { "Done" } else { "Failed" })
        .to_string();

    Ok(RemoteResult {
        success: out.status.success(),
        output: summary,
    })
}
