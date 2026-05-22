//! Fuzz target: arbitrary sequences of Wayland-IME lifecycle events.
//!
//! Models the activate / deactivate / done(serial) / unavailable lifecycle
//! using the composition engine's FocusIn / FocusOut / Reset primitives,
//! interleaved with arbitrary key events.
//!
//! Invariants:
//! - The engine must never panic.
//! - Every committed string must be valid NFC.
//! - No committed string may contain a surrogate codepoint (D800–DFFF).
#![no_main]

use libfuzzer_sys::fuzz_target;
use unicode_normalization::UnicodeNormalization;
use vi_core::{
    CompositionEngine, InputEvent, InputMethod, Key, Modifiers, StandardEngine, StateTransition,
};

/// Verify `s` is NFC and contains no surrogates.
fn check_output(s: &str) {
    let nfc: String = s.nfc().collect();
    assert_eq!(s, nfc, "committed string is not NFC: {s:?}");
    for ch in s.chars() {
        assert!(
            !(0xD800u32..=0xDFFF).contains(&(ch as u32)),
            "surrogate codepoint in output: U+{:04X}",
            ch as u32
        );
    }
}

fuzz_target!(|data: &[u8]| {
    if data.is_empty() {
        return;
    }

    // First byte selects the input method.
    let method = match data[0] % 3 {
        0 => InputMethod::Telex,
        1 => InputMethod::Vni,
        _ => InputMethod::Viqr,
    };
    let mut engine = StandardEngine::new(method);

    // Remaining bytes drive lifecycle + key events in pairs:
    //   byte[0]: event-type selector
    //   byte[1]: payload (codepoint offset or unused)
    for chunk in data[1..].chunks(2) {
        let tag = chunk[0];
        let payload = chunk.get(1).copied().unwrap_or(0);

        let event = match tag % 8 {
            // Lifecycle events (activate / deactivate / unavailable)
            0 => InputEvent::FocusIn,
            1 => InputEvent::FocusOut,
            2 => InputEvent::Reset,
            // Printable ASCII key press
            3 => {
                let ch = char::from_u32(0x20 + (payload as u32 % 0x60)).unwrap_or('a');
                InputEvent::KeyDown(Key::Char(ch), Modifiers::none())
            }
            // Special keys
            4 => InputEvent::KeyDown(Key::Backspace, Modifiers::none()),
            5 => InputEvent::KeyDown(Key::Return, Modifiers::none()),
            6 => InputEvent::KeyDown(Key::Escape, Modifiers::none()),
            // Key repeat
            _ => {
                let ch = char::from_u32(0x20 + (payload as u32 % 0x60)).unwrap_or('a');
                InputEvent::KeyRepeat(Key::Char(ch), Modifiers::none())
            }
        };

        if let Ok(transition) = engine.process(&event) {
            match transition {
                StateTransition::Commit(c) | StateTransition::CommitAndClear(c) => {
                    check_output(c.as_str());
                }
                StateTransition::CommitThenPreedit(c, _) => {
                    check_output(c.as_str());
                }
                _ => {}
            }
        }
    }

    // Final flush.
    if let Ok(StateTransition::Commit(c) | StateTransition::CommitAndClear(c)) =
        engine.process(&InputEvent::KeyDown(Key::Return, Modifiers::none()))
    {
        check_output(c.as_str());
    }
});
