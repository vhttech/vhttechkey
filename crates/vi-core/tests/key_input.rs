#![allow(clippy::unwrap_used)]
//! Hardening tests for the key-input pipeline.
//!
//! Covers:
//!  • Dead-key combinations for all four Vietnamese diacritics (^, (, +, d).
//!  • Non-combining dead-key sequences emit the literal dead char.
//!  • AltGr / Mod5 passthrough — Vietnamese rules must not fire when altgr is set.
//!  • Repeat-guard — two `KeyDown` for the same character without `KeyUp` between
//!    are suppressed (bounce); after `KeyUp`, re-presses are never suppressed.
//!  • Proptest: arbitrary modifier combinations never panic or produce
//!    non-NFC output.

use std::{thread::sleep, time::Duration};

use proptest::prelude::*;
use unicode_normalization::UnicodeNormalization;
use vi_core::{
    CompositionEngine, InputEvent, InputMethod, Key, Modifiers, StandardEngine, StateTransition,
};

// ── Helpers ───────────────────────────────────────────────────────────────────

fn telex() -> StandardEngine {
    StandardEngine::new(InputMethod::Telex)
}

fn char_down(c: char) -> InputEvent {
    InputEvent::KeyDown(Key::Char(c), Modifiers::none())
}

fn dead(c: char) -> InputEvent {
    InputEvent::KeyDown(Key::DeadKey(c), Modifiers::none())
}

fn key_up(c: char) -> InputEvent {
    InputEvent::KeyUp(Key::Char(c))
}

fn ret() -> InputEvent {
    InputEvent::KeyDown(Key::Return, Modifiers::none())
}

/// Feed `events` into a fresh Telex engine, flush with Return, and return the
/// committed string.
fn commit_sequence(events: &[InputEvent]) -> String {
    let mut e = telex();
    let mut last = String::new();
    for ev in events {
        if let Ok(t) = e.process(ev) {
            if let Some(s) = extract_committed(t) {
                last = s;
            }
        }
    }
    if let Ok(t) = e.process(&ret()) {
        if let Some(s) = extract_committed(t) {
            last = s;
        }
    }
    last
}

fn extract_committed(t: StateTransition) -> Option<String> {
    match t {
        StateTransition::CommitAndClear(c) | StateTransition::Commit(c) => {
            Some(c.as_str().to_owned())
        }
        StateTransition::CommitThenPreedit(c, _) => Some(c.as_str().to_owned()),
        _ => None,
    }
}

// ── Dead key: circumflex (^) ──────────────────────────────────────────────────

#[test]
fn dead_circumflex_a_produces_a_circ() {
    assert_eq!(commit_sequence(&[dead('^'), char_down('a')]), "â");
}

#[test]
fn dead_circumflex_e_produces_e_circ() {
    assert_eq!(commit_sequence(&[dead('^'), char_down('e')]), "ê");
}

#[test]
fn dead_circumflex_o_produces_o_circ() {
    assert_eq!(commit_sequence(&[dead('^'), char_down('o')]), "ô");
}

#[test]
fn dead_circumflex_uppercase_a_produces_a_circ_upper() {
    assert_eq!(commit_sequence(&[dead('^'), char_down('A')]), "Â");
}

// ── Dead key: breve (() → ă ──────────────────────────────────────────────────

#[test]
fn dead_breve_a_produces_a_breve() {
    assert_eq!(commit_sequence(&[dead('('), char_down('a')]), "ă");
}

#[test]
fn dead_breve_uppercase_a_produces_a_breve_upper() {
    assert_eq!(commit_sequence(&[dead('('), char_down('A')]), "Ă");
}

// ── Dead key: horn (+) → ơ / ư ────────────────────────────────────────────────

#[test]
fn dead_horn_o_produces_o_horn() {
    assert_eq!(commit_sequence(&[dead('+'), char_down('o')]), "ơ");
}

#[test]
fn dead_horn_u_produces_u_horn() {
    assert_eq!(commit_sequence(&[dead('+'), char_down('u')]), "ư");
}

#[test]
fn dead_horn_uppercase_o_produces_o_horn_upper() {
    assert_eq!(commit_sequence(&[dead('+'), char_down('O')]), "Ơ");
}

#[test]
fn dead_horn_uppercase_u_produces_u_horn_upper() {
    assert_eq!(commit_sequence(&[dead('+'), char_down('U')]), "Ư");
}

// ── Dead key: stroke (d) → đ ──────────────────────────────────────────────────

#[test]
fn dead_stroke_d_produces_d_stroke() {
    assert_eq!(commit_sequence(&[dead('d'), char_down('d')]), "đ");
}

#[test]
fn dead_stroke_uppercase_d_produces_d_stroke_upper() {
    assert_eq!(commit_sequence(&[dead('d'), char_down('D')]), "Đ");
}

// ── Dead key + tone mark ──────────────────────────────────────────────────────

/// Dead circumflex + a → â, then 's' → ấ (sắc on â).
#[test]
fn dead_circumflex_a_then_tone_sac() {
    let mut e = telex();
    e.process(&dead('^')).unwrap();
    e.process(&char_down('a')).unwrap(); // â
    e.process(&char_down('s')).unwrap(); // ấ
    assert_eq!(e.preedit().as_str(), "ấ");
}

/// Dead horn + o → ơ, then 'j' → ợ (nặng on ơ).
#[test]
fn dead_horn_o_then_tone_nang() {
    let mut e = telex();
    e.process(&dead('+')).unwrap();
    e.process(&char_down('o')).unwrap(); // ơ
    e.process(&char_down('j')).unwrap(); // ợ
    assert_eq!(e.preedit().as_str(), "ợ");
}

// ── Dead key: non-combining emits literal, then processes next char ───────────

#[test]
fn dead_circumflex_n_emits_literal_then_n() {
    // ^ does not combine with 'n'; engine must not panic and preedit must
    // contain both the dead char literal and 'n'.
    let mut e = telex();
    e.process(&dead('^')).unwrap();
    e.process(&char_down('n')).unwrap();
    let p = e.preedit().as_str().to_owned();
    // '^' literal + 'n' via Telex (no rule for ['^','n'])
    assert!(p.contains('n'), "preedit must contain 'n': {p:?}");
    // Must be valid UTF-8 and NFC
    let nfc: String = p.nfc().collect();
    assert_eq!(p, nfc, "preedit not NFC: {p:?}");
}

#[test]
fn dead_horn_b_emits_literal_then_b() {
    // + does not combine with 'b'
    let mut e = telex();
    e.process(&dead('+')).unwrap();
    e.process(&char_down('b')).unwrap();
    let p = e.preedit().as_str().to_owned();
    assert!(p.contains('b'), "preedit must contain 'b': {p:?}");
}

// ── Consecutive dead keys emit first as literal ──────────────────────────────

#[test]
fn two_consecutive_dead_keys_emits_first_literal() {
    let mut e = telex();
    e.process(&dead('^')).unwrap(); // pending = '^'
    // Pressing another dead key must emit '^' as literal and set new pending
    e.process(&dead('(')).unwrap();
    // Buffer now has '^' as literal; pending = '('
    // Then 'a' combines with '('
    e.process(&char_down('a')).unwrap();
    let p = e.preedit().as_str().to_owned();
    // preedit should contain '^' and 'ă' (combined) → "^ă"
    assert!(p.contains('^'), "literal '^' must be in preedit: {p:?}");
    assert!(p.contains('ă'), "combined 'ă' must be in preedit: {p:?}");
}

// ── AltGr / Mod5 passthrough ─────────────────────────────────────────────────

#[test]
fn altgr_with_no_preedit_passes_through() {
    let mut e = telex();
    let t = e
        .process(&InputEvent::KeyDown(Key::Char('e'), Modifiers::altgr()))
        .unwrap();
    assert_eq!(t, StateTransition::PassThrough, "AltGr on empty preedit must PassThrough");
}

#[test]
fn altgr_commits_preedit_without_applying_rules() {
    let mut e = telex();
    // Without AltGr: typing 'o' then 'o' would produce ô (Telex vowel rule).
    e.process(&char_down('o')).unwrap();

    // AltGr + 'o': must commit the pending 'o' and NOT fire the oo→ô rule.
    let t = e
        .process(&InputEvent::KeyDown(Key::Char('o'), Modifiers::altgr()))
        .unwrap();
    assert!(
        matches!(t, StateTransition::CommitAndClear(_)),
        "AltGr must commit pending preedit; got {t:?}"
    );
    if let StateTransition::CommitAndClear(committed) = t {
        assert_eq!(committed.as_str(), "o", "committed text must be plain 'o', not 'ô'");
    }
    assert!(e.preedit().is_empty(), "preedit must be empty after AltGr commit");
}

#[test]
fn altgr_does_not_apply_telex_vowel_rules() {
    // Full 'oo'→'ô' sequence but second 'o' has AltGr set.
    let mut e = telex();
    e.process(&char_down('o')).unwrap();
    assert_eq!(e.preedit().as_str(), "o");

    // Explicit Mod5 (altgr) flag — must commit 'o' and not produce 'ô'.
    let mods = Modifiers { altgr: true, ..Modifiers::none() };
    let t = e.process(&InputEvent::KeyDown(Key::Char('o'), mods)).unwrap();
    match t {
        StateTransition::CommitAndClear(c) => {
            assert_eq!(c.as_str(), "o", "AltGr+o must not fire oo→ô Telex rule");
        }
        other => panic!("expected CommitAndClear, got {other:?}"),
    }
}

#[test]
fn altgr_clears_pending_dead_key() {
    // If a dead key is pending and AltGr fires, the dead key must be discarded.
    let mut e = telex();
    e.process(&dead('^')).unwrap();
    // AltGr on empty preedit (dead key was pending but buf is empty)
    let t = e
        .process(&InputEvent::KeyDown(Key::Char('e'), Modifiers::altgr()))
        .unwrap();
    // Engine committed nothing (buf was empty), returned PassThrough.
    assert_eq!(t, StateTransition::PassThrough);
    // pending_dead_key must be cleared; next char should not combine.
    e.process(&char_down('a')).unwrap();
    assert_eq!(
        e.preedit().as_str(),
        "a",
        "dead key must be cleared by AltGr; 'a' must not combine with prior '^'"
    );
}

// ── Repeat suppression ────────────────────────────────────────────────────────

#[test]
fn sub_threshold_duplicate_keydown_is_suppressed() {
    // Two KeyDown events for the same char in rapid succession (no KeyUp between)
    // must result in the second being consumed (ghost press guard).
    let mut e = telex();
    e.process(&char_down('a')).unwrap();
    assert_eq!(e.preedit().as_str(), "a");

    // Second press arrives immediately (no KeyUp between).
    let t = e.process(&char_down('a')).unwrap();
    assert_eq!(
        t,
        StateTransition::Consumed,
        "second rapid KeyDown must be Consumed (repeat guard)"
    );
    // Preedit must still be just "a" — the duplicate was not appended.
    assert_eq!(e.preedit().as_str(), "a");
}

#[test]
fn keyup_resets_repeat_guard_allowing_legitimate_repress() {
    // After `KeyUp`, a second `KeyDown` for the same character is a new press,
    // not electrical bounce (bounce is only: two `KeyDown` without `KeyUp`).
    let mut e = telex();
    e.process(&char_down('a')).unwrap();
    e.process(&key_up('a')).unwrap();

    let t = e.process(&char_down('a')).unwrap();
    assert_ne!(
        t,
        StateTransition::Consumed,
        "press after KeyUp must not be suppressed"
    );
    // Preedit should now have two 'a' characters ("aa" → "â" via Telex)
    assert_eq!(e.preedit().as_str(), "â");
}

#[test]
fn above_threshold_duplicate_keydown_is_not_suppressed() {
    // Two presses separated by `KeyUp` are never suppressed, regardless of delay.
    let mut e = telex();
    e.process(&char_down('a')).unwrap();
    e.process(&key_up('a')).unwrap();

    let t = e.process(&char_down('a')).unwrap();
    assert!(
        !matches!(t, StateTransition::Consumed),
        "press after KeyUp must not be suppressed"
    );
}

/// Bounce guard: two `KeyDown` for the same character **without** `KeyUp` in
/// between are always suppressed, even if wall-clock time passes (no ms timer).
#[test]
fn nested_duplicate_keydown_still_suppressed_after_sleep_without_keyup() {
    let mut e = telex();
    e.process(&char_down('z')).unwrap();
    sleep(Duration::from_millis(25));
    let t = e.process(&char_down('z')).unwrap();
    assert_eq!(
        t,
        StateTransition::Consumed,
        "second KeyDown before KeyUp must stay Consumed (nested bounce)"
    );
}

/// After `KeyUp`, the same character may be typed again after a delay.
#[test]
fn after_keyup_same_char_after_delay_is_not_suppressed() {
    let mut e = telex();
    e.process(&char_down('z')).unwrap();
    e.process(&key_up('z')).unwrap();
    sleep(Duration::from_millis(25));
    let t = e.process(&char_down('z')).unwrap();
    assert_ne!(
        t,
        StateTransition::Consumed,
        "KeyDown after KeyUp must not be suppressed"
    );
}

fn arb_modifiers() -> impl Strategy<Value = Modifiers> {
    (
        any::<bool>(),
        any::<bool>(),
        any::<bool>(),
        any::<bool>(),
        any::<bool>(),
        any::<bool>(),
    )
        .prop_map(|(shift, ctrl, alt, super_key, caps_lock, altgr)| Modifiers {
            shift,
            ctrl,
            alt,
            super_key,
            caps_lock,
            altgr,
        })
}

fn arb_key() -> impl Strategy<Value = Key> {
    prop_oneof![
        (0x20u32..=0x7Eu32).prop_map(|c| Key::Char(char::from_u32(c).unwrap())),
        Just(Key::Backspace),
        Just(Key::Return),
        Just(Key::Escape),
        Just(Key::DeadKey('^')),
        Just(Key::DeadKey('(')),
        Just(Key::DeadKey('+')),
        Just(Key::DeadKey('d')),
        Just(Key::ComposeKey),
    ]
}

fn arb_event_with_mods() -> impl Strategy<Value = InputEvent> {
    prop_oneof![
        (arb_key(), arb_modifiers()).prop_map(|(k, m)| InputEvent::KeyDown(k, m)),
        arb_key().prop_map(InputEvent::KeyUp),
        Just(InputEvent::FocusIn),
        Just(InputEvent::FocusOut),
        Just(InputEvent::Reset),
    ]
}

proptest! {
    /// Random modifier combinations must never cause a panic or produce
    /// non-NFC preedit / committed text.
    #[test]
    fn random_modifier_combinations_never_panic_or_produce_nfd(
        events in prop::collection::vec(arb_event_with_mods(), 1..40),
    ) {
        for method in [InputMethod::Telex, InputMethod::Vni, InputMethod::Viqr] {
            let mut engine = StandardEngine::new(method);
            for event in &events {
                let result = engine.process(event);
                prop_assert!(result.is_ok(), "engine error: {:?}", result.err());

                if let Ok(t) = result {
                    match t {
                        StateTransition::CommitAndClear(c)
                        | StateTransition::Commit(c) => {
                            let s = c.as_str();
                            let nfc: String = s.nfc().collect();
                            prop_assert_eq!(s, &nfc, "committed text not NFC");
                        }
                        StateTransition::CommitThenPreedit(c, _) => {
                            let s = c.as_str();
                            let nfc: String = s.nfc().collect();
                            prop_assert_eq!(s, &nfc, "committed text not NFC");
                        }
                        _ => {}
                    }
                }

                // Preedit must always be NFC.
                let p = engine.preedit();
                let s = p.as_str();
                let nfc: String = s.nfc().collect();
                prop_assert_eq!(s, &nfc, "preedit not NFC after event");
            }
        }
    }

    /// Rapid identical key-down events (simulating hardware bounce) must not
    /// double-commit: at most one commit per logical key-press sequence.
    #[test]
    fn rapid_identical_keydowns_do_not_double_commit(
        ch in (0x61u32..=0x7Au32).prop_map(|c| char::from_u32(c).unwrap()),
        extra_presses in 1usize..10,
    ) {
        let mut e = StandardEngine::new(InputMethod::Telex);
        let mut commits = 0usize;

        let first = e.process(&InputEvent::KeyDown(Key::Char(ch), Modifiers::none())).unwrap();
        if matches!(first, StateTransition::Commit(_) | StateTransition::CommitAndClear(_)) {
            commits += 1;
        }

        for _ in 0..extra_presses {
            let r = e.process(&InputEvent::KeyDown(Key::Char(ch), Modifiers::none())).unwrap();
            if matches!(r, StateTransition::Commit(_) | StateTransition::CommitAndClear(_)) {
                commits += 1;
            }
        }

        // Rapid presses without KeyUp should be suppressed; at most 1 commit.
        prop_assert!(
            commits <= 1,
            "rapid duplicate KeyDowns must not produce more than one commit; got {commits}"
        );
    }
}
