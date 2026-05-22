//! Fuzz target: interpret arbitrary bytes as a sequence of `InputEvent`s fed
//! to the vi-core composition engine.
//!
//! Invariants:
//! - The engine must never panic.
//! - Every committed `NfcString` must be valid NFC.
//! - No committed string may contain a surrogate code point (U+D800–U+DFFF).
//!
//! NOTE: superseded by fuzz_key_sequence.rs — kept for historical corpus compatibility.
#![no_main]

use libfuzzer_sys::fuzz_target;
use unicode_normalization::UnicodeNormalization;
use vi_core::{
    CompositionEngine, InputEvent, InputMethod, Key, Modifiers, StandardEngine, StateTransition,
};

fuzz_target!(|data: &[u8]| {
    let events: Vec<InputEvent> = data
        .chunks(3)
        .take(64)
        .map(|chunk| {
            let b = chunk[0];
            match b {
                0 => InputEvent::KeyDown(Key::Backspace, Modifiers::none()),
                1 => InputEvent::KeyDown(Key::Escape, Modifiers::none()),
                2 => InputEvent::KeyDown(Key::Return, Modifiers::none()),
                3 => InputEvent::KeyDown(Key::Tab, Modifiers::none()),
                4 => InputEvent::KeyDown(Key::Left, Modifiers::none()),
                5 => InputEvent::KeyDown(Key::Right, Modifiers::none()),
                _ => {
                    let ch = char::from_u32(b as u32).unwrap_or('a');
                    InputEvent::KeyDown(Key::Char(ch), Modifiers::none())
                }
            }
        })
        .collect();

    for method in [InputMethod::Telex, InputMethod::Vni, InputMethod::Viqr] {
        let mut engine = StandardEngine::new(method);

        for event in &events {
            if let Ok(transition) = engine.process(event) {
                match transition {
                    StateTransition::Commit(c) | StateTransition::CommitAndClear(c) => {
                        let s = c.as_str();
                        let normalized: String = s.nfc().collect();
                        assert_eq!(s, normalized, "committed string is not NFC");
                        for ch in s.chars() {
                            assert!(
                                !matches!(ch as u32, 0xD800..=0xDFFF),
                                "surrogate in commit output: U+{:04X}",
                                ch as u32
                            );
                        }
                    }
                    _ => {}
                }
            }
        }

        // Final flush.
        if let Ok(StateTransition::Commit(c) | StateTransition::CommitAndClear(c)) =
            engine.process(&InputEvent::KeyDown(Key::Return, Modifiers::none()))
        {
            let s = c.as_str();
            let normalized: String = s.nfc().collect();
            assert_eq!(s, normalized, "flushed string is not NFC");
        }
    }
});
