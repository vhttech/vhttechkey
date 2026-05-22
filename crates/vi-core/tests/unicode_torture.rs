#![allow(clippy::unwrap_used)]
//! Unicode edge-case tests for `UnicodePipeline`.
//!
//! Extended coverage added here includes:
//! - All 72 Vietnamese vowel-tone nuclei (12 vowel bases × 6 tones) in NFC passthrough and NFD→NFC.
//! - Three known regression codepoints: à U+00E0, ô U+00F4, ư U+01B0.
//! - Surrogate-adjacent codepoints U+D7FF and U+E000.
//! - Zero-width joiner edge cases.
//! - Emoji adjacent to Vietnamese text.
//! - Orphaned combining marks from U+1AB0, U+1DC0, U+20D0 extended blocks.
//! - `normalize_nfc` fast-path for already-NFC and pure-ASCII input.

use std::borrow::Cow;

use vi_core::{unicode_pipeline::normalize_nfc, CompositionError, UnicodePipeline};

#[test]
fn plain_ascii_passes() {
    UnicodePipeline::process("hello world").unwrap();
}

#[test]
fn double_encoded_nfd_corrected_to_nfc() {
    // "à" in NFD: 'a' + U+0300
    let nfd = "a\u{0300}";
    let r = UnicodePipeline::process(nfd).unwrap();
    // Must come out as U+00E0 (NFC precomposed)
    assert_eq!(r.as_str(), "\u{00E0}");
    assert_eq!(r.as_str().chars().count(), 1);
}

#[test]
fn output_is_always_nfc() {
    use unicode_normalization::UnicodeNormalization;
    let inputs = ["tôi", "được", "Việt Nam", "a\u{0300}", "o\u{0302}\u{0301}"];
    for input in inputs {
        let r = UnicodePipeline::process(input).unwrap();
        let nfc: String = r.as_str().nfc().collect();
        assert_eq!(r.as_str(), nfc, "output not NFC for input {input:?}");
    }
}

#[test]
fn zwj_sequence_passes() {
    // Family emoji with ZWJ — should pass through
    let zwj = "\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}";
    let r = UnicodePipeline::process(zwj).unwrap();
    assert!(!r.as_str().is_empty());
}

#[test]
fn combining_mark_with_no_base_rejected() {
    // Lone combining grave accent with no preceding base
    let input = "\u{0300}";
    let err = UnicodePipeline::process(input).unwrap_err();
    assert!(
        matches!(err, CompositionError::OrphanedCombiningMark(_)),
        "expected OrphanedCombiningMark, got {err:?}"
    );
}

#[test]
fn combining_mark_after_letter_accepted() {
    // 'a' + combining grave — valid, should normalize to U+00E0
    let r = UnicodePipeline::process("a\u{0300}").unwrap();
    assert_eq!(r.as_str(), "\u{00E0}");
}

#[test]
fn multiple_combining_marks_on_one_base() {
    // Vietnamese "ậ" can be represented as â + combining dot below.
    // NFC should collapse this.
    let nfd = "a\u{0302}\u{0323}"; // â + dot below
    let r = UnicodePipeline::process(nfd).unwrap();
    assert_eq!(r.as_str(), "ậ");
}

#[test]
fn empty_string_passes() {
    let r = UnicodePipeline::process("").unwrap();
    assert_eq!(r.as_str(), "");
}

#[test]
fn pure_ascii_unchanged() {
    let r = UnicodePipeline::process("The quick brown fox").unwrap();
    assert_eq!(r.as_str(), "The quick brown fox");
}

#[test]
fn full_vietnamese_sentence_nfc() {
    use unicode_normalization::UnicodeNormalization;
    let sentence = "Tôi yêu Việt Nam";
    let r = UnicodePipeline::process(sentence).unwrap();
    let nfc: String = sentence.nfc().collect();
    assert_eq!(r.as_str(), nfc);
}

// ── Regression: three canonical double-encoding cases ────────────────────────

#[test]
fn regression_a_grave_u00e0_vs_u0061_u0300() {
    // NFD "a\u{0300}" must produce the same NFC as the precomposed U+00E0.
    let nfd = "\u{0061}\u{0300}"; // U+0061 'a' + U+0300 combining grave
    let r = UnicodePipeline::process(nfd).unwrap();
    assert_eq!(r.as_str(), "\u{00E0}", "à: NFD must produce U+00E0");
    assert_eq!(r.as_str().chars().count(), 1, "must be one precomposed codepoint");

    // Precomposed form must pass through unchanged.
    let precomposed = "\u{00E0}";
    let r2 = UnicodePipeline::process(precomposed).unwrap();
    assert_eq!(r2.as_str(), precomposed);
}

#[test]
fn regression_o_circumflex_u00f4_vs_u006f_u0302() {
    // NFD "o\u{0302}" must produce the same NFC as the precomposed U+00F4.
    let nfd = "\u{006F}\u{0302}"; // U+006F 'o' + U+0302 combining circumflex
    let r = UnicodePipeline::process(nfd).unwrap();
    assert_eq!(r.as_str(), "\u{00F4}", "ô: NFD must produce U+00F4");
    assert_eq!(r.as_str().chars().count(), 1);

    let precomposed = "\u{00F4}";
    let r2 = UnicodePipeline::process(precomposed).unwrap();
    assert_eq!(r2.as_str(), precomposed);
}

#[test]
fn regression_u_horn_u01b0_vs_u0075_u031b() {
    // NFD "u\u{031B}" must produce the same NFC as the precomposed U+01B0 ư.
    let nfd = "\u{0075}\u{031B}"; // U+0075 'u' + U+031B combining horn
    let r = UnicodePipeline::process(nfd).unwrap();
    assert_eq!(r.as_str(), "\u{01B0}", "ư: NFD must produce U+01B0");
    assert_eq!(r.as_str().chars().count(), 1);

    let precomposed = "\u{01B0}";
    let r2 = UnicodePipeline::process(precomposed).unwrap();
    assert_eq!(r2.as_str(), precomposed);
}

// ── All 72 Vietnamese nuclei × 6 tones: NFC passthrough + NFD→NFC ────────────

#[test]
fn all_nuclei_nfc_passthrough_and_nfd_roundtrip() {
    use unicode_normalization::UnicodeNormalization;

    // 12 vowel bases × 6 tones, drawn from the same golden matrix used by
    // crates/vi-testing/src/golden.rs.  Each string is already NFC-precomposed.
    #[rustfmt::skip]
    let nuclei: &[&str] = &[
        // flat (no tone mark)
        "a", "ă", "â", "e", "ê", "i", "o", "ô", "ơ", "u", "ư", "y",
        // huyền (grave)
        "à", "ằ", "ầ", "è", "ề", "ì", "ò", "ồ", "ờ", "ù", "ừ", "ỳ",
        // sắc (acute)
        "á", "ắ", "ấ", "é", "ế", "í", "ó", "ố", "ớ", "ú", "ứ", "ý",
        // hỏi (hook above)
        "ả", "ẳ", "ẩ", "ẻ", "ể", "ỉ", "ỏ", "ổ", "ở", "ủ", "ử", "ỷ",
        // ngã (tilde)
        "ã", "ẵ", "ẫ", "ẽ", "ễ", "ĩ", "õ", "ỗ", "ỡ", "ũ", "ữ", "ỹ",
        // nặng (dot below)
        "ạ", "ặ", "ậ", "ẹ", "ệ", "ị", "ọ", "ộ", "ợ", "ụ", "ự", "ỵ",
    ];

    for &nucleus in nuclei {
        // 1. Precomposed NFC must pass through unchanged.
        let r = UnicodePipeline::process(nucleus).unwrap_or_else(|e| {
            panic!("process({nucleus:?}) failed: {e:?}");
        });
        assert_eq!(r.as_str(), nucleus, "NFC passthrough failed for {nucleus:?}");

        // 2. NFD form must normalize back to the precomposed NFC.
        let nfd: String = nucleus.nfd().collect();
        let r2 = UnicodePipeline::process(&nfd).unwrap_or_else(|e| {
            panic!("process(NFD of {nucleus:?} = {nfd:?}) failed: {e:?}");
        });
        assert_eq!(r2.as_str(), nucleus, "NFD→NFC roundtrip failed for {nucleus:?}");
    }
}

// ── Surrogate-adjacent codepoints ────────────────────────────────────────────

#[test]
fn surrogate_adjacent_codepoints_pass_through() {
    // U+D7FF is the last codepoint before the surrogate range; U+E000 is the
    // first Private Use Area codepoint after it.  Both are valid in UTF-8 and
    // must not be rejected.
    let just_before = "\u{D7FF}"; // Hangul Jamo Extended-B last char
    let just_after = "\u{E000}";  // Private Use Area start

    let r1 = UnicodePipeline::process(just_before).unwrap_or_else(|e| {
        panic!("U+D7FF rejected unexpectedly: {e:?}");
    });
    assert!(!r1.as_str().is_empty());

    let r2 = UnicodePipeline::process(just_after).unwrap_or_else(|e| {
        panic!("U+E000 rejected unexpectedly: {e:?}");
    });
    assert!(!r2.as_str().is_empty());

    // Together in one string.
    let combined = "\u{D7FF}\u{E000}";
    let r3 = UnicodePipeline::process(combined).unwrap_or_else(|e| {
        panic!("U+D7FF U+E000 combined rejected unexpectedly: {e:?}");
    });
    assert_eq!(r3.as_str(), combined);
}

// ── Zero-width joiners ────────────────────────────────────────────────────────

#[test]
fn zwj_between_emoji_is_valid() {
    // U+200D between two emoji base characters is a valid ZWJ sequence.
    let zwj_pair = "\u{1F468}\u{200D}\u{1F469}"; // man ZWJ woman
    UnicodePipeline::process(zwj_pair).unwrap();
}

#[test]
fn zwj_at_start_of_string_is_rejected() {
    // ZWJ alone at the start (no preceding base) — the ZWJ itself is not a
    // combining mark but the next combining grave after it would be orphaned.
    // A standalone ZWJ with no context is allowed; ZWJ + combining mark is not.
    let zwj_then_mark = "\u{200D}\u{0301}"; // ZWJ + combining acute
    let err = UnicodePipeline::process(zwj_then_mark).unwrap_err();
    assert!(
        matches!(err, vi_core::CompositionError::OrphanedCombiningMark(_)),
        "expected OrphanedCombiningMark after ZWJ, got {err:?}"
    );
}

#[test]
fn zwj_in_family_emoji_sequence_passes() {
    // 👨‍👩‍👧‍👦 man-ZWJ-woman-ZWJ-girl-ZWJ-boy
    let family = "\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}\u{200D}\u{1F466}";
    UnicodePipeline::process(family).unwrap();
}

// ── Emoji adjacent to Vietnamese text ────────────────────────────────────────

#[test]
fn emoji_before_vietnamese_text() {
    // 🇻🇳 (regional indicator V + N) followed by Vietnamese text.
    let s = "\u{1F1FB}\u{1F1F3}Việt Nam";
    let r = UnicodePipeline::process(s).unwrap();
    use unicode_normalization::UnicodeNormalization;
    let nfc: String = s.nfc().collect();
    assert_eq!(r.as_str(), nfc);
}

#[test]
fn emoji_after_vietnamese_text() {
    let s = "Xin chào \u{1F44B}"; // "Xin chào 👋"
    let r = UnicodePipeline::process(s).unwrap();
    use unicode_normalization::UnicodeNormalization;
    let nfc: String = s.nfc().collect();
    assert_eq!(r.as_str(), nfc);
}

#[test]
fn emoji_interleaved_with_vietnamese() {
    let s = "tôi \u{2764} Việt Nam \u{1F1FB}\u{1F1F3}"; // tôi ❤ Việt Nam 🇻🇳
    let r = UnicodePipeline::process(s).unwrap();
    use unicode_normalization::UnicodeNormalization;
    let nfc: String = s.nfc().collect();
    assert_eq!(r.as_str(), nfc);
}

#[test]
fn precomposed_vietnamese_roundtrip() {
    // 'ấ' (U+1EA5) is a precomposed NFC Vietnamese character; it must pass
    // through unchanged and remain a single codepoint.
    let r = UnicodePipeline::process("ấ").unwrap();
    assert_eq!(r.as_str(), "ấ");
    assert_eq!(r.as_str().chars().count(), 1);
}

#[test]
fn emoji_plus_combining_rejected() {
    // U+1F600 GRINNING FACE + U+0301 COMBINING ACUTE ACCENT
    assert!(UnicodePipeline::process("\u{1F600}\u{0301}").is_err());
}

#[test]
fn mixed_valid_invalid_rejected() {
    // U+0080 is a C1 control character, must cause rejection
    assert!(UnicodePipeline::process("hello\u{0080}world").is_err());
}

// ── Orphaned combining marks from extended Unicode blocks ─────────────────────

#[test]
fn orphaned_combining_mark_u1ab0_block_rejected() {
    // U+1AB0 COMBINING DOUBLED CIRCUMFLEX ACCENT (Combining Diacritical Marks Extended)
    let err = UnicodePipeline::process("\u{1AB0}a").unwrap_err();
    assert!(
        matches!(err, CompositionError::OrphanedCombiningMark(_)),
        "expected OrphanedCombiningMark for U+1AB0 at start, got {err:?}"
    );
}

#[test]
fn orphaned_combining_mark_u1dc0_block_rejected() {
    // U+1DC0 COMBINING DOTTED GRAVE ACCENT (Combining Diacritical Marks Supplement)
    let err = UnicodePipeline::process("\u{1DC0}a").unwrap_err();
    assert!(
        matches!(err, CompositionError::OrphanedCombiningMark(_)),
        "expected OrphanedCombiningMark for U+1DC0 at start, got {err:?}"
    );
}

#[test]
fn orphaned_combining_mark_u20d0_block_rejected() {
    // U+20D0 COMBINING LEFT HARPOON ABOVE (Combining Diacritical Marks for Symbols)
    let err = UnicodePipeline::process("\u{20D0}a").unwrap_err();
    assert!(
        matches!(err, CompositionError::OrphanedCombiningMark(_)),
        "expected OrphanedCombiningMark for U+20D0 at start, got {err:?}"
    );
}

#[test]
fn combining_mark_u1ab0_after_base_accepted() {
    use unicode_normalization::UnicodeNormalization;
    let input = "a\u{1AB0}";
    let r = UnicodePipeline::process(input).unwrap();
    let expected: String = input.nfc().collect();
    assert_eq!(r.as_str(), expected);
}

#[test]
fn combining_mark_u1dc0_after_base_accepted() {
    use unicode_normalization::UnicodeNormalization;
    let input = "a\u{1DC0}";
    let r = UnicodePipeline::process(input).unwrap();
    let expected: String = input.nfc().collect();
    assert_eq!(r.as_str(), expected);
}

#[test]
fn combining_mark_u20d0_after_base_accepted() {
    use unicode_normalization::UnicodeNormalization;
    let input = "a\u{20D0}";
    let r = UnicodePipeline::process(input).unwrap();
    let expected: String = input.nfc().collect();
    assert_eq!(r.as_str(), expected);
}

// ── normalize_nfc fast-path: already-NFC and pure ASCII ──────────────────────

#[test]
fn normalize_fast_path_pure_ascii() {
    let input = "hello world";
    let result = normalize_nfc(input);
    assert!(
        matches!(result, Cow::Borrowed(_)),
        "pure ASCII must take the is_nfc fast-path (no allocation)"
    );
    assert_eq!(result.as_ref(), input);
}

#[test]
fn normalize_fast_path_already_nfc_vietnamese() {
    // Precomposed NFC Vietnamese — is_nfc() must return true, no reallocation.
    let input = "tôi yêu Việt Nam";
    let result = normalize_nfc(input);
    assert!(
        matches!(result, Cow::Borrowed(_)),
        "already-NFC Vietnamese string must take the fast-path (no reallocation)"
    );
    assert_eq!(result.as_ref(), input);
}

// ── 36 test vectors: 6 tones × 6 modified vowels, nucleus position check ─────

#[test]
fn modified_vowel_tone_matrix_nfc_and_nucleus_position() {
    use unicode_normalization::{char::canonical_combining_class, UnicodeNormalization};

    // Vietnamese tone combining marks (level = no mark).
    const TONES: [Option<char>; 6] = [
        None,               // level   — no diacritic
        Some('\u{0301}'),   // sắc     — acute accent
        Some('\u{0300}'),   // huyền   — grave accent
        Some('\u{0309}'),   // hỏi     — hook above
        Some('\u{0303}'),   // ngã     — tilde
        Some('\u{0323}'),   // nặng    — dot below
    ];

    // (base vowel, vowel modifier, [expected NFC for each of the 6 tones])
    let rows: &[(char, char, [&str; 6])] = &[
        ('a', '\u{0302}', ["â", "ấ", "ầ", "ẩ", "ẫ", "ậ"]),
        ('a', '\u{0306}', ["ă", "ắ", "ằ", "ẳ", "ẵ", "ặ"]),
        ('e', '\u{0302}', ["ê", "ế", "ề", "ể", "ễ", "ệ"]),
        ('o', '\u{0302}', ["ô", "ố", "ồ", "ổ", "ỗ", "ộ"]),
        ('o', '\u{031B}', ["ơ", "ớ", "ờ", "ở", "ỡ", "ợ"]),
        ('u', '\u{031B}', ["ư", "ứ", "ừ", "ử", "ữ", "ự"]),
    ];

    for (base, modifier, expected) in rows {
        for (tone_idx, tone_opt) in TONES.iter().enumerate() {
            let expected_toned: &str = expected[tone_idx];

            // NFD input: consonant 'b' + base vowel + vowel modifier + optional tone.
            let mut nfd_input = String::from('b');
            nfd_input.push(*base);
            nfd_input.push(*modifier);
            if let Some(tone) = tone_opt {
                nfd_input.push(*tone);
            }

            let result = UnicodePipeline::process(&nfd_input)
                .unwrap_or_else(|e| panic!("process({nfd_input:?}) failed: {e:?}"));
            let s = result.as_str();

            // 1. Output must be NFC.
            let nfc_check: String = s.nfc().collect();
            assert_eq!(s, nfc_check, "output not NFC for {nfd_input:?}");

            // 2. Result is 'b' followed by the single precomposed toned vowel.
            let expected_str = format!("b{expected_toned}");
            assert_eq!(s, expected_str, "wrong toned form for {nfd_input:?}");

            // 3. Consonant 'b' carries no combining marks (CCC == 0).
            let mut chars_iter = s.chars();
            let consonant = chars_iter.next().unwrap();
            assert_eq!(
                canonical_combining_class(consonant),
                0,
                "consonant carries combining mark for {nfd_input:?}"
            );

            // 4. Tone is fused into the single precomposed vowel codepoint.
            let vowel_ch = chars_iter.next().unwrap();
            assert_eq!(
                vowel_ch.to_string(),
                expected_toned,
                "tone not on vowel nucleus for {nfd_input:?}"
            );
            assert!(
                chars_iter.next().is_none(),
                "extra codepoints after toned vowel for {nfd_input:?}"
            );
        }
    }
}

// ── Roundtrip stability: NFC → NFD → NFC must not drift ──────────────────────

#[test]
fn roundtrip_stability_nfc_nfd_nfc() {
    use unicode_normalization::UnicodeNormalization;

    // 36 NFC test vectors: 6 modified vowels × 6 tones.
    let test_vectors: &[&str] = &[
        "â", "ấ", "ầ", "ẩ", "ẫ", "ậ",
        "ă", "ắ", "ằ", "ẳ", "ẵ", "ặ",
        "ê", "ế", "ề", "ể", "ễ", "ệ",
        "ô", "ố", "ồ", "ổ", "ỗ", "ộ",
        "ơ", "ớ", "ờ", "ở", "ỡ", "ợ",
        "ư", "ứ", "ừ", "ử", "ữ", "ự",
    ];

    for &nfc_original in test_vectors {
        // NFC → NFD → NFC must reproduce the original (no codepoint drift).
        let nfd: String = nfc_original.nfd().collect();
        let nfc_restored: String = nfd.nfc().collect();
        assert_eq!(
            nfc_original, nfc_restored,
            "NFC→NFD→NFC drift: {nfc_original:?} → {nfc_restored:?}"
        );

        // The pipeline must also accept the restored form and return it unchanged.
        let r = UnicodePipeline::process(nfc_restored.as_str()).unwrap_or_else(|e| {
            panic!("pipeline rejected roundtripped {nfc_original:?}: {e:?}");
        });
        assert_eq!(
            r.as_str(),
            nfc_original,
            "pipeline roundtrip mismatch for {nfc_original:?}"
        );
    }
}
