#![allow(clippy::unwrap_used)]
//! Key-repeat tests: simulating 50 repeats of a held key must not produce
//! duplicate commits.

use unicode_normalization::UnicodeNormalization;
use vi_core::{
    CompositionEngine, InputEvent, InputMethod, Key, Modifiers, StandardEngine, StateTransition,
};

const REPEATS: usize = 50;

fn kd(c: char) -> InputEvent {
    InputEvent::KeyDown(Key::Char(c), Modifiers::none())
}

fn kr(c: char) -> InputEvent {
    InputEvent::KeyRepeat(Key::Char(c), Modifiers::none())
}

fn is_commit(t: &StateTransition) -> bool {
    matches!(t, StateTransition::Commit(_) | StateTransition::CommitAndClear(_))
}

/// Holding a non-rule character for 50 repeats must not commit; the single
/// final Return commits exactly once.
#[test]
fn plain_char_repeat_no_spurious_commit() {
    for method in [InputMethod::Telex, InputMethod::Vni, InputMethod::Viqr] {
        let mut engine = StandardEngine::new(method);
        let mut commits = 0usize;

        engine.process(&kd('n')).unwrap();
        for _ in 0..REPEATS {
            let r = engine.process(&kr('n')).unwrap();
            assert!(!is_commit(&r), "KeyRepeat must not commit for {method:?}");
        }
        if is_commit(&engine.process(&InputEvent::KeyDown(Key::Return, Modifiers::none())).unwrap()) {
            commits += 1;
        }
        assert_eq!(commits, 1, "expected exactly one commit for {method:?}");
    }
}

/// After the first KeyDown(Return) commits, further KeyRepeat(Return) events
/// must return PassThrough without a second commit.
#[test]
fn return_repeat_does_not_double_commit() {
    let mut engine = StandardEngine::new(InputMethod::Telex);
    engine.process(&kd('a')).unwrap();

    let first = engine.process(&InputEvent::KeyDown(Key::Return, Modifiers::none())).unwrap();
    assert!(is_commit(&first), "first Return must commit");

    for _ in 0..REPEATS {
        let r = engine
            .process(&InputEvent::KeyRepeat(Key::Return, Modifiers::none()))
            .unwrap();
        assert_eq!(
            r,
            StateTransition::PassThrough,
            "KeyRepeat(Return) must PassThrough after preedit is cleared"
        );
    }
}

/// Holding Backspace for more repeats than there are characters in the preedit
/// must drain it cleanly without any panic or underflow.
#[test]
fn backspace_repeat_drains_preedit_cleanly() {
    let mut engine = StandardEngine::new(InputMethod::Telex);
    for ch in ['t', 'o', 'o', 'i'] {
        engine.process(&kd(ch)).unwrap();
        let _ = engine.process(&InputEvent::KeyUp(Key::Char(ch)));
    }
    assert_eq!(engine.preedit().as_str(), "tôi");

    for _ in 0..REPEATS {
        let _ = engine.process(&InputEvent::KeyRepeat(Key::Backspace, Modifiers::none()));
    }
    assert!(engine.preedit().is_empty(), "preedit must be empty after excess backspace repeats");
}

/// Holding a Telex tone-key ('s' = sắc) for 50 repeats keeps preedit NFC and
/// never produces a commit.
#[test]
fn tone_key_repeat_preedit_remains_nfc() {
    let mut engine = StandardEngine::new(InputMethod::Telex);
    engine.process(&kd('a')).unwrap();
    engine.process(&kd('s')).unwrap(); // "as" → "á"

    for _ in 0..REPEATS {
        let r = engine.process(&kr('s')).unwrap();
        assert!(!is_commit(&r), "tone KeyRepeat must not commit");
        let p = engine.preedit().as_str().to_owned();
        let nfc: String = p.nfc().collect();
        assert_eq!(p, nfc, "preedit not NFC after tone repeat: {p:?}");
    }
}

/// Holding a VNI digit key for 50 repeats must not commit; preedit stays NFC.
#[test]
fn vni_digit_repeat_no_spurious_commit() {
    let mut engine = StandardEngine::new(InputMethod::Vni);
    engine.process(&kd('a')).unwrap();

    for _ in 0..REPEATS {
        let r = engine.process(&kr('1')).unwrap();
        assert!(!is_commit(&r), "VNI digit KeyRepeat must not commit");
        let p = engine.preedit().as_str().to_owned();
        let nfc: String = p.nfc().collect();
        assert_eq!(p, nfc, "VNI preedit not NFC after digit repeat: {p:?}");
    }
}
