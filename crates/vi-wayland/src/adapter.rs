//! zwp_text_input_v3 state-machine adapter.
//!
//! Drives the IM's secondary text-input-v3 client binding through the full
//! lifecycle required by the protocol:
//!
//!   Enter → enable()+commit() → Done(serial) → [set_surrounding_text+commit]
//!         → Leave → disable()+commit()
//!
//! Tracking the compositor's done-event serial prevents stale state on
//! compositors (GNOME, Niri) that use it for desync detection.

use tracing::debug;
use wayland_protocols::wp::text_input::zv3::client::zwp_text_input_v3::ZwpTextInputV3;

/// State machine for the IM's zwp_text_input_v3 client binding.
///
/// One instance per bound text-input object; call the `on_*` methods directly
/// from the corresponding `Dispatch<ZwpTextInputV3, ()>` event arms.
#[derive(Debug, Default)]
pub struct TextInputAdapter {
    /// Serial from the latest compositor `done` event.
    ///
    /// Compositors that track this serial (GNOME/Mutter, Niri) silently drop
    /// commits whose serial does not match the last acknowledged state.
    pub done_serial: u32,
    enabled: bool,
}

impl TextInputAdapter {
    /// Handle an `enter` event: reset serial and send `enable()+commit()`.
    pub fn on_enter(&mut self, ti: &ZwpTextInputV3) {
        debug!("TextInput: enter → enable+commit");
        self.done_serial = 0;
        self.enabled = true;
        ti.enable();
        ti.commit();
    }

    /// Handle a `leave` event: send `disable()+commit()`.
    pub fn on_leave(&mut self, ti: &ZwpTextInputV3) {
        debug!("TextInput: leave → disable+commit");
        self.enabled = false;
        ti.disable();
        ti.commit();
    }

    /// Handle a `done { serial }` event from the compositor.
    pub fn on_done(&mut self, serial: u32) {
        self.done_serial = serial;
    }

    /// Re-acknowledge surrounding text after receiving it from the compositor.
    ///
    /// Sends `set_surrounding_text + commit` so the compositor knows the IM
    /// has processed the latest application context.  Required by Niri's
    /// dual-protocol path where both text-input-v3 and input-method-v2 must
    /// remain in sync.
    pub fn acknowledge_state(&self, ti: &ZwpTextInputV3, text: &str, cursor: i32, anchor: i32) {
        if !self.enabled {
            return;
        }
        ti.set_surrounding_text(text.to_owned(), cursor, anchor);
        ti.commit();
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }
}

/// Clamp `cursor` to the byte range `[0, text.len()]` then snap backward to
/// the nearest UTF-8 character boundary.
///
/// KWin forwards the preedit cursor position unchanged to Qt; Qt panics when
/// `cursor_begin` or `cursor_end` lie outside the preedit string's byte range
/// or straddle a multi-byte codepoint.
pub fn clamp_preedit_cursor(text: &str, cursor: i32) -> i32 {
    let max = text.len() as i32;
    let mut pos = cursor.clamp(0, max) as usize;
    while pos > 0 && !text.is_char_boundary(pos) {
        pos -= 1;
    }
    pos as i32
}

// ── Unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clamp_negative_to_zero() {
        assert_eq!(clamp_preedit_cursor("hello", -5), 0);
    }

    #[test]
    fn clamp_past_end_to_len() {
        assert_eq!(clamp_preedit_cursor("hello", 100), 5);
    }

    #[test]
    fn clamp_snaps_to_char_boundary() {
        // "ô" is 2 bytes (0xC3 0xB4); cursor=1 is mid-codepoint.
        assert_eq!(clamp_preedit_cursor("tôi", 1), 1); // 't' boundary
        assert_eq!(clamp_preedit_cursor("tôi", 2), 1); // mid-ô → snap back
        assert_eq!(clamp_preedit_cursor("tôi", 3), 3); // start of 'i'
    }

    #[test]
    fn clamp_ascii_unchanged() {
        for i in 0..=5i32 {
            assert_eq!(clamp_preedit_cursor("hello", i), i);
        }
    }

    #[test]
    fn adapter_default_not_enabled() {
        let a = TextInputAdapter::default();
        assert!(!a.is_enabled());
        assert_eq!(a.done_serial, 0);
    }
}
