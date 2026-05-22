use std::borrow::Cow;

use unicode_normalization::UnicodeNormalization;

use crate::{error::CompositionError, types::NfcString};

/// C1 control characters (U+0080–U+009F).  These appear when TCVN3/VPS/VISCII
/// byte values in the range 0x80–0x9F are decoded as Latin-1 and re-encoded as
/// UTF-8, producing nonsense codepoints rather than the intended Vietnamese glyphs.
const C1_CONTROL_START: u32 = 0x0080;
const C1_CONTROL_END: u32 = 0x009F;
/// Zero-width joiner — valid only between emoji base characters; not a valid
/// base for a following combining mark.
const ZWJ: u32 = 0x200D;

pub struct UnicodePipeline;

impl UnicodePipeline {
    /// Normalize a single composed character to NFC and return it as an
    /// [`NfcString`].
    ///
    /// Use this to integrate the output of a dead-key or compose-sequence
    /// resolver (e.g. from `xkbcommon::compose::State` in the platform layer)
    /// into the NFC pipeline before Vietnamese input-method processing.
    /// The character is assumed to be structurally valid; no surrogate or
    /// combining-mark validation is performed.
    pub fn process_composed_char(ch: char) -> NfcString {
        let s: String = std::iter::once(ch).nfc().collect();
        NfcString::new_unchecked(s)
    }

    /// Normalize `input` to NFC, fixing double-encoded Vietnamese and rejecting
    /// any surrogate codepoints or orphaned combining marks.
    pub fn process(input: &str) -> Result<NfcString, CompositionError> {
        validate_no_surrogates(input)?;
        validate_no_legacy_encoding(input)?;
        let nfc = if unicode_normalization::is_nfc(input) {
            input.to_owned()
        } else {
            fix_double_encoded(input)
        };
        validate_combining_marks(&nfc)?;
        verify_nfc(&nfc);
        Ok(NfcString::new_unchecked(nfc))
    }

    /// Normalize an already-known-good string without full validation (fast path).
    pub fn normalize_only(input: &str) -> NfcString {
        let nfc: String = input.nfc().collect();
        verify_nfc(&nfc);
        NfcString::new_unchecked(nfc)
    }
}

/// Assert that `s` is in NFC form. Compiles to nothing in release builds.
#[inline(always)]
fn verify_nfc(s: &str) {
    debug_assert_eq!(
        s.chars().nfc().collect::<String>(),
        s,
        "output is not NFC: {s:?}"
    );
}

/// Normalize `s` to NFC form.
///
/// Returns a borrowed slice when `s` is already NFC (pure ASCII or canonical),
/// avoiding any allocation.  Otherwise returns an owned NFC-normalized string.
pub fn normalize_nfc(s: &str) -> Cow<'_, str> {
    if unicode_normalization::is_nfc(s) {
        return Cow::Borrowed(s);
    }
    Cow::Owned(s.nfc().collect())
}

/// Reject strings that contain C1 control characters (U+0080–U+009F).
///
/// These codepoints have no defined meaning in modern Unicode text.  Their
/// presence almost always means the caller received a TCVN3, VPS, or VISCII
/// document and decoded its raw bytes as Latin-1 before encoding as UTF-8,
/// mapping legacy byte values 0x80–0x9F to U+0080–U+009F.  The pipeline
/// cannot recover the intended Vietnamese characters without a lookup table, so
/// it rejects the string with [`CompositionError::LegacyEncoding`].
fn validate_no_legacy_encoding(s: &str) -> Result<(), CompositionError> {
    for ch in s.chars() {
        let cp = ch as u32;
        if (C1_CONTROL_START..=C1_CONTROL_END).contains(&cp) {
            return Err(CompositionError::LegacyEncoding(cp));
        }
    }
    Ok(())
}

fn validate_no_surrogates(_s: &str) -> Result<(), CompositionError> {
    // Rust's `char` excludes surrogates, so valid `&str` input can never
    // contain surrogate codepoints. This function is kept for API compatibility.
    Ok(())
}

/// Fix legacy double-encoded Vietnamese text and return the NFC result.
///
/// Examples of what this corrects:
/// - "a\u{0300}" (U+0061 U+0300)  →  "à" (U+00E0)
/// - "o\u{0302}" (U+006F U+0302)  →  "ô" (U+00F4)
///
/// Decomposes to NFD (exposing all combining-mark artifacts) then recomposes
/// to NFC in a single iterator chain — one allocation, not two.
fn fix_double_encoded(s: &str) -> String {
    s.nfd().nfc().collect()
}

fn validate_combining_marks(nfc: &str) -> Result<(), CompositionError> {
    let mut prev_is_base = false;
    for ch in nfc.chars() {
        let cp = ch as u32;
        let is_combining = unicode_normalization::char::canonical_combining_class(ch) > 0;
        if is_combining && !prev_is_base {
            return Err(CompositionError::OrphanedCombiningMark(cp));
        }
        // A combining mark does not itself count as a base for the next mark.
        // ZWJ (U+200D) is not a valid base either — it only joins emoji clusters.
        // Supplementary-plane characters (U+10000+, which includes most emoji) are
        // not valid bases for Vietnamese combining marks: emoji cannot form
        // Vietnamese composites, so a combining mark after an emoji is rejected
        // as an orphaned mark even though emoji have CCC=0 in Unicode.
        let is_zwj = cp == ZWJ;
        let is_smp = cp > 0xFFFF;
        prev_is_base = !is_combining && !is_zwj && !is_smp;
    }
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn nfc_output_for_plain_ascii() {
        let r = UnicodePipeline::process("hello").unwrap();
        assert_eq!(r.as_str(), "hello");
    }

    #[test]
    fn double_encoded_a_grave_becomes_nfc() {
        // "a" + combining grave should collapse to U+00E0
        let input = "a\u{0300}";
        let r = UnicodePipeline::process(input).unwrap();
        assert_eq!(r.as_str(), "\u{00E0}");
    }

    #[test]
    fn double_encoded_o_circumflex_becomes_nfc() {
        let input = "o\u{0302}";
        let r = UnicodePipeline::process(input).unwrap();
        assert_eq!(r.as_str(), "\u{00F4}");
    }

    #[test]
    fn already_nfc_passthrough() {
        let input = "tôi"; // already NFC
        let r = UnicodePipeline::process(input).unwrap();
        assert_eq!(r.as_str(), "tôi");
    }

    #[test]
    fn orphaned_combining_mark_rejected() {
        // A combining mark with no preceding base character
        let input = "\u{0300}a";
        let err = UnicodePipeline::process(input).unwrap_err();
        assert!(matches!(err, CompositionError::OrphanedCombiningMark(_)));
    }

    #[test]
    fn emoji_plus_combining_rejected() {
        // U+1F600 GRINNING FACE + U+0301 COMBINING ACUTE ACCENT
        // Emoji are treated as base characters by NFC and cannot form Vietnamese
        // composites; a combining mark after an emoji is rejected as orphaned.
        let input = "\u{1F600}\u{0301}";
        let err = UnicodePipeline::process(input).unwrap_err();
        assert!(
            matches!(err, CompositionError::OrphanedCombiningMark(_)),
            "expected OrphanedCombiningMark, got {err:?}"
        );
    }

    #[test]
    fn test_zwj_mid_sequence_combining_mark_rejected() {
        // ZWJ (U+200D) is not a valid base for a combining mark.
        let input = "\u{1F468}\u{200D}\u{0300}";
        let err = UnicodePipeline::process(input).unwrap_err();
        assert!(
            matches!(err, CompositionError::OrphanedCombiningMark(_)),
            "expected OrphanedCombiningMark, got {err:?}"
        );
    }

    #[test]
    fn test_supplement_combining_mark_orphaned() {
        // U+1DC0 (Supplement combining mark, outside the old 0300–036F range)
        // with no preceding base must be rejected.
        let input = "\u{1DC0}a";
        let err = UnicodePipeline::process(input).unwrap_err();
        assert!(
            matches!(err, CompositionError::OrphanedCombiningMark(_)),
            "expected OrphanedCombiningMark, got {err:?}"
        );
    }


    #[test]
    fn zwj_sequence_passthrough() {
        // Zero-width joiners in emoji sequences should pass through unchanged.
        let input = "\u{1F468}\u{200D}\u{1F469}";
        let r = UnicodePipeline::process(input).unwrap();
        assert!(!r.as_str().is_empty());
    }

    #[test]
    fn c1_control_u0080_rejected_as_legacy_encoding() {
        // U+0080 is the first C1 control character.  It appears when TCVN3/VPS
        // byte 0x80 is decoded as Latin-1 and re-encoded as UTF-8.
        let input = "\u{0080}";
        let err = UnicodePipeline::process(input).unwrap_err();
        assert!(
            matches!(err, CompositionError::LegacyEncoding(0x0080)),
            "expected LegacyEncoding(0x0080), got {err:?}"
        );
    }

    #[test]
    fn c1_control_u009f_rejected_as_legacy_encoding() {
        let input = "\u{009F}";
        let err = UnicodePipeline::process(input).unwrap_err();
        assert!(
            matches!(err, CompositionError::LegacyEncoding(0x009F)),
            "expected LegacyEncoding(0x009F), got {err:?}"
        );
    }

    #[test]
    fn c1_control_mixed_with_ascii_rejected() {
        // TCVN3 byte 0x81 mixed with ASCII letters is a hallmark of improperly
        // decoded legacy Vietnamese documents.
        let input = "vie\u{0081}t";
        let err = UnicodePipeline::process(input).unwrap_err();
        assert!(
            matches!(err, CompositionError::LegacyEncoding(0x0081)),
            "expected LegacyEncoding(0x0081), got {err:?}"
        );
    }

    #[test]
    fn latin1_supplement_above_c1_is_not_rejected() {
        // U+00A0 (NBSP) and above are NOT C1 controls; they should pass.
        let input = "\u{00A0}";
        assert!(UnicodePipeline::process(input).is_ok(), "NBSP must not be rejected");
    }

    #[test]
    fn test_zwj_combining_mark_rejected() {
        // Bug 8: ZWJ (U+200D) is not a valid base for a combining mark.
        // A combining grave immediately after ZWJ must be rejected.
        let input = "\u{200D}\u{0300}"; // ZWJ + combining grave
        let err = UnicodePipeline::process(input).unwrap_err();
        assert!(
            matches!(err, CompositionError::OrphanedCombiningMark(_)),
            "expected OrphanedCombiningMark, got {err:?}"
        );
    }
}
