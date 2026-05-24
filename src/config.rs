use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use crate::error::AppError;

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Preferences {
    pub theme: Option<String>,
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
