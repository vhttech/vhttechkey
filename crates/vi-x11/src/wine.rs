use std::{fs, sync::Arc};

use tracing::debug;
use vi_platform::{PlatformError, Result};
use x11rb::{
    connection::Connection as _,
    protocol::xproto::{self, ConnectionExt as _, EventMask, Window},
    rust_connection::RustConnection,
};

use crate::{build_key_events, char_to_x11_keycode};

/// Sends committed text to Wine/Proton windows via raw XSendEvent.
///
/// Wine's XIM implementation mishandles `XIM_COMMIT`, so we bypass it by
/// synthesising `KeyPress`/`KeyRelease` pairs for each character.
pub struct WineXimBridge {
    conn: Arc<RustConnection>,
    net_wm_pid: xproto::Atom,
}

impl WineXimBridge {
    pub fn new(conn: Arc<RustConnection>, net_wm_pid: xproto::Atom) -> Self {
        Self { conn, net_wm_pid }
    }

    /// Returns `true` when `window` belongs to a Wine/Proton/Steam process.
    pub fn is_wine_window(&self, window: Window) -> bool {
        let Some(pid) = self.read_pid(window) else {
            return false;
        };
        is_wine_process(pid)
    }

    fn read_pid(&self, window: Window) -> Option<u32> {
        let reply = self
            .conn
            .get_property(
                false,
                window,
                self.net_wm_pid,
                xproto::AtomEnum::CARDINAL,
                0,
                1,
            )
            .ok()?
            .reply()
            .ok()?;
        let bytes = reply.value;
        if bytes.len() < 4 {
            return None;
        }
        Some(u32::from_ne_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    /// Commit `text` to `window` using `XSendEvent` `KeyPress`/`KeyRelease` pairs.
    pub fn commit(&self, window: Window, root: Window, text: &str) -> Result<()> {
        let mut sent = 0usize;
        for ch in text.chars() {
            let keycode = char_to_x11_keycode(ch);
            if keycode == 0 {
                continue;
            }
            let state: u16 = if ch.is_uppercase() { 0x0001 } else { 0 };
            for ev in build_key_events(keycode, state, window, root) {
                self.conn
                    .send_event(false, window, EventMask::NO_EVENT, ev)
                    .map_err(|e| PlatformError::X11(e.to_string()))?
                    .check()
                    .map_err(|e| PlatformError::X11(e.to_string()))?;
            }
            sent += 1;
        }
        self.conn
            .flush()
            .map_err(|e| PlatformError::X11(e.to_string()))?;
        debug!("Wine XSendEvent: {sent} key(s) sent to {window:#x}");
        Ok(())
    }
}

/// Returns `true` when `/proc/<pid>/exe` resolves to a Wine/Proton/Steam binary.
pub fn is_wine_process(pid: u32) -> bool {
    match fs::read_link(format!("/proc/{pid}/exe")) {
        Ok(path) => {
            let s = path.to_string_lossy();
            s.contains("wine") || s.contains("proton") || s.contains("steam")
        }
        Err(_) => false,
    }
}
