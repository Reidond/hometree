use std::path::PathBuf;

#[derive(thiserror::Error, Debug)]
pub enum HometreeError {
    #[error("unable to resolve XDG base directories")]
    NoBaseDirs,
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("toml serialization error: {0}")]
    TomlSer(#[from] toml::ser::Error),
    #[error("toml deserialization error: {0}")]
    TomlDe(#[from] toml::de::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("glob error: {0}")]
    Glob(#[from] globset::Error),
    #[error("git error: {0}")]
    Git(#[from] crate::git::GitError),
    #[error("invalid path: {0}")]
    InvalidPath(PathBuf),
    #[error("config validation error: {0}")]
    Config(String),
}

pub type Result<T> = std::result::Result<T, HometreeError>;
