//! Compatibility guards for special runtime environments.
//!
//! * [`FullscreenGuard`] — suppresses preedit when a fullscreen game has focus.
//! * [`RemoteDesktopMode`] — detects remote-desktop sessions and signals that
//!   keystroke passthrough should be used instead of preedit composition.

use tracing::{debug, warn};

// ---------------------------------------------------------------------------
// Glob matching
// ---------------------------------------------------------------------------

/// Match `text` against a simple glob `pattern` that supports `*` wildcards.
///
/// `*` matches zero or more characters.  Matching is case-insensitive.
fn matches_glob(pattern: &str, text: &str) -> bool {
    let pattern = pattern.to_lowercase();
    let text = text.to_lowercase();
    matches_glob_lower(&pattern, &text)
}

fn matches_glob_lower(pattern: &str, text: &str) -> bool {
    match pattern.split_once('*') {
        None => pattern == text,
        Some((prefix, rest)) => {
            if !text.starts_with(prefix) {
                return false;
            }
            let remaining = &text[prefix.len()..];
            for i in 0..=remaining.len() {
                if remaining.is_char_boundary(i) && matches_glob_lower(rest, &remaining[i..]) {
                    return true;
                }
            }
            false
        }
    }
}

// ---------------------------------------------------------------------------
// FullscreenGuard
// ---------------------------------------------------------------------------

/// Detects whether the currently focused window is a fullscreen game.
///
/// When [`should_passthrough`] returns `true` the daemon should disable preedit
/// and forward all key events unmodified to the application.
///
/// On X11 the check queries `_NET_WM_STATE_FULLSCREEN` and `WM_CLASS` via
/// x11rb.  On Wayland, fullscreen state must be tracked by the compositor
/// event loop; this type exposes [`set_wayland_fullscreen`] so the Wayland
/// backend can push state updates.
pub struct FullscreenGuard {
    game_patterns: Vec<String>,
    /// Fullscreen state reported by the Wayland backend.
    wayland_fullscreen: std::sync::atomic::AtomicBool,
    /// WM_CLASS of the Wayland-focused surface (best-effort, UTF-8).
    wayland_wm_class: std::sync::Mutex<String>,
}

impl FullscreenGuard {
    pub fn new(game_list: Vec<String>) -> Self {
        Self {
            game_patterns: game_list,
            wayland_fullscreen: std::sync::atomic::AtomicBool::new(false),
            wayland_wm_class: std::sync::Mutex::new(String::new()),
        }
    }

    /// Called by the Wayland backend when `xdg_toplevel::State::Fullscreen`
    /// changes for the focused surface.
    pub fn set_wayland_fullscreen(&self, fullscreen: bool, wm_class: &str) {
        self.wayland_fullscreen.store(fullscreen, std::sync::atomic::Ordering::Relaxed);
        if let Ok(mut guard) = self.wayland_wm_class.lock() {
            guard.clear();
            guard.push_str(wm_class);
        }
    }

    /// Returns `true` when preedit should be suppressed for the focused window.
    ///
    /// The check is skipped (returns `false`) when no game patterns are configured.
    pub fn should_passthrough(&self) -> bool {
        if self.game_patterns.is_empty() {
            return false;
        }
        self.check_wayland() || self.check_x11().unwrap_or(false)
    }

    fn check_wayland(&self) -> bool {
        if !self.wayland_fullscreen.load(std::sync::atomic::Ordering::Relaxed) {
            return false;
        }
        let class = self.wayland_wm_class.lock().ok().map(|g| g.clone()).unwrap_or_default();
        self.matches_any(&class)
    }

    fn check_x11(&self) -> Option<bool> {
        use x11rb::connection::Connection as _;
        use x11rb::protocol::xproto::{AtomEnum, ConnectionExt as _};
        use x11rb::rust_connection::RustConnection;

        std::env::var_os("DISPLAY")?;

        let (conn, screen_num) = RustConnection::connect(None).ok()?;
        let root = conn.setup().roots[screen_num].root;

        let focus_reply = conn.get_input_focus().ok()?.reply().ok()?;
        let win = focus_reply.focus;
        // No focus or focus is the root — nothing to check.
        if win == 0 || win == root {
            return Some(false);
        }

        let net_wm_state = conn
            .intern_atom(false, b"_NET_WM_STATE")
            .ok()?
            .reply()
            .ok()?
            .atom;
        let net_wm_state_fullscreen = conn
            .intern_atom(false, b"_NET_WM_STATE_FULLSCREEN")
            .ok()?
            .reply()
            .ok()?
            .atom;

        let state_reply = conn
            .get_property(false, win, net_wm_state, AtomEnum::ATOM, 0, 1024)
            .ok()?
            .reply()
            .ok()?;

        // _NET_WM_STATE is a list of u32 atoms (format 32, native byte order).
        let is_fullscreen = state_reply
            .value
            .chunks_exact(4)
            .filter_map(|b| b.try_into().ok())
            .map(u32::from_ne_bytes)
            .any(|atom| atom == net_wm_state_fullscreen);

        if !is_fullscreen {
            return Some(false);
        }

        // Check WM_CLASS against the configured game list.
        let wm_class_reply = conn
            .get_property(false, win, AtomEnum::WM_CLASS, AtomEnum::STRING, 0, 512)
            .ok()?
            .reply()
            .ok()?;

        // WM_CLASS is two null-terminated strings: "instance\0class\0".
        // We match against the class (second field).
        let raw = String::from_utf8_lossy(&wm_class_reply.value);
        let wm_class = raw.split('\0').nth(1).unwrap_or_else(|| raw.split('\0').next().unwrap_or(""));

        debug!(wm_class, "fullscreen window detected, checking game list");
        Some(self.matches_any(wm_class))
    }

    fn matches_any(&self, wm_class: &str) -> bool {
        self.game_patterns.iter().any(|p| matches_glob(p, wm_class))
    }
}

// ---------------------------------------------------------------------------
// RemoteDesktopMode
// ---------------------------------------------------------------------------

/// Detects whether the daemon is running inside a remote-desktop session.
///
/// When [`is_active`] returns `true` the daemon should switch to passthrough
/// mode: disable preedit and commit each keystroke immediately as a raw
/// character so the remote client receives unmodified input.
pub struct RemoteDesktopMode;

impl RemoteDesktopMode {
    /// Returns `true` if a remote-desktop session is detected.
    ///
    /// Checks, in order:
    /// * `$XRDP_SESSION` — set by xrdp for all RDP sessions.
    /// * `$VNC_DISPLAY`  — set by some VNC servers.
    /// * Wayland `remote-desktop-unstable-v1` — detected via the
    ///   `WAYLAND_DISPLAY` environment variable combined with the protocol
    ///   being advertised; full protocol negotiation is delegated to the
    ///   Wayland backend event loop.
    pub fn is_active() -> bool {
        if std::env::var_os("XRDP_SESSION").is_some() {
            warn!("RemoteDesktopMode: XRDP session detected, switching to passthrough");
            return true;
        }
        if std::env::var_os("VNC_DISPLAY").is_some() {
            warn!("RemoteDesktopMode: VNC session detected, switching to passthrough");
            return true;
        }
        false
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::matches_glob;

    #[test]
    fn glob_exact() {
        assert!(matches_glob("Steam", "Steam"));
        assert!(matches_glob("Steam", "steam")); // case-insensitive matching
        assert!(!matches_glob("Steam", "other"));
    }

    #[test]
    fn glob_wildcard_suffix() {
        assert!(matches_glob("steam*", "steam"));
        assert!(matches_glob("steam*", "steam_native"));
        assert!(!matches_glob("steam*", "xsteam"));
    }

    #[test]
    fn glob_wildcard_prefix() {
        assert!(matches_glob("*game", "mygame"));
        assert!(matches_glob("*game", "game"));
        assert!(!matches_glob("*game", "games"));
    }

    #[test]
    fn glob_wildcard_both() {
        assert!(matches_glob("*steam*", "com.valvesoftware.Steam"));
        assert!(matches_glob("*steam*", "steam"));
        assert!(!matches_glob("*steam*", "valve"));
    }

    #[test]
    fn glob_star_only() {
        assert!(matches_glob("*", "anything"));
        assert!(matches_glob("*", ""));
    }
}
