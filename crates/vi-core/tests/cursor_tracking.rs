//! Regression tests for `PreeditText::cursor_byte_offset`.
//!
//! Verifies that:
//! 1. After every composition step the cursor byte offset is sourced from the
//!    struct field and equals the NFC byte length of the preedit at that step.
//! 2. For multi-byte Vietnamese codepoints the byte offset differs from the
//!    Unicode scalar (char) count, confirming the field is necessary.
//! 3. For pure ASCII preedit the byte offset equals both byte length and char
//!    count (regression guard).

use vi_core::{CompositionEngine, InputEvent, InputMethod, Key, Modifiers, StandardEngine, StateTransition};

fn telex() -> StandardEngine {
    StandardEngine::new(InputMethod::Telex)
}

fn kd(c: char) -> InputEvent {
    InputEvent::KeyDown(Key::Char(c), Modifiers::none())
}

/// Type "việt" via the Telex sequence v-i-e-e-j-t.
///
/// After each keystroke that produces a `PreeditUpdated` transition the
/// `cursor_byte_offset` must equal the NFC byte length of the preedit string.
/// For steps that produce a multi-byte codepoint the byte length exceeds the
/// Unicode scalar count, so a naive `.chars().count()` cursor would be wrong.
#[test]
fn viet_telex_cursor_byte_offset_tracks_nfc_end() {
    let mut engine = telex();

    for ch in ['v', 'i', 'e', 'e', 'j', 't'] {
        let t = engine.process(&kd(ch)).unwrap();
        let _ = engine.process(&InputEvent::KeyUp(Key::Char(ch)));
        if let StateTransition::PreeditUpdated(ref p) = t {
            let s = p.as_str();
            let cursor = p.cursor_byte_offset;

            // cursor_byte_offset must equal the NFC byte length at this step.
            assert_eq!(
                cursor,
                s.len(),
                "after '{}': cursor_byte_offset={} but NFC byte length={} for preedit '{}'",
                ch, cursor, s.len(), s,
            );

            // For multi-byte codepoints (Vietnamese), byte length > char count.
            // A cursor expressed as char count would be smaller than the correct
            // byte offset and would place the cursor inside a multi-byte sequence.
            if s.chars().count() < s.len() {
                assert_ne!(
                    cursor,
                    s.chars().count(),
                    "multi-byte preedit '{}': cursor_byte_offset ({}) must differ \
                     from char count ({}) — byte offset is correct, char count is not",
                    s, cursor, s.chars().count(),
                );
            }
        }
    }
}

/// Regression guard: for a pure ASCII word the cursor byte offset equals both
/// the byte length and the Unicode scalar count.
#[test]
fn ascii_cursor_byte_offset_equals_byte_len() {
    let mut engine = telex();

    // 'n' is inert in Telex/VNI/VIQR: no composition rule fires, so each
    // keystroke just appends to the preedit without transforming any char.
    for i in 0..4usize {
        let t = engine.process(&kd('n')).unwrap();
        if let StateTransition::PreeditUpdated(ref p) = t {
            let s = p.as_str();
            // Byte length == char count for ASCII.
            assert_eq!(
                p.cursor_byte_offset,
                s.len(),
                "ASCII step {}: cursor_byte_offset={} len={}",
                i, p.cursor_byte_offset, s.len(),
            );
            assert_eq!(
                p.cursor_byte_offset,
                s.chars().count(),
                "ASCII step {}: cursor_byte_offset must equal char count",
                i,
            );
        }
    }
}
