use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Git error: {0}")]
    Git(#[from] git2::Error),

    #[error("Config parse error: {0}")]
    Toml(#[from] toml::de::Error),

    #[error("Config serialize error: {0}")]
    TomlSer(#[from] toml::ser::Error),

    #[error("Theme parse error: {0}")]
    Yaml(#[from] serde_yaml_ng::Error),

    #[allow(dead_code)]
    #[error("{0}")]
    Invalid(String),
}
