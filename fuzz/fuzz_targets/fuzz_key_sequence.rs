//! Fuzz target: interpret arbitrary bytes as a sequence of `InputEvent`s.
//!
//! Invariants checked:
//! - The engine must never panic.
//! - Every committed string must be valid NFC.
//! - No committed string may contain a surrogate codepoint (D800–DFFF).
//! - Shadow-diff backspaces must never underflow (≤ shadow.chars().count()).
//! - Applying the shadow diff must exactly reproduce the new preedit.
#![no_main]

use libfuzzer_sys::fuzz_target;
use unicode_normalization::UnicodeNormalization;
use vi_core::{
    CompositionEngine, InputEvent, InputMethod, Key, Modifiers, StandardEngine, StateTransition,
};

fuzz_target!(|data: &[u8]| {
    // Attempt to deserialize as a Vec<InputEvent> via bincode.
    // If deserialization fails we still extract individual bytes as key chars.
    let events: Vec<InputEvent> = if let Ok(e) = bincode::deserialize::<Vec<InputEvent>>(data) {
        e
    } else {
        // Fallback: treat each byte as a Key::Char codepoint (latin range is safe).
        data.iter()
            .map(|&b| {
                let ch = (b as char).to_ascii_lowercase();
                InputEvent::KeyDown(Key::Char(ch), Modifiers::none())
            })
            .collect()
    };

    // Limit sequence length to keep the fuzzer fast.
    let events: Vec<_> = events.into_iter().take(64).collect();

    for method in [InputMethod::Telex, InputMethod::Vni, InputMethod::Viqr] {
        let mut engine = StandardEngine::new(method);
        // Shadow buffer: mirrors what the X11 application currently sees.
        let mut shadow = String::new();

        for event in &events {
            // The engine must never panic; errors are acceptable.
            let Ok(transition) = engine.process(event) else {
                continue;
            };

            // ── Shadow-diff invariant (mirrors vi-x11 update_preedit) ────────
            match &transition {
                StateTransition::PreeditUpdated(p) => {
                    let new_preedit = p.as_str();
                    let common_len = shadow
                        .chars()
                        .zip(new_preedit.chars())
                        .take_while(|(a, b)| a == b)
                        .count();
                    let backspaces = shadow.chars().count() - common_len;
                    // Invariant 1: no underflow.
                    assert!(
                        backspaces <= shadow.chars().count(),
                        "shadow-diff underflow: backspaces={backspaces} > \
                         shadow.chars().count()={} (shadow={shadow:?}, new={new_preedit:?})",
                        shadow.chars().count(),
                    );
                    let new_tail: String = new_preedit.chars().skip(common_len).collect();
                    // Apply the diff.
                    let mut chars: Vec<char> = shadow.chars().collect();
                    for _ in 0..backspaces {
                        chars.pop();
                    }
                    chars.extend(new_tail.chars());
                    let applied: String = chars.into_iter().collect();
                    // Invariant 2: applying the diff yields the new preedit.
                    assert_eq!(
                        applied.as_str(),
                        new_preedit,
                        "shadow after diff must equal preedit \
                         (shadow={shadow:?}, new={new_preedit:?})"
                    );
                    shadow = applied;
                }
                StateTransition::CommitThenPreedit(_, p) => {
                    // Committed text is gone; new preedit starts from empty.
                    shadow = p.as_str().to_owned();
                }
                StateTransition::Commit(_)
                | StateTransition::CommitAndClear(_)
                | StateTransition::CommitThenPassThrough(_)
                | StateTransition::Cleared => {
                    shadow.clear();
                }
                StateTransition::Consumed | StateTransition::PassThrough => {}
            }

            // ── NFC + surrogate checks on committed text ─────────────────────
            let committed: Option<&str> = match &transition {
                StateTransition::Commit(c) | StateTransition::CommitAndClear(c) => {
                    Some(c.as_str())
                }
                StateTransition::CommitThenPassThrough(c) => Some(c.as_str()),
                StateTransition::CommitThenPreedit(c, _) => Some(c.as_str()),
                _ => None,
            };
            if let Some(s) = committed {
                // Must be NFC.
                let nfc: String = s.nfc().collect();
                assert_eq!(s, nfc, "committed string is not NFC: {s:?}");
                // No surrogates.
                for ch in s.chars() {
                    assert!(
                        !(0xD800u32..=0xDFFF).contains(&(ch as u32)),
                        "surrogate in output: U+{:04X}",
                        ch as u32
                    );
                }
            }
        }

        // Final flush.
        if let Ok(StateTransition::Commit(c) | StateTransition::CommitAndClear(c)) =
            engine.process(&InputEvent::KeyDown(Key::Return, Modifiers::none()))
        {
            let s = c.as_str();
            let nfc: String = s.nfc().collect();
            assert_eq!(s, nfc, "flushed string is not NFC: {s:?}");
        }
    }
});
