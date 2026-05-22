//! D-Bus diagnostics service for vi-daemon.
//!
//! Exposes an `org.freedesktop.vime.Diagnostics1` interface so that desktop
//! environments can query which Electron flags the daemon recommends and
//! retrieve structured daemon status for debugging.

use std::collections::HashMap;

use tracing::{debug, info, warn};
use zbus::interface;

/// D-Bus object that exposes IME diagnostics to desktop environments.
pub struct ImeDiagnostics {
    pub electron_pids: Vec<u32>,
    /// Current input method name (e.g. "Telex", "VNI", "VIQR").
    pub input_method: String,
    /// Backend name (e.g. "ibus", "wayland", "x11", "fcitx5", "portal").
    pub backend: String,
}

#[interface(name = "org.freedesktop.vime.Diagnostics1")]
impl ImeDiagnostics {
    /// Return command-line flags that detected Electron apps should use.
    ///
    /// Returns an empty list when no Electron processes are detected.
    fn suggest_electron_flags(&self) -> Vec<String> {
        if self.electron_pids.is_empty() {
            vec![]
        } else {
            vec![
                "--ozone-platform=wayland".to_string(),
                "--enable-features=UseOzonePlatform".to_string(),
            ]
        }
    }

    /// Return a structured status snapshot for debugging.
    ///
    /// Fields:
    ///   - `input_method`: active input method name
    ///   - `backend`: active backend name
    ///   - `electron_pids`: comma-separated list of detected Electron PIDs (or empty)
    fn get_status(&self) -> HashMap<String, String> {
        let mut map = HashMap::new();
        map.insert("input_method".to_string(), self.input_method.clone());
        map.insert("backend".to_string(), self.backend.clone());
        map.insert(
            "electron_pids".to_string(),
            if self.electron_pids.is_empty() {
                String::new()
            } else {
                self.electron_pids
                    .iter()
                    .map(|p| p.to_string())
                    .collect::<Vec<_>>()
                    .join(",")
            },
        );
        map
    }
}

/// Attempt to publish the diagnostics interface on the session bus.
///
/// Errors are silently logged at debug level; the daemon continues even if
/// the D-Bus service cannot be registered (e.g., no session bus available).
pub async fn spawn_diagnostics_service(
    electron_pids: Vec<u32>,
    input_method: String,
    backend: String,
) {
    if !electron_pids.is_empty() {
        warn!(
            electron_pid_count = electron_pids.len(),
            pids = ?electron_pids,
            "Electron apps detected — suggest --ozone-platform=wayland flag"
        );
    }
    if let Err(e) = try_serve(electron_pids, input_method, backend).await {
        debug!("D-Bus diagnostics service unavailable: {e}");
    }
}

async fn try_serve(
    electron_pids: Vec<u32>,
    input_method: String,
    backend: String,
) -> zbus::Result<()> {
    let iface = ImeDiagnostics { electron_pids, input_method: input_method.clone(), backend: backend.clone() };
    let _conn = zbus::ConnectionBuilder::session()?
        .name("org.freedesktop.vime")?
        .serve_at("/org/freedesktop/vime/Diagnostics", iface)?
        .build()
        .await?;
    info!(
        input_method = %input_method,
        backend = %backend,
        "D-Bus diagnostics interface registered at org.freedesktop.vime"
    );
    std::future::pending::<()>().await;
    Ok(())
}
