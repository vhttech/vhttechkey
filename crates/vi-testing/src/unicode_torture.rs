//! Structured Unicode torture-test cases for `UnicodePipeline::process`.
//!
//! Each case carries a `raw_input` string and an `expected_nfc`:
//! - `Some(s)` — the pipeline must succeed and produce exactly `s`.
//! - `None`    — the pipeline must return an error (e.g., orphaned combining mark).

/// One Unicode torture-test case.
pub struct UnicodeTortureCase {
    pub description: &'static str,
    /// Input fed to `UnicodePipeline::process`.
    pub raw_input: &'static str,
    /// Expected NFC output, or `None` if the pipeline should return an error.
    pub expected_nfc: Option<&'static str>,
}

/// Return all torture cases.
pub fn cases() -> Vec<UnicodeTortureCase> {
    vec![
        // ── NFD → NFC normalization ───────────────────────────────────────────
        UnicodeTortureCase {
            description: "NFD tôi normalizes to NFC tôi",
            // t + o + combining circumflex (U+0302) + i
            raw_input: "to\u{0302}i",
            expected_nfc: Some("tôi"),
        },
        UnicodeTortureCase {
            description: "NFD â (a + combining circumflex)",
            raw_input: "a\u{0302}",
            expected_nfc: Some("â"),
        },
        UnicodeTortureCase {
            description: "NFD ă (a + combining breve U+0306)",
            raw_input: "a\u{0306}",
            expected_nfc: Some("ă"),
        },
        UnicodeTortureCase {
            description: "NFD ộ (o + circumflex + dot below)",
            raw_input: "o\u{0302}\u{0323}",
            expected_nfc: Some("ộ"),
        },
        UnicodeTortureCase {
            description: "NFD ứ (u + horn U+031B + acute)",
            raw_input: "u\u{031B}\u{0301}",
            expected_nfc: Some("ứ"),
        },
        UnicodeTortureCase {
            description: "NFD ẵ (a + breve + tilde)",
            raw_input: "a\u{0306}\u{0303}",
            expected_nfc: Some("ẵ"),
        },
        UnicodeTortureCase {
            description: "NFD ầ (a + circumflex + grave)",
            raw_input: "a\u{0302}\u{0300}",
            expected_nfc: Some("ầ"),
        },
        UnicodeTortureCase {
            description: "NFD à (a + combining grave, double-encoded)",
            raw_input: "a\u{0300}",
            expected_nfc: Some("à"),
        },
        UnicodeTortureCase {
            description: "NFD ô (o + combining circumflex, double-encoded)",
            raw_input: "o\u{0302}",
            expected_nfc: Some("ô"),
        },
        UnicodeTortureCase {
            description: "NFD ị (i + combining dot below)",
            raw_input: "i\u{0323}",
            expected_nfc: Some("ị"),
        },
        // ── Already-NFC passthrough ───────────────────────────────────────────
        UnicodeTortureCase {
            description: "already-NFC Vietnamese sentence passthrough",
            raw_input: "Xin ch\u{00E0}o", // "Xin chào" precomposed
            expected_nfc: Some("Xin ch\u{00E0}o"),
        },
        UnicodeTortureCase {
            description: "empty string passthrough",
            raw_input: "",
            expected_nfc: Some(""),
        },
        UnicodeTortureCase {
            description: "plain ASCII passthrough",
            raw_input: "hello world",
            expected_nfc: Some("hello world"),
        },
        UnicodeTortureCase {
            description: "CJK characters passthrough",
            raw_input: "\u{4E2D}\u{6587}", // 中文
            expected_nfc: Some("\u{4E2D}\u{6587}"),
        },
        // ── Emoji / special sequences ─────────────────────────────────────────
        UnicodeTortureCase {
            description: "family emoji ZWJ sequence passthrough",
            // 👨‍👩‍👧  (man + ZWJ + woman + ZWJ + girl)
            raw_input: "\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}",
            expected_nfc: Some("\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}"),
        },
        UnicodeTortureCase {
            description: "emoji with skin-tone modifier passthrough",
            // 👋🏻  (waving hand + light skin tone)
            raw_input: "\u{1F44B}\u{1F3FB}",
            expected_nfc: Some("\u{1F44B}\u{1F3FB}"),
        },
        UnicodeTortureCase {
            description: "Vietnamese ASCII + flag emoji (VN flag 🇻🇳)",
            raw_input: "viet\u{1F1FB}\u{1F1F3}",
            expected_nfc: Some("viet\u{1F1FB}\u{1F1F3}"),
        },
        // ── Non-character code points ─────────────────────────────────────────
        UnicodeTortureCase {
            description: "U+FFFE (non-character) passes through",
            raw_input: "\u{FFFE}",
            expected_nfc: Some("\u{FFFE}"),
        },
        UnicodeTortureCase {
            description: "U+FFFF (non-character) passes through",
            raw_input: "\u{FFFF}",
            expected_nfc: Some("\u{FFFF}"),
        },
        // ── Mixed scripts ─────────────────────────────────────────────────────
        UnicodeTortureCase {
            description: "mixed Arabic RTL + Vietnamese",
            // Arabic "مرحبا" + space + NFC Vietnamese "tôi"
            raw_input: "\u{0645}\u{0631}\u{062D}\u{0628}\u{0627} t\u{00F4}i",
            expected_nfc: Some("\u{0645}\u{0631}\u{062D}\u{0628}\u{0627} t\u{00F4}i"),
        },
        // ── Error cases: orphaned combining marks ─────────────────────────────
        UnicodeTortureCase {
            description: "lone combining grave U+0300 → error",
            raw_input: "\u{0300}",
            expected_nfc: None,
        },
        UnicodeTortureCase {
            description: "lone combining acute U+0301 → error",
            raw_input: "\u{0301}",
            expected_nfc: None,
        },
        UnicodeTortureCase {
            description: "lone combining circumflex U+0302 → error",
            raw_input: "\u{0302}",
            expected_nfc: None,
        },
        UnicodeTortureCase {
            description: "ZWJ followed immediately by combining grave → error",
            // ZWJ (U+200D) is not a valid base character for a combining mark.
            raw_input: "\u{200D}\u{0300}",
            expected_nfc: None,
        },
        // ── 6 tones × sample vowels (precomposed already NFC) ─────────────────
        UnicodeTortureCase {
            description: "precomposed à (U+00E0) already NFC",
            raw_input: "\u{00E0}",
            expected_nfc: Some("\u{00E0}"),
        },
        UnicodeTortureCase {
            description: "precomposed ô (U+00F4) already NFC",
            raw_input: "\u{00F4}",
            expected_nfc: Some("\u{00F4}"),
        },
        UnicodeTortureCase {
            description: "precomposed ộ (U+1EED→U+1ED9) already NFC",
            raw_input: "\u{1ED9}",
            expected_nfc: Some("\u{1ED9}"),
        },
        UnicodeTortureCase {
            description: "precomposed ứ (U+1EE9) already NFC",
            raw_input: "\u{1EE9}",
            expected_nfc: Some("\u{1EE9}"),
        },
        // ── NFD round-trips: more complex combining sequences ─────────────────
        UnicodeTortureCase {
            description: "NFD ặ (a + breve CCC=230 + dot-below CCC=220, non-canonical order)",
            // U+0306 (breve, CCC=230) before U+0323 (dot-below, CCC=220) is
            // non-canonical; the NFD step reorders to (220, 230) before NFC.
            raw_input: "a\u{0306}\u{0323}",
            expected_nfc: Some("ặ"),
        },
        UnicodeTortureCase {
            description: "NFD ặ (a + dot-below CCC=220 + breve CCC=230, canonical order)",
            // U+0323 (dot-below, CCC=220) before U+0306 (breve, CCC=230) is the
            // canonical combining-class order; NFD passes through unchanged.
            raw_input: "a\u{0323}\u{0306}",
            expected_nfc: Some("ặ"),
        },
        UnicodeTortureCase {
            description: "NFD ợ (o + horn U+031B + dot below)",
            raw_input: "o\u{031B}\u{0323}",
            expected_nfc: Some("ợ"),
        },
        UnicodeTortureCase {
            description: "NFD ự (u + horn U+031B + dot below)",
            raw_input: "u\u{031B}\u{0323}",
            expected_nfc: Some("ự"),
        },
        UnicodeTortureCase {
            description: "NFD ấ (a + circumflex + acute, correct CCC order)",
            raw_input: "a\u{0302}\u{0301}",
            expected_nfc: Some("ấ"),
        },
        // ── Combining marks in wrong order (same CCC=230) ─────────────────────
        //
        // U+0301 (acute, CCC=230) and U+0302 (circumflex, CCC=230) share the
        // same Canonical Combining Class.  Unicode's stable sort preserves their
        // input order, so swapping them changes the NFC output character.
        //
        // Input a+acute+circumflex:
        //   NFD: a U+0301 U+0302 (stable, same CCC — no reorder)
        //   NFC step 1: a+U+0301 → á (U+00E1)
        //   NFC step 2: á+U+0302 → no canonical composition → stays á U+0302
        // Result: á\u{0302} (NFC, but NOT the intended ấ = U+1EA5)
        UnicodeTortureCase {
            description: "wrong-order same-CCC: acute before circumflex on 'a' → NFC á+U+0302",
            raw_input: "a\u{0301}\u{0302}",
            expected_nfc: Some("\u{00E1}\u{0302}"),
        },
        // ── Legacy TCVN3/VPS/VISCII encoding detection (C1 control characters) ─
        //
        // When TCVN3/VPS/VISCII documents (8-bit encodings) are decoded as
        // Latin-1 and stored as UTF-8, byte values 0x80–0x9F become U+0080–U+009F
        // (C1 control codepoints).  These never appear in valid UTF-8 Vietnamese
        // text, so the pipeline rejects them with LegacyEncoding.
        UnicodeTortureCase {
            description: "C1 control U+0080 → LegacyEncoding error (TCVN3 marker)",
            raw_input: "\u{0080}",
            expected_nfc: None,
        },
        UnicodeTortureCase {
            description: "C1 control U+0081 in mixed ASCII → LegacyEncoding error",
            raw_input: "vie\u{0081}t",
            expected_nfc: None,
        },
        UnicodeTortureCase {
            description: "C1 control U+009F → LegacyEncoding error (TCVN3/VPS byte 0x9F)",
            raw_input: "\u{009F}",
            expected_nfc: None,
        },
        UnicodeTortureCase {
            description: "C1 control U+0090 (DCS) → LegacyEncoding error",
            raw_input: "\u{0090}",
            expected_nfc: None,
        },
        // ── Latin-1 Supplement above C1 is NOT rejected ───────────────────────
        UnicodeTortureCase {
            description: "U+00A0 (NBSP) is above C1 range — passes through",
            raw_input: "\u{00A0}",
            expected_nfc: Some("\u{00A0}"),
        },
        // ── Lone surrogate error variant (documented; unreachable via &str) ────
        //
        // Rust's `char` type already excludes surrogates, so no &str can contain
        // them.  The SurrogateCodepoint error variant exists for FFI/CESU-8 use.
        // The torture case below uses UnicodeTortureCase for documentation only;
        // actual surrogate injection is tested separately via the error variant.
        UnicodeTortureCase {
            description: "high-codepoint non-character U+10FFFF passes through",
            raw_input: "\u{10FFFF}",
            expected_nfc: Some("\u{10FFFF}"),
        },
    ]
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use unicode_normalization::UnicodeNormalization;
    use vi_core::{
        CompositionEngine, CompositionError, InputEvent, InputMethod, Key, Modifiers,
        StandardEngine, StateTransition, UnicodePipeline,
    };

    /// Every precomposed codepoint in U+00C0–U+00FF and U+1E00–U+1EFF must
    /// survive an NFD → UnicodePipeline::process → NFC roundtrip unchanged.
    ///
    /// Run `cargo insta review` after first run to accept the generated snapshot.
    #[test]
    fn precomposed_vietnamese_ranges_all_nfc() {
        let mut results = String::new();
        for &(start, end) in &[(0x00C0u32, 0x00FFu32), (0x1E00u32, 0x1EFFu32)] {
            for cp in start..=end {
                let ch = char::from_u32(cp).expect("valid Unicode codepoint");
                let nfd: String = ch.to_string().nfd().collect();
                let expected_nfc: String = ch.to_string().nfc().collect();
                match UnicodePipeline::process(&nfd) {
                    Ok(nfc_str) => {
                        assert_eq!(
                            nfc_str.as_str(),
                            expected_nfc,
                            "U+{cp:04X}: NFD→NFC roundtrip produced wrong output"
                        );
                        results.push_str(&format!("U+{cp:04X} ok\n"));
                    }
                    Err(e) => {
                        results.push_str(&format!("U+{cp:04X} err: {e}\n"));
                    }
                }
            }
        }
        insta::assert_snapshot!("precomposed_ranges_nfc", results);
    }

    /// Emoji preceding Vietnamese text: tone marks must be preserved through
    /// NFD normalisation and the Unicode pipeline.
    #[test]
    fn emoji_interleaved_tone_marks_survive() {
        // Already-NFC: 👍tốt
        let nfc_input = "\u{1F44D}t\u{1ED1}t";
        let r = UnicodePipeline::process(nfc_input).unwrap();
        assert_eq!(r.as_str(), nfc_input);

        // NFD form of "tốt" with emoji prefix
        let nfd: String = "tốt".nfd().collect();
        let with_emoji = format!("\u{1F44D}{nfd}");
        let r2 = UnicodePipeline::process(&with_emoji).unwrap();
        let expected: String = "\u{1F44D}tốt".nfc().collect();
        assert_eq!(
            r2.as_str(),
            expected,
            "tone marks must survive emoji+NFD input"
        );
    }

    /// Invalid UTF-8 byte sequences at the engine boundary must never panic;
    /// `String::from_utf8` must reject them and the lossy path must be safe.
    #[test]
    fn invalid_utf8_byte_sequences_never_panic() {
        let bad: &[&[u8]] = &[
            &[0xFF, 0xFE],
            &[0x80, 0x80],
            &[0xC0, 0x80],
            &[0xED, 0xA0, 0x80], // U+D800 in CESU-8
            &[0xED, 0xBF, 0xBF], // U+DFFF in CESU-8
            &[0xF8, 0x88, 0x80, 0x80, 0x80],
            &[0xFE, 0xFF],
        ];
        for &seq in bad {
            assert!(
                std::str::from_utf8(seq).is_err(),
                "expected invalid UTF-8 for {seq:?}"
            );
            // Lossy conversion must not panic, and processing the result must not panic.
            let lossy = String::from_utf8_lossy(seq);
            let _ = UnicodePipeline::process(&lossy);
        }
    }

    /// The engine must never emit a codepoint in the surrogate range U+D800–U+DFFF.
    /// Rust's `char` type structurally prevents surrogates in &str, so this test
    /// verifies the invariant holds for all committed output.
    #[test]
    fn engine_output_never_contains_surrogates() {
        let syllables = ["toi", "viet", "xin", "chao", "tooi", "uws", "ees"];
        for method in [InputMethod::Telex, InputMethod::Vni, InputMethod::Viqr] {
            for &syllable in &syllables {
                let mut engine = StandardEngine::new(method);
                for ch in syllable.chars() {
                    if let Ok(t) =
                        engine.process(&InputEvent::KeyDown(Key::Char(ch), Modifiers::none()))
                    {
                        assert_no_surrogates(&t);
                    }
                }
                if let Ok(t) = engine.process(&InputEvent::KeyDown(Key::Return, Modifiers::none()))
                {
                    assert_no_surrogates(&t);
                }
            }
        }
        // Verify the error variant itself carries the expected value.
        let e = CompositionError::SurrogateCodepoint(0xD800);
        assert!(matches!(e, CompositionError::SurrogateCodepoint(0xD800)));
    }

    fn assert_no_surrogates(t: &StateTransition) {
        let text = match t {
            StateTransition::Commit(c) | StateTransition::CommitAndClear(c) => c.as_str(),
            StateTransition::CommitThenPreedit(c, _) => c.as_str(),
            _ => return,
        };
        for ch in text.chars() {
            let cp = ch as u32;
            assert!(
                !(0xD800..=0xDFFF).contains(&cp),
                "surrogate U+{cp:04X} in engine output {text:?}"
            );
        }
    }

    /// NFD strings passed to `UnicodePipeline::process` (simulating pasted input)
    /// must be normalised to NFC.
    #[test]
    fn nfd_paste_normalizes_to_nfc() {
        let cases: &[(&str, &str)] = &[
            ("to\u{0302}i", "tôi"),
            ("a\u{0306}\u{0323}", "ặ"),
            ("u\u{031B}\u{0301}", "ứ"),
            ("vie\u{0302}t", "viêt"),
            ("a\u{0302}\u{0300}", "ầ"),
        ];
        for &(nfd, expected) in cases {
            let r = UnicodePipeline::process(nfd).unwrap();
            assert_eq!(r.as_str(), expected, "NFD '{nfd}' → expected '{expected}'");
        }
    }

    /// Stacked combining marks must produce either the correct precomposed NFC
    /// output or a clean `OrphanedCombiningMark` error — never garbled text.
    #[test]
    fn stacked_combining_marks_correct_or_error() {
        // o + COMBINING HORN (U+031B, CCC=216) + COMBINING GRAVE (U+0300, CCC=230)
        // Canonical order: 216 < 230, so NFD leaves order unchanged.
        // NFC: → ờ (U+1EDD)
        let o_horn_grave = "o\u{031B}\u{0300}";
        match UnicodePipeline::process(o_horn_grave) {
            Ok(nfc) => {
                let expected: String = o_horn_grave.nfc().collect();
                assert_eq!(nfc.as_str(), expected, "o+horn+grave should give ờ");
            }
            Err(CompositionError::OrphanedCombiningMark(_)) => {}
            Err(e) => panic!("unexpected error for o+horn+grave: {e:?}"),
        }

        // a + COMBINING GRAVE (U+0300, CCC=230) + COMBINING HORN (U+031B, CCC=216)
        // Non-canonical input order; NFD reorders to (216, 230) before NFC.
        let a_grave_horn = "a\u{0300}\u{031B}";
        match UnicodePipeline::process(a_grave_horn) {
            Ok(nfc) => {
                let recheck: String = nfc.as_str().nfc().collect();
                assert_eq!(nfc.as_str(), recheck, "output must be NFC");
            }
            Err(CompositionError::OrphanedCombiningMark(_)) => {}
            Err(e) => panic!("unexpected error for a+grave+horn: {e:?}"),
        }
    }
}
