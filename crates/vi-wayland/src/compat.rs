//! Compositor-compatibility helpers (pure functions, no Wayland I/O).
//!
//! Each function encapsulates a single compositor-family rule so that call
//! sites remain free of compositor-name strings or magic constants.

use crate::quirks::CompositorProfile;

/// Return `false` when `text` should be discarded rather than stored as
/// surrounding context.
///
/// Hyprland/wlroots sends an empty `surrounding_text` event for widgets that
/// have no text (e.g. password fields or terminals in raw mode).  Storing the
/// empty string would clobber valid context accumulated from the previous focus
/// target, causing the engine to lose its position.
pub fn guard_surrounding_text(text: &str) -> bool {
    !text.is_empty()
}

/// Return `true` when the compositor requires the IM to actively manage the
/// zwp_text_input_v3 lifecycle (enable → done → disable) alongside the normal
/// zwp_input_method_v2 sequence.
///
/// - **GNOME/Mutter**: fires text-input-v3 `leave` *before* input-method-v2
///   `deactivate`, so the IM must call `disable()+commit()` on leave to avoid
///   the compositor treating the text-input session as still open.
/// - **Niri**: requires a full enable → done → disable round-trip via
///   text-input-v3 in addition to input-method-v2 events.
pub fn needs_text_input_lifecycle(profile: CompositorProfile) -> bool {
    matches!(profile, CompositorProfile::Niri | CompositorProfile::Gnome)
}

// ── Unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_text_is_rejected() {
        assert!(!guard_surrounding_text(""));
    }

    #[test]
    fn non_empty_text_is_accepted() {
        assert!(guard_surrounding_text("hello"));
        assert!(guard_surrounding_text(" "));
    }

    #[test]
    fn lifecycle_required_for_gnome_and_niri() {
        assert!(needs_text_input_lifecycle(CompositorProfile::Gnome));
        assert!(needs_text_input_lifecycle(CompositorProfile::Niri));
    }

    #[test]
    fn lifecycle_not_required_for_others() {
        assert!(!needs_text_input_lifecycle(CompositorProfile::Standard));
        assert!(!needs_text_input_lifecycle(CompositorProfile::KWin));
        assert!(!needs_text_input_lifecycle(CompositorProfile::Hyprland));
    }
}
