use crate::error::ConfigError;
use crate::schema::Config;

/// Validate a parsed `Config`, returning human-readable errors.
pub fn validate(config: &Config) -> Result<(), ConfigError> {
    if config.profiles.is_empty() {
        return Err(ConfigError::Validation(
            "at least one profile must be defined".to_string(),
        ));
    }

    if !config.profiles.contains_key(&config.active_profile) {
        return Err(ConfigError::ProfileNotFound(config.active_profile.clone()));
    }

    for (name, profile) in &config.profiles {
        if name.is_empty() {
            return Err(ConfigError::Validation(
                "profile name must not be empty".to_string(),
            ));
        }
        if let Some(size) = profile.ui.font_size {
            if size <= 0.0 || !size.is_finite() {
                return Err(ConfigError::Validation(format!(
                    "profile '{name}': font_size must be a positive finite number, got {size}"
                )));
            }
        }
        validate_key_binding(
            name,
            "toggle_input_method",
            &profile.key_bindings.toggle_input_method,
        )?;
        validate_key_binding(name, "next_profile", &profile.key_bindings.next_profile)?;
        validate_key_binding(name, "prev_profile", &profile.key_bindings.prev_profile)?;
        validate_key_binding(name, "reset", &profile.key_bindings.reset)?;
    }

    for ov in &config.app_overrides {
        if ov.app_name_pattern.is_empty() {
            return Err(ConfigError::Validation(
                "app_override app_name_pattern must not be empty".to_string(),
            ));
        }
    }

    Ok(())
}

fn validate_key_binding(
    profile: &str,
    field: &str,
    binding: &Option<String>,
) -> Result<(), ConfigError> {
    if let Some(b) = binding {
        if b.is_empty() {
            return Err(ConfigError::Validation(format!(
                "profile '{profile}': key binding '{field}' must not be an empty string; \
                 omit the field to disable"
            )));
        }
    }
    Ok(())
}
