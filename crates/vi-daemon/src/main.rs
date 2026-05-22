//! vi-daemon — the Vietnamese IME orchestrator.
//!
//! Startup sequence (must complete in < 50 ms):
//! 1. Initialise tracing.
//! 2. Try to reload a preedit snapshot from a previous graceful restart.
//! 3. Auto-detect the best available backend.
//! 4. Bind the Unix IPC socket.
//! 5. Send `READY=1` to systemd.
//! 6. Spawn the systemd watchdog heartbeat task.
//! 7. Block on the main event loop.
//!
//! On SIGTERM: serialise current preedit to /tmp, broadcast shutdown, exit.

mod dictionary;

use std::{
    sync::{
        atomic::{AtomicU8, Ordering},
        Arc, Mutex, RwLock,
    },
    time::Duration,
};

use tokio::sync::{broadcast, mpsc};
use tracing::{error, info, warn};
use vi_config::Config;
use vi_core::{CompositionEngine, InputEvent, InputMethod, StandardEngine};
use vi_daemon::{
    dbus_service, detect, event_loop,
    ipc::{self, Request, Response},
    signal::{save_snapshot, wait_for_sigterm, PreeditSnapshot},
    watchdog,
};

/// Load the on-disk config file, returning `Config::default()` on any error.
///
/// Checks `$XDG_CONFIG_HOME/vime/config.toml` then
/// `$HOME/.config/vime/config.toml`.  Called once at startup before backend
/// detection so that options like `[ibus] force_preedit_mode` take effect.
fn load_startup_config() -> Config {
    let path = std::env::var_os("XDG_CONFIG_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join(".config")))
        .map(|base| base.join("vime").join("config.toml"));

    if let Some(p) = path {
        match vi_config::load(&p) {
            Ok(cfg) => {
                info!("Loaded config from {}", p.display());
                return cfg;
            }
            Err(e) if p.exists() => {
                warn!("Failed to load config from {}: {e}", p.display());
            }
            Err(_) => {} // file simply does not exist
        }
    }
    Config::default()
}

fn parse_method(s: &str) -> Option<InputMethod> {
    match s {
        "telex" => Some(InputMethod::Telex),
        "vni" => Some(InputMethod::Vni),
        "viqr" => Some(InputMethod::Viqr),
        _ => None,
    }
}

fn method_to_u8(method: InputMethod) -> u8 {
    match method {
        InputMethod::Telex => 0,
        InputMethod::Vni => 1,
        InputMethod::Viqr => 2,
    }
}

fn method_name_from_u8(v: u8) -> &'static str {
    match v {
        1 => "vni",
        2 => "viqr",
        _ => "telex",
    }
}

/// Lock the engine, handling a poisoned mutex by logging, resetting, and
/// returning the inner guard so the caller can continue operating.
fn lock_engine(engine: &Arc<Mutex<StandardEngine>>) -> std::sync::MutexGuard<'_, StandardEngine> {
    match engine.lock() {
        Ok(g) => g,
        Err(poisoned) => {
            error!("engine mutex poisoned, resetting state");
            let mut g = poisoned.into_inner();
            g.reset();
            g
        }
    }
}

#[tokio::main]
async fn main() {
    // If RUST_LOG is set (e.g. via systemd debug.conf override), honour it
    // directly so crate-level debug directives aren't masked by the hard-coded
    // info fallbacks below.
    let log_filter = tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        tracing_subscriber::EnvFilter::new(
            "vi_daemon=info,vi_ibus=info,vi_fcitx5=info,vi_wayland=info,vi_x11=info",
        )
    });
    tracing_subscriber::fmt().with_env_filter(log_filter).init();

    info!("vi-daemon starting");

    // Reload any preedit text that was snapshotted before a previous SIGTERM.
    let snapshot = PreeditSnapshot::load();
    if let Some(ref s) = snapshot {
        info!("Restored preedit snapshot: {:?}", s.preedit);
    }

    let rt = tokio::runtime::Handle::current();

    // Parse initial method from snapshot or default to Telex.
    let initial_method = snapshot
        .as_ref()
        .and_then(|s| parse_method(&s.method))
        .unwrap_or(InputMethod::Telex);

    // Load config before constructing the engine so spell-check / IBus flags apply.
    let startup_config = load_startup_config();

    let dict_arc = dictionary::load_dictionary_arc();
    let spell_opts = dictionary::spell_options_from_config(&startup_config, Some(dict_arc));

    let engine: Arc<Mutex<StandardEngine>> = Arc::new(Mutex::new(StandardEngine::with_spell(
        initial_method,
        spell_opts,
    )));

    // Channel: backends push InputEvents here; the event loop drains them.
    let (tx, rx) = mpsc::channel::<InputEvent>(256);

    // Broadcast channel for coordinated shutdown.
    let (shutdown_tx, shutdown_rx) = broadcast::channel::<()>(1);

    // Pick IBus overrides from the same config snapshot.
    let force_preedit_mode = startup_config.ibus.force_preedit_mode;
    let force_chrome_direct = startup_config.ibus.force_chrome_direct;
    let default_commit_mode = startup_config.ibus.commit_mode;

    // Auto-detect and connect the backend.
    let (kind, backend) = match detect::detect_and_connect(
        rt,
        tx.clone(),
        engine.clone(),
        force_preedit_mode,
        force_chrome_direct,
        default_commit_mode,
    )
    .await
    {
        Ok(pair) => pair,
        Err(e) => {
            error!("No IME backend available: {e}");
            std::process::exit(1);
        }
    };
    info!("Backend: {kind}");

    // Detect sandboxed applications and emit per-type recommendations.
    let sandboxed = detect::detect_sandboxed_apps();
    let on_wayland = std::env::var_os("WAYLAND_DISPLAY").is_some();
    let mut electron_pids: Vec<u32> = Vec::new();
    let mut has_flatpak = false;
    for app in &sandboxed {
        match app {
            detect::SandboxedApp::Electron(pid) => {
                if on_wayland {
                    warn!(
                        pid,
                        "Electron process detected on Wayland; relaunch with \
                         `--ozone-platform=wayland --enable-features=UseOzonePlatform` \
                         to enable IME input"
                    );
                }
                electron_pids.push(*pid);
            }
            detect::SandboxedApp::Flatpak { pid, app_id } => {
                info!(pid, app_id = %app_id, "Flatpak application detected");
                has_flatpak = true;
            }
            detect::SandboxedApp::Snap { pid, snap_name } => {
                info!(pid, snap_name = %snap_name, "Snap application detected");
            }
        }
    }

    // For Flatpak apps, check whether the InputMethod portal is available.
    if has_flatpak {
        if detect::portal_input_method_available().await {
            info!(
                "org.freedesktop.portal.InputMethod is available; \
                 Flatpak apps should use portal-based input"
            );
        } else {
            warn!(
                "Flatpak app(s) detected but org.freedesktop.portal.InputMethod \
                 is not available; IME input in Flatpak apps may not work"
            );
        }
    }

    let backend_label = kind.to_string();

    // Always expose the D-Bus diagnostics interface so desktop environments and
    // debugging tools can query status; the electron_pids warn fires inside the
    // service when detected.
    {
        let method_name = method_name_from_u8(method_to_u8(initial_method)).to_owned();
        let backend_diag = backend_label.clone();
        tokio::spawn(dbus_service::spawn_diagnostics_service(
            electron_pids,
            method_name,
            backend_diag,
        ));
    }

    // Spawn the central event routing loop; keep the handle for graceful join.
    let event_loop_handle = tokio::spawn(event_loop::run_event_loop(
        engine.clone(),
        backend,
        rx,
        shutdown_rx,
    ));

    // Track the current input method as an atomic u8 (0=telex, 1=vni, 2=viqr).
    // The atomic is written inside the engine lock so method and engine state
    // never diverge even under concurrent IPC requests.
    let current_method: Arc<AtomicU8> = Arc::new(AtomicU8::new(method_to_u8(initial_method)));

    // In-memory config store for settings sent via IPC (e.g. key bindings).
    let config_arc: Arc<RwLock<Config>> = Arc::new(RwLock::new(startup_config));

    // IPC handler — any blocking work is offloaded to spawn_blocking in ipc.rs.
    let current_method_ipc = Arc::clone(&current_method);
    let engine_ipc = Arc::clone(&engine);
    let backend_label_ipc = backend_label.clone();
    let config_ipc = Arc::clone(&config_arc);
    let ipc_handler = move |req: Request| -> Response {
        match req {
            Request::Status => {
                // Hold the engine lock for both reads so the snapshot is consistent.
                let eng = lock_engine(&engine_ipc);
                let preedit = eng.preedit().as_str().to_owned();
                let method =
                    method_name_from_u8(current_method_ipc.load(Ordering::SeqCst)).to_owned();
                drop(eng);
                Response::Status {
                    backend: backend_label_ipc.clone(),
                    method,
                    preedit,
                }
            }
            Request::SetMethod { method } => match parse_method(&method) {
                Some(new_method) => {
                    let new_u8 = method_to_u8(new_method);
                    // Update engine and atomic in the same critical section so readers
                    // never observe a method name that disagrees with the engine state.
                    let mut eng = lock_engine(&engine_ipc);
                    eng.set_method(new_method);
                    current_method_ipc.store(new_u8, Ordering::SeqCst);
                    drop(eng);
                    info!("Input method changed to: {method}");
                    Response::Ok
                }
                None => Response::Error {
                    message: format!("Unknown method: {method}"),
                },
            },
            Request::SetKeyBindings { bindings } => ipc::apply_key_bindings(&config_ipc, bindings),
            Request::Shutdown => {
                info!("Shutdown requested via IPC");
                std::process::exit(0);
            }
        }
    };

    let socket_path = ipc::socket_path();
    let ipc_handle = tokio::spawn(ipc::serve(
        socket_path,
        ipc_handler,
        Duration::from_secs(30),
    ));

    // Notify systemd and start watchdog.
    watchdog::notify_ready();
    let watchdog_token = tokio_util::sync::CancellationToken::new();
    watchdog::spawn_watchdog(watchdog_token);

    // Keep the event-tx alive until main exits so the channel stays open.
    drop(tx);

    // SIGTERM: snapshot preedit (2 s deadline), broadcast shutdown, then wait
    // up to 3 s for tasks before forcing exit.
    let method_final = Arc::clone(&current_method);
    let engine_final = Arc::clone(&engine);
    wait_for_sigterm().await;

    let (preedit, method) = {
        let mut eng = lock_engine(&engine_final);
        let preedit = eng
            .flush_commit()
            .map(|s| s.as_str().to_owned())
            .unwrap_or_default();
        let method = method_name_from_u8(method_final.load(Ordering::SeqCst)).to_owned();
        (preedit, method)
    };
    let cursor = preedit.chars().count();
    let snap = PreeditSnapshot {
        preedit,
        cursor,
        method,
    };
    save_snapshot(snap).await;
    shutdown_tx.send(()).ok();

    let _ = tokio::time::timeout(Duration::from_secs(3), async {
        let _ = event_loop_handle.await;
        let _ = ipc_handle.await;
    })
    .await;
    std::process::exit(0);
}
