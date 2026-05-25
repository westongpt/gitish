use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use crate::error::AppError;

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Preferences {
    pub theme: Option<String>,
    #[serde(default)]
    pub transparent: bool,
}

impl Preferences {
    /// Load preferences from disk. Returns `(prefs, error_msg)` where `error_msg` is
    /// `Some(...)` when the file existed but could not be parsed. On parse failure the
    /// corrupt file is copied to `config.toml.bak` so it is not silently overwritten.
    pub fn load(config_dir: &Path) -> (Self, Option<String>) {
        let path = config_dir.join("config.toml");
        let Ok(content) = std::fs::read_to_string(&path) else {
            return (Self::default(), None);
        };
        match toml::from_str(&content) {
            Ok(prefs) => (prefs, None),
            Err(e) => {
                let _ = std::fs::copy(&path, config_dir.join("config.toml.bak"));
                (
                    Self::default(),
                    Some(format!(
                        "config.toml parse error (backed up to config.toml.bak): {e}"
                    )),
                )
            }
        }
    }

    pub fn save(&self, config_dir: &Path) -> Result<(), AppError> {
        std::fs::create_dir_all(config_dir)?;
        let content = toml::to_string(self)?;
        let tmp = config_dir.join("config.toml.tmp");
        let dest = config_dir.join("config.toml");
        std::fs::write(&tmp, content)?;
        std::fs::rename(&tmp, &dest)?;
        Ok(())
    }
}

pub fn config_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("gitish")
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn load_returns_default_when_file_missing() {
        let dir = TempDir::new().unwrap();
        let (prefs, err) = Preferences::load(dir.path());
        assert!(prefs.theme.is_none());
        assert!(!prefs.transparent);
        assert!(err.is_none());
    }

    #[test]
    fn save_then_load_roundtrip() {
        let dir = TempDir::new().unwrap();
        let prefs = Preferences {
            theme: Some("Catppuccin Mocha".into()),
            transparent: true,
        };
        prefs.save(dir.path()).unwrap();
        let (loaded, err) = Preferences::load(dir.path());
        assert_eq!(loaded.theme.as_deref(), Some("Catppuccin Mocha"));
        assert!(loaded.transparent);
        assert!(err.is_none());
    }

    #[test]
    fn save_transparent_false_roundtrips() {
        let dir = TempDir::new().unwrap();
        let prefs = Preferences { theme: None, transparent: false };
        prefs.save(dir.path()).unwrap();
        let (loaded, err) = Preferences::load(dir.path());
        assert!(loaded.theme.is_none());
        assert!(!loaded.transparent);
        assert!(err.is_none());
    }

    #[test]
    fn load_returns_default_and_error_on_invalid_toml() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("config.toml"), "not valid toml !!!").unwrap();
        let (prefs, err) = Preferences::load(dir.path());
        assert!(prefs.theme.is_none());
        assert!(!prefs.transparent);
        assert!(err.is_some(), "must return an error message for invalid toml");
        let msg = err.unwrap();
        assert!(msg.contains("config.toml.bak"), "error message must mention the backup path");
    }

    #[test]
    fn load_backs_up_corrupt_config() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("config.toml"), "not valid toml !!!").unwrap();
        let _ = Preferences::load(dir.path());
        assert!(
            dir.path().join("config.toml.bak").exists(),
            "corrupt config.toml must be copied to config.toml.bak"
        );
        let bak = std::fs::read_to_string(dir.path().join("config.toml.bak")).unwrap();
        assert_eq!(bak, "not valid toml !!!", "backup must contain original corrupt content");
    }

    #[test]
    fn config_dir_returns_gitish_subdir() {
        let dir = config_dir();
        assert!(dir.ends_with("gitish"));
    }
}
