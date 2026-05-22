#![allow(clippy::unwrap_used)]
//! Golden tests for Vietnamese triphthong nuclei and tone-mark placement.
//!
//! Covers the five triphthong nuclei listed in `syllable::VALID_NUCLEI`:
//!   `iêu`, `oai`, `uôi`, `ươi`, `ươu`
//!
//! For each nucleus all 6 Telex tone marks are exercised (s/f/r/x/j/z) to
//! verify that `find_tone_position` routes the diacritic to the correct vowel:
//!
//! | Nucleus | Tone carrier | Rule                                  |
//! |---------|--------------|---------------------------------------|
//! | iêu     | ê (idx 1)    | Rule 1 — ê is form-marked             |
//! | oai     | a (idx 1)    | Rule 3 — final glide i, penult = a    |
//! | uôi     | ô (idx 1)    | Rule 1 — ô is form-marked             |
//! | ươi     | ơ (idx 1)    | Rule 1 — last form-marked of ư/ơ is ơ |
//! | ươu     | ơ (idx 1)    | Rule 1 — last form-marked of ư/ơ is ơ |

use vi_core::{
    CompositionEngine, InputEvent, InputMethod, Key, Modifiers, StandardEngine, StateTransition,
};

fn type_and_commit(keys: &str) -> String {
    let mut e = StandardEngine::new(InputMethod::Telex);
    for ch in keys.chars() {
        e.process(&InputEvent::KeyDown(Key::Char(ch), Modifiers::none()))
            .expect("engine must not fail on valid input");
        let _ = e.process(&InputEvent::KeyUp(Key::Char(ch)));
    }
    match e
        .process(&InputEvent::KeyDown(Key::Return, Modifiers::none()))
        .unwrap()
    {
        StateTransition::CommitAndClear(c) => c.as_str().to_owned(),
        StateTransition::PassThrough => String::new(),
        other => panic!("unexpected transition on Return: {other:?}"),
    }
}

/// Test cases: (telex_key_sequence, expected_committed_text, description).
///
/// Telex input sequences for each nucleus:
///   iêu  → "ieeu"  (ee fires ê rule)
///   oai  → "oai"   (plain pushes)
///   uôi  → "uooi"  (oo fires ô rule)
///   ươi  → "uwowi" (uw→ư, ow→ơ)
///   ươu  → "uwowu" (uw→ư, ow→ơ)
fn triphthong_cases() -> Vec<(&'static str, &'static str, &'static str)> {
    vec![
        // ── iêu — tone falls on ê ────────────────────────────────────────────
        ("ieeu", "iêu", "iêu level"),
        ("ieeus", "iếu", "iêu sắc   (s → ế)"),
        ("ieeuf", "iều", "iêu huyền (f → ề)"),
        ("ieeur", "iểu", "iêu hỏi   (r → ể)"),
        ("ieeux", "iễu", "iêu ngã   (x → ễ)"),
        ("ieeuj", "iệu", "iêu nặng  (j → ệ)"),
        // Note: 'z' on an already-level-tone syllable triggers the vhttechkey
        // double-tone undo, making the composition invalid → English fallback.
        (
            "ieeuz",
            "ieeuz",
            "iêu level (z on level tone → invalid → raw keys)",
        ),
        // ── oai — tone falls on a ────────────────────────────────────────────
        ("oai", "oai", "oai level"),
        ("oais", "oái", "oai sắc   (s → á)"),
        ("oaif", "oài", "oai huyền (f → à)"),
        ("oair", "oải", "oai hỏi   (r → ả)"),
        ("oaix", "oãi", "oai ngã   (x → ã)"),
        ("oaij", "oại", "oai nặng  (j → ạ)"),
        (
            "oaiz",
            "oaiz",
            "oai level (z on level tone → invalid → raw keys)",
        ),
        // ── uôi — tone falls on ô ────────────────────────────────────────────
        ("uooi", "uôi", "uôi level"),
        ("uoois", "uối", "uôi sắc   (s → ố)"),
        ("uooif", "uồi", "uôi huyền (f → ồ)"),
        ("uooir", "uổi", "uôi hỏi   (r → ổ)"),
        ("uooix", "uỗi", "uôi ngã   (x → ỗ)"),
        ("uooij", "uội", "uôi nặng  (j → ộ)"),
        (
            "uooiz",
            "uooiz",
            "uôi level (z on level tone → invalid → raw keys)",
        ),
        // ── ươi — tone falls on ơ ────────────────────────────────────────────
        ("uwowi", "ươi", "ươi level"),
        ("uwowis", "ưới", "ươi sắc   (s → ớ)"),
        ("uwowif", "ười", "ươi huyền (f → ờ)"),
        ("uwowir", "ưởi", "ươi hỏi   (r → ở)"),
        ("uwowix", "ưỡi", "ươi ngã   (x → ỡ)"),
        ("uwowij", "ượi", "ươi nặng  (j → ợ)"),
        (
            "uwowiz",
            "uwowiz",
            "ươi level (z on level tone → invalid → raw keys)",
        ),
        // ── ươu — tone falls on ơ ────────────────────────────────────────────
        ("uwowu", "ươu", "ươu level"),
        ("uwowus", "ướu", "ươu sắc   (s → ớ)"),
        ("uwowuf", "ườu", "ươu huyền (f → ờ)"),
        ("uwowur", "ưởu", "ươu hỏi   (r → ở)"),
        ("uwowux", "ưỡu", "ươu ngã   (x → ỡ)"),
        ("uwowuj", "ượu", "ươu nặng  (j → ợ)"),
        (
            "uwowuz",
            "uwowuz",
            "ươu level (z on level tone → invalid → raw keys)",
        ),
    ]
}

#[test]
fn triphthong_tone_placement() {
    for (keys, expected, desc) in triphthong_cases() {
        assert_eq!(
            type_and_commit(keys),
            expected,
            "Telex {desc}: keys={keys:?}"
        );
    }
}
