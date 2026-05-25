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
    pub fn load(config_dir: &Path) -> Self {
        let path = config_dir.join("config.toml");
        let Ok(content) = std::fs::read_to_string(&path) else {
            return Self::default();
        };
        toml::from_str(&content).unwrap_or_else(|e| {
            eprintln!("warning: could not parse config.toml: {e}");
            Self::default()
        })
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
        let prefs = Preferences::load(dir.path());
        assert!(prefs.theme.is_none());
        assert!(!prefs.transparent);
    }

    #[test]
    fn save_then_load_roundtrip() {
        let dir = TempDir::new().unwrap();
        let prefs = Preferences {
            theme: Some("Catppuccin Mocha".into()),
            transparent: true,
        };
        prefs.save(dir.path()).unwrap();
        let loaded = Preferences::load(dir.path());
        assert_eq!(loaded.theme.as_deref(), Some("Catppuccin Mocha"));
        assert!(loaded.transparent);
    }

    #[test]
    fn save_transparent_false_roundtrips() {
        let dir = TempDir::new().unwrap();
        let prefs = Preferences { theme: None, transparent: false };
        prefs.save(dir.path()).unwrap();
        let loaded = Preferences::load(dir.path());
        assert!(loaded.theme.is_none());
        assert!(!loaded.transparent);
    }

    #[test]
    fn load_returns_default_on_invalid_toml() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("config.toml"), "not valid toml !!!").unwrap();
        let prefs = Preferences::load(dir.path());
        assert!(prefs.theme.is_none());
        assert!(!prefs.transparent);
    }

    #[test]
    fn config_dir_returns_gitish_subdir() {
        let dir = config_dir();
        assert!(dir.ends_with("gitish"));
    }
}
