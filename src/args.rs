use std::path::PathBuf;

use crate::error::AppError;

pub struct Args {
    pub path: Option<PathBuf>,
}

const HELP: &str = "\
gitish — interactive git staging TUI

USAGE:
    gitish [OPTIONS]

OPTIONS:
    --path <path>    Open the git repository at <path> instead of the current directory
    --help, -h, -?   Print this help message and exit
";

pub fn parse_args() -> Result<Option<Args>, AppError> {
    let raw: Vec<String> = std::env::args().skip(1).collect();
    let mut path: Option<PathBuf> = None;
    let mut i = 0;

    while i < raw.len() {
        match raw[i].as_str() {
            "--help" | "-h" | "-?" => {
                print!("{}", HELP);
                return Ok(None);
            }
            "--path" => {
                i += 1;
                let value = raw.get(i).ok_or_else(|| {
                    AppError::Invalid("--path requires a directory argument".into())
                })?;
                path = Some(PathBuf::from(value));
            }
            other => {
                return Err(AppError::Invalid(format!(
                    "Unknown option '{}'. Run with --help for usage.",
                    other
                )));
            }
        }
        i += 1;
    }

    Ok(Some(Args { path }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Result<Option<Args>, AppError> {
        let raw: Vec<String> = args.iter().map(|s| s.to_string()).collect();
        let mut path: Option<PathBuf> = None;
        let mut i = 0;

        while i < raw.len() {
            match raw[i].as_str() {
                "--help" | "-h" | "-?" => return Ok(None),
                "--path" => {
                    i += 1;
                    let value = raw.get(i).ok_or_else(|| {
                        AppError::Invalid("--path requires a directory argument".into())
                    })?;
                    path = Some(PathBuf::from(value));
                }
                other => {
                    return Err(AppError::Invalid(format!(
                        "Unknown option '{}'. Run with --help for usage.",
                        other
                    )));
                }
            }
            i += 1;
        }

        Ok(Some(Args { path }))
    }

    #[test]
    fn no_args_returns_default() {
        let result = parse(&[]).unwrap().unwrap();
        assert!(result.path.is_none());
    }

    #[test]
    fn path_flag_sets_path() {
        let result = parse(&["--path", "/tmp/myrepo"]).unwrap().unwrap();
        assert_eq!(result.path, Some(PathBuf::from("/tmp/myrepo")));
    }

    #[test]
    fn help_long_returns_none() {
        let result = parse(&["--help"]).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn help_short_h_returns_none() {
        let result = parse(&["-h"]).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn help_question_mark_returns_none() {
        let result = parse(&["-?"]).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn unknown_flag_is_error() {
        let result = parse(&["--unknown"]);
        assert!(result.is_err());
    }

    #[test]
    fn path_flag_missing_value_is_error() {
        let result = parse(&["--path"]);
        assert!(result.is_err());
    }
}
