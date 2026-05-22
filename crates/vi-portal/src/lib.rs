//! Deprecated: use the vime-* equivalent crate instead.
//! XDG portal-based IME backend for sandboxed environments (Flatpak, Snap, AppImage).
//!
//! Routes input-method operations through `org.freedesktop.portal.InputMethod`
//! on the session D-Bus instead of talking directly to the compositor.

pub mod detect;
pub mod proxy;

pub use detect::{detect_sandbox, is_electron, is_sandboxed, SandboxKind};

use std::sync::Mutex;

use tokio::runtime::Handle;
use tracing::{debug, warn};
use vi_core::{InputEvent, NfcString, PreeditText};
use vi_platform::{Capabilities, CharCursor, ImeBackend, PlatformError, Result, SurroundingText};

/// Determine from the given configuration whether the portal backend should
/// be used for this process, taking into account the sandbox environment.
///
/// Returns `true` when `sandbox_mode` is `XdgPortal`, or when `sandbox_mode`
/// is `Auto` and a sandbox environment is detected.  Returns `false` when
/// `sandbox_mode` is `Direct`.
pub fn should_use_portal(config: &vi_config::Config) -> bool {
    use vi_config::schema::SandboxMode;
    match config.sandbox_mode {
        SandboxMode::XdgPortal => true,
        SandboxMode::Direct => false,
        SandboxMode::Auto => is_sandboxed(),
    }
}

/// IME backend that communicates through the XDG portal D-Bus interface.
///
/// Prefer this backend when running inside a sandboxed environment where
/// direct compositor socket access may be restricted.
pub struct XdgInputMethodPortal {
    rt: Handle,
    conn: zbus::Connection,
    surrounding: Mutex<Option<SurroundingText>>,
}

impl XdgInputMethodPortal {
    /// Connect to the portal D-Bus service.
    ///
    /// Call this from a blocking context, e.g. inside
    /// `tokio::task::spawn_blocking`.
    pub fn connect(rt: Handle) -> Result<Self> {
        let conn = rt
            .block_on(zbus::Connection::session())
            .map_err(|e| PlatformError::DBus(e.to_string()))?;
        Ok(Self {
            rt,
            conn,
            surrounding: Mutex::new(None),
        })
    }
}

impl ImeBackend for XdgInputMethodPortal {
    fn commit(&self, text: &NfcString) -> Result<()> {
        debug!(text = text.as_str(), "portal: commit");
        let conn = self.conn.clone();
        let text = text.as_str().to_owned();
        self.rt.block_on(async move {
            let proxy = proxy::InputMethodPortalProxy::new(&conn)
                .await
                .map_err(|e| PlatformError::DBus(e.to_string()))?;
            proxy
                .commit_string(&text)
                .await
                .map_err(|e| PlatformError::DBus(e.to_string()))
        })
    }

    fn update_preedit(&self, text: &PreeditText, cursor: CharCursor) -> Result<()> {
        debug!("portal: update_preedit");
        let conn = self.conn.clone();
        let text = text.as_str().to_owned();
        let cursor = cursor.0.min(text.chars().count()) as u32;
        self.rt.block_on(async move {
            let proxy = proxy::InputMethodPortalProxy::new(&conn)
                .await
                .map_err(|e| PlatformError::DBus(e.to_string()))?;
            proxy
                .set_preedit_string(&text, cursor, cursor)
                .await
                .map_err(|e| PlatformError::DBus(e.to_string()))
        })
    }

    fn clear_preedit(&self) -> Result<()> {
        debug!("portal: clear_preedit");
        let conn = self.conn.clone();
        self.rt.block_on(async move {
            let proxy = proxy::InputMethodPortalProxy::new(&conn)
                .await
                .map_err(|e| PlatformError::DBus(e.to_string()))?;
            proxy
                .set_preedit_string("", 0, 0)
                .await
                .map_err(|e| PlatformError::DBus(e.to_string()))
        })
    }

    fn forward_key(&self, key: &InputEvent) -> Result<()> {
        warn!(
            ?key,
            "portal: forward_key not supported by portal protocol, dropping"
        );
        Ok(())
    }

    fn surrounding_text(&self) -> Result<Option<SurroundingText>> {
        Ok(self.surrounding.lock().ok().and_then(|g| g.clone()))
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            surrounding_text: false,
            preedit: true,
            lookup_table: false,
        }
    }
}
