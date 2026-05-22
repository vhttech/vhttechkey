//! Regression tests for SurroundingText event handling in the composition engine.
//!
//! These tests verify that:
//! 1. The engine stores a SurroundingText event without affecting active composition.
//! 2. Backspace with an empty preedit re-enters composition when the character
//!    immediately before the cursor is a Vietnamese precomposed syllable.
//! 3. Backspace with a non-Vietnamese character before the cursor passes through.

use vi_core::{InputEvent, InputMethod, Key, Modifiers};
use vi_testing::integration::IntegrationHarness;

fn key_down(c: char) -> InputEvent {
    InputEvent::KeyDown(Key::Char(c), Modifiers::none())
}

fn backspace() -> InputEvent {
    InputEvent::KeyDown(Key::Backspace, Modifiers::none())
}

/// SurroundingText event is silently absorbed; preedit stays empty.
#[tokio::test]
async fn surrounding_text_consumed_silently() {
    let h = IntegrationHarness::new(InputMethod::Telex);
    h.send_event(InputEvent::SurroundingText {
        text: "ắ".to_owned(),
        cursor: 3,
    })
    .await;
    assert_eq!(h.preedit(), "");
    assert!(h.committed().is_empty());
}

/// Backspace with a Vietnamese precomposed character before the cursor
/// re-enters composition with the base vowel (tone mark stripped).
#[tokio::test]
async fn backspace_vietnamese_char_reenter_composition() {
    // "ắ" (U+1EAF = ă + acute) has base "ă".
    let h = IntegrationHarness::new(InputMethod::Telex);
    h.send_event(InputEvent::SurroundingText {
        text: "ắ".to_owned(),
        cursor: 3, // byte length of "ắ" in UTF-8
    })
    .await;
    h.send_event(backspace()).await;
    assert_eq!(h.preedit(), "ă");
    assert!(h.committed().is_empty());
}

/// After re-entering composition the user can apply a new tone rule.
#[tokio::test]
async fn backspace_reenter_then_retone() {
    // Surrounding text ends with "ắ"; backspace gives preedit "ă";
    // pressing 's' in Telex applies sắc tone → "ắ" again.
    let h = IntegrationHarness::new(InputMethod::Telex);
    h.send_event(InputEvent::SurroundingText {
        text: "ắ".to_owned(),
        cursor: 3,
    })
    .await;
    h.send_event(backspace()).await;
    assert_eq!(h.preedit(), "ă");
    h.send_event(key_down('s')).await;
    assert_eq!(h.preedit(), "ắ");
}

/// Backspace with a non-Vietnamese character before the cursor is a pass-through;
/// the engine does not start any new preedit.
#[tokio::test]
async fn backspace_non_vietnamese_passthrough() {
    let h = IntegrationHarness::new(InputMethod::Telex);
    h.send_event(InputEvent::SurroundingText {
        text: "hello".to_owned(),
        cursor: 5,
    })
    .await;
    h.send_event(backspace()).await;
    // No re-composition: preedit stays empty.
    assert_eq!(h.preedit(), "");
    assert!(h.committed().is_empty());
}

/// Backspace with no surrounding text set is a pass-through.
#[tokio::test]
async fn backspace_no_surrounding_text_passthrough() {
    let h = IntegrationHarness::new(InputMethod::Telex);
    h.send_event(backspace()).await;
    assert_eq!(h.preedit(), "");
    assert!(h.committed().is_empty());
}

/// Backspace during active composition still rolls back the preedit buffer
/// regardless of surrounding text.
#[tokio::test]
async fn backspace_during_composition_ignores_surrounding() {
    let h = IntegrationHarness::new(InputMethod::Telex);
    h.send_event(InputEvent::SurroundingText {
        text: "ắ".to_owned(),
        cursor: 3,
    })
    .await;
    // Start composing "tô".
    h.send_event(key_down('t')).await;
    h.send_event(key_down('o')).await;
    h.send_event(key_down('o')).await;
    assert_eq!(h.preedit(), "tô");
    // Backspace inside composition rolls back "tô" → "t", not re-entering from surrounding.
    h.send_event(backspace()).await;
    assert_eq!(h.preedit(), "t");
}

/// SurroundingText carrying a Vietnamese word followed by a cursor in the middle
/// correctly picks the char just before the cursor.
#[tokio::test]
async fn surrounding_text_cursor_mid_word() {
    // Text: "ắn", cursor at 3 (after "ắ", before "n").
    // Character before cursor is "ắ" → re-enter with "ă".
    let h = IntegrationHarness::new(InputMethod::Telex);
    // "ắ" = 3 bytes, "n" = 1 byte; cursor=3 points between them.
    h.send_event(InputEvent::SurroundingText {
        text: "ắn".to_owned(),
        cursor: 3,
    })
    .await;
    h.send_event(backspace()).await;
    assert_eq!(h.preedit(), "ă");
}
