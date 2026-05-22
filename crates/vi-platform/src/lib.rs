//! Deprecated: use the vime-* equivalent crate instead.

pub use vi_core as core;

use vi_core::{InputEvent, NfcString, PreeditText};

pub use error::PlatformError;

pub type Result<T> = std::result::Result<T, PlatformError>;

mod error;

/// Text surrounding the current cursor position, as reported by the application.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct SurroundingText {
    /// The text surrounding the cursor in the focused widget.
    pub text: String,
    /// UTF-8 byte offset into `text` (not the document).
    ///
    /// Must satisfy `cursor_pos <= text.len()` and must land on a valid UTF-8
    /// character boundary.
    pub cursor_pos: u32,
    /// UTF-8 byte offset of the selection anchor.
    ///
    /// Equals `cursor_pos` when there is no selection.  Must satisfy the same
    /// boundary constraints as `cursor_pos`.
    pub anchor_pos: u32,
}

impl SurroundingText {
    /// Check that `cursor_pos` and `anchor_pos` are within bounds and on valid
    /// UTF-8 character boundaries.
    pub fn validate(&self) -> Result<()> {
        let len = self.text.len();
        let cursor = self.cursor_pos as usize;
        let anchor = self.anchor_pos as usize;
        if cursor > len {
            return Err(PlatformError::InvalidSurroundingText(format!(
                "cursor_pos {cursor} exceeds text length {len}"
            )));
        }
        if !self.text.is_char_boundary(cursor) {
            return Err(PlatformError::InvalidSurroundingText(format!(
                "cursor_pos {cursor} is not a UTF-8 character boundary"
            )));
        }
        if anchor > len {
            return Err(PlatformError::InvalidSurroundingText(format!(
                "anchor_pos {anchor} exceeds text length {len}"
            )));
        }
        if !self.text.is_char_boundary(anchor) {
            return Err(PlatformError::InvalidSurroundingText(format!(
                "anchor_pos {anchor} is not a UTF-8 character boundary"
            )));
        }
        Ok(())
    }
}

/// Character-count cursor position passed to [`ImeBackend::update_preedit`].
///
/// Backends and compositors (IBus, Wayland text-input-v3) expect a cursor
/// expressed as the number of Unicode scalar values before the insertion
/// point, not as a UTF-8 byte offset.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CharCursor(pub usize);

/// Return the [`CharCursor`] for the end of `s` (i.e. `s.chars().count()`).
pub fn char_cursor(s: &str) -> CharCursor {
    CharCursor(s.chars().count())
}

/// Capabilities advertised by or requested from the application / compositor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Capabilities {
    pub surrounding_text: bool,
    pub preedit: bool,
    pub lookup_table: bool,
}

/// The platform-agnostic interface every backend must implement.
///
/// Implementations are driven by the daemon which calls these methods in
/// response to composition-engine output. The trait is object-safe so the
/// daemon can hold `Arc<dyn ImeBackend>`.
///
/// # Thread safety
///
/// The trait requires `Send + Sync` because tokio tasks may call backend
/// methods from any thread — not necessarily the thread that created the
/// backend.  Every implementation must therefore be safe to share across
/// threads without external synchronisation.
pub trait ImeBackend: Send + Sync {
    /// Send committed (NFC-normalised) text to the focused application.
    ///
    /// The caller guarantees that `text` is NFC-normalised.  The
    /// implementation must forward the exact string to the focused widget
    /// via whatever protocol the backend uses (D-Bus, Wayland, XIM, …).
    fn commit(&self, text: &NfcString) -> Result<()>;

    /// Update the in-progress preedit string shown inside the application.
    ///
    /// `cursor` is the number of Unicode scalar values before the insertion
    /// point (not a byte offset).  Callers must ensure `cursor.0` does not
    /// exceed `text.chars().count()`; passing an out-of-range value is a
    /// programming error and implementations may return
    /// [`PlatformError::BadCursorOffset`].  Implementations that cannot
    /// report the error should clamp silently.
    fn update_preedit(&self, text: &PreeditText, cursor: CharCursor) -> Result<()>;

    /// Clear the preedit region without committing any text.
    ///
    /// Equivalent to calling `update_preedit("", CharCursor(0))`.
    /// Callers invoke this when composition is cancelled or reset.
    /// Implementations must ensure the preedit decoration is removed from
    /// the focused widget.
    fn clear_preedit(&self) -> Result<()>;

    /// Forward a raw key event to the focused application unchanged.
    ///
    /// The caller guarantees that the event was not consumed by the
    /// composition engine.  Implementations must deliver the keystroke to
    /// the focused widget as if the IM had never seen it.  Backends that
    /// have no forwarding mechanism (e.g. portal) should drop the event and
    /// return `Ok(())`.
    fn forward_key(&self, key: &InputEvent) -> Result<()>;

    /// Return the text surrounding the cursor, if available.
    ///
    /// Returns `Ok(Some(_))` when the backend has fresh surrounding-text
    /// data from the focused application, `Ok(None)` when the backend
    /// supports surrounding text but none has been received yet, and
    /// `Err(PlatformError::SurroundingTextUnavailable)` when the backend
    /// does not support the feature at all.
    fn surrounding_text(&self) -> Result<Option<SurroundingText>>;

    /// Atomically clear the preedit and commit text in a single protocol frame.
    ///
    /// The default implementation calls [`clear_preedit`] (ignoring errors) and
    /// then [`commit`], which works correctly for backends whose "clear" and
    /// "commit" operations are independent protocol messages.
    ///
    /// **Wayland override required**: `zwp_input_method_v2` uses a serial
    /// counter — every `im.commit(serial)` consumes one serial slot.  Sending
    /// `clear_preedit` and `commit` as separate calls means two `im.commit(N)`
    /// requests with the **same** serial, because the compositor only increments
    /// the expected serial when it sends a `done` event (which does not happen
    /// synchronously between the two client requests).  Mutter/GNOME silently
    /// drops the second request, so the text is never inserted.  The Wayland
    /// backend therefore overrides this method to fold both operations into a
    /// single `im.commit(serial)` call.
    fn commit_replacing_preedit(&self, text: &NfcString) -> Result<()> {
        self.clear_preedit().ok();
        self.commit(text)
    }

    /// Commit text and immediately set a new preedit string in one logical operation.
    ///
    /// The default implementation calls [`commit`] (ignoring errors) and then
    /// [`update_preedit`], which is correct for backends where those are
    /// independent operations.
    ///
    /// **Wayland override required**: same serial-collision hazard as
    /// [`commit_replacing_preedit`] — two separate `im.commit(serial)` calls
    /// consume the same serial and the compositor drops the second one.  The
    /// Wayland backend overrides this to fold both operations into a single
    /// `im.commit(serial)` call.
    fn commit_then_update_preedit(
        &self,
        text: &NfcString,
        preedit: &PreeditText,
        cursor: CharCursor,
    ) -> Result<()> {
        self.commit(text)?;
        self.update_preedit(preedit, cursor)
    }

    /// Abort the current composition session without committing any text.
    ///
    /// Called when an external event (focus loss, escape key, etc.) requires
    /// the preedit to be discarded entirely.  The default no-op
    /// implementation is suitable for backends that handle cancellation
    /// implicitly (e.g. by responding to a focus-out signal from the
    /// compositor).  Backends that must send an explicit cancel message
    /// (e.g. Wayland `zwp_input_method_v2.deactivate`) should override this.
    fn cancel_composition(&self) -> Result<()> {
        Ok(())
    }

    /// Return capabilities the backend can provide / has been granted.
    ///
    /// The caller uses this to decide which optional features (surrounding
    /// text, preedit, lookup tables) to request from the composition engine.
    /// Implementations should reflect the capabilities actually negotiated
    /// with the compositor or input-method framework.
    fn capabilities(&self) -> Capabilities;
}
