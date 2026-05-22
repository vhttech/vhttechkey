use thiserror::Error;

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("TOML parse error: {0}")]
    ParseToml(#[from] toml::de::Error),

    #[error("TOML serialize error: {0}")]
    SerializeToml(toml::ser::Error),

    #[error("validation error: {0}")]
    Validation(String),

    #[error("profile not found: '{0}'")]
    ProfileNotFound(String),

    #[error("unsupported config version {found} (max supported: {max})")]
    UnsupportedVersion { found: u32, max: u32 },

    #[error("migration error at version {version}: {message}")]
    Migration { version: u32, message: String },

    #[error("file watcher error: {0}")]
    Watch(notify::Error),

    #[cfg(feature = "json")]
    #[error("JSON error: {0}")]
    Json(serde_json::Error),
}

impl From<toml::ser::Error> for ConfigError {
    fn from(e: toml::ser::Error) -> Self {
        Self::SerializeToml(e)
    }
}

#[cfg(feature = "json")]
impl From<serde_json::Error> for ConfigError {
    fn from(e: serde_json::Error) -> Self {
        Self::Json(e)
    }
}
