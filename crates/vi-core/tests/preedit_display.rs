#![allow(clippy::unwrap_used)]
//! Regression tests verifying the preedit display path end-to-end.
//!
//! Covers:
//!  • Every character keypress emits PreeditUpdated (not PassThrough/Consumed).
//!  • Telex composing sequences produce the correct intermediate preedit at
//!    each step: t→"t", to→"to", too→"tô", tooi→"tôi".
//!  • Return on a non-empty buffer commits and clears (not PassThrough).
//!  • Ctrl+char on a non-empty buffer commits and clears (not PassThrough).
//!  • Rapid-typing stress tests: 50 distinct KeyDown and 100 KeyRepeat events
//!    all produce PreeditUpdated or CommitThenPreedit (no silent drops).

use vi_core::{
    CompositionEngine, InputEvent, InputMethod, Key, Modifiers, StandardEngine, StateTransition,
};

fn telex() -> StandardEngine {
    StandardEngine::new(InputMethod::Telex)
}

fn kd(c: char) -> InputEvent {
    InputEvent::KeyDown(Key::Char(c), Modifiers::none())
}

fn ku(c: char) -> InputEvent {
    InputEvent::KeyUp(Key::Char(c))
}

fn kr(c: char) -> InputEvent {
    InputEvent::KeyRepeat(Key::Char(c), Modifiers::none())
}

fn ret() -> InputEvent {
    InputEvent::KeyDown(Key::Return, Modifiers::none())
}

fn ctrl(c: char) -> InputEvent {
    InputEvent::KeyDown(
        Key::Char(c),
        Modifiers {
            ctrl: true,
            ..Modifiers::none()
        },
    )
}

// ── Single-keypress emission ───────────────────────────────────────────────────

#[test]
fn every_alpha_keypress_returns_preedit_updated() {
    // Each letter pressed on a fresh engine must return PreeditUpdated.
    // A new engine is used per letter so there is no accumulated composition state
    // and no repeat-guard timer to contend with.
    for c in 'a'..='z' {
        let mut e = telex();
        let t = e.process(&kd(c)).unwrap();
        assert!(
            matches!(t, StateTransition::PreeditUpdated(_)),
            "KeyDown('{c}') on a fresh engine must return PreeditUpdated, got {t:?}"
        );
    }
}

// ── Telex intermediate preedit states ─────────────────────────────────────────

#[test]
fn telex_composing_sequence_t_to_toi() {
    // Walk through the "tooi" sequence one key at a time and verify both the
    // transition variant and the preedit string at every step.
    // KeyUp is sent between repeated characters so the repeat guard does not
    // suppress the second 'o' keypress.
    let mut e = telex();

    let t = e.process(&kd('t')).unwrap();
    assert!(
        matches!(t, StateTransition::PreeditUpdated(_)),
        "step 't': {t:?}"
    );
    assert_eq!(e.preedit().as_str(), "t");

    e.process(&ku('t')).unwrap();
    let t = e.process(&kd('o')).unwrap();
    assert!(
        matches!(t, StateTransition::PreeditUpdated(_)),
        "step 'o': {t:?}"
    );
    assert_eq!(e.preedit().as_str(), "to");

    e.process(&ku('o')).unwrap();
    let t = e.process(&kd('o')).unwrap(); // oo → ô rule fires
    assert!(
        matches!(t, StateTransition::PreeditUpdated(_)),
        "step 'oo': {t:?}"
    );
    assert_eq!(e.preedit().as_str(), "tô");

    e.process(&ku('o')).unwrap();
    let t = e.process(&kd('i')).unwrap();
    assert!(
        matches!(t, StateTransition::PreeditUpdated(_)),
        "step 'i': {t:?}"
    );
    assert_eq!(e.preedit().as_str(), "tôi");
}

// ── Return on non-empty buffer ────────────────────────────────────────────────

#[test]
fn enter_on_nonempty_buffer_returns_commit_and_clear() {
    let mut e = telex();
    e.process(&kd('a')).unwrap();
    assert!(!e.preedit().is_empty());

    let t = e.process(&ret()).unwrap();
    assert!(
        matches!(t, StateTransition::CommitAndClear(_)),
        "Return on non-empty buffer must be CommitAndClear, got {t:?}"
    );
    assert!(e.preedit().is_empty(), "preedit must be clear after commit");
}

#[test]
fn enter_on_toi_commits_nfc_vietnamese_text() {
    // Build up preedit "tôi" via the Telex sequence t-o-o-i, then press Return.
    // Verifies that the committed text is the NFC-normalised Vietnamese string.
    let mut e = telex();
    e.process(&kd('t')).unwrap();
    e.process(&ku('t')).unwrap();
    e.process(&kd('o')).unwrap();
    e.process(&ku('o')).unwrap();
    e.process(&kd('o')).unwrap(); // second o triggers oo→ô
    e.process(&ku('o')).unwrap();
    e.process(&kd('i')).unwrap();
    assert_eq!(e.preedit().as_str(), "tôi");

    let t = e.process(&ret()).unwrap();
    match t {
        StateTransition::CommitAndClear(committed) => {
            assert_eq!(
                committed.as_str(),
                "tôi",
                "Enter on 'tôi' must commit NFC-normalised Vietnamese text"
            );
        }
        other => panic!("expected CommitAndClear, got {other:?}"),
    }
    assert!(e.preedit().is_empty(), "preedit must be clear after commit");
}

#[test]
fn enter_on_empty_buffer_is_passthrough() {
    let mut e = telex();
    let t = e.process(&ret()).unwrap();
    assert_eq!(t, StateTransition::PassThrough);
}

// ── Ctrl+char on non-empty buffer ─────────────────────────────────────────────

#[test]
fn ctrl_char_on_nonempty_buffer_returns_commit_and_clear() {
    let mut e = telex();
    // Build up preedit "tô".
    e.process(&kd('t')).unwrap();
    e.process(&ku('t')).unwrap();
    e.process(&kd('o')).unwrap();
    e.process(&ku('o')).unwrap();
    e.process(&kd('o')).unwrap();
    assert_eq!(e.preedit().as_str(), "tô");

    let t = e.process(&ctrl('c')).unwrap();
    assert!(
        matches!(t, StateTransition::CommitAndClear(_)),
        "Ctrl+char on non-empty buffer must be CommitAndClear, got {t:?}"
    );
    assert!(e.preedit().is_empty());
}

#[test]
fn ctrl_char_commits_correct_preedit_text() {
    let mut e = telex();
    e.process(&kd('t')).unwrap();
    e.process(&ku('t')).unwrap();
    e.process(&kd('o')).unwrap();
    e.process(&ku('o')).unwrap();
    e.process(&kd('o')).unwrap(); // preedit = "tô"

    let t = e.process(&ctrl('s')).unwrap();
    match t {
        StateTransition::CommitAndClear(committed) => {
            assert_eq!(committed.as_str(), "tô");
        }
        other => panic!("expected CommitAndClear, got {other:?}"),
    }
}

#[test]
fn ctrl_char_on_empty_buffer_is_passthrough() {
    let mut e = telex();
    let t = e.process(&ctrl('c')).unwrap();
    assert_eq!(t, StateTransition::PassThrough);
}

// ── Stress / rapid typing ─────────────────────────────────────────────────────

#[test]
fn rapid_keydown_50_distinct_chars_no_drops() {
    // 50 KeyDown events for distinct characters without any KeyUp in between.
    // Because each character is different, the repeat guard never fires.
    // Every press must produce PreeditUpdated or CommitThenPreedit (buffer overflow).
    let chars: Vec<char> = ('a'..='z').chain('A'..='X').collect(); // 26 + 24 = 50
    assert_eq!(chars.len(), 50, "sanity check on test input length");

    let mut e = telex();
    for &c in &chars {
        let t = e
            .process(&InputEvent::KeyDown(Key::Char(c), Modifiers::none()))
            .unwrap();
        assert!(
            matches!(
                t,
                StateTransition::PreeditUpdated(_) | StateTransition::CommitThenPreedit(_, _)
            ),
            "char '{c}': expected PreeditUpdated or CommitThenPreedit, got {t:?}"
        );
    }
}

#[test]
fn rapid_keyrepeat_100_events_no_drops() {
    // Simulate a held key: 1 initial KeyDown followed by 99 KeyRepeat events for 'c'.
    // No KeyUp is sent between any of them.
    // The repeat guard does not apply to KeyRepeat, so every event is processed.
    // The buffer overflows at 64 codepoints and resets (CommitThenPreedit), then
    // continues — none of the 100 events are silently dropped.
    let mut e = telex();

    let t = e.process(&kd('c')).unwrap();
    assert!(
        matches!(t, StateTransition::PreeditUpdated(_)),
        "initial KeyDown('c') must be PreeditUpdated, got {t:?}"
    );

    for i in 0..99 {
        let t = e.process(&kr('c')).unwrap();
        assert!(
            matches!(
                t,
                StateTransition::PreeditUpdated(_) | StateTransition::CommitThenPreedit(_, _)
            ),
            "KeyRepeat('c') #{}: expected PreeditUpdated or CommitThenPreedit, got {:?}",
            i + 1,
            t
        );
    }

    // After 100 'c' events: the first 64 were committed on overflow, and the
    // subsequent 36 remain in the preedit buffer.
    assert_eq!(
        e.preedit().as_str(),
        "c".repeat(36),
        "preedit after 100 'c' events must contain the 36 chars that follow the overflow commit"
    );
}
