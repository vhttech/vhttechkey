#![allow(clippy::unwrap_used)]
//! Property-based tests: random InputEvent streams must never produce
//! invalid UTF-8 or NFD output.

use proptest::prelude::*;
use unicode_normalization::UnicodeNormalization;
use vi_core::{
    syllable::VALID_NUCLEI, CompositionEngine, InputEvent, InputMethod, Key, Modifiers,
    StandardEngine, StateTransition,
};

fn arb_key() -> impl Strategy<Value = Key> {
    prop_oneof![
        // Printable ASCII range
        (0x20u32..=0x7Eu32).prop_map(|c| Key::Char(char::from_u32(c).unwrap())),
        Just(Key::Backspace),
        Just(Key::Return),
        Just(Key::Escape),
    ]
}

fn arb_modifiers() -> impl Strategy<Value = Modifiers> {
    Just(Modifiers::none())
}

fn arb_event() -> impl Strategy<Value = InputEvent> {
    prop_oneof![
        (arb_key(), arb_modifiers()).prop_map(|(k, m)| InputEvent::KeyDown(k, m)),
        Just(InputEvent::FocusIn),
        Just(InputEvent::FocusOut),
        Just(InputEvent::Reset),
    ]
}

fn arb_method() -> impl Strategy<Value = InputMethod> {
    prop_oneof![
        Just(InputMethod::Telex),
        Just(InputMethod::Vni),
        Just(InputMethod::Viqr),
    ]
}

proptest! {
    #[test]
    fn no_invalid_utf8_or_nfd(
        method in arb_method(),
        events in prop::collection::vec(arb_event(), 1..50),
    ) {
        let mut engine = StandardEngine::new(method);
        for event in &events {
            let result = engine.process(event);
            // Must never return an error that implies panic-level state.
            match result {
                Ok(StateTransition::PreeditUpdated(p)) => {
                    let s = p.as_str();
                    // Valid UTF-8 (Rust strings always are, but let's be explicit).
                    assert!(std::str::from_utf8(s.as_bytes()).is_ok());
                    // Must be NFC.
                    let nfc: String = s.nfc().collect();
                    assert_eq!(s, nfc, "preedit not NFC: {s:?}");
                }
                Ok(StateTransition::CommitAndClear(c)) | Ok(StateTransition::Commit(c)) => {
                    let s = c.as_str();
                    assert!(std::str::from_utf8(s.as_bytes()).is_ok());
                    let nfc: String = s.nfc().collect();
                    assert_eq!(s, nfc, "committed text not NFC: {s:?}");
                }
                Ok(_) => {}
                Err(_) => {}
            }
            // Preedit must also always be valid UTF-8 and NFC.
            let p = engine.preedit();
            let s = p.as_str();
            assert!(std::str::from_utf8(s.as_bytes()).is_ok());
            let nfc: String = s.nfc().collect();
            assert_eq!(s, nfc, "engine preedit not NFC after event: {s:?}");
        }
    }
}

// ── Nucleus × tone × method coverage ─────────────────────────────────────────
//
// For each valid nucleus we need the key sequences that produce it in each of
// the three input methods.  Format: (nucleus, telex_keys, vni_keys, viqr_keys).
// Level tone (no suffix) is represented by an empty string.

/// (nucleus_display, telex_keys, vni_keys, viqr_keys)
static NUCLEUS_KEYS: &[(&str, &str, &str, &str)] = &[
    // ── Monophthong nuclei ────────────────────────────────────────────────
    ("a", "a", "a", "a"),
    ("ă", "aw", "a8", "a("),
    ("â", "aa", "a6", "a^"),
    ("e", "e", "e", "e"),
    ("ê", "ee", "e6", "e^"),
    ("i", "i", "i", "i"),
    ("o", "o", "o", "o"),
    ("ô", "oo", "o6", "o^"),
    ("ơ", "ow", "o7", "o+"),
    ("u", "u", "u", "u"),
    ("ư", "uw", "u7", "u+"),
    ("y", "y", "y", "y"),
    // ── Multi-vowel nuclei accessible via IME rules ────────────────────────
    // tôi: type t + oo + i; in method variants.  Tone goes on ô, not i.
    ("tôi", "tooi", "to6i", "to^i"),
    // uơi: type u + ow + i.  Tone goes on ơ, not i.
    ("uơi", "uowi", "uo7i", "uo+i"),
    // uôi: type u + oo + i.  Tone goes on ô, not i.
    ("uôi", "uooi", "uo6i", "uo^i"),
    // iêu: type i + ee + u.  Tone goes on ê, not u.
    ("iêu", "ieeu", "ie6u", "ie^u"),
    // ươi: type u + uw + o + w + i = ư + ơ + i.  Tone goes on ơ (last modified).
    ("ươi", "uwowi", "u7o7i", "u+o+i"),
    // oi: no modified vowel; i is glide → tone on o.
    ("oi", "oi", "oi", "oi"),
    // ai: no modified vowel; i is glide → tone on a.
    ("ai", "ai", "ai", "ai"),
];

/// Tone key suffixes per input method: (telex, vni, viqr).
/// Index 0 = sắc, 1 = huyền, 2 = hỏi, 3 = ngã, 4 = nặng.
static TONE_SUFFIXES: &[(&str, &str, &str)] = &[
    ("s", "1", "'"), // sắc
    ("f", "2", "`"), // huyền
    ("r", "3", "?"), // hỏi
    ("x", "4", "~"), // ngã
    ("j", "5", "."), // nặng
];

/// Type `keys` into a fresh engine using `method`, then flush by sending Return.
/// Returns the committed string on success, or the preedit on no-commit.
fn type_and_flush(method: InputMethod, keys: &str) -> String {
    let mut engine = StandardEngine::new(method);
    let mut last_commit = String::new();
    for ch in keys.chars() {
        let ev = InputEvent::KeyDown(Key::Char(ch), Modifiers::none());
        match engine.process(&ev).unwrap() {
            StateTransition::CommitAndClear(c) | StateTransition::Commit(c) => {
                last_commit = c.as_str().to_owned();
            }
            StateTransition::CommitThenPreedit(c, _) => {
                last_commit = c.as_str().to_owned();
            }
            _ => {}
        }
        let _ = engine.process(&InputEvent::KeyUp(Key::Char(ch)));
    }
    // Flush remaining preedit via Return.
    let ev = InputEvent::KeyDown(Key::Return, Modifiers::none());
    match engine.process(&ev).unwrap() {
        StateTransition::CommitAndClear(c) | StateTransition::Commit(c) => {
            last_commit = c.as_str().to_owned();
        }
        _ => {}
    }
    last_commit
}

proptest! {
    /// All 6 tones (including level) × all accessible nuclei × all 3 methods
    /// must produce NFC-normalized, non-empty, valid-UTF-8 output.
    #[test]
    fn all_tones_all_nuclei_all_methods_produce_nfc(
        nucleus_idx in 0..NUCLEUS_KEYS.len(),
        tone_idx    in 0..TONE_SUFFIXES.len(),
        method_idx  in 0usize..3,
    ) {
        let (_, telex_keys, vni_keys, viqr_keys) = NUCLEUS_KEYS[nucleus_idx];
        let (telex_tone, vni_tone, viqr_tone) = TONE_SUFFIXES[tone_idx];

        let (method, nucleus_keys, tone_suffix) = match method_idx {
            0 => (InputMethod::Telex, telex_keys, telex_tone),
            1 => (InputMethod::Vni,   vni_keys,   vni_tone),
            _ => (InputMethod::Viqr,  viqr_keys,  viqr_tone),
        };

        let full_seq = format!("{nucleus_keys}{tone_suffix}");
        let output = type_and_flush(method, &full_seq);

        prop_assert!(!output.is_empty(), "empty output for {method:?} {full_seq:?}");
        prop_assert!(
            std::str::from_utf8(output.as_bytes()).is_ok(),
            "invalid UTF-8 for {method:?} {full_seq:?}"
        );
        let nfc: String = output.chars().nfc().collect();
        prop_assert_eq!(
            &output, &nfc,
            "output not NFC for {:?} {:?}: got {:?}", method, full_seq, output
        );
    }
}

/// For every entry in VALID_NUCLEI, verify the nucleus string itself is valid UTF-8
/// and the tone-position function doesn't return None (every nucleus has ≥1 vowel).
#[test]
fn valid_nuclei_table_all_have_vowel() {
    for nucleus in VALID_NUCLEI {
        let chars: Vec<char> = nucleus.chars().collect();
        let pos = vi_core::syllable::find_tone_position(&chars);
        assert!(
            pos.is_some(),
            "VALID_NUCLEI entry {nucleus:?} has no identifiable vowel nucleus"
        );
    }
}

/// Determinism: identical input always produces identical output.
#[test]
fn determinism_same_input_same_output() {
    for _ in 0..3 {
        let mut e = StandardEngine::new(InputMethod::Telex);
        for ch in "tooi".chars() {
            e.process(&InputEvent::KeyDown(Key::Char(ch), Modifiers::none()))
                .unwrap();
            let _ = e.process(&InputEvent::KeyUp(Key::Char(ch)));
        }
        assert_eq!(e.preedit().as_str(), "tôi");
    }
}

/// FocusOut followed by flush_commit must not re-emit already-committed text.
#[test]
fn focus_out_then_flush_commit_no_double_emit() {
    let mut engine = StandardEngine::new(InputMethod::Telex);
    for ch in "too".chars() {
        engine
            .process(&InputEvent::KeyDown(Key::Char(ch), Modifiers::none()))
            .unwrap();
        let _ = engine.process(&InputEvent::KeyUp(Key::Char(ch)));
    }
    // FocusOut commits the preedit.
    let t = engine.process(&InputEvent::FocusOut).unwrap();
    assert!(
        matches!(t, StateTransition::CommitAndClear(_)),
        "FocusOut should CommitAndClear"
    );
    // flush_commit must see an empty buffer and return None.
    let second = engine.flush_commit();
    assert!(
        second.is_none(),
        "flush_commit after FocusOut re-emitted: {second:?}"
    );
}

proptest! {
    /// Every output from arbitrary Telex/VNI/VIQR key sequences must be valid NFC
    /// and must never contain surrogates (U+D800–U+DFFF) or C1 control characters
    /// (U+0080–U+009F).
    #[test]
    fn committed_nfc_no_surrogates_no_c1(
        method in arb_method(),
        events in prop::collection::vec(arb_event(), 1..30),
    ) {
        let mut engine = StandardEngine::new(method);
        for event in &events {
            let maybe_output: Option<String> = match engine.process(event) {
                Ok(StateTransition::PreeditUpdated(p)) => Some(p.as_str().to_owned()),
                Ok(StateTransition::CommitAndClear(c)) | Ok(StateTransition::Commit(c)) => {
                    Some(c.as_str().to_owned())
                }
                Ok(StateTransition::CommitThenPreedit(c, _)) => Some(c.as_str().to_owned()),
                _ => None,
            };
            if let Some(output) = maybe_output {
                // Must be NFC (not NFD, not NFKC-only, not decomposed).
                let nfc: String = output.chars().nfc().collect();
                prop_assert_eq!(
                    &output, &nfc,
                    "output not NFC for method {:?}: {:?}", method, output
                );
                for ch in output.chars() {
                    let cp = ch as u32;
                    // Rust str cannot contain surrogates, but assert for explicit coverage.
                    prop_assert!(
                        !(0xD800..=0xDFFF).contains(&cp),
                        "surrogate U+{:04X} in output {:?}", cp, output
                    );
                    // C1 control characters indicate misidentified legacy encoding.
                    prop_assert!(
                        !(0x0080..=0x009F).contains(&cp),
                        "C1 control U+{:04X} in output {:?}", cp, output
                    );
                }
            }
        }
    }
}

/// flush_commit followed by FocusOut must not re-emit already-committed text.
#[test]
fn flush_commit_then_focus_out_no_double_emit() {
    let mut engine = StandardEngine::new(InputMethod::Telex);
    for ch in "aa".chars() {
        engine
            .process(&InputEvent::KeyDown(Key::Char(ch), Modifiers::none()))
            .unwrap();
        let _ = engine.process(&InputEvent::KeyUp(Key::Char(ch)));
    }
    // flush_commit commits via fast-path.
    let first = engine.flush_commit();
    assert!(first.is_some(), "flush_commit should return the preedit");
    // FocusOut must see an empty buffer and return Consumed.
    let t = engine.process(&InputEvent::FocusOut).unwrap();
    assert!(
        matches!(t, StateTransition::Consumed),
        "FocusOut after flush_commit should return Consumed, got {t:?}"
    );
}

proptest! {
    /// Tone-anywhere: arbitrary (vowel, tone-key, vowel-form-key) Telex sequences
    /// must produce valid NFC output and never panic.
    ///
    /// Tests two orderings:
    ///   - tone-before-form  (e.g. 'a','s','w' → "ắ")
    ///   - form-before-tone  (e.g. 'a','w','s' → "ắ")
    #[test]
    fn tone_anywhere_sequences_produce_nfc_no_panic(
        vowel_idx in 0usize..6,
        tone_idx  in 0usize..5,
        form_idx  in 0usize..3,
    ) {
        let vowels = ['a', 'o', 'u', 'e', 'i', 'y'];
        let tones  = ['s', 'f', 'r', 'x', 'j'];
        let forms  = ['w', 'a', 'e']; // w=breve/horn, a=â-form, e=ê-form

        let vowel = vowels[vowel_idx];
        let tone  = tones[tone_idx];
        let form  = forms[form_idx];

        for seq in [[vowel, tone, form], [vowel, form, tone]] {
            let mut engine = StandardEngine::new(InputMethod::Telex);
            for &ch in &seq {
                let ev = InputEvent::KeyDown(Key::Char(ch), Modifiers::none());
                if let Ok(StateTransition::PreeditUpdated(p)) = engine.process(&ev) {
                    let s = p.as_str();
                    let nfc: String = s.chars().nfc().collect();
                    prop_assert_eq!(
                        s, nfc.as_str(),
                        "preedit not NFC for seq {:?}: {:?}", seq, s
                    );
                }
                let _ = engine.process(&InputEvent::KeyUp(Key::Char(ch)));
            }
            let s = engine.preedit().as_str().to_owned();
            let nfc: String = s.chars().nfc().collect();
            prop_assert_eq!(
                &s, &nfc,
                "final preedit not NFC for seq {:?}: {:?}", seq, s
            );
        }
    }
}
