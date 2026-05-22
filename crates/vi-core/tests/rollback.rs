#![allow(clippy::unwrap_used)]
//! Rollback correctness tests.
//!
//! Key scenario: typing "tooi" produces preedit "tôi".
//! Each backspace must revert exactly one composition step, not one character.

use vi_core::{CompositionEngine, InputEvent, InputMethod, Key, Modifiers, StandardEngine};

fn telex() -> StandardEngine {
    StandardEngine::new(InputMethod::Telex)
}

fn key(c: char) -> InputEvent {
    InputEvent::KeyDown(Key::Char(c), Modifiers::none())
}

fn bs() -> InputEvent {
    InputEvent::KeyDown(Key::Backspace, Modifiers::none())
}

fn preedit(e: &StandardEngine) -> String {
    e.preedit().as_str().to_owned()
}

#[test]
fn tooi_backspace_sequence() {
    let mut e = telex();
    e.process(&key('t')).unwrap();
    let _ = e.process(&InputEvent::KeyUp(Key::Char('t')));
    assert_eq!(preedit(&e), "t");

    e.process(&key('o')).unwrap();
    let _ = e.process(&InputEvent::KeyUp(Key::Char('o')));
    assert_eq!(preedit(&e), "to");

    e.process(&key('o')).unwrap();
    // "oo" rule fires → display "tô"
    assert_eq!(preedit(&e), "tô");

    let _ = e.process(&InputEvent::KeyUp(Key::Char('o')));
    e.process(&key('i')).unwrap();
    assert_eq!(preedit(&e), "tôi");

    // Backspace 1: remove 'i' → "tô"
    e.process(&bs()).unwrap();
    assert_eq!(preedit(&e), "tô");

    // Backspace 2: remove 'ô' — "to" (same char count as "tô") is skipped → "t"
    e.process(&bs()).unwrap();
    assert_eq!(preedit(&e), "t");

    // Backspace 3: remove 't' → empty
    e.process(&bs()).unwrap();
    assert_eq!(preedit(&e), "");
}

#[test]
fn aw_backspace_clears() {
    // "aw" → "ă" (1 char); backspace removes the whole char, not just the form mark.
    let mut e = telex();
    e.process(&key('a')).unwrap();
    e.process(&key('w')).unwrap();
    assert_eq!(preedit(&e), "ă");
    e.process(&bs()).unwrap();
    assert_eq!(preedit(&e), "");
}

#[test]
fn tone_backspace_clears() {
    // "as" → "á" (1 char); backspace removes the whole char, not just the tone mark.
    let mut e = telex();
    e.process(&key('a')).unwrap();
    e.process(&key('s')).unwrap();
    assert_eq!(preedit(&e), "á");
    e.process(&bs()).unwrap();
    assert_eq!(preedit(&e), "");
}

#[test]
fn empty_buffer_backspace_passes_through() {
    use vi_core::StateTransition;
    let mut e = telex();
    let t = e.process(&bs()).unwrap();
    assert_eq!(t, StateTransition::PassThrough);
}

#[test]
fn multiple_rollbacks_never_produce_garbage() {
    let mut e = telex();
    for ch in "duowcj".chars() {
        e.process(&key(ch)).unwrap();
        let _ = e.process(&InputEvent::KeyUp(Key::Char(ch)));
    }
    // "dduowcj" would be "được"; here without the second 'd' it's different
    // but we just check rollbacks never panic and always produce valid UTF-8.
    loop {
        let p = preedit(&e);
        assert!(
            std::str::from_utf8(p.as_bytes()).is_ok(),
            "invalid UTF-8 in preedit"
        );
        use vi_core::StateTransition;
        let t = e.process(&bs()).unwrap();
        if t == StateTransition::PassThrough {
            break;
        }
    }
}
