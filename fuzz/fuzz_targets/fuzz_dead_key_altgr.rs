//! Fuzz target: dead-key state × AltGr modifier × Vietnamese character sequences.
//!
//! Exercises interactions between Key::DeadKey, Modifiers::altgr(), and normal
//! Vietnamese composition to catch state-machine bugs at their intersection.
//!
//! Invariants:
//! - Engine must never panic on any combination of these events.
//! - Every committed string must be valid NFC.
//! - No committed string may contain a codepoint in the surrogate range U+D800–U+DFFF.
#![no_main]

use libfuzzer_sys::fuzz_target;
use unicode_normalization::UnicodeNormalization;
use vi_core::{
    CompositionEngine, InputEvent, InputMethod, Key, Modifiers, StandardEngine, StateTransition,
};

/// Dead-key identifiers recognised by the engine.
const DEAD_KEYS: &[char] = &['^', '(', '+', 'd', '~', '.', '`', '\'', '?'];
/// Vietnamese-relevant characters for all three input methods.
const VI_CHARS: &[char] = &[
    'a', 'e', 'i', 'o', 'u', 'y', 'n', 't', 's', 'f', 'r', 'x', 'j', 'w', 'd',
    '1', '2', '3', '4', '5', '6', '7', '8', '9',
];

fuzz_target!(|data: &[u8]| {
    if data.is_empty() {
        return;
    }

    let method = match data[0] % 3 {
        0 => InputMethod::Telex,
        1 => InputMethod::Vni,
        _ => InputMethod::Viqr,
    };

    let mut engine = StandardEngine::new(method);

    for (i, &byte) in data[1..].iter().enumerate().take(64) {
        let event = match byte % 5 {
            0 => {
                // Regular Vietnamese character, no modifier.
                let ch = VI_CHARS[(byte as usize) % VI_CHARS.len()];
                InputEvent::KeyDown(Key::Char(ch), Modifiers::none())
            }
            1 => {
                // Dead key, no modifier.
                let dk = DEAD_KEYS[(byte as usize) % DEAD_KEYS.len()];
                InputEvent::KeyDown(Key::DeadKey(dk), Modifiers::none())
            }
            2 => {
                // Vietnamese character with AltGr — engine must flush and forward.
                let ch = VI_CHARS[(byte as usize) % VI_CHARS.len()];
                InputEvent::KeyDown(Key::Char(ch), Modifiers::altgr())
            }
            3 => {
                // Dead key with AltGr.
                let dk = DEAD_KEYS[i % DEAD_KEYS.len()];
                InputEvent::KeyDown(Key::DeadKey(dk), Modifiers::altgr())
            }
            _ => {
                // Lifecycle events to vary engine state.
                match byte % 3 {
                    0 => InputEvent::FocusOut,
                    1 => InputEvent::FocusIn,
                    _ => InputEvent::Reset,
                }
            }
        };

        if let Ok(transition) = engine.process(&event) {
            check_invariants(transition);
        }
    }

    // Final flush.
    if let Ok(t) =
        engine.process(&InputEvent::KeyDown(Key::Return, Modifiers::none()))
    {
        check_invariants(t);
    }
});

fn check_invariants(transition: StateTransition) {
    let text = match transition {
        StateTransition::Commit(c) | StateTransition::CommitAndClear(c) => {
            c.as_str().to_owned()
        }
        StateTransition::CommitThenPreedit(c, _) => c.as_str().to_owned(),
        _ => return,
    };

    let nfc: String = text.nfc().collect();
    assert_eq!(text, nfc, "committed string is not NFC: {text:?}");

    for ch in text.chars() {
        let cp = ch as u32;
        assert!(
            !(0xD800..=0xDFFF).contains(&cp),
            "surrogate codepoint U+{cp:04X} in output {text:?}"
        );
    }
}
