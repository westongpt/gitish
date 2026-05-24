use std::path::PathBuf;

use crate::error::AppError;

#[derive(Debug, Clone, PartialEq)]
pub enum InitialMode {
    ThemePicker,
}

impl InitialMode {
    fn parse(s: &str) -> Result<Self, AppError> {
        match s {
            "theme-picker" => Ok(Self::ThemePicker),
            other => Err(AppError::Invalid(format!(
                "Unknown --open value '{other}'. Valid values: theme-picker"
            ))),
        }
    }
}

pub struct Args {
    pub path: Option<PathBuf>,
    pub open: Option<InitialMode>,
}

const HELP: &str = "\
gitish — interactive git staging TUI

USAGE:
    gitish [OPTIONS]

CLI OPTIONS:
    --path <path>
        Open the git repository at <path> instead of discovering one from the current
        working directory. Useful when running gitish from a script or another directory.
        Example: gitish --path ~/projects/myrepo

    --open <state>
        Launch directly into a specific UI state instead of the normal file list.
        Valid values:
          theme-picker   Open the theme picker immediately on startup
        Example: gitish --open theme-picker

    --help, -h, -?
        Print this help message and exit.

CONFIG:
    Config file: ~/.config/gitish/config.toml

    theme = \"<name>\"
        The name of the active color theme. Must match a theme file in
        ~/.config/gitish/themes/ or one of the built-in Catppuccin variants.
        Built-in values: Catppuccin Mocha, Catppuccin Macchiato, Catppuccin Frappe,
                         Catppuccin Latte
        Example: theme = \"Catppuccin Mocha\"
        Default: Catppuccin Mocha (first run)

    transparent = <bool>
        When true, gitish renders with a transparent background so your terminal's
        compositor transparency shows through. When false (default), uses the theme's
        background color.
        Example: transparent = true
        Default: false
";

pub fn parse_args() -> Result<Option<Args>, AppError> {
    let raw: Vec<String> = std::env::args().skip(1).collect();
    let mut path: Option<PathBuf> = None;
    let mut open: Option<InitialMode> = None;
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
            "--open" => {
                i += 1;
                let value = raw.get(i).ok_or_else(|| {
                    AppError::Invalid("--open requires a state argument".into())
                })?;
                open = Some(InitialMode::parse(value)?);
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

    Ok(Some(Args { path, open }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Result<Option<Args>, AppError> {
        let raw: Vec<String> = args.iter().map(|s| s.to_string()).collect();
        let mut path: Option<PathBuf> = None;
        let mut open: Option<InitialMode> = None;
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
                "--open" => {
                    i += 1;
                    let value = raw.get(i).ok_or_else(|| {
                        AppError::Invalid("--open requires a state argument".into())
                    })?;
                    open = Some(InitialMode::parse(value)?);
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

        Ok(Some(Args { path, open }))
    }

    #[test]
    fn no_args_returns_default() {
        let result = parse(&[]).unwrap().unwrap();
        assert!(result.path.is_none());
        assert!(result.open.is_none());
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

    #[test]
    fn open_theme_picker_sets_mode() {
        let result = parse(&["--open", "theme-picker"]).unwrap().unwrap();
        assert_eq!(result.open, Some(InitialMode::ThemePicker));
    }

    #[test]
    fn open_missing_value_is_error() {
        let result = parse(&["--open"]);
        assert!(result.is_err());
    }

    #[test]
    fn open_unknown_value_is_error() {
        let result = parse(&["--open", "banana"]);
        assert!(result.is_err());
    }
}
