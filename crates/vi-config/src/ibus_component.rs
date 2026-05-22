//! IBus component XML — single source of truth for engine metadata and packaging.

use std::path::{Path, PathBuf};

/// D-Bus / IBus engine identifier (also used in GNOME input sources).
pub const ENGINE_NAME: &str = "vhttechkey";

/// IBus component D-Bus name.
pub const COMPONENT_NAME: &str = "org.freedesktop.IBus.vhttechkey";

pub const COMPONENT_DESCRIPTION: &str = "VHTTechKey — bộ gõ tiếng Việt";
pub const ENGINE_LONGNAME: &str = "VHTTechKey";
pub const ENGINE_DESCRIPTION: &str =
    "Bộ gõ tiếng Việt — chọn kiểu gõ (Telex/VNI/VIQR) trong cài đặt";
pub const ENGINE_LANGUAGE: &str = "vi";
pub const ENGINE_LAYOUT: &str = "default";
pub const ENGINE_RANK: u32 = 99;
pub const LICENSE: &str = "GPLv3+";
pub const AUTHOR: &str = "VHT Tech <vinhhp@vhttech.com>";
pub const HOMEPAGE: &str = "https://git.hocitvn.com/vhttech/miliondolar/vhttechkey";
pub const TEXTDOMAIN: &str = "vhttechkey";

pub const SYSTEM_DAEMON: &str = "/usr/lib/vhttechkey/vi-daemon";
pub const SYSTEM_UI: &str = "/usr/lib/vhttechkey/vi-ui";
pub const SYSTEM_ICON: &str = "/usr/share/vhttechkey/icons/vi.svg";

/// Install paths used when rendering component XML.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IbusInstallPaths {
    pub exec: String,
    pub setup: String,
    pub icon: String,
}

impl IbusInstallPaths {
    /// Paths used by `packaging/ibus/vhttechkey-daemon.xml` and system install.
    pub fn system_default() -> Self {
        Self {
            exec: SYSTEM_DAEMON.to_string(),
            setup: SYSTEM_UI.to_string(),
            icon: SYSTEM_ICON.to_string(),
        }
    }

    /// Paths for a user-local install (daemon and UI next to each other).
    pub fn from_binaries(daemon: &Path, ui: &Path) -> Self {
        Self {
            exec: daemon.display().to_string(),
            setup: ui.display().to_string(),
            icon: SYSTEM_ICON.to_string(),
        }
    }
}

/// Resolve the `vi-ui` binary for IBus `<setup>` / engine preferences.
pub fn resolve_ui_setup_path() -> String {
    if let Ok(p) = std::env::var("VHTTECHKEY_UI") {
        if !p.is_empty() {
            return p;
        }
    }
    for candidate in [SYSTEM_UI, "/usr/local/lib/vhttechkey/vi-ui"] {
        if Path::new(candidate).exists() {
            return candidate.to_string();
        }
    }
    if let Ok(home) = std::env::var("HOME") {
        let local = PathBuf::from(&home).join(".local/bin/vi-ui");
        if local.exists() {
            return local.display().to_string();
        }
    }
    if let Ok(exe) = std::env::current_exe() {
        if exe.exists() {
            return exe.display().to_string();
        }
    }
    SYSTEM_UI.to_string()
}

/// Escape text for XML element content.
fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// Render the IBus component XML consumed by ibus-daemon and `ibus write-cache`.
pub fn component_xml(paths: &IbusInstallPaths) -> String {
    let version = env!("CARGO_PKG_VERSION");
    let author = xml_escape(AUTHOR);
    format!(
        r#"<?xml version="1.0" encoding="utf-8"?>
<component>
    <name>{COMPONENT_NAME}</name>
    <description>{COMPONENT_DESCRIPTION}</description>
    <exec>{exec}</exec>
    <version>{version}</version>
    <author>{author}</author>
    <license>{LICENSE}</license>
    <homepage>{HOMEPAGE}</homepage>
    <textdomain>{TEXTDOMAIN}</textdomain>
    <engines>
        <engine>
            <name>{ENGINE_NAME}</name>
            <longname>{ENGINE_LONGNAME}</longname>
            <description>{ENGINE_DESCRIPTION}</description>
            <language>{ENGINE_LANGUAGE}</language>
            <license>{LICENSE}</license>
            <author>{author}</author>
            <icon>{icon}</icon>
            <layout>{ENGINE_LAYOUT}</layout>
            <rank>{ENGINE_RANK}</rank>
            <setup>{setup}</setup>
        </engine>
    </engines>
</component>
"#,
        exec = paths.exec,
        setup = paths.setup,
        icon = paths.icon,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_engine_no_method_suffix() {
        let xml = component_xml(&IbusInstallPaths::system_default());
        assert!(xml.contains("<name>vhttechkey</name>"));
        assert!(!xml.contains("vhttechkey-telex"));
        assert!(!xml.contains("vhttechkey-vni"));
        assert!(!xml.contains("vhttechkey-viqr"));
    }

    #[test]
    fn includes_setup_for_preferences() {
        let xml = component_xml(&IbusInstallPaths::system_default());
        assert!(xml.contains(&format!("<setup>{SYSTEM_UI}</setup>")));
    }

    #[test]
    fn packaging_xml_matches_template() {
        let expected = include_str!("../../../packaging/ibus/vhttechkey-daemon.xml");
        let generated = component_xml(&IbusInstallPaths::system_default());
        assert_eq!(generated.trim(), expected.trim());
    }
}
