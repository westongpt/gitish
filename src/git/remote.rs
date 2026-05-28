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
/// keep only one meaningful line so the status bar stays tidy.
///
/// On success the last non-empty line is the result (e.g. `main -> main`).
/// On failure the last line is usually noise like
/// `error: failed to push some refs to '...'`, while the actionable cause
/// (auth failure, non-fast-forward, hook rejection) appears earlier — so we
/// prefer the first line flagged by git as an error, falling back to last().
pub(crate) fn summarize(stdout: &[u8], stderr: &[u8], success: bool) -> String {
    let stdout = String::from_utf8_lossy(stdout);
    let stderr = String::from_utf8_lossy(stderr);

    let combined = match (stdout.trim(), stderr.trim()) {
        ("", s) => s.to_string(),
        (o, "") => o.to_string(),
        (o, s) => format!("{o} {s}"),
    };

    let non_empty = || combined.lines().filter(|l| !l.trim().is_empty());

    if !success {
        if let Some(line) = non_empty().find(|l| is_error_line(l)) {
            return line.trim().to_string();
        }
    }

    non_empty()
        .last()
        .unwrap_or(if success { "Done" } else { "Failed" })
        .trim()
        .to_string()
}

/// Whether a git output line is one git itself flags as an error/rejection.
fn is_error_line(line: &str) -> bool {
    let l = line.trim_start();
    l.starts_with("error:") || l.starts_with("fatal:") || l.starts_with("! [")
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

    #[test]
    fn summarize_non_fast_forward_prefers_rejected_line() {
        let stderr = b"To github.com:foo/bar.git\n \
            ! [rejected]        main -> main (fetch first)\n\
            error: failed to push some refs to 'github.com:foo/bar.git'\n\
            hint: Updates were rejected because the remote contains work\n\
            hint: you do not have locally.";
        let out = summarize(b"", stderr, false);
        assert_eq!(out, "! [rejected]        main -> main (fetch first)");
    }

    #[test]
    fn summarize_auth_failure_prefers_fatal_line() {
        let stderr = b"fatal: Authentication failed for 'https://github.com/foo/bar.git/'";
        let out = summarize(b"", stderr, false);
        assert_eq!(out, "fatal: Authentication failed for 'https://github.com/foo/bar.git/'");
    }

    #[test]
    fn summarize_pre_push_hook_rejection_prefers_error_line() {
        let stderr = b"Running pre-push checks...\n\
            lint failed: trailing whitespace\n\
            error: failed to push some refs to 'git@github.com:foo/bar.git'";
        let out = summarize(b"", stderr, false);
        assert_eq!(out, "error: failed to push some refs to 'git@github.com:foo/bar.git'");
    }

    #[test]
    fn summarize_branch_protection_prefers_remote_rejected_line() {
        let stderr = b"To github.com:foo/bar.git\n \
            ! [remote rejected] main -> main (protected branch hook declined)\n\
            error: failed to push some refs to 'github.com:foo/bar.git'";
        let out = summarize(b"", stderr, false);
        assert_eq!(
            out,
            "! [remote rejected] main -> main (protected branch hook declined)"
        );
    }

    #[test]
    fn summarize_large_file_prefers_first_error_line() {
        let stderr = b"remote: error: GH001: Large files detected.\n\
            remote: error: File big.bin is 120.00 MB; exceeds 100.00 MB limit\n \
            ! [remote rejected] main -> main (pre-receive hook declined)\n\
            error: failed to push some refs to 'github.com:foo/bar.git'";
        let out = summarize(b"", stderr, false);
        assert_eq!(out, "! [remote rejected] main -> main (pre-receive hook declined)");
    }

    #[test]
    fn summarize_failure_without_error_line_falls_back_to_last() {
        let stderr = b"some unexpected output\nanother line";
        let out = summarize(b"", stderr, false);
        assert_eq!(out, "another line");
    }

    #[test]
    fn summarize_success_ignores_error_heuristic() {
        let stderr = b"error: this looks like an error\nmain -> main";
        let out = summarize(b"", stderr, true);
        assert_eq!(out, "main -> main");
    }
}
