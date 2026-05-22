#![allow(clippy::unwrap_used)]
//! Property-based and exhaustive tests for all 216 Vietnamese syllable
//! combinations: 12 vowel bases × 6 tones × 3 input methods (Telex, VNI, VIQR).
//!
//! Each case asserts:
//!  1. The committed text equals the expected Vietnamese character (no ghost chars).
//!  2. The committed text is NFC-normalised.
//!  3. Exactly one commit event is produced — no double-commit.

use proptest::prelude::*;
use unicode_normalization::UnicodeNormalization;
use vi_core::{CompositionEngine, InputEvent, InputMethod, Key, Modifiers, StandardEngine, StateTransition};

// ── Syllable tables ──────────────────────────────────────────────────────────
//
// Format: (key_sequence, expected_output)
// Rows: 12 vowel bases; columns: level, sắc, huyền, hỏi, ngã, nặng

static TELEX: &[(&str, &str)] = &[
    // a × 6
    ("a", "a"), ("as", "á"), ("af", "à"), ("ar", "ả"), ("ax", "ã"), ("aj", "ạ"),
    // ă × 6  (aw → ă)
    ("aw", "ă"), ("aws", "ắ"), ("awf", "ằ"), ("awr", "ẳ"), ("awx", "ẵ"), ("awj", "ặ"),
    // â × 6  (aa → â)
    ("aa", "â"), ("aas", "ấ"), ("aaf", "ầ"), ("aar", "ẩ"), ("aax", "ẫ"), ("aaj", "ậ"),
    // e × 6
    ("e", "e"), ("es", "é"), ("ef", "è"), ("er", "ẻ"), ("ex", "ẽ"), ("ej", "ẹ"),
    // ê × 6  (ee → ê)
    ("ee", "ê"), ("ees", "ế"), ("eef", "ề"), ("eer", "ể"), ("eex", "ễ"), ("eej", "ệ"),
    // i × 6
    ("i", "i"), ("is", "í"), ("if", "ì"), ("ir", "ỉ"), ("ix", "ĩ"), ("ij", "ị"),
    // o × 6
    ("o", "o"), ("os", "ó"), ("of", "ò"), ("or", "ỏ"), ("ox", "õ"), ("oj", "ọ"),
    // ô × 6  (oo → ô)
    ("oo", "ô"), ("oos", "ố"), ("oof", "ồ"), ("oor", "ổ"), ("oox", "ỗ"), ("ooj", "ộ"),
    // ơ × 6  (ow → ơ)
    ("ow", "ơ"), ("ows", "ớ"), ("owf", "ờ"), ("owr", "ở"), ("owx", "ỡ"), ("owj", "ợ"),
    // u × 6
    ("u", "u"), ("us", "ú"), ("uf", "ù"), ("ur", "ủ"), ("ux", "ũ"), ("uj", "ụ"),
    // ư × 6  (uw → ư)
    ("uw", "ư"), ("uws", "ứ"), ("uwf", "ừ"), ("uwr", "ử"), ("uwx", "ữ"), ("uwj", "ự"),
    // y × 6
    ("y", "y"), ("ys", "ý"), ("yf", "ỳ"), ("yr", "ỷ"), ("yx", "ỹ"), ("yj", "ỵ"),
];

static VNI: &[(&str, &str)] = &[
    // a × 6
    ("a", "a"), ("a1", "á"), ("a2", "à"), ("a3", "ả"), ("a4", "ã"), ("a5", "ạ"),
    // ă × 6  (a8 → ă, then tone digit)
    ("a8", "ă"), ("a81", "ắ"), ("a82", "ằ"), ("a83", "ẳ"), ("a84", "ẵ"), ("a85", "ặ"),
    // â × 6  (a6 → â)
    ("a6", "â"), ("a61", "ấ"), ("a62", "ầ"), ("a63", "ẩ"), ("a64", "ẫ"), ("a65", "ậ"),
    // e × 6
    ("e", "e"), ("e1", "é"), ("e2", "è"), ("e3", "ẻ"), ("e4", "ẽ"), ("e5", "ẹ"),
    // ê × 6  (e6 → ê)
    ("e6", "ê"), ("e61", "ế"), ("e62", "ề"), ("e63", "ể"), ("e64", "ễ"), ("e65", "ệ"),
    // i × 6
    ("i", "i"), ("i1", "í"), ("i2", "ì"), ("i3", "ỉ"), ("i4", "ĩ"), ("i5", "ị"),
    // o × 6
    ("o", "o"), ("o1", "ó"), ("o2", "ò"), ("o3", "ỏ"), ("o4", "õ"), ("o5", "ọ"),
    // ô × 6  (o6 → ô)
    ("o6", "ô"), ("o61", "ố"), ("o62", "ồ"), ("o63", "ổ"), ("o64", "ỗ"), ("o65", "ộ"),
    // ơ × 6  (o7 → ơ)
    ("o7", "ơ"), ("o71", "ớ"), ("o72", "ờ"), ("o73", "ở"), ("o74", "ỡ"), ("o75", "ợ"),
    // u × 6
    ("u", "u"), ("u1", "ú"), ("u2", "ù"), ("u3", "ủ"), ("u4", "ũ"), ("u5", "ụ"),
    // ư × 6  (u7 → ư)
    ("u7", "ư"), ("u71", "ứ"), ("u72", "ừ"), ("u73", "ử"), ("u74", "ữ"), ("u75", "ự"),
    // y × 6
    ("y", "y"), ("y1", "ý"), ("y2", "ỳ"), ("y3", "ỷ"), ("y4", "ỹ"), ("y5", "ỵ"),
];

static VIQR: &[(&str, &str)] = &[
    // a × 6
    ("a", "a"), ("a'", "á"), ("a`", "à"), ("a?", "ả"), ("a~", "ã"), ("a.", "ạ"),
    // ă × 6  (a( → ă)
    ("a(", "ă"), ("a('", "ắ"), ("a(`", "ằ"), ("a(?", "ẳ"), ("a(~", "ẵ"), ("a(.", "ặ"),
    // â × 6  (a^ → â)
    ("a^", "â"), ("a^'", "ấ"), ("a^`", "ầ"), ("a^?", "ẩ"), ("a^~", "ẫ"), ("a^.", "ậ"),
    // e × 6
    ("e", "e"), ("e'", "é"), ("e`", "è"), ("e?", "ẻ"), ("e~", "ẽ"), ("e.", "ẹ"),
    // ê × 6  (e^ → ê)
    ("e^", "ê"), ("e^'", "ế"), ("e^`", "ề"), ("e^?", "ể"), ("e^~", "ễ"), ("e^.", "ệ"),
    // i × 6
    ("i", "i"), ("i'", "í"), ("i`", "ì"), ("i?", "ỉ"), ("i~", "ĩ"), ("i.", "ị"),
    // o × 6
    ("o", "o"), ("o'", "ó"), ("o`", "ò"), ("o?", "ỏ"), ("o~", "õ"), ("o.", "ọ"),
    // ô × 6  (o^ → ô)
    ("o^", "ô"), ("o^'", "ố"), ("o^`", "ồ"), ("o^?", "ổ"), ("o^~", "ỗ"), ("o^.", "ộ"),
    // ơ × 6  (o+ → ơ)
    ("o+", "ơ"), ("o+'", "ớ"), ("o+`", "ờ"), ("o+?", "ở"), ("o+~", "ỡ"), ("o+.", "ợ"),
    // u × 6
    ("u", "u"), ("u'", "ú"), ("u`", "ù"), ("u?", "ủ"), ("u~", "ũ"), ("u.", "ụ"),
    // ư × 6  (u+ → ư)
    ("u+", "ư"), ("u+'", "ứ"), ("u+`", "ừ"), ("u+?", "ử"), ("u+~", "ữ"), ("u+.", "ự"),
    // y × 6
    ("y", "y"), ("y'", "ý"), ("y`", "ỳ"), ("y?", "ỷ"), ("y~", "ỹ"), ("y.", "ỵ"),
];

// ── Helpers ──────────────────────────────────────────────────────────────────

#[derive(Clone, Copy)]
struct Case {
    method: InputMethod,
    keys: &'static str,
    expected: &'static str,
}

fn all_216() -> Vec<Case> {
    let mut v = Vec::with_capacity(216);
    for &(keys, expected) in TELEX {
        v.push(Case { method: InputMethod::Telex, keys, expected });
    }
    for &(keys, expected) in VNI {
        v.push(Case { method: InputMethod::Vni, keys, expected });
    }
    for &(keys, expected) in VIQR {
        v.push(Case { method: InputMethod::Viqr, keys, expected });
    }
    v
}

fn is_commit(t: &StateTransition) -> bool {
    matches!(
        t,
        StateTransition::Commit(_)
            | StateTransition::CommitAndClear(_)
            | StateTransition::CommitThenPreedit(_, _)
    )
}

/// Type each key in `keys`, then press Return to commit.
/// Returns (all transitions including the final one, committed text).
fn run_syllable(method: InputMethod, keys: &str) -> (Vec<StateTransition>, String) {
    let mut engine = StandardEngine::new(method);
    let mut transitions = Vec::new();

    for ch in keys.chars() {
        let ev = InputEvent::KeyDown(Key::Char(ch), Modifiers::none());
        transitions.push(engine.process(&ev).unwrap());
        let _ = engine.process(&InputEvent::KeyUp(Key::Char(ch)));
    }
    let ret = InputEvent::KeyDown(Key::Return, Modifiers::none());
    let final_t = engine.process(&ret).unwrap();
    let committed = match &final_t {
        StateTransition::CommitAndClear(c) | StateTransition::Commit(c) => c.as_str().to_owned(),
        other => panic!("Return on non-empty preedit must commit; got {other:?} for method={method:?} keys={keys:?}"),
    };
    transitions.push(final_t);
    (transitions, committed)
}

fn assert_syllable(method: InputMethod, keys: &str, expected: &str) {
    let (transitions, committed) = run_syllable(method, keys);

    // Exactly one commit event.
    let commit_count = transitions.iter().filter(|t| is_commit(t)).count();
    assert_eq!(
        commit_count, 1,
        "{method:?} keys={keys:?}: expected 1 commit, got {commit_count}"
    );

    // No ghost characters — committed text matches expected exactly.
    assert_eq!(
        committed.as_str(), expected,
        "{method:?} keys={keys:?}: committed {committed:?}, want {expected:?}"
    );

    // Output is NFC.
    let nfc: String = committed.nfc().collect();
    assert_eq!(
        committed, nfc,
        "{method:?} keys={keys:?}: committed text is not NFC: {committed:?}"
    );
}

// ── Deterministic exhaustive test ────────────────────────────────────────────

#[test]
fn all_216_syllables_nfc_single_commit_no_ghost() {
    for case in all_216() {
        assert_syllable(case.method, case.keys, case.expected);
    }
}

// ── Property-based test: random selection from the 216 cases ─────────────────

proptest! {
    /// Randomly select from the 216 canonical Vietnamese syllable combinations and
    /// verify: NFC output, exactly one commit, committed text matches expected.
    #[test]
    fn syllable_properties_hold_for_random_cases(idx in 0usize..216usize) {
        let cases = all_216();
        let case = cases[idx];

        let (transitions, committed) = run_syllable(case.method, case.keys);

        // Exactly one commit.
        let commit_count = transitions.iter().filter(|t| is_commit(t)).count();
        prop_assert_eq!(commit_count, 1usize);

        // Output is NFC.
        let nfc: String = committed.nfc().collect();
        prop_assert!(
            committed == nfc,
            "committed text not NFC: method={:?} keys={:?} committed={:?}",
            case.method, case.keys, committed
        );

        // No ghost chars — committed text must match expected exactly.
        prop_assert!(
            committed.as_str() == case.expected,
            "method={:?} keys={:?}: committed {:?}, want {:?}",
            case.method, case.keys, committed, case.expected
        );
    }
}
