//! Regression tests: realtime preedit works correctly across all backends.
//!
//! Every printable composition character must produce `PreeditUpdated` on each
//! keystroke — never `Consumed`, `PassThrough`, or `CommitAndClear` mid-word.
#![allow(clippy::unwrap_used)]

use vi_core::{
    CompositionEngine, InputEvent, InputMethod, Key, Modifiers, StandardEngine, StateTransition,
};

fn key_down(ch: char) -> InputEvent {
    InputEvent::KeyDown(Key::Char(ch), Modifiers::none())
}

fn key_up(ch: char) -> InputEvent {
    InputEvent::KeyUp(Key::Char(ch))
}

fn backspace() -> InputEvent {
    InputEvent::KeyDown(Key::Backspace, Modifiers::none())
}

/// Type a character and return the StateTransition; also send the matching
/// KeyUp so the repeat guard is reset for subsequent presses of the same key.
fn type_char(engine: &mut StandardEngine, ch: char) -> StateTransition {
    let result = engine
        .process(&key_down(ch))
        .expect("engine must not error");
    let _ = engine.process(&key_up(ch));
    result
}

// ── Every letter (a-z) produces PreeditUpdated on a fresh engine ─────────────

#[test]
fn every_letter_produces_preedit_updated_telex() {
    for ch in 'a'..='z' {
        let mut engine = StandardEngine::new(InputMethod::Telex);
        let result = engine
            .process(&key_down(ch))
            .expect("engine must not error");
        assert!(
            matches!(result, StateTransition::PreeditUpdated(_)),
            "Telex: letter '{ch}' on fresh engine must produce PreeditUpdated, got {result:?}"
        );
    }
}

#[test]
fn every_letter_produces_preedit_updated_vni() {
    for ch in 'a'..='z' {
        let mut engine = StandardEngine::new(InputMethod::Vni);
        let result = engine
            .process(&key_down(ch))
            .expect("engine must not error");
        assert!(
            matches!(result, StateTransition::PreeditUpdated(_)),
            "VNI: letter '{ch}' on fresh engine must produce PreeditUpdated, got {result:?}"
        );
    }
}

#[test]
fn every_letter_produces_preedit_updated_viqr() {
    for ch in 'a'..='z' {
        let mut engine = StandardEngine::new(InputMethod::Viqr);
        let result = engine
            .process(&key_down(ch))
            .expect("engine must not error");
        assert!(
            matches!(result, StateTransition::PreeditUpdated(_)),
            "VIQR: letter '{ch}' on fresh engine must produce PreeditUpdated, got {result:?}"
        );
    }
}

// ── Digits (0-9) typed alone produce PreeditUpdated ──────────────────────────
// Digits are not in the word-boundary commit list, so they always go to preedit.

#[test]
fn every_digit_produces_preedit_updated_telex() {
    for ch in '0'..='9' {
        let mut engine = StandardEngine::new(InputMethod::Telex);
        let result = engine
            .process(&key_down(ch))
            .expect("engine must not error");
        assert!(
            matches!(result, StateTransition::PreeditUpdated(_)),
            "Telex: digit '{ch}' on fresh engine must produce PreeditUpdated, got {result:?}"
        );
    }
}

#[test]
fn every_digit_produces_preedit_updated_vni() {
    for ch in '0'..='9' {
        let mut engine = StandardEngine::new(InputMethod::Vni);
        let result = engine
            .process(&key_down(ch))
            .expect("engine must not error");
        assert!(
            matches!(result, StateTransition::PreeditUpdated(_)),
            "VNI: digit '{ch}' on fresh engine must produce PreeditUpdated, got {result:?}"
        );
    }
}

// ── 'v','i','e','t' sequence: each keystroke produces PreeditUpdated ──────────

#[test]
fn telex_viet_sequence_all_preedit_updated() {
    let mut engine = StandardEngine::new(InputMethod::Telex);
    for ch in ['v', 'i', 'e', 't'] {
        let result = type_char(&mut engine, ch);
        assert!(
            matches!(result, StateTransition::PreeditUpdated(_)),
            "Telex viet: char '{ch}' must produce PreeditUpdated, got {result:?}"
        );
    }
    assert_eq!(engine.preedit().as_str(), "viet");
}

#[test]
fn vni_viet_sequence_all_preedit_updated() {
    let mut engine = StandardEngine::new(InputMethod::Vni);
    for ch in ['v', 'i', 'e', 't'] {
        let result = type_char(&mut engine, ch);
        assert!(
            matches!(result, StateTransition::PreeditUpdated(_)),
            "VNI viet: char '{ch}' must produce PreeditUpdated, got {result:?}"
        );
    }
    assert_eq!(engine.preedit().as_str(), "viet");
}

#[test]
fn viqr_viet_sequence_all_preedit_updated() {
    let mut engine = StandardEngine::new(InputMethod::Viqr);
    for ch in ['v', 'i', 'e', 't'] {
        let result = type_char(&mut engine, ch);
        assert!(
            matches!(result, StateTransition::PreeditUpdated(_)),
            "VIQR viet: char '{ch}' must produce PreeditUpdated, got {result:?}"
        );
    }
    assert_eq!(engine.preedit().as_str(), "viet");
}

// ── Tone mark after 'viet': 's' fires Telex tone rule but stays PreeditUpdated

#[test]
fn telex_viets_tone_mark_produces_preedit_updated_not_commit() {
    let mut engine = StandardEngine::new(InputMethod::Telex);
    for ch in ['v', 'i', 'e', 't'] {
        type_char(&mut engine, ch);
    }
    // 's' in Telex applies the sắc tone mark. The engine must return
    // PreeditUpdated — not CommitAndClear or PassThrough.
    let result = type_char(&mut engine, 's');
    assert!(
        matches!(result, StateTransition::PreeditUpdated(_)),
        "Telex: 's' after 'viet' (tone mark) must produce PreeditUpdated, got {result:?}"
    );
    assert!(
        !engine.preedit().is_empty(),
        "preedit must remain non-empty after tone mark applied to 'viet'"
    );
}

#[test]
fn telex_aas_tone_mark_produces_preedit_updated() {
    // 'aa' → 'â' (vowel form rule), then 's' adds sắc → 'ấ'.
    // Each step must produce PreeditUpdated.
    let mut engine = StandardEngine::new(InputMethod::Telex);
    let a1 = type_char(&mut engine, 'a');
    assert!(
        matches!(a1, StateTransition::PreeditUpdated(_)),
        "'a' must give PreeditUpdated"
    );
    let a2 = type_char(&mut engine, 'a');
    assert!(
        matches!(a2, StateTransition::PreeditUpdated(_)),
        "second 'a' must give PreeditUpdated"
    );
    let s = type_char(&mut engine, 's');
    assert!(
        matches!(s, StateTransition::PreeditUpdated(_)),
        "'s' after 'aa' (tone on 'â') must give PreeditUpdated, got {s:?}"
    );
    assert_eq!(engine.preedit().as_str(), "ấ");
}

// ── Backspace mid-word: both backspaces produce PreeditUpdated (not PassThrough)

#[test]
fn backspace_mid_word_produces_preedit_updated_telex() {
    let mut engine = StandardEngine::new(InputMethod::Telex);
    for ch in ['a', 'b', 'c', 'd'] {
        type_char(&mut engine, ch);
    }
    assert_eq!(engine.preedit().as_str(), "abcd");

    let bs1 = engine
        .process(&backspace())
        .expect("backspace must not error");
    assert!(
        matches!(bs1, StateTransition::PreeditUpdated(_)),
        "first backspace on non-empty preedit must produce PreeditUpdated, got {bs1:?}"
    );
    assert_eq!(engine.preedit().as_str(), "abc");

    let bs2 = engine
        .process(&backspace())
        .expect("backspace must not error");
    assert!(
        matches!(bs2, StateTransition::PreeditUpdated(_)),
        "second backspace on non-empty preedit must produce PreeditUpdated, got {bs2:?}"
    );
    assert_eq!(engine.preedit().as_str(), "ab");
}

#[test]
fn backspace_mid_word_produces_preedit_updated_vni() {
    let mut engine = StandardEngine::new(InputMethod::Vni);
    for ch in ['a', 'b', 'c', 'd'] {
        type_char(&mut engine, ch);
    }
    let bs1 = engine
        .process(&backspace())
        .expect("backspace must not error");
    assert!(
        matches!(bs1, StateTransition::PreeditUpdated(_)),
        "VNI: first backspace must produce PreeditUpdated, got {bs1:?}"
    );
    let bs2 = engine
        .process(&backspace())
        .expect("backspace must not error");
    assert!(
        matches!(bs2, StateTransition::PreeditUpdated(_)),
        "VNI: second backspace must produce PreeditUpdated, got {bs2:?}"
    );
}

#[test]
fn backspace_mid_word_produces_preedit_updated_viqr() {
    let mut engine = StandardEngine::new(InputMethod::Viqr);
    for ch in ['a', 'b', 'c', 'd'] {
        type_char(&mut engine, ch);
    }
    let bs1 = engine
        .process(&backspace())
        .expect("backspace must not error");
    assert!(
        matches!(bs1, StateTransition::PreeditUpdated(_)),
        "VIQR: first backspace must produce PreeditUpdated, got {bs1:?}"
    );
    let bs2 = engine
        .process(&backspace())
        .expect("backspace must not error");
    assert!(
        matches!(bs2, StateTransition::PreeditUpdated(_)),
        "VIQR: second backspace must produce PreeditUpdated, got {bs2:?}"
    );
}

// ── PreeditUpdated must never carry empty text ────────────────────────────────

#[test]
fn preedit_updated_never_empty_telex() {
    for ch in 'a'..='z' {
        let mut engine = StandardEngine::new(InputMethod::Telex);
        let result = engine
            .process(&key_down(ch))
            .expect("engine must not error");
        if let StateTransition::PreeditUpdated(ref p) = result {
            assert!(
                !p.is_empty(),
                "Telex: PreeditUpdated for '{ch}' must not carry empty text"
            );
        }
    }
}

#[test]
fn preedit_updated_never_empty_vni() {
    for ch in 'a'..='z' {
        let mut engine = StandardEngine::new(InputMethod::Vni);
        let result = engine
            .process(&key_down(ch))
            .expect("engine must not error");
        if let StateTransition::PreeditUpdated(ref p) = result {
            assert!(
                !p.is_empty(),
                "VNI: PreeditUpdated for '{ch}' must not carry empty text"
            );
        }
    }
}

#[test]
fn preedit_updated_never_empty_viqr() {
    for ch in 'a'..='z' {
        let mut engine = StandardEngine::new(InputMethod::Viqr);
        let result = engine
            .process(&key_down(ch))
            .expect("engine must not error");
        if let StateTransition::PreeditUpdated(ref p) = result {
            assert!(
                !p.is_empty(),
                "VIQR: PreeditUpdated for '{ch}' must not carry empty text"
            );
        }
    }
}

// ── No CommitAndClear on plain composition characters ────────────────────────

#[test]
fn plain_letters_never_produce_commit_and_clear_telex() {
    for ch in 'a'..='z' {
        let mut engine = StandardEngine::new(InputMethod::Telex);
        let result = engine
            .process(&key_down(ch))
            .expect("engine must not error");
        assert!(
            !matches!(result, StateTransition::CommitAndClear(_)),
            "Telex: plain letter '{ch}' on fresh engine must not produce CommitAndClear, got {result:?}"
        );
    }
}

#[test]
fn plain_letters_never_produce_pass_through_telex() {
    for ch in 'a'..='z' {
        let mut engine = StandardEngine::new(InputMethod::Telex);
        let result = engine
            .process(&key_down(ch))
            .expect("engine must not error");
        assert!(
            !matches!(result, StateTransition::PassThrough),
            "Telex: plain letter '{ch}' on fresh engine must not produce PassThrough, got {result:?}"
        );
    }
}

// ── VNI-specific: vowel modifier digits stay in preedit ──────────────────────

#[test]
fn vni_vowel_modifier_produces_preedit_updated() {
    // VNI: a6 → â. Both 'a' and '6' must produce PreeditUpdated.
    let mut engine = StandardEngine::new(InputMethod::Vni);
    let a = type_char(&mut engine, 'a');
    assert!(
        matches!(a, StateTransition::PreeditUpdated(_)),
        "VNI: 'a' must produce PreeditUpdated"
    );
    let six = type_char(&mut engine, '6');
    assert!(
        matches!(six, StateTransition::PreeditUpdated(_)),
        "VNI: '6' after 'a' (vowel modifier a6→â) must produce PreeditUpdated, got {six:?}"
    );
    assert_eq!(engine.preedit().as_str(), "â");
}

#[test]
fn vni_tone_digit_produces_preedit_updated() {
    // VNI: a1 → á. '1' (tone mark) must produce PreeditUpdated, not CommitAndClear.
    let mut engine = StandardEngine::new(InputMethod::Vni);
    type_char(&mut engine, 'a');
    let one = type_char(&mut engine, '1');
    assert!(
        matches!(one, StateTransition::PreeditUpdated(_)),
        "VNI: '1' after 'a' (tone digit a1→á) must produce PreeditUpdated, got {one:?}"
    );
    assert_eq!(engine.preedit().as_str(), "á");
}

// ── VIQR-specific: composition punctuation stays in preedit ──────────────────

#[test]
fn viqr_hat_vowel_modifier_produces_preedit_updated() {
    // VIQR: a^ → â. '^' must produce PreeditUpdated.
    let mut engine = StandardEngine::new(InputMethod::Viqr);
    type_char(&mut engine, 'a');
    let hat = engine
        .process(&key_down('^'))
        .expect("engine must not error");
    assert!(
        matches!(hat, StateTransition::PreeditUpdated(_)),
        "VIQR: '^' after 'a' (a^→â) must produce PreeditUpdated, got {hat:?}"
    );
    assert_eq!(engine.preedit().as_str(), "â");
}

#[test]
fn viqr_tilde_tone_mark_produces_preedit_updated() {
    // VIQR: a~ → ã. '~' must produce PreeditUpdated.
    let mut engine = StandardEngine::new(InputMethod::Viqr);
    type_char(&mut engine, 'a');
    let tilde = engine
        .process(&key_down('~'))
        .expect("engine must not error");
    assert!(
        matches!(tilde, StateTransition::PreeditUpdated(_)),
        "VIQR: '~' after 'a' (a~→ã) must produce PreeditUpdated, got {tilde:?}"
    );
    assert_eq!(engine.preedit().as_str(), "ã");
}
