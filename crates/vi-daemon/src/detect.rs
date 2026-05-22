//! Backend auto-detection: probe the environment at startup and pick the best
//! available IME backend.
//!
//! Detection order:
//! 1. XDG Portal — sandbox detected (Flatpak, Snap, AppImage).
//! 2. IBus  — `$IBUS_ADDRESS` set or ibus-daemon is on the session D-Bus.
//! 3. Wayland — `$WAYLAND_DISPLAY` is set and `zwp_input_method_manager_v2` is available.
//! 4. Fcitx5 — `org.fcitx.Fcitx5` is on the session D-Bus.
//! 5. X11    — `$DISPLAY` is set (fallback).

use std::sync::{Arc, Mutex};

use tokio::sync::mpsc;
use tracing::info;
use vi_config::schema::IbusCommitMode;
use vi_core::{InputEvent, StandardEngine};
use vi_platform::{ImeBackend, PlatformError};
use wayland_client::{
    globals::{registry_queue_init, GlobalListContents},
    protocol::wl_registry,
    Connection, Dispatch, QueueHandle,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackendKind {
    Portal,
    IBus,
    Fcitx5,
    Wayland,
    X11,
}

impl std::fmt::Display for BackendKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BackendKind::Portal => write!(f, "portal"),
            BackendKind::IBus => write!(f, "ibus"),
            BackendKind::Fcitx5 => write!(f, "fcitx5"),
            BackendKind::Wayland => write!(f, "wayland"),
            BackendKind::X11 => write!(f, "x11"),
        }
    }
}

/// Probe the environment and return a running backend.
///
/// Probing happens on the calling thread; D-Bus checks are synchronous
/// but fast (< 5 ms each in practice).
///
/// `force_preedit_mode`: forwarded to `IbusBackend::connect` when IBus is
/// selected; see `[ibus] force_preedit_mode` in the config file.
///
/// `force_chrome_direct`: forwarded to `IbusBackend::connect`; see
/// `[ibus] force_chrome_direct` in the config file.
///
/// `default_commit_mode`: forwarded to `IbusBackend::connect`; see
/// `[ibus] commit_mode` in the config file.
pub async fn detect_and_connect(
    rt: tokio::runtime::Handle,
    tx: mpsc::Sender<InputEvent>,
    engine: Arc<Mutex<StandardEngine>>,
    force_preedit_mode: bool,
    force_chrome_direct: bool,
    default_commit_mode: IbusCommitMode,
) -> Result<(BackendKind, Arc<dyn ImeBackend>), PlatformError> {
    if vi_portal::is_sandboxed() {
        info!("Backend: detected sandbox environment, using XDG portal");
        let backend = tokio::task::spawn_blocking({
            let rt = rt.clone();
            move || vi_portal::XdgInputMethodPortal::connect(rt)
        })
        .await
        .map_err(|e| PlatformError::DBus(e.to_string()))??;
        return Ok((BackendKind::Portal, Arc::new(backend)));
    }

    if probe_ibus().await {
        info!("Backend: detected IBus");
        let backend = tokio::task::spawn_blocking({
            let rt = rt.clone();
            let engine = Arc::clone(&engine);
            move || {
                vi_ibus::IbusBackend::connect(
                    rt,
                    engine,
                    force_preedit_mode,
                    force_chrome_direct,
                    default_commit_mode,
                )
            }
        })
        .await
        .map_err(|e| PlatformError::DBus(e.to_string()))??;
        return Ok((BackendKind::IBus, Arc::new(backend)));
    }

    if probe_wayland_input_method() {
        info!("Wayland zwp_input_method_v2 available; preferring native backend over Fcitx5 for inline preedit support");
        let backend = tokio::task::spawn_blocking({
            let rt = rt.clone();
            let tx = tx.clone();
            move || vi_wayland::WaylandBackend::connect(rt, tx)
        })
        .await
        .map_err(|e| PlatformError::Wayland(e.to_string()))??;
        return Ok((BackendKind::Wayland, Arc::new(backend)));
    }

    if probe_fcitx5().await {
        info!("Backend: detected Fcitx5");
        let socket_path = vi_fcitx5::engine_socket_path();
        let backend = Arc::new(vi_fcitx5::FcitxShimBackend::new(
            socket_path,
            Arc::clone(&engine),
        ));
        tokio::spawn({
            let b = Arc::clone(&backend);
            async move {
                if let Err(e) = b.run().await {
                    tracing::warn!("Fcitx5 shim server error: {e}");
                }
            }
        });
        return Ok((BackendKind::Fcitx5, backend));
    }

    if std::env::var_os("DISPLAY").is_some() {
        info!("Backend: detected X11");
        let backend = tokio::task::spawn_blocking({
            let rt = rt.clone();
            let tx = tx.clone();
            move || vi_x11::X11Backend::connect(rt, tx)
        })
        .await
        .map_err(|e| PlatformError::X11(e.to_string()))??;
        return Ok((BackendKind::X11, Arc::new(backend)));
    }

    Err(PlatformError::NotSupported)
}

// ── Sandboxed application detection ──────────────────────────────────────────

/// A sandboxed application process detected on the running system.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SandboxedApp {
    /// Electron/Chromium renderer or a known Electron-based executable.
    Electron(u32),
    /// Application running inside a Flatpak sandbox.
    Flatpak { pid: u32, app_id: String },
    /// Application running inside a Snap sandbox.
    Snap { pid: u32, snap_name: String },
}

/// Scan `/proc` for running sandboxed application processes.
pub fn detect_sandboxed_apps() -> Vec<SandboxedApp> {
    detect_in(std::path::Path::new("/proc"))
}

/// Like [`detect_sandboxed_apps`] but scans `proc_root` instead of `/proc`.
///
/// Accepts an alternate root so tests can supply a temporary directory tree.
pub fn detect_in(proc_root: &std::path::Path) -> Vec<SandboxedApp> {
    let mut apps = Vec::new();
    let dir = match std::fs::read_dir(proc_root) {
        Ok(d) => d,
        Err(_) => return apps,
    };
    for entry in dir.flatten() {
        let file_name = entry.file_name();
        let pid: u32 = match file_name.to_string_lossy().parse() {
            Ok(p) => p,
            Err(_) => continue, // skip non-PID entries like "self", "sys"
        };
        let base = entry.path();

        // Read environment variables (NUL-separated key=value pairs).
        let environ = match std::fs::read(base.join("environ")) {
            Ok(b) => b,
            Err(_) => continue,
        };

        let mut flatpak_id: Option<String> = None;
        let mut snap_name: Option<String> = None;
        let mut snap_path: Option<String> = None;

        for var in environ.split(|&b| b == 0) {
            if let Some(rest) = var.strip_prefix(b"FLATPAK_ID=") {
                flatpak_id = Some(String::from_utf8_lossy(rest).into_owned());
            } else if let Some(rest) = var.strip_prefix(b"SNAP_NAME=") {
                snap_name = Some(String::from_utf8_lossy(rest).into_owned());
            } else if snap_path.is_none() {
                if let Some(rest) = var.strip_prefix(b"SNAP=") {
                    snap_path = Some(String::from_utf8_lossy(rest).into_owned());
                }
            }
        }

        if let Some(app_id) = flatpak_id {
            apps.push(SandboxedApp::Flatpak { pid, app_id });
            continue;
        }

        // Derive snap name from SNAP_NAME; fall back to parsing the SNAP path.
        let resolved_snap = snap_name.or_else(|| {
            snap_path
                .as_deref()
                .and_then(|p| p.split('/').nth(2))
                .map(str::to_owned)
        });
        if let Some(name) = resolved_snap {
            apps.push(SandboxedApp::Snap {
                pid,
                snap_name: name,
            });
            continue;
        }

        // Electron: check cmdline for known binary names or --type=renderer.
        if let Ok(cmdline) = std::fs::read(base.join("cmdline")) {
            let args: Vec<&[u8]> = cmdline.split(|&b| b == 0).collect();
            let is_known_exe = args.first().is_some_and(|exe| {
                let s = String::from_utf8_lossy(exe);
                let base_name = s.rsplit('/').next().unwrap_or(s.as_ref());
                matches!(base_name, "electron" | "code" | "slack" | "discord")
            });
            let has_renderer = args.iter().any(|a| *a == b"--type=renderer");
            if is_known_exe || has_renderer {
                apps.push(SandboxedApp::Electron(pid));
            }
        }
    }
    apps
}

/// Return `true` if the XDG portal exposes the `InputMethod` interface.
///
/// Uses D-Bus introspection on `org.freedesktop.portal.Desktop`.
pub async fn portal_input_method_available() -> bool {
    let conn = match zbus::Connection::session().await {
        Ok(c) => c,
        Err(_) => return false,
    };
    let proxy = match zbus::Proxy::new(
        &conn,
        "org.freedesktop.portal.Desktop",
        "/org/freedesktop/portal/desktop",
        "org.freedesktop.DBus.Introspectable",
    )
    .await
    {
        Ok(p) => p,
        Err(_) => return false,
    };
    let result: zbus::Result<String> = proxy.call("Introspect", &()).await;
    result
        .map(|xml| xml.contains("org.freedesktop.portal.InputMethod"))
        .unwrap_or(false)
}

// ── Backend probing ───────────────────────────────────────────────────────────

/// Minimal Wayland dispatch state used only for global-list initialisation.
struct WlProbeState;

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for WlProbeState {
    fn event(
        _: &mut Self,
        _: &wl_registry::WlRegistry,
        _: wl_registry::Event,
        _: &GlobalListContents,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

/// Return `true` if `$WAYLAND_DISPLAY` is set and the compositor advertises
/// `zwp_input_method_manager_v2` in its global registry.
fn probe_wayland_input_method() -> bool {
    if std::env::var_os("WAYLAND_DISPLAY").is_none() {
        return false;
    }
    let conn = match Connection::connect_to_env() {
        Ok(c) => c,
        Err(_) => return false,
    };
    let (globals, _eq) = match registry_queue_init::<WlProbeState>(&conn) {
        Ok(r) => r,
        Err(_) => return false,
    };
    globals.contents().with_list(|list| {
        list.iter()
            .any(|g| g.interface == "zwp_input_method_manager_v2")
    })
}

/// Return true if IBus is available (address resolves or daemon is on D-Bus).
async fn probe_ibus() -> bool {
    if std::env::var_os("IBUS_ADDRESS").is_some() {
        return true;
    }
    // Try listing the session bus.
    probe_dbus_name("org.freedesktop.IBus").await
}

async fn probe_fcitx5() -> bool {
    probe_dbus_name("org.fcitx.Fcitx5").await
}

async fn probe_dbus_name(name: &str) -> bool {
    match zbus::Connection::session().await {
        Ok(conn) => match zbus::fdo::DBusProxy::new(&conn).await {
            Ok(proxy) => proxy
                .list_names()
                .await
                .map(|names| names.iter().any(|n| n.as_str() == name))
                .unwrap_or(false),
            Err(_) => false,
        },
        Err(_) => false,
    }
}
