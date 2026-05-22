use std::sync::Arc;

use tracing::debug;
use vi_platform::{PlatformError, Result};
use x11rb::{
    protocol::xproto::{self, ConnectionExt as _, EventMask, Window},
    rust_connection::RustConnection,
};

/// Tracks `_NET_WM_STATE_FULLSCREEN` on focused windows.
pub struct FullscreenMonitor {
    conn: Arc<RustConnection>,
    pub net_wm_state: xproto::Atom,
    net_wm_state_fullscreen: xproto::Atom,
}

impl FullscreenMonitor {
    pub fn new(
        conn: Arc<RustConnection>,
        net_wm_state: xproto::Atom,
        net_wm_state_fullscreen: xproto::Atom,
    ) -> Self {
        Self {
            conn,
            net_wm_state,
            net_wm_state_fullscreen,
        }
    }

    /// Subscribe to `PropertyNotify` on `root` so WM state changes are observed.
    pub fn subscribe_root(&self, root: Window) -> Result<()> {
        self.conn
            .change_window_attributes(
                root,
                &xproto::ChangeWindowAttributesAux::new().event_mask(EventMask::PROPERTY_CHANGE),
            )
            .map_err(|e| PlatformError::X11(e.to_string()))?
            .check()
            .map_err(|e| PlatformError::X11(e.to_string()))
    }

    /// Subscribe to `PropertyNotify` on a specific client window.
    pub fn subscribe_window(&self, window: Window) -> Result<()> {
        self.conn
            .change_window_attributes(
                window,
                &xproto::ChangeWindowAttributesAux::new().event_mask(EventMask::PROPERTY_CHANGE),
            )
            .map_err(|e| PlatformError::X11(e.to_string()))?
            .check()
            .map_err(|e| PlatformError::X11(e.to_string()))
    }

    /// Returns `true` when `window` currently has `_NET_WM_STATE_FULLSCREEN`.
    pub fn is_fullscreen(&self, window: Window) -> bool {
        match self.read_state_atoms(window) {
            Some(atoms) => {
                let fs = is_fullscreen_atoms(&atoms, self.net_wm_state_fullscreen);
                if fs {
                    debug!("fullscreen: window {window:#x} is fullscreen");
                }
                fs
            }
            None => false,
        }
    }

    fn read_state_atoms(&self, window: Window) -> Option<Vec<u32>> {
        let reply = self
            .conn
            .get_property(
                false,
                window,
                self.net_wm_state,
                xproto::AtomEnum::ATOM,
                0,
                64,
            )
            .ok()?
            .reply()
            .ok()?;
        let bytes = reply.value;
        if bytes.len() % 4 != 0 {
            return None;
        }
        Some(
            bytes
                .chunks_exact(4)
                .map(|c| u32::from_ne_bytes([c[0], c[1], c[2], c[3]]))
                .collect(),
        )
    }
}

/// Pure predicate: does `state_atoms` contain `fullscreen_atom`?
pub fn is_fullscreen_atoms(state_atoms: &[u32], fullscreen_atom: u32) -> bool {
    state_atoms.contains(&fullscreen_atom)
}
