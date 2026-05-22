use crate::error::ConfigError;
use crate::schema::CURRENT_VERSION;

/// Apply all migrations needed to bring `value` from `from_version` up to
/// `CURRENT_VERSION`.  Migrations are applied in order, one step at a time.
pub fn apply(mut value: toml::Value, from_version: u32) -> Result<toml::Value, ConfigError> {
    let mut v = from_version;
    while v < CURRENT_VERSION {
        value = step(value, v)?;
        v += 1;
    }
    Ok(value)
}

fn step(value: toml::Value, from: u32) -> Result<toml::Value, ConfigError> {
    match from {
        0 => migrate_v0_to_v1(value),
        other => Err(ConfigError::Migration {
            version: other,
            message: format!("no migration defined from version {other}"),
        }),
    }
}

/// v0 → v1: add `version = 1` and wrap bare input_method strings inside a
/// profile table if no `profiles` key is present.
fn migrate_v0_to_v1(mut value: toml::Value) -> Result<toml::Value, ConfigError> {
    {
        let table = value.as_table_mut().ok_or_else(|| ConfigError::Migration {
            version: 0,
            message: "top-level value is not a TOML table".to_string(),
        })?;

        table.insert("version".to_string(), toml::Value::Integer(1));

        if !table.contains_key("profiles") {
            let mut default_profile = toml::value::Table::new();

            if let Some(method) = table.remove("input_method") {
                default_profile.insert("input_method".to_string(), method);
            } else {
                default_profile.insert(
                    "input_method".to_string(),
                    toml::Value::String("telex".to_string()),
                );
            }

            let mut profiles = toml::value::Table::new();
            profiles.insert("default".to_string(), toml::Value::Table(default_profile));
            table.insert("profiles".to_string(), toml::Value::Table(profiles));
        }

        if !table.contains_key("active_profile") {
            table.insert(
                "active_profile".to_string(),
                toml::Value::String("default".to_string()),
            );
        }
    } // borrow of `value` via `table` ends here

    Ok(value)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn migrate_empty_v0_adds_version_and_profile() {
        let v0: toml::Value = toml::from_str("").expect("parse");
        let v1 = apply(v0, 0).expect("migrate");
        assert_eq!(v1.get("version").and_then(|v| v.as_integer()), Some(1));
        assert!(v1.get("profiles").is_some());
        assert_eq!(
            v1.get("active_profile").and_then(|v| v.as_str()),
            Some("default")
        );
    }

    #[test]
    fn already_current_no_op() {
        let current: toml::Value =
            toml::from_str("version = 1\nactive_profile = \"default\"\n[profiles.default]\ninput_method = \"telex\"\n")
                .expect("parse");
        let result = apply(current, 1).expect("migrate");
        assert_eq!(result.get("version").and_then(|v| v.as_integer()), Some(1));
    }
}
