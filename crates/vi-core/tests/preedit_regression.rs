#![allow(clippy::unwrap_used)]
//! Regression tests for the 'text invisible until Enter' bug and related preedit issues.
//!
//! The original bug: `buffer_preedit_updates` in the Wayland layer sent preedit
//! updates without flushing the socket, so the platform never saw them until
//! the next flush (triggered by Enter). These tests verify the engine layer:
//! it must return `PreeditUpdated` for every composing keystroke so the platform
//! has accurate information to display. They also cover rapid-typing safety,
//! backspace correctness, and focus-change behaviour.

use vi_core::{
    CompositionEngine, InputEvent, InputMethod, Key, Modifiers, StandardEngine, StateTransition,
};

// ── Helpers ───────────────────────────────────────────────────────────────────

fn engine(method: InputMethod) -> StandardEngine {
    StandardEngine::new(method)
}

fn kd(c: char) -> InputEvent {
    InputEvent::KeyDown(Key::Char(c), Modifiers::none())
}

fn ku(c: char) -> InputEvent {
    InputEvent::KeyUp(Key::Char(c))
}

fn bs() -> InputEvent {
    InputEvent::KeyDown(Key::Backspace, Modifiers::none())
}

fn ret() -> InputEvent {
    InputEvent::KeyDown(Key::Return, Modifiers::none())
}

fn preedit(e: &StandardEngine) -> String {
    e.preedit().as_str().to_owned()
}

/// Type each character with an intervening `KeyUp` so the repeat guard never
/// suppresses consecutive identical chars. Returns `(char, StateTransition)`
/// pairs for each `KeyDown`.
fn feed_with_keyup(e: &mut StandardEngine, chars: &str) -> Vec<(char, StateTransition)> {
    let mut out = Vec::new();
    for ch in chars.chars() {
        let t = e.process(&kd(ch)).unwrap();
        out.push((ch, t));
        let _ = e.process(&ku(ch));
    }
    out
}

// ── Preedit-per-keystroke: Telex ──────────────────────────────────────────────

/// Single Telex word: "tooi" → "tôi". Every intermediate keystroke must return
/// `PreeditUpdated` so the platform can display the preedit immediately.
/// This is the core engine-level invariant tested by the invisible-text regression.
#[test]
fn telex_composing_chars_return_preedit_updated() {
    let mut e = engine(InputMethod::Telex);

    let steps = [('t', "t"), ('o', "to"), ('o', "tô"), ('i', "tôi")];
    for (ch, expected) in steps {
        let t = e.process(&kd(ch)).unwrap();
        let _ = e.process(&ku(ch));
        assert!(
            matches!(t, StateTransition::PreeditUpdated(_)),
            "Telex 'tooi': char {ch:?} must return PreeditUpdated, got {t:?}",
        );
        assert_eq!(preedit(&e), expected, "preedit mismatch after {ch:?}");
    }

    let t = e.process(&ret()).unwrap();
    assert!(
        matches!(t, StateTransition::CommitAndClear(_)),
        "Return on non-empty preedit must CommitAndClear, got {t:?}",
    );
    if let StateTransition::CommitAndClear(c) = t {
        assert_eq!(c.as_str(), "tôi");
    }
    assert!(e.preedit().is_empty(), "preedit must be empty after commit");
}

/// Multiple Telex sequences: every character in each composing sequence must
/// return `PreeditUpdated` and the final committed text must be correct.
#[test]
fn telex_phrase_every_char_returns_preedit_updated() {
    let cases: &[(&str, &str)] = &[
        ("tooi", "tôi"),
        ("yeeu", "yêu"),
        ("vieejt", "việt"),
    ];

    for &(seq, expected) in cases {
        let mut e = engine(InputMethod::Telex);
        for (ch, t) in feed_with_keyup(&mut e, seq) {
            assert!(
                matches!(t, StateTransition::PreeditUpdated(_)),
                "Telex {seq:?}: char {ch:?} returned {t:?}, expected PreeditUpdated",
            );
        }
        let commit = e.process(&ret()).unwrap();
        match commit {
            StateTransition::CommitAndClear(c) => {
                assert_eq!(c.as_str(), expected, "committed text mismatch for Telex {seq:?}");
            }
            other => panic!("Return must CommitAndClear for Telex {seq:?}, got {other:?}"),
        }
    }
}

// ── Preedit-per-keystroke: VNI ────────────────────────────────────────────────

/// VNI sequences: every composing character must return `PreeditUpdated`.
#[test]
fn vni_composing_chars_return_preedit_updated() {
    let cases: &[(&str, &str)] = &[
        ("to6i", "tôi"),
        ("vie65t", "việt"),
    ];

    for &(seq, expected) in cases {
        let mut e = engine(InputMethod::Vni);
        for (ch, t) in feed_with_keyup(&mut e, seq) {
            assert!(
                matches!(t, StateTransition::PreeditUpdated(_)),
                "VNI {seq:?}: char {ch:?} returned {t:?}, expected PreeditUpdated",
            );
        }
        let commit = e.process(&ret()).unwrap();
        match commit {
            StateTransition::CommitAndClear(c) => {
                assert_eq!(c.as_str(), expected, "committed text mismatch for VNI {seq:?}");
            }
            other => panic!("Return must CommitAndClear for VNI {seq:?}, got {other:?}"),
        }
    }
}

// ── Preedit-per-keystroke: VIQR ───────────────────────────────────────────────

/// VIQR sequences: every composing character must return `PreeditUpdated`.
#[test]
fn viqr_composing_chars_return_preedit_updated() {
    let cases: &[(&str, &str)] = &[
        ("to^i", "tôi"),
        ("a'", "á"),
    ];

    for &(seq, expected) in cases {
        let mut e = engine(InputMethod::Viqr);
        for (ch, t) in feed_with_keyup(&mut e, seq) {
            assert!(
                matches!(t, StateTransition::PreeditUpdated(_)),
                "VIQR {seq:?}: char {ch:?} returned {t:?}, expected PreeditUpdated",
            );
        }
        let commit = e.process(&ret()).unwrap();
        match commit {
            StateTransition::CommitAndClear(c) => {
                assert_eq!(c.as_str(), expected, "committed text mismatch for VIQR {seq:?}");
            }
            other => panic!("Return must CommitAndClear for VIQR {seq:?}, got {other:?}"),
        }
    }
}

// ── Rapid typing stress test ──────────────────────────────────────────────────

/// Feed 200 characters rapidly (no `KeyUp`) through each input method.
///
/// Verifies:
/// - No panic or engine error at any point.
/// - The preedit is updated at least once (composing chars reach the engine).
/// - A final `Return` cleanly commits or clears without error.
/// - The preedit is empty after the final `Return`.
///
/// The repeat guard will suppress second occurrences of the same key within
/// 20 ms; that is expected and correct. The test cycles through all printable
/// ASCII characters so the first 95 events are always distinct.
#[test]
fn rapid_typing_200_chars_stress() {
    let chars: Vec<char> = (0x20u8..=0x7eu8).map(|b| b as char).collect();
    let seq: Vec<char> = chars.iter().cycle().copied().take(200).collect();

    for method in [InputMethod::Telex, InputMethod::Vni, InputMethod::Viqr] {
        let mut e = engine(method);
        let mut saw_preedit_update = false;

        for &ch in &seq {
            let t = e.process(&kd(ch)).expect("engine must not error during stress run");
            if matches!(t, StateTransition::PreeditUpdated(_)) {
                saw_preedit_update = true;
            }
            // Preedit must remain accessible (no internal corruption / panic).
            let _ = e.preedit().as_str().to_owned();
        }

        assert!(
            saw_preedit_update,
            "{method:?}: preedit was never updated during 200-char stress run",
        );

        let final_t = e.process(&ret()).expect("Return must not error after stress run");
        assert!(
            matches!(
                final_t,
                StateTransition::CommitAndClear(_) | StateTransition::PassThrough
            ),
            "{method:?}: Return after stress run must CommitAndClear or PassThrough; got {final_t:?}",
        );
        assert!(e.preedit().is_empty(), "{method:?}: preedit must be empty after final Return");
    }
}

// ── Backspace regression ──────────────────────────────────────────────────────

/// Type "tuong" in Telex, then backspace three times. Each backspace must update
/// the preedit correctly with no ghost characters or state corruption.
#[test]
fn backspace_regression_tuong_three_steps() {
    let mut e = engine(InputMethod::Telex);

    for ch in "tuong".chars() {
        e.process(&kd(ch)).unwrap();
        let _ = e.process(&ku(ch));
    }
    assert_eq!(preedit(&e), "tuong", "preedit after typing 'tuong'");

    // Backspace 1: 'g' removed → "tuon"
    let t1 = e.process(&bs()).unwrap();
    assert!(
        matches!(t1, StateTransition::PreeditUpdated(_)),
        "Backspace 1 must return PreeditUpdated, got {t1:?}",
    );
    assert_eq!(preedit(&e), "tuon", "preedit after BS 1");

    // Backspace 2: 'n' removed → "tuo"
    let t2 = e.process(&bs()).unwrap();
    assert!(
        matches!(t2, StateTransition::PreeditUpdated(_)),
        "Backspace 2 must return PreeditUpdated, got {t2:?}",
    );
    assert_eq!(preedit(&e), "tuo", "preedit after BS 2");

    // Backspace 3: 'o' removed → "tu"
    let t3 = e.process(&bs()).unwrap();
    assert!(
        matches!(t3, StateTransition::PreeditUpdated(_)),
        "Backspace 3 must return PreeditUpdated, got {t3:?}",
    );
    assert_eq!(preedit(&e), "tu", "preedit after BS 3");
}

// ── Focus-switch test ─────────────────────────────────────────────────────────

/// Start composing "viê" (Telex: v-i-e-e), then send `FocusOut`.
/// `FocusOut` must commit the pending preedit and leave the engine clean.
/// After `FocusIn` the engine accepts new input from a fresh state.
#[test]
fn focus_out_commits_pending_preedit() {
    let mut e = engine(InputMethod::Telex);

    // Build "viê" in the preedit: v, i, e, e (KeyUp between each e).
    for ch in "vi".chars() {
        e.process(&kd(ch)).unwrap();
        let _ = e.process(&ku(ch));
    }
    e.process(&kd('e')).unwrap();
    let _ = e.process(&ku('e'));
    e.process(&kd('e')).unwrap(); // second 'e' triggers ee→ê
    assert_eq!(preedit(&e), "viê", "preedit before FocusOut");

    // FocusOut must commit the pending preedit.
    let t = e.process(&InputEvent::FocusOut).unwrap();
    match t {
        StateTransition::CommitAndClear(c) => {
            assert_eq!(c.as_str(), "viê", "FocusOut must commit the preedit content");
        }
        other => panic!("FocusOut on non-empty preedit must CommitAndClear; got {other:?}"),
    }
    assert!(e.preedit().is_empty(), "preedit must be empty after FocusOut commit");

    // FocusIn returns Consumed and leaves the engine in a clean state.
    let t = e.process(&InputEvent::FocusIn).unwrap();
    assert_eq!(t, StateTransition::Consumed, "FocusIn must return Consumed");
    assert!(e.preedit().is_empty(), "preedit must remain empty after FocusIn");

    // Engine is ready for fresh composition.
    let t = e.process(&kd('a')).unwrap();
    assert!(
        matches!(t, StateTransition::PreeditUpdated(_)),
        "First char after FocusIn must be PreeditUpdated; got {t:?}",
    );
    assert_eq!(preedit(&e), "a", "preedit must start fresh after focus cycle");
}

/// `FocusOut` on an already-empty preedit must return `Consumed`, not an error.
#[test]
fn focus_out_on_empty_preedit_returns_consumed() {
    let mut e = engine(InputMethod::Telex);
    let t = e.process(&InputEvent::FocusOut).unwrap();
    assert_eq!(t, StateTransition::Consumed, "FocusOut on empty buffer must return Consumed");
}

// ── Rapid-typing regression ───────────────────────────────────────────────────

/// Simulate rapid typing at 50 chars/sec (20 ms per key): every composing
/// character must return `PreeditUpdated` with no gaps.
///
/// Uses Telex "vieejt " × N so that every letter is a composing step and space
/// is the word-boundary commit.  `KeyUp` between each character resets the
/// 20 ms repeat-guard, accurately modelling the key-up events a physical
/// keyboard sends.
///
/// A gap (returning `PassThrough`, `Consumed`, etc. for a composing letter)
/// would mean the platform never receives that preedit update — reproducing the
/// original invisible-text bug at the engine layer.
#[test]
fn rapid_typing_50_chars_every_composing_char_returns_preedit_updated() {
    let mut e = engine(InputMethod::Telex);

    // 50-char sequence: "vieejt " × 7 (49 chars) + "v" (1 char) = 50 chars.
    // Every letter is a composing step → must produce PreeditUpdated.
    // Space is the word-boundary commit → CommitThenPassThrough (excluded from check).
    let chars: Vec<char> = "vieejt ".chars().cycle().take(50).collect();

    let mut violations: Vec<(char, StateTransition)> = Vec::new();

    for &ch in &chars {
        let t = e.process(&kd(ch)).expect("engine must not error during rapid typing");
        let _ = e.process(&ku(ch));

        if ch != ' ' {
            // Every composing (non-space) character must update the preedit.
            if !matches!(t, StateTransition::PreeditUpdated(_)) {
                violations.push((ch, t));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "rapid typing at 50 chars/sec: composing chars that did NOT return \
         PreeditUpdated (engine layer gap = invisible-text regression): {violations:?}"
    );
}

// ── Backspace mid-composition regression ──────────────────────────────────────

/// Typing 't', 'o', then Backspace must produce the preedit sequence
/// `["t", "to", "t"]` — backspace removes the last character and the engine
/// updates the preedit immediately via `PreeditUpdated`.
///
/// A failure here (e.g. returning `Cleared` or `PassThrough` for backspace)
/// would leave the application showing stale preedit text.
#[test]
fn backspace_mid_composition_gives_correct_preedit_sequence() {
    let mut e = engine(InputMethod::Telex);
    let mut seq: Vec<String> = Vec::new();

    // 't' → PreeditUpdated("t")
    let t1 = e.process(&kd('t')).unwrap();
    let _ = e.process(&ku('t'));
    match t1 {
        StateTransition::PreeditUpdated(ref p) => seq.push(p.as_str().to_owned()),
        ref other => panic!("'t' must produce PreeditUpdated, got {other:?}"),
    }

    // 'o' → PreeditUpdated("to")
    let t2 = e.process(&kd('o')).unwrap();
    let _ = e.process(&ku('o'));
    match t2 {
        StateTransition::PreeditUpdated(ref p) => seq.push(p.as_str().to_owned()),
        ref other => panic!("'o' must produce PreeditUpdated, got {other:?}"),
    }

    // Backspace → PreeditUpdated("t")  (removes 'o', not 't')
    let t3 = e.process(&bs()).unwrap();
    match t3 {
        StateTransition::PreeditUpdated(ref p) => seq.push(p.as_str().to_owned()),
        ref other => panic!("backspace must produce PreeditUpdated, got {other:?}"),
    }

    assert_eq!(
        seq,
        ["t", "to", "t"],
        "backspace mid-composition must give preedit sequence [\"t\", \"to\", \"t\"]"
    );
}
