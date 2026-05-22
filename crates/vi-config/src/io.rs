use std::path::Path;
use std::sync::Arc;

use crate::error::ConfigError;
use crate::migration;
use crate::schema::{Config, CURRENT_VERSION};
use crate::validation::validate;

/// Load and validate a config file from `path`.
///
/// Applies any pending schema migrations before validation.
pub fn load(path: &Path) -> Result<Config, ConfigError> {
    let content = std::fs::read_to_string(path)?;
    load_str(&content)
}

/// Parse, migrate, and validate a config from a TOML string.
pub fn load_str(s: &str) -> Result<Config, ConfigError> {
    let mut value: toml::Value = toml::from_str(s)?;

    let version = value
        .get("version")
        .and_then(|v| v.as_integer())
        .map(|v| v as u32)
        .unwrap_or(0);

    if version > CURRENT_VERSION {
        return Err(ConfigError::UnsupportedVersion {
            found: version,
            max: CURRENT_VERSION,
        });
    }

    if version < CURRENT_VERSION {
        value = migration::apply(value, version)?;
    }

    let toml_str = toml::to_string(&value)?;
    let config: Config = toml::from_str(&toml_str)?;
    validate(&config)?;
    Ok(config)
}

/// Serialize `config` to a TOML string.
pub fn to_toml_string(config: &Config) -> Result<String, ConfigError> {
    Ok(toml::to_string_pretty(config)?)
}

/// Write `config` to `path` as TOML.
pub fn save(path: &Path, config: &Config) -> Result<(), ConfigError> {
    let s = to_toml_string(config)?;
    std::fs::write(path, s)?;
    Ok(())
}

/// Export `config` as a JSON string (requires the `json` feature).
#[cfg(feature = "json")]
pub fn to_json_string(config: &Config) -> Result<String, ConfigError> {
    Ok(serde_json::to_string_pretty(config)?)
}

/// Import a `Config` from a JSON string (requires the `json` feature).
#[cfg(feature = "json")]
pub fn from_json_str(s: &str) -> Result<Config, ConfigError> {
    let config: Config = serde_json::from_str(s)?;
    validate(&config)?;
    Ok(config)
}

/// Shared, cheaply cloneable reference to a config — the type emitted by the
/// watch channel.
pub type SharedConfig = Arc<Config>;

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_default_config() {
        let cfg = Config::default();
        let s = to_toml_string(&cfg).unwrap();
        let cfg2 = load_str(&s).unwrap();
        assert_eq!(cfg.active_profile, cfg2.active_profile);
        assert_eq!(cfg.version, cfg2.version);
    }

    #[test]
    fn load_minimal_toml() {
        let s = r#"
version = 1
active_profile = "default"
[profiles.default]
"#;
        let cfg = load_str(s).unwrap();
        assert_eq!(cfg.active_profile, "default");
    }

    #[test]
    fn future_version_rejected() {
        let s = "version = 999\nactive_profile = \"default\"\n[profiles.default]\n";
        let err = load_str(s).unwrap_err();
        assert!(matches!(err, ConfigError::UnsupportedVersion { .. }));
    }

    #[test]
    fn missing_active_profile_rejected() {
        let s = r#"
version = 1
active_profile = "nonexistent"
[profiles.default]
"#;
        let err = load_str(s).unwrap_err();
        assert!(matches!(err, ConfigError::ProfileNotFound(_)));
    }

    #[test]
    fn v0_migration_on_load() {
        let s = "input_method = \"telex\"\n";
        let cfg = load_str(s).unwrap();
        assert_eq!(cfg.version, 1);
        assert!(cfg.profiles.contains_key("default"));
    }
}
