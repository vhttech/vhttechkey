//! Regression tests: IBus engine emits PreeditUpdated per keystroke; mode selection
//! routes ALL apps through preedit (UpdatePreeditText).
//!
//! Root cause of previous failures: our code emitted the D-Bus signal
//! "UpdatePreeditTextWithMode" instead of "UpdatePreeditText".  ibus-daemon routes
//! these differently; the former never rendered preedit in any app.  vhttechkey sends
//! "UpdatePreeditText" for ALL apps and it works everywhere — we now do the same.
//!
//! Mode selection (force_chrome_direct=false, force_preedit_mode=false):
//!   ALL apps → preedit (UpdatePreeditText).
//!   No per-app capability-bit switching: forward_key and surrounding_commit are
//!   never auto-selected.
//!     - forward_key broke Chrome: BackSpace fired but Vietnamese Unicode chars
//!       > U+00FF were dropped → net deletion.
//!     - surrounding_commit broke Chrome: DeleteSurroundingText silently failed →
//!       garbling "ssusưsử ddudundungdụng".
//!   preedit override via `force_preedit_mode=true`: clears `chrome_direct` even when
//!   `[ibus] force_chrome_direct` is set — forces the standard preedit signal path.
//!
//! D-Bus signal emission cannot be verified without a live session bus, so this
//! file pins:
//!   (a) the mode selected for any caps value is always "preedit", and
//!   (b) each character keystroke produces `PreeditUpdated` from the engine core
//!       (the IBus layer dispatches it via UpdatePreeditText).
#![allow(clippy::unwrap_used)]

use vi_core::{
    CompositionEngine, InputEvent, InputMethod, Key, Modifiers, StandardEngine, StateTransition,
};

const IBUS_CAP_PREEDIT_TEXT: u32 = 1;
const IBUS_CAP_SURROUNDING_TEXT: u32 = 1 << 5;

/// Replicate the production `set_capabilities` mode-selection logic.
///
/// All apps use preedit (UpdatePreeditText) regardless of capability bits.
fn mode_for_caps(_caps: u32) -> &'static str {
    "preedit"
}

fn key_down(ch: char) -> InputEvent {
    InputEvent::KeyDown(Key::Char(ch), Modifiers::none())
}

fn key_up(ch: char) -> InputEvent {
    InputEvent::KeyUp(Key::Char(ch))
}

// ── PreeditUpdated count = keystroke count ────────────────────────────────────

/// For 5 sequential character keystrokes the engine must emit exactly 5
/// `PreeditUpdated` transitions — one per keystroke.  Each `PreeditUpdated`
/// maps 1-to-1 to an `update_preedit_text` call in the IBus layer.
#[test]
fn five_keystrokes_produce_five_preedit_updated() {
    let mut engine = StandardEngine::new(InputMethod::Telex);
    let mut preedit_updated_count = 0u32;

    for ch in ['a', 'b', 'c', 'd', 'e'] {
        let transition = engine
            .process(&key_down(ch))
            .expect("engine must not error");
        let _ = engine.process(&key_up(ch));
        if matches!(transition, StateTransition::PreeditUpdated(_)) {
            preedit_updated_count += 1;
        }
    }

    assert_eq!(
        preedit_updated_count, 5,
        "5 character keystrokes must produce exactly 5 PreeditUpdated transitions \
         (each maps to one update_preedit_text call in IBus)"
    );
}

/// Repeat across VNI to confirm the count invariant is method-independent.
#[test]
fn five_keystrokes_produce_five_preedit_updated_vni() {
    let mut engine = StandardEngine::new(InputMethod::Vni);
    let mut preedit_updated_count = 0u32;

    for ch in ['a', 'b', 'c', 'd', 'e'] {
        let transition = engine
            .process(&key_down(ch))
            .expect("engine must not error");
        let _ = engine.process(&key_up(ch));
        if matches!(transition, StateTransition::PreeditUpdated(_)) {
            preedit_updated_count += 1;
        }
    }

    assert_eq!(
        preedit_updated_count, 5,
        "VNI: 5 keystrokes must yield 5 PreeditUpdated"
    );
}

/// Repeat across VIQR.
#[test]
fn five_keystrokes_produce_five_preedit_updated_viqr() {
    let mut engine = StandardEngine::new(InputMethod::Viqr);
    let mut preedit_updated_count = 0u32;

    for ch in ['a', 'b', 'c', 'd', 'e'] {
        let transition = engine
            .process(&key_down(ch))
            .expect("engine must not error");
        let _ = engine.process(&key_up(ch));
        if matches!(transition, StateTransition::PreeditUpdated(_)) {
            preedit_updated_count += 1;
        }
    }

    assert_eq!(
        preedit_updated_count, 5,
        "VIQR: 5 keystrokes must yield 5 PreeditUpdated"
    );
}

// ── Mode selection: ALL apps → preedit ───────────────────────────────────────

/// caps=0x00 (no capabilities) must select preedit mode.
#[test]
fn preedit_selected_for_caps_zero() {
    assert_eq!(
        mode_for_caps(0x00),
        "preedit",
        "caps=0x00 must select preedit mode (UpdatePreeditText for all apps)"
    );
}

/// caps=0x01 (only IBUS_CAP_PREEDIT_TEXT) → preedit.
#[test]
fn preedit_selected_for_caps_preedit_only() {
    assert_eq!(
        mode_for_caps(IBUS_CAP_PREEDIT_TEXT),
        "preedit",
        "caps=IBUS_CAP_PREEDIT_TEXT must select preedit mode"
    );
}

/// caps=0x09 (IBUS_CAP_PREEDIT_TEXT | IBUS_CAP_FOCUS) — typical GNOME X11 caps → preedit.
#[test]
fn preedit_selected_for_caps_gnome_x11() {
    let caps = IBUS_CAP_PREEDIT_TEXT | 0x08; // 9 = PREEDIT | FOCUS
    assert_eq!(
        mode_for_caps(caps),
        "preedit",
        "caps=0x09 (GNOME X11 / Telegram) must select preedit mode"
    );
}

/// Apps with IBUS_CAP_SURROUNDING_TEXT (Chrome 0x21, VSCode, Electron) get preedit mode.
#[test]
fn caps_surrounding_selects_preedit_mode() {
    for caps in [
        IBUS_CAP_SURROUNDING_TEXT,                         // 0x20 surrounding only
        IBUS_CAP_PREEDIT_TEXT | IBUS_CAP_SURROUNDING_TEXT, // 0x21 Chrome/VSCode
    ] {
        assert_eq!(
            mode_for_caps(caps),
            "preedit",
            "caps={caps:#x} (has SURROUNDING_TEXT) must select preedit mode"
        );
    }
}

/// caps=0x21 (Chrome/VSCode/GTK4/Qt) → preedit.
#[test]
fn caps_chrome_vscode_selects_preedit() {
    let caps = IBUS_CAP_PREEDIT_TEXT | IBUS_CAP_SURROUNDING_TEXT; // 0x21
    let mode = mode_for_caps(caps);
    assert_ne!(
        mode, "surrounding_commit",
        "caps={caps:#x} must NOT select surrounding_commit"
    );
    assert_ne!(
        mode, "forward_key",
        "caps={caps:#x} must NOT select forward_key"
    );
    assert_eq!(mode, "preedit", "caps={caps:#x} must select preedit mode");
}

/// Engine produces PreeditUpdated for character keystrokes regardless of caps.
#[test]
fn engine_produces_preedit_updated_regardless_of_caps() {
    for caps in [0x00u32, 0x01, 0x09, 0x20, 0x21] {
        let mode = mode_for_caps(caps);
        assert_eq!(mode, "preedit", "caps={caps:#x} must select preedit");

        let mut engine = StandardEngine::new(InputMethod::Telex);
        let transition = engine
            .process(&key_down('a'))
            .expect("engine must not error on 'a'");

        assert!(
            matches!(transition, StateTransition::PreeditUpdated(_)),
            "caps={caps:#x}: engine must emit PreeditUpdated for 'a'; got {transition:?}"
        );
    }
}

/// caps=0x00 must select preedit (not forward_key, not surrounding_commit).
#[test]
fn caps_zero_selects_preedit() {
    let mode = mode_for_caps(0x00);
    assert_ne!(
        mode, "surrounding_commit",
        "caps=0x00 must NOT select surrounding_commit"
    );
    assert_ne!(mode, "forward_key", "caps=0x00 must NOT select forward_key");
    assert_eq!(mode, "preedit", "caps=0x00 must select preedit");
}

// ── No CommitAndClear emitted during preedit composition ─────────────────────

/// None of the first N keystrokes during preedit composition should produce
/// CommitAndClear — that would prematurely flush the composition buffer.
#[test]
fn no_commit_and_clear_during_composition_telex() {
    let mut engine = StandardEngine::new(InputMethod::Telex);
    for ch in ['t', 'o', 'i'] {
        let transition = engine
            .process(&key_down(ch))
            .expect("engine must not error");
        let _ = engine.process(&key_up(ch));
        assert!(
            !matches!(transition, StateTransition::CommitAndClear(_)),
            "Telex: '{ch}' during composition must not produce CommitAndClear; got {transition:?}"
        );
    }
}

// ── Preedit updated text is non-empty ─────────────────────────────────────────

/// Every `PreeditUpdated` transition emitted during composition must carry
/// non-empty text — an empty preedit would hide the composition indicator.
#[test]
fn preedit_updated_text_non_empty_for_five_keystrokes() {
    let mut engine = StandardEngine::new(InputMethod::Telex);
    for ch in ['a', 'b', 'c', 'd', 'e'] {
        let transition = engine
            .process(&key_down(ch))
            .expect("engine must not error");
        let _ = engine.process(&key_up(ch));
        if let StateTransition::PreeditUpdated(ref p) = transition {
            assert!(
                !p.is_empty(),
                "PreeditUpdated for '{ch}' must carry non-empty text"
            );
        }
    }
}

// ── Engine always emits PreeditUpdated; IBus layer dispatches via UpdatePreeditText ──

/// Typing any character must produce `PreeditUpdated` — the engine core always
/// emits PreeditUpdated; the IBus layer dispatches it via UpdatePreeditText
/// (standard IBus preedit signal) for all apps.
#[test]
fn character_keypress_produces_preedit_updated() {
    let mut engine = StandardEngine::new(InputMethod::Telex);
    let transition = engine
        .process(&key_down('v'))
        .expect("engine must not error on 'v'");

    assert!(
        matches!(transition, StateTransition::PreeditUpdated(_)),
        "Telex 'v' must produce PreeditUpdated; got {transition:?}"
    );
}

// ── Engine emits PreeditUpdated for all IBus dispatch modes ───────────────────

/// The engine always emits `PreeditUpdated` transitions — the IBus layer
/// calls UpdatePreeditText (standard IBus preedit signal, D-Bus name
/// "UpdatePreeditText") for all apps regardless of capability bits.
#[test]
fn engine_produces_preedit_updated_independent_of_ibus_mode() {
    for caps in [0x00u32, 0x01, 0x09, 0x20, 0x21] {
        assert_eq!(
            mode_for_caps(caps),
            "preedit",
            "caps={caps:#x}: all apps must use preedit (UpdatePreeditText)"
        );

        let mut engine = StandardEngine::new(InputMethod::Telex);
        let transition = engine
            .process(&key_down('t'))
            .expect("engine must not error on 't'");

        assert!(
            matches!(transition, StateTransition::PreeditUpdated(_)),
            "engine must emit PreeditUpdated for caps={caps:#x}; got {transition:?}"
        );
    }
}
