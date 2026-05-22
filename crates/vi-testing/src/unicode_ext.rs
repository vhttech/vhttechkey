//! Extended Unicode torture test helpers and tests.
//!
//! Covers: emoji + Vietnamese combining marks, U+FFFE / U+FFFF non-characters,
//! NFD → NFC normalization, orphaned combining marks, and ZWJ sequences — all
//! exercised both through `UnicodePipeline` directly and through the engine.

use unicode_normalization::UnicodeNormalization;
use vi_core::UnicodePipeline;

/// Assert that U+FFFE and U+FFFF do not panic the `UnicodePipeline`.
pub fn fffe_ffff_no_panic() {
    let _ = UnicodePipeline::process("\u{FFFE}");
    let _ = UnicodePipeline::process("\u{FFFF}");
    let _ = UnicodePipeline::process("\u{FFFE}\u{FFFF}");
}

/// Assert that NFD Vietnamese vowels are normalized to NFC by the pipeline.
pub fn nfd_normalizes_to_nfc() {
    let cases: &[(&str, &str)] = &[
        ("a\u{0300}", "à"),
        ("o\u{0302}", "ô"),
        ("a\u{0302}\u{0323}", "ậ"),
        ("u\u{031B}\u{0301}", "ứ"),
    ];
    for &(nfd, expected) in cases {
        let result = UnicodePipeline::process(nfd).expect("NFD should normalize");
        assert_eq!(result.as_str(), expected, "NFD → NFC failed for {nfd:?}");
    }
}

/// Assert that a ZWJ emoji sequence passes the pipeline and stays NFC.
pub fn zwj_sequence_passes() {
    let zwj = "\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}";
    let r = UnicodePipeline::process(zwj).expect("ZWJ sequence must pass");
    let nfc: String = r.as_str().nfc().collect();
    assert_eq!(r.as_str(), nfc.as_str(), "ZWJ result not NFC");
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use vi_core::{
        CompositionError, CompositionEngine, InputEvent, InputMethod, Key, Modifiers,
        StandardEngine, StateTransition,
    };

    #[test]
    fn test_fffe_ffff_no_panic() {
        fffe_ffff_no_panic();
    }

    #[test]
    fn test_nfd_normalizes_to_nfc() {
        nfd_normalizes_to_nfc();
    }

    #[test]
    fn test_zwj_sequence_passes() {
        zwj_sequence_passes();
    }

    /// U+FFFE and U+FFFF as Key::Char must not panic the engine for any method.
    /// If they produce committed text, that text must be NFC.
    #[test]
    fn fffe_ffff_as_engine_key_no_panic() {
        for method in [InputMethod::Telex, InputMethod::Vni, InputMethod::Viqr] {
            let mut engine = StandardEngine::new(method);
            let _ = engine.process(&InputEvent::KeyDown(Key::Char('\u{FFFE}'), Modifiers::none()));
            let _ = engine.process(&InputEvent::KeyDown(Key::Char('\u{FFFF}'), Modifiers::none()));
            if let Ok(StateTransition::Commit(c) | StateTransition::CommitAndClear(c)) =
                engine.process(&InputEvent::FocusOut)
            {
                let nfc: String = c.as_str().nfc().collect();
                assert_eq!(c.as_str(), nfc.as_str(), "FFFE/FFFF committed text not NFC");
            }
        }
    }

    /// Emoji typed before Vietnamese tone marks: preedit must stay NFC throughout.
    #[test]
    fn emoji_then_vietnamese_preedit_is_nfc() {
        let cases: &[(InputMethod, char, char)] = &[
            (InputMethod::Telex, 'a', 'f'), // "af" → "à" (huyền)
            (InputMethod::Vni, 'a', '2'),   // "a2" → "à" (huyền)
        ];
        for &(method, vowel, tone) in cases {
            let mut engine = StandardEngine::new(method);
            let _ = engine.process(&InputEvent::KeyDown(Key::Char('😀'), Modifiers::none()));
            let _ = engine.process(&InputEvent::KeyDown(Key::Char(vowel), Modifiers::none()));
            let _ = engine.process(&InputEvent::KeyDown(Key::Char(tone), Modifiers::none()));

            let p = engine.preedit().as_str().to_owned();
            let nfc: String = p.nfc().collect();
            assert_eq!(p, nfc, "preedit not NFC for {method:?}: {p:?}");
        }
    }

    /// Orphaned combining marks (no preceding base character) are rejected.
    #[test]
    fn orphaned_combining_marks_rejected() {
        let orphans: &[&str] = &[
            "\u{0300}", // lone grave
            "\u{0301}", // lone acute
            "\u{0302}", // lone circumflex
            "\u{0323}", // lone dot below
        ];
        for &s in orphans {
            let result = UnicodePipeline::process(s);
            assert!(
                matches!(result, Err(CompositionError::OrphanedCombiningMark(_))),
                "expected OrphanedCombiningMark for {s:?}, got {result:?}"
            );
        }
    }

    /// ZWJ emoji sequence typed through the engine must not panic and preedit
    /// must remain NFC with no surrogate codepoints.
    #[test]
    fn zwj_sequence_through_engine_nfc_no_surrogates() {
        let chars = ['\u{1F468}', '\u{200D}', '\u{1F469}', '\u{200D}', '\u{1F467}'];
        let mut engine = StandardEngine::new(InputMethod::Telex);
        for ch in chars {
            let _ = engine.process(&InputEvent::KeyDown(Key::Char(ch), Modifiers::none()));
        }
        let p = engine.preedit().as_str().to_owned();
        let nfc: String = p.nfc().collect();
        assert_eq!(p, nfc, "ZWJ preedit not NFC: {p:?}");
        for ch in p.chars() {
            let cp = ch as u32;
            assert!(!(0xD800..=0xDFFF).contains(&cp), "surrogate U+{cp:04X} in ZWJ preedit");
        }
    }

    /// U+FFFE mixed with Vietnamese text: output must be NFC and surrogate-free.
    #[test]
    fn fffe_mixed_with_vietnamese_nfc() {
        let mut engine = StandardEngine::new(InputMethod::Telex);
        let _ = engine.process(&InputEvent::KeyDown(Key::Char('\u{FFFE}'), Modifiers::none()));
        let _ = engine.process(&InputEvent::KeyDown(Key::Char('t'), Modifiers::none()));
        let _ = engine.process(&InputEvent::KeyDown(Key::Char('o'), Modifiers::none()));
        let _ = engine.process(&InputEvent::KeyDown(Key::Char('o'), Modifiers::none())); // "oo" → "ô"

        let p = engine.preedit().as_str().to_owned();
        let nfc: String = p.nfc().collect();
        assert_eq!(p, nfc, "mixed FFFE+Vietnamese preedit not NFC: {p:?}");
        for ch in p.chars() {
            let cp = ch as u32;
            assert!(!(0xD800..=0xDFFF).contains(&cp), "surrogate U+{cp:04X} in preedit");
        }
    }
}
