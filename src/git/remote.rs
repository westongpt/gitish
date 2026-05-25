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

    Ok(RemoteResult {
        success: out.status.success(),
        output: summarize(&out.stdout, &out.stderr, out.status.success()),
    })
}

/// Distil raw git stdout/stderr into a single status-bar line.
/// git writes progress to stderr and results to stdout; we combine both and
/// keep only the last non-empty line so the status bar stays tidy.
pub(crate) fn summarize(stdout: &[u8], stderr: &[u8], success: bool) -> String {
    let stdout = String::from_utf8_lossy(stdout);
    let stderr = String::from_utf8_lossy(stderr);

    let combined = match (stdout.trim(), stderr.trim()) {
        ("", s) => s.to_string(),
        (o, "") => o.to_string(),
        (o, s) => format!("{o} {s}"),
    };

    combined
        .lines()
        .filter(|l| !l.trim().is_empty())
        .last()
        .unwrap_or(if success { "Done" } else { "Failed" })
        .to_string()
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
        assert_send::<Result<RemoteResult, AppError>>();
    }

    #[test]
    fn summarize_success_with_stdout() {
        let out = summarize(b"Everything up-to-date", b"", true);
        assert_eq!(out, "Everything up-to-date");
    }

    #[test]
    fn summarize_failure_with_stderr() {
        let out = summarize(b"", b"error: failed to push some refs", false);
        assert_eq!(out, "error: failed to push some refs");
    }

    #[test]
    fn summarize_empty_output_success_returns_done() {
        let out = summarize(b"", b"", true);
        assert_eq!(out, "Done");
    }

    #[test]
    fn summarize_empty_output_failure_returns_failed() {
        let out = summarize(b"", b"", false);
        assert_eq!(out, "Failed");
    }

    #[test]
    fn summarize_multiline_stderr_returns_last_line() {
        let stderr = b"Counting objects: 3\nWriting objects: 100%\nmaster -> master";
        let out = summarize(b"", stderr, true);
        assert_eq!(out, "master -> master");
    }

    #[test]
    fn summarize_combines_stdout_and_stderr() {
        let out = summarize(b"result", b"progress", true);
        assert_eq!(out, "result progress");
    }

    #[test]
    fn summarize_skips_blank_lines() {
        let stderr = b"line one\n\n\nlast line\n\n";
        let out = summarize(b"", stderr, true);
        assert_eq!(out, "last line");
    }
}
