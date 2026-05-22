//! Comprehensive Unicode edge-case tests covering NFC guarantees, surrogate
//! detection, mixed-script sessions, and ill-formed input handling.
#![allow(clippy::unwrap_used)]

use unicode_normalization::UnicodeNormalization;
use vi_core::{
    CompositionEngine, CompositionError, InputEvent, InputMethod, Key, Modifiers, StandardEngine,
    StateTransition, UnicodePipeline,
};

// ── NFC round-trip ────────────────────────────────────────────────────────────

/// normalize(denormalize(s)) == s for every toned Vietnamese vowel.
#[test]
fn nfc_round_trip_all_toned_vowels() {
    let toned: &[&str] = &[
        // a-family
        "à", "á", "ả", "ã", "ạ", "ằ", "ắ", "ẳ", "ẵ", "ặ", "ầ", "ấ", "ẩ", "ẫ", "ậ",
        // e-family
        "è", "é", "ẻ", "ẽ", "ẹ", "ề", "ế", "ể", "ễ", "ệ", // i-family
        "ì", "í", "ỉ", "ĩ", "ị", // o-family
        "ò", "ó", "ỏ", "õ", "ọ", "ồ", "ố", "ổ", "ỗ", "ộ", "ờ", "ớ", "ở", "ỡ", "ợ",
        // u-family
        "ù", "ú", "ủ", "ũ", "ụ", "ừ", "ứ", "ử", "ữ", "ự", // y-family
        "ỳ", "ý", "ỷ", "ỹ", "ỵ",
    ];

    for s in toned {
        let nfc: String = s.nfc().collect();
        let nfd: String = s.nfd().collect();
        let renfc: String = nfd.nfc().collect();
        assert_eq!(nfc, renfc, "NFC round-trip failed for {s:?}");

        // Pipeline should also produce NFC.
        let result = UnicodePipeline::process(s).unwrap();
        assert_eq!(result.as_str(), nfc.as_str(), "pipeline not NFC for {s:?}");
    }
}

// ── Surrogate detection ───────────────────────────────────────────────────────

/// No output of the composition engine should ever contain a codepoint in the
/// surrogate range D800–DFFF.
#[test]
fn no_surrogate_codepoints_in_engine_output() {
    use vi_core::InputMethod;
    let sequences: &[(InputMethod, &str)] = &[
        (InputMethod::Telex, "tooi"),
        (InputMethod::Telex, "vieejt"),
        (InputMethod::Vni, "to6i"),
        (InputMethod::Vni, "d9u7o7c5"),
        (InputMethod::Viqr, "to^i"),
        (InputMethod::Viqr, "ddu+o+c."),
    ];

    for &(method, input) in sequences {
        let mut engine = StandardEngine::new(method);
        let mut committed = String::new();
        for ch in input.chars() {
            if let Ok(StateTransition::CommitAndClear(c) | StateTransition::Commit(c)) =
                engine.process(&InputEvent::KeyDown(Key::Char(ch), Modifiers::none()))
            {
                committed.push_str(c.as_str());
            }
        }
        if let Ok(StateTransition::CommitAndClear(c) | StateTransition::Commit(c)) =
            engine.process(&InputEvent::KeyDown(Key::Return, Modifiers::none()))
        {
            committed.push_str(c.as_str());
        }
        for ch in committed.chars() {
            let cp = ch as u32;
            assert!(
                !(0xD800..=0xDFFF).contains(&cp),
                "surrogate U+{cp:04X} in output of {method:?}/{input:?}"
            );
        }
    }
}

/// UnicodePipeline also rejects surrogate codepoints (defence-in-depth).
/// Rust's `char` type already prevents surrogates, but we document the
/// invariant via the error variant.
#[test]
fn surrogate_error_variant_exists() {
    let _: CompositionError = CompositionError::SurrogateCodepoint(0xD800);
}

// ── Mixed script sessions ─────────────────────────────────────────────────────

/// Emoji typed before Vietnamese characters must not corrupt the preedit.
#[test]
fn mixed_emoji_then_vietnamese_telex() {
    let mut engine = StandardEngine::new(InputMethod::Telex);
    let key = |c: char| InputEvent::KeyDown(Key::Char(c), Modifiers::none());

    // '😀' is a single codepoint (U+1F600), valid in Key::Char.
    engine.process(&key('😀')).unwrap();
    engine.process(&key('a')).unwrap();
    engine.process(&key('f')).unwrap(); // af → à

    let committed = match engine
        .process(&InputEvent::KeyDown(Key::Return, Modifiers::none()))
        .unwrap()
    {
        StateTransition::CommitAndClear(c) | StateTransition::Commit(c) => c.as_str().to_owned(),
        StateTransition::PassThrough => String::new(),
        other => panic!("unexpected: {other:?}"),
    };

    // Output must be NFC.
    let nfc: String = committed.nfc().collect();
    assert_eq!(committed, nfc, "mixed emoji+Vietnamese output is not NFC");

    // No surrogates.
    for ch in committed.chars() {
        assert!(!(0xD800u32..=0xDFFF).contains(&(ch as u32)));
    }
}

/// CJK characters interleaved with Vietnamese tone marks.
#[test]
fn mixed_cjk_and_vietnamese_vni() {
    let mut engine = StandardEngine::new(InputMethod::Vni);
    let key = |c: char| InputEvent::KeyDown(Key::Char(c), Modifiers::none());

    // Type a CJK char, then a VNI vowel+tone.
    engine.process(&key('中')).unwrap();
    engine.process(&key('a')).unwrap();
    engine.process(&key('1')).unwrap(); // a1 → á

    let committed = match engine
        .process(&InputEvent::KeyDown(Key::Return, Modifiers::none()))
        .unwrap()
    {
        StateTransition::CommitAndClear(c) | StateTransition::Commit(c) => c.as_str().to_owned(),
        StateTransition::PassThrough => String::new(),
        other => panic!("unexpected: {other:?}"),
    };

    let nfc: String = committed.nfc().collect();
    assert_eq!(committed, nfc);
    for ch in committed.chars() {
        assert!(!(0xD800u32..=0xDFFF).contains(&(ch as u32)));
    }
}

// ── Ill-formed / edge-case inputs ─────────────────────────────────────────────

/// Sending every `Key` variant to the engine must never panic.
#[test]
fn all_key_variants_do_not_panic() {
    let mut engine = StandardEngine::new(InputMethod::Telex);
    let events = [
        InputEvent::KeyDown(Key::Char('a'), Modifiers::none()),
        InputEvent::KeyDown(Key::Backspace, Modifiers::none()),
        InputEvent::KeyDown(Key::Delete, Modifiers::none()),
        InputEvent::KeyDown(Key::Return, Modifiers::none()),
        InputEvent::KeyDown(Key::Escape, Modifiers::none()),
        InputEvent::KeyDown(Key::Tab, Modifiers::none()),
        InputEvent::KeyDown(Key::Left, Modifiers::none()),
        InputEvent::KeyDown(Key::Right, Modifiers::none()),
        InputEvent::KeyDown(Key::Up, Modifiers::none()),
        InputEvent::KeyDown(Key::Down, Modifiers::none()),
        InputEvent::KeyDown(Key::Home, Modifiers::none()),
        InputEvent::KeyDown(Key::End, Modifiers::none()),
        InputEvent::KeyDown(Key::Keysym(0xDEAD_BEEF), Modifiers::none()),
        InputEvent::KeyUp(Key::Char('a')),
        InputEvent::KeyRepeat(Key::Char('a'), Modifiers::none()),
        InputEvent::FocusIn,
        InputEvent::FocusOut,
        InputEvent::Reset,
    ];
    for event in &events {
        let _ = engine.process(event);
    }
}

/// UnicodePipeline rejects a lone combining grave with no base character.
#[test]
fn orphaned_combining_mark_is_rejected() {
    let result = UnicodePipeline::process("\u{0300}");
    assert!(
        matches!(result, Err(CompositionError::OrphanedCombiningMark(_))),
        "expected OrphanedCombiningMark, got {result:?}"
    );
}

/// UnicodePipeline normalises NFD input to NFC.
#[test]
fn nfd_input_normalized_to_nfc() {
    let nfd = "a\u{0302}\u{0323}"; // a + combining circumflex + combining dot below
    let result = UnicodePipeline::process(nfd).unwrap();
    // NFC of this should be "ậ" (U+1EAD)
    let expected: String = nfd.nfc().collect();
    assert_eq!(result.as_str(), expected.as_str());
}

/// The `InvalidUtf8` error variant is present for FFI-boundary use.
#[test]
fn invalid_utf8_variant_exists() {
    let _: CompositionError = CompositionError::InvalidUtf8;
}

/// ZWJ emoji sequences pass through the pipeline unchanged.
#[test]
fn zwj_emoji_sequence_passes() {
    let zwj = "\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}";
    let result = UnicodePipeline::process(zwj).unwrap();
    assert!(!result.as_str().is_empty());
    let nfc: String = result.as_str().nfc().collect();
    assert_eq!(result.as_str(), nfc);
}

/// Empty string passes the pipeline.
#[test]
fn empty_string_passes() {
    let result = UnicodePipeline::process("").unwrap();
    assert_eq!(result.as_str(), "");
}

/// ASCII-only input is unchanged.
#[test]
fn ascii_unchanged() {
    let result = UnicodePipeline::process("hello world 123").unwrap();
    assert_eq!(result.as_str(), "hello world 123");
}

// ── NFD round-trip completeness ───────────────────────────────────────────────

/// Every toned vowel decomposed to NFD must produce the same NFC on round-trip.
/// Covers both "natural" CCC order and reversed CCC order on input.
#[test]
fn nfd_round_trip_complex_vowels() {
    let cases: &[(&str, &str)] = &[
        // ặ: a + breve (CCC=230) + dot-below (CCC=220) — non-canonical order; NFD reorders
        ("a\u{0306}\u{0323}", "ặ"),
        // ặ: a + dot-below (CCC=220) + breve (CCC=230) — canonical order
        ("a\u{0323}\u{0306}", "ặ"),
        // ợ: o + horn (CCC=216) + dot-below (CCC=220)
        ("o\u{031B}\u{0323}", "ợ"),
        // ự: u + horn (CCC=216) + dot-below (CCC=220)
        ("u\u{031B}\u{0323}", "ự"),
        // ấ: a + circumflex (CCC=230) + acute (CCC=230) — same CCC, correct order for ấ
        ("a\u{0302}\u{0301}", "ấ"),
        // ầ: a + circumflex + grave — same as above, different tone
        ("a\u{0302}\u{0300}", "ầ"),
    ];
    for &(nfd_input, expected_nfc) in cases {
        let result = UnicodePipeline::process(nfd_input).unwrap();
        assert_eq!(
            result.as_str(),
            expected_nfc,
            "NFD→NFC failed for {:?} (expected {:?})",
            nfd_input,
            expected_nfc,
        );
        // Verify the output is genuinely NFC.
        let renfc: String = result.as_str().nfc().collect();
        assert_eq!(
            result.as_str(),
            renfc.as_str(),
            "output not NFC for {expected_nfc:?}"
        );
    }
}

// ── Combining marks in wrong order (same CCC=230) ─────────────────────────────

/// When two combining marks share CCC=230, Unicode's stable sort preserves their
/// order, so swapping them can produce a different NFC character.  This test
/// documents that the pipeline is deterministic and always produces valid NFC —
/// even when the input order yields a character the user did not intend.
#[test]
fn same_ccc_wrong_order_produces_nfc_not_intended_char() {
    // Input: a + U+0301 (acute, CCC=230) + U+0302 (circumflex, CCC=230).
    // Intended: ấ (U+1EA5 = a + circumflex + acute).
    // Actual NFC: á (U+00E1, from a+acute) followed by U+0302 (no composition).
    let wrong_order = "a\u{0301}\u{0302}";
    let result = UnicodePipeline::process(wrong_order).unwrap();

    // Must be NFC.
    let renfc: String = result.as_str().nfc().collect();
    assert_eq!(
        result.as_str(),
        renfc.as_str(),
        "wrong-order output is not NFC: {:?}",
        result.as_str()
    );

    // Must differ from the intended ấ because order was wrong.
    assert_ne!(
        result.as_str(),
        "ấ",
        "wrong-order marks must not produce the same char as ấ"
    );

    // Correct order for comparison: a + circumflex + acute → ấ
    let correct_order = "a\u{0302}\u{0301}";
    let correct_result = UnicodePipeline::process(correct_order).unwrap();
    assert_eq!(correct_result.as_str(), "ấ");
}

// ── TCVN3/VPS/VISCII detection (C1 control characters) ───────────────────────

/// C1 control codepoints (U+0080–U+009F) appear when TCVN3/VPS/VISCII bytes
/// 0x80–0x9F are decoded as Latin-1 and stored as UTF-8.  The pipeline must
/// reject these rather than silently passing through mojibake.
#[test]
fn c1_controls_are_rejected_as_legacy_encoding() {
    let c1_samples: &[&str] = &[
        "\u{0080}",           // first C1 — TCVN3 byte 0x80 as Latin-1
        "\u{009F}",           // last C1 — TCVN3 byte 0x9F as Latin-1
        "\u{0090}",           // DCS control — VPS byte 0x90 as Latin-1
        "vie\u{0081}t",       // C1 mixed with ASCII (common TCVN3 pattern)
        "\u{0085}t\u{00F4}i", // C1 + valid Vietnamese NFC char
    ];
    for &s in c1_samples {
        let result = UnicodePipeline::process(s);
        assert!(
            matches!(result, Err(CompositionError::LegacyEncoding(_))),
            "expected LegacyEncoding for {:?}, got {result:?}",
            s
        );
    }
}

/// Characters just above the C1 range (U+00A0 and above) must not be rejected.
#[test]
fn above_c1_range_not_rejected() {
    // U+00A0 = NBSP, U+00E0 = à, U+1EAD = ậ
    for s in ["\u{00A0}", "à", "ậ", "\u{00BF}"] {
        let result = UnicodePipeline::process(s);
        assert!(
            result.is_ok(),
            "U+{:04X} must not be rejected as legacy encoding",
            s.chars().next().unwrap() as u32
        );
    }
}

// ── Lone surrogate error variant ──────────────────────────────────────────────

/// The SurrogateCodepoint error variant must be constructable (for FFI/CESU-8
/// paths that can inject surrogates outside Rust's normal type system).
#[test]
fn surrogate_error_variant_constructable() {
    for cp in [0xD800u32, 0xDBFF, 0xDC00, 0xDFFF] {
        let err = CompositionError::SurrogateCodepoint(cp);
        assert!(err.to_string().contains(&format!("U+{cp:04X}")));
    }
}

/// Engine output must never contain surrogate codepoints regardless of method.
#[test]
fn all_methods_output_no_surrogates() {
    let sequences: &[(InputMethod, &str)] = &[
        (InputMethod::Telex, "vieejt"),
        (InputMethod::Vni, "d9u7o7c5"),
        (InputMethod::Viqr, "ddu+o+c."),
    ];
    for &(method, input) in sequences {
        let mut engine = StandardEngine::new(method);
        let mut out = String::new();
        for ch in input.chars() {
            if let Ok(StateTransition::CommitAndClear(c) | StateTransition::Commit(c)) =
                engine.process(&InputEvent::KeyDown(Key::Char(ch), Modifiers::none()))
            {
                out.push_str(c.as_str());
            }
        }
        if let Ok(StateTransition::CommitAndClear(c) | StateTransition::Commit(c)) =
            engine.process(&InputEvent::KeyDown(Key::Return, Modifiers::none()))
        {
            out.push_str(c.as_str());
        }
        for ch in out.chars() {
            let cp = ch as u32;
            assert!(
                !(0xD800..=0xDFFF).contains(&cp),
                "surrogate U+{cp:04X} in output of {method:?}/{input:?}"
            );
        }
    }
}
