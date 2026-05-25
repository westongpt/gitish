use std::path::{Path, PathBuf};
use std::process::Command;

use crate::error::AppError;

pub struct RemoteResult {
    pub success: bool,
    pub output: String,
}

/// Blocking push — takes an owned path so it can run on a worker thread
/// without borrowing Repository across the thread boundary.
pub fn push_in_dir(workdir: PathBuf) -> Result<RemoteResult, AppError> {
    run_git(&workdir, &["push"])
}

/// Blocking pull — same threading rationale as `push_in_dir`.
pub fn pull_in_dir(workdir: PathBuf) -> Result<RemoteResult, AppError> {
    run_git(&workdir, &["pull"])
}

fn run_git(workdir: &Path, args: &[&str]) -> Result<RemoteResult, AppError> {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remote_result_fields() {
        let r = RemoteResult { success: true, output: "ok".into() };
        assert!(r.success);
        assert_eq!(r.output, "ok");
    }

    #[test]
    fn push_in_dir_and_pull_in_dir_are_send() {
        fn assert_send<T: Send>() {}
        // These functions return a Send type — verifies the signatures compile
        // for use across thread boundaries.
        assert_send::<Result<RemoteResult, AppError>>();
    }
}
