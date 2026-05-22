//! Deprecated: use the vime-* equivalent crate instead.

pub mod error;
pub mod ibus_component;
pub mod io;
pub mod migration;
pub mod schema;
pub mod validation;
pub mod watcher;

pub use error::ConfigError;
pub use io::{load, load_str, save, to_toml_string, SharedConfig};
pub use ibus_component::{
    component_xml, resolve_ui_setup_path, IbusInstallPaths, AUTHOR, COMPONENT_DESCRIPTION,
    COMPONENT_NAME, ENGINE_DESCRIPTION, ENGINE_LANGUAGE, ENGINE_LONGNAME, ENGINE_NAME,
    ENGINE_RANK, HOMEPAGE, LICENSE, SYSTEM_DAEMON, SYSTEM_ICON, SYSTEM_UI, TEXTDOMAIN,
};
pub use schema::{
    AppOverride, CandidateOrientation, CompositionRule, Config, CustomMethod, IbusConfig,
    InputMethodKind, KeyBindings, Profile, SpellCheckProfile, TonePlacement, UiPreferences,
    CURRENT_VERSION,
};
pub use validation::validate;
pub use watcher::{watch, ConfigWatcher};

#[cfg(feature = "json")]
pub use io::{from_json_str, to_json_string};
