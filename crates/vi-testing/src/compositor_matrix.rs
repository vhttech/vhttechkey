//! Parameterised compositor-compatibility tests.
//!
//! Defines a minimal [`ImeBackend`] trait, [`CompositorQuirks`] bit-flags, and a
//! [`MockCompositor`] backend that simulates per-compositor quirks.  Tests iterate
//! over all meaningful quirk combinations and assert core IME invariants.

use vi_core::{
    CompositionEngine, InputEvent, StandardEngine, StateTransition,
};

// ── Quirk flags ───────────────────────────────────────────────────────────────

/// Bitfield of known compositor quirks that affect IME behaviour.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CompositorQuirks(pub u32);

impl CompositorQuirks {
    pub const NONE: Self = Self(0);
    /// GNOME: suppress forwarding empty preedit-clear updates (causes cursor flicker).
    pub const GNOME_EMPTY_PREEDIT: Self = Self(1 << 0);
    /// KDE/KWin: preedit cursor offsets are byte positions, not char positions.
    pub const KDE_BYTE_OFFSETS: Self = Self(1 << 1);
    /// Hyprland: batch preedit updates and flush on commit.
    pub const HYPRLAND_BUFFERING: Self = Self(1 << 2);
    /// Sway: focus-in arrives slightly after the first key event of a new word.
    pub const SWAY_TIMING: Self = Self(1 << 3);
    /// Weston: strict text-input lifecycle — activate before preedit, deactivate after commit.
    pub const WESTON_LIFECYCLE: Self = Self(1 << 4);

    pub fn contains(self, other: Self) -> bool {
        (self.0 & other.0) == other.0
    }
}

// ── ImeBackend trait ──────────────────────────────────────────────────────────

/// Minimal interface a compositor exposes to the IME.
pub trait ImeBackend {
    fn preedit_update(&mut self, text: &str, cursor_begin: i32, cursor_end: i32);
    fn commit_string(&mut self, text: &str);
}

// ── MockCompositor ────────────────────────────────────────────────────────────

/// Recording mock compositor that enforces quirk-specific rules.
pub struct MockCompositor {
    pub quirks: CompositorQuirks,
    pub committed: Vec<String>,
    pub preedit_history: Vec<String>,
    /// Buffered preedit text under `HYPRLAND_BUFFERING`; flushed on commit.
    buffered_preedit: Option<String>,
}

impl MockCompositor {
    pub fn new(quirks: CompositorQuirks) -> Self {
        Self {
            quirks,
            committed: Vec::new(),
            preedit_history: Vec::new(),
            buffered_preedit: None,
        }
    }
}

impl ImeBackend for MockCompositor {
    fn preedit_update(&mut self, text: &str, cursor_begin: i32, cursor_end: i32) {
        if self.quirks.contains(CompositorQuirks::GNOME_EMPTY_PREEDIT) && text.is_empty() {
            return;
        }
        if self.quirks.contains(CompositorQuirks::HYPRLAND_BUFFERING) {
            self.buffered_preedit = Some(text.to_owned());
            let _ = (cursor_begin, cursor_end);
            return;
        }
        self.preedit_history.push(text.to_owned());
    }

    fn commit_string(&mut self, text: &str) {
        if self.quirks.contains(CompositorQuirks::HYPRLAND_BUFFERING) {
            if let Some(buf) = self.buffered_preedit.take() {
                self.preedit_history.push(buf);
            }
        }
        self.committed.push(text.to_owned());
    }
}

// ── Drive helper ──────────────────────────────────────────────────────────────

#[allow(dead_code)]
fn drive_engine<B: ImeBackend>(
    engine: &mut StandardEngine,
    backend: &mut B,
    events: &[InputEvent],
) {
    for event in events {
        match engine.process(event) {
            Ok(StateTransition::PreeditUpdated(p)) => {
                let text = p.as_str();
                let end = text.chars().count() as i32;
                backend.preedit_update(text, 0, end);
            }
            Ok(StateTransition::Commit(c) | StateTransition::CommitAndClear(c)) => {
                backend.commit_string(c.as_str());
                backend.preedit_update("", 0, 0);
            }
            Ok(StateTransition::CommitThenPreedit(c, p)) => {
                backend.commit_string(c.as_str());
                let text = p.as_str();
                let end = text.chars().count() as i32;
                backend.preedit_update(text, 0, end);
            }
            Ok(StateTransition::Cleared) => {
                backend.preedit_update("", 0, 0);
            }
            Ok(StateTransition::CommitThenPassThrough(c)) => {
                backend.commit_string(c.as_str());
                backend.preedit_update("", 0, 0);
            }
            Ok(StateTransition::PassThrough | StateTransition::Consumed) => {}
            Err(_) => {}
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use vi_core::{InputMethod, Key, Modifiers};

    /// Collect events for a Telex key sequence followed by Return.
    fn telex_events(keys: &str) -> Vec<InputEvent> {
        let mut events: Vec<InputEvent> = Vec::new();
        for c in keys.chars() {
            events.push(InputEvent::KeyDown(Key::Char(c), Modifiers::none()));
            events.push(InputEvent::KeyUp(Key::Char(c)));
        }
        events.push(InputEvent::KeyDown(Key::Return, Modifiers::none()));
        events
    }

    /// After FocusOut the engine preedit must be empty for every quirk combination.
    #[test]
    fn preedit_never_stuck_after_focus_out() {
        for quirk_bits in 0u32..32 {
            let quirks = CompositorQuirks(quirk_bits);
            let mut engine = StandardEngine::new(InputMethod::Telex);
            let mut compositor = MockCompositor::new(quirks);

            // Type two characters so there is a non-empty preedit, then FocusOut.
            let events = [
                InputEvent::KeyDown(Key::Char('t'), Modifiers::none()),
                InputEvent::KeyDown(Key::Char('o'), Modifiers::none()),
                InputEvent::FocusOut,
            ];
            drive_engine(&mut engine, &mut compositor, &events);

            assert!(
                engine.preedit().is_empty(),
                "engine preedit stuck after FocusOut (quirks={quirk_bits:#07b}): {:?}",
                engine.preedit().as_str()
            );

            // Under non-GNOME quirks the compositor must have received an explicit
            // empty-preedit update or a commit that consumed the pending text.
            if !quirks.contains(CompositorQuirks::GNOME_EMPTY_PREEDIT) {
                let cleared_or_committed = compositor
                    .preedit_history
                    .last()
                    .map(|s| s.is_empty())
                    .unwrap_or(false)
                    || !compositor.committed.is_empty();
                assert!(
                    cleared_or_committed,
                    "compositor shows stale preedit after FocusOut \
                     (quirks={quirk_bits:#07b}): preedit={:?}, committed={:?}",
                    compositor.preedit_history.last(),
                    compositor.committed,
                );
            }
        }
    }

    /// A completed Telex syllable followed by Return must produce exactly one
    /// commit for every quirk combination.
    #[test]
    fn commit_fires_exactly_once_per_syllable() {
        // Each pair is (telex_keys, expected_committed_text).
        let syllables: &[(&str, &str)] = &[
            ("as", "á"),  // a + sắc
            ("af", "à"),  // a + huyền
            ("ar", "ả"),  // a + hỏi
        ];

        for quirk_bits in 0u32..32 {
            let quirks = CompositorQuirks(quirk_bits);
            for &(keys, expected) in syllables {
                let mut engine = StandardEngine::new(InputMethod::Telex);
                let mut compositor = MockCompositor::new(quirks);
                drive_engine(&mut engine, &mut compositor, &telex_events(keys));

                assert_eq!(
                    compositor.committed.len(),
                    1,
                    "expected 1 commit for '{keys}' (quirks={quirk_bits:#07b}), \
                     got {}: {:?}",
                    compositor.committed.len(),
                    compositor.committed,
                );
                assert_eq!(
                    compositor.committed[0], expected,
                    "wrong committed text for '{keys}' (quirks={quirk_bits:#07b})"
                );
            }
        }
    }

    /// Under the GNOME empty-preedit workaround, commits must not be doubled.
    #[test]
    fn no_double_commit_gnome_empty_preedit() {
        let quirks = CompositorQuirks::GNOME_EMPTY_PREEDIT;
        let syllables = ["as", "af", "ar", "ax", "aj"];
        for &keys in &syllables {
            let mut engine = StandardEngine::new(InputMethod::Telex);
            let mut compositor = MockCompositor::new(quirks);
            drive_engine(&mut engine, &mut compositor, &telex_events(keys));

            assert_eq!(
                compositor.committed.len(),
                1,
                "double-commit under GNOME_EMPTY_PREEDIT for '{keys}': \
                 got {} commits: {:?}",
                compositor.committed.len(),
                compositor.committed,
            );
        }
    }

    /// Verify that KDE byte-offset quirk does not affect commit correctness.
    #[test]
    fn kde_byte_offsets_quirk_commits_correctly() {
        let quirks = CompositorQuirks::KDE_BYTE_OFFSETS;
        // "ứ" is a multi-byte character; byte offsets ≠ char offsets.
        let mut engine = StandardEngine::new(InputMethod::Telex);
        let mut compositor = MockCompositor::new(quirks);
        drive_engine(&mut engine, &mut compositor, &telex_events("uws"));

        assert_eq!(compositor.committed.len(), 1);
        assert_eq!(compositor.committed[0], "ứ");
    }

    /// Hyprland buffering must not cause missed commits: the buffered preedit is
    /// flushed and the commit arrives exactly once.
    #[test]
    fn hyprland_buffering_flushes_on_commit() {
        let quirks = CompositorQuirks::HYPRLAND_BUFFERING;
        let mut engine = StandardEngine::new(InputMethod::Telex);
        let mut compositor = MockCompositor::new(quirks);
        drive_engine(&mut engine, &mut compositor, &telex_events("oos"));

        assert_eq!(
            compositor.committed.len(),
            1,
            "expected 1 commit under HYPRLAND_BUFFERING, got {:?}",
            compositor.committed
        );
        assert_eq!(compositor.committed[0], "ố");
    }
}
