//! Vietnamese syllable nucleus lookup and tone-placement logic.
//!
//! The tone mark (dấu) must land on the vowel *nucleus* of the syllable, which
//! is not always the last vowel in the preedit display string.
//!
//! Priority order used by [`find_tone_position`]:
//!
//! 1. **Modified-vowel rule** — if any vowel has an inherent form mark
//!    (ă, â, ê, ô, ơ, ư), the *last* such vowel in the sequence carries the
//!    tone.  Examples: "tôi" → ô; "uơi" → ơ; "ươi" → ơ (ơ comes after ư).
//!
//! 2. **"ia" / "ua" rule** — when the nucleus is exactly two unmodified vowels
//!    whose second is 'a' after 'i' or 'u', the *first* vowel carries the tone.
//!    Examples: "ia" → i (gives "ía"); "ua" → u (gives "úa").
//!
//! 3. **Final-glide rule** — if the last vowel is i, u, y, or o following
//!    another vowel it is a final glide; the preceding vowel carries the tone.
//!    Examples: "oi" → o; "ai" → a; "ao" → a; "âu" → â.
//!
//! 4. **Default** — the last vowel in the sequence.

/// All valid Vietnamese syllable nuclei in their undecorated (no-tone) display form.
///
/// Covered by the proptest suite to generate phonologically valid inputs.
/// The list includes monophthongs, inherent diphthongs, medial-onset forms,
/// closed diphthongs with a final glide, and all triphthongs.
///
/// # Non-nuclei deliberately excluded
///
/// - `"oao"` / `"uao"` — occasionally seen in loanwords or dialect forms but are
///   not part of the standard Vietnamese phonological inventory.  They are not
///   listed here and should be rejected by callers that validate against this set.
///   (Tracked in PDCA Round 4 open items.)
pub const VALID_NUCLEI: &[&str] = &[
    // ── Monophthong nuclei (12) ───────────────────────────────────────────────
    "a", "ă", "â", "e", "ê", "i", "o", "ô", "ơ", "u", "ư", "y",
    // ── Inherent diphthong nuclei (open syllables) ────────────────────────────
    // These are the pre-final forms; final-consonant forms (iê/uô/ươ) are below.
    "ia", // iê before a final consonant; tone on i
    "ua", // uô before a final consonant; tone on u
    "ưa", // ươ before a final consonant; tone on ơ (ư has form mark)
    // ── Medial-onset nucleus (o / u is initial glide) ─────────────────────────
    "oa", "oă", "oe", "oê", "uy", "uê",
    // ── Closed diphthong nuclei (final glide is coda) ─────────────────────────
    "ai", "ay", "ao", "au", "âu", "ây", "eo", "êu", "iu", "ôi", "ôu", "ơi", "ơu",
    "oi", // informal / southern Vietnamese
    "ui", "ưi", "ưu",
    // ── Closed diphthong nucleus forms (preedit display) ─────────────────────
    "iê", // ia before final consonant; ê gets tone
    "uô", // ua before final consonant; ô gets tone
    "ươ", // ưa before final consonant; ơ gets tone
    // ── Triphthong nuclei ────────────────────────────────────────────────────
    "iêu", // ê gets tone
    "oai", "oay", // a gets tone (before final i/y glide)
    "uâu", "uây", // â gets tone
    "uôi", // ô gets tone
    "uơi", // ơ gets tone  (u + ow + i in Telex: "uowi")
    "ươi", "ươu", // ơ gets tone
];

/// Returns `true` if `chars[idx]` is an initial-onset medial that must not be
/// treated as part of the vowel nucleus:
///   - 'u' immediately after 'q'  (the "qu" onset cluster)
///   - 'i' immediately after 'g' and followed by another vowel  (the "gi" onset)
fn is_initial_medial(chars: &[char], idx: usize) -> bool {
    const ALL_VOWELS: &str = "aăâeêioôơuưy";
    match strip_tone(chars[idx]) {
        'u' => idx > 0 && chars[idx - 1] == 'q',
        'i' => {
            idx > 0
                && chars[idx - 1] == 'g'
                && idx + 1 < chars.len()
                && ALL_VOWELS.contains(strip_tone(chars[idx + 1]))
        }
        _ => false,
    }
}

/// Return the index within `chars` of the vowel that should receive the tone mark.
///
/// `chars` is the full display buffer and may include consonants as well as
/// vowels.  Returns `None` only when `chars` contains no vowel at all.
pub fn find_tone_position(chars: &[char]) -> Option<usize> {
    const ALL_VOWELS: &str = "aăâeêioôơuưy";

    // Collect (position_in_chars, base_char) for every vowel that is not an
    // initial-onset medial ('u' after 'q', 'i' after 'g' before a vowel).
    let vp: Vec<(usize, char)> = chars
        .iter()
        .enumerate()
        .filter_map(|(i, &c)| {
            let b = strip_tone(c);
            if ALL_VOWELS.contains(b) && !is_initial_medial(chars, i) {
                Some((i, b))
            } else {
                None
            }
        })
        .collect();

    if vp.is_empty() {
        return None;
    }

    // Rule 1 — modified vowel: last vowel with an inherent diacritic form mark.
    if let Some(&(idx, _)) = vp.iter().rev().find(|&&(_, b)| is_form_marked(b)) {
        return Some(idx);
    }

    if vp.len() >= 2 {
        let (first_idx, first_base) = vp[0];
        let (_, last_base) = *vp.last().unwrap();
        let (penult_idx, _) = vp[vp.len() - 2];

        // Rule 2 — ia / ua: exactly two unmodified vowels, second is 'a'.
        if vp.len() == 2 && last_base == 'a' && matches!(first_base, 'i' | 'u') {
            return Some(first_idx);
        }

        // Rule 3 — final glide: last vowel is i, u, y, or o after another vowel.
        if matches!(last_base, 'i' | 'u' | 'y' | 'o') {
            return Some(penult_idx);
        }
    }

    // Rule 4 — default: last vowel.
    Some(vp.last().unwrap().0)
}

/// `true` if `base` carries an inherent form diacritic: ă, â, ê, ô, ơ, or ư.
fn is_form_marked(base: char) -> bool {
    matches!(base, 'ă' | 'â' | 'ê' | 'ô' | 'ơ' | 'ư')
}

/// Strip the tone mark from a Vietnamese vowel, preserving its form diacritic.
///
/// Non-vowel and already-untoned characters are returned unchanged.
///
/// Examples: `'ấ'` → `'â'`, `'ợ'` → `'ơ'`, `'ị'` → `'i'`, `'t'` → `'t'`.
pub fn strip_tone(c: char) -> char {
    match c {
        'à' | 'á' | 'ả' | 'ã' | 'ạ' => 'a',
        'ắ' | 'ằ' | 'ẳ' | 'ẵ' | 'ặ' => 'ă',
        'ấ' | 'ầ' | 'ẩ' | 'ẫ' | 'ậ' => 'â',
        'è' | 'é' | 'ẻ' | 'ẽ' | 'ẹ' => 'e',
        'ế' | 'ề' | 'ể' | 'ễ' | 'ệ' => 'ê',
        'ì' | 'í' | 'ỉ' | 'ĩ' | 'ị' => 'i',
        'ò' | 'ó' | 'ỏ' | 'õ' | 'ọ' => 'o',
        'ố' | 'ồ' | 'ổ' | 'ỗ' | 'ộ' => 'ô',
        'ớ' | 'ờ' | 'ở' | 'ỡ' | 'ợ' => 'ơ',
        'ù' | 'ú' | 'ủ' | 'ũ' | 'ụ' => 'u',
        'ứ' | 'ừ' | 'ử' | 'ữ' | 'ự' => 'ư',
        'ỳ' | 'ý' | 'ỷ' | 'ỹ' | 'ỵ' => 'y',
        other => other,
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn pos(s: &str) -> Option<usize> {
        find_tone_position(&s.chars().collect::<Vec<_>>())
    }

    // ── Rule 1: modified vowel ────────────────────────────────────────────────

    #[test]
    fn single_modified_vowel() {
        assert_eq!(pos("ô"), Some(0));
        assert_eq!(pos("ơ"), Some(0));
        assert_eq!(pos("ư"), Some(0));
        assert_eq!(pos("ă"), Some(0));
        assert_eq!(pos("â"), Some(0));
        assert_eq!(pos("ê"), Some(0));
    }

    #[test]
    fn modified_vowel_in_cluster_toi() {
        // tôi: ô is form-marked → index 1
        assert_eq!(pos("tôi"), Some(1));
    }

    #[test]
    fn modified_vowel_in_cluster_uoi() {
        // uơi: ơ is form-marked (last modified) → index 1
        assert_eq!(pos("uơi"), Some(1));
    }

    #[test]
    fn modified_vowel_in_cluster_uoi_circ() {
        // uôi: ô is form-marked → index 1
        assert_eq!(pos("uôi"), Some(1));
    }

    #[test]
    fn modified_vowel_in_triphthong_ieu() {
        // iêu: ê is form-marked → index 1
        assert_eq!(pos("iêu"), Some(1));
    }

    #[test]
    fn two_form_marked_last_wins() {
        // ươi: both ư (idx 0) and ơ (idx 1) are form-marked; last = ơ at 1
        assert_eq!(pos("ươi"), Some(1));
        // ươ: ư at 0, ơ at 1; last modified = ơ at 1
        assert_eq!(pos("ươ"), Some(1));
    }

    // ── Rule 2: ia / ua ──────────────────────────────────────────────────────

    #[test]
    fn ia_tone_on_first_vowel() {
        assert_eq!(pos("ia"), Some(0)); // 'i' gets tone → "ía"
    }

    #[test]
    fn ua_tone_on_first_vowel() {
        assert_eq!(pos("ua"), Some(0)); // 'u' gets tone → "úa"
    }

    // ── Rule 3: final glide ──────────────────────────────────────────────────

    #[test]
    fn oi_tone_on_o() {
        assert_eq!(pos("oi"), Some(0));
    }

    #[test]
    fn ai_tone_on_a() {
        assert_eq!(pos("ai"), Some(0));
    }

    #[test]
    fn ao_tone_on_a() {
        assert_eq!(pos("ao"), Some(0));
    }

    #[test]
    fn au_tone_on_a() {
        assert_eq!(pos("au"), Some(0));
    }

    #[test]
    fn oai_tone_on_a() {
        // o is medial glide, a is nucleus, i is coda glide → penultimate vowel = a
        assert_eq!(pos("oai"), Some(1));
    }

    // ── Rule 4: default (last vowel) ─────────────────────────────────────────

    #[test]
    fn oa_tone_on_a() {
        // o is medial glide, a is nucleus (not a final glide) → default = a
        assert_eq!(pos("oa"), Some(1));
    }

    #[test]
    fn single_plain_vowels() {
        assert_eq!(pos("a"), Some(0));
        assert_eq!(pos("i"), Some(0));
        assert_eq!(pos("e"), Some(0));
        assert_eq!(pos("y"), Some(0));
    }

    #[test]
    fn consonant_then_vowel() {
        assert_eq!(pos("ta"), Some(1));
    }

    #[test]
    fn toi_plain_tone_on_o_not_i() {
        // 'o' precedes 'i' (glide) → tone goes on 'o' at index 1 in "toi"
        assert_eq!(pos("toi"), Some(1));
    }

    #[test]
    fn no_vowel_returns_none() {
        assert_eq!(pos("bcd"), None);
        assert_eq!(pos("đ"), None);
    }

    // ── strip_tone correctness ────────────────────────────────────────────────

    #[test]
    fn strip_tone_preserves_form_marks() {
        assert_eq!(strip_tone('ấ'), 'â');
        assert_eq!(strip_tone('ợ'), 'ơ');
        assert_eq!(strip_tone('ệ'), 'ê');
        assert_eq!(strip_tone('ộ'), 'ô');
        assert_eq!(strip_tone('ự'), 'ư');
        assert_eq!(strip_tone('ặ'), 'ă');
    }

    #[test]
    fn strip_tone_plain_vowels_unchanged() {
        for c in "aăâeêioôơuưy".chars() {
            assert_eq!(strip_tone(c), c, "strip_tone({c:?}) should be identity");
        }
    }

    #[test]
    fn strip_tone_consonants_unchanged() {
        for c in "bcdfghjklmnpqrstvwxz".chars() {
            assert_eq!(strip_tone(c), c);
        }
    }
}
