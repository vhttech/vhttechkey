//! Production-style **composition gating**: decide when *not* to run Telex/VNI
//! transforms on the preedit buffer.
//!
//! Design goals (no artificial latency):
//! - **Default-on**: keep normal Vietnamese typing (`viet`, `tooi`, `dduwowcj`, …).
//! - **Block** tone keys and vowel-form triggers when the buffer looks like
//!   **ASCII spam** (strict `a,b,a,b,…` alternation **where `a` or `b` is a Telex
//!   tone letter** `s`/`f`/`r`/`x`/`j`/`z` — e.g. `xox` / `xoxoxo`; long same-letter
//!   vowel runs; repeating short suffix patterns) or **code-ish** fragments — until
//!   the user has produced at least one real Vietnamese letter (đ / precomposed
//!   tones / U+1EA0–U+1EF9, etc.).
//!
//! This mirrors mature IME behaviour: *know when not to convert*, not *convert
//! as much as possible*.
//!
//! **Trade-off:** pure `abab` alternation without a Telex tone letter in the pair is **not**
//! blocked (e.g. `totos` → `tốt`); that is preferable to blocking legitimate CV-CV typing.
//!
//! Limitations (documented, acceptable without delay):
//! - Short English words whose prefixes look Vietnamese (e.g. `test` + tone key)
//!   may still tone-map; fixing that needs a small English blocklist or dict,
//!   not a timer.

use crate::types::Modifiers;

/// True if `c` already carries Vietnamese composition (tone, diacritic, đ).
#[inline]
pub fn is_vietnamese_composed_char(c: char) -> bool {
    let n = c as u32;
    (0x0300..=0x036F).contains(&n)
        || (0x1EA0..=0x1EF9).contains(&n)
        || matches!(
            c,
            'à' | 'á'
                | 'â'
                | 'ã'
                | 'è'
                | 'é'
                | 'ê'
                | 'ì'
                | 'í'
                | 'ò'
                | 'ó'
                | 'ô'
                | 'õ'
                | 'ù'
                | 'ú'
                | 'ý'
                | 'À'
                | 'Á'
                | 'Â'
                | 'Ã'
                | 'È'
                | 'É'
                | 'Ê'
                | 'Ì'
                | 'Í'
                | 'Ò'
                | 'Ó'
                | 'Ô'
                | 'Õ'
                | 'Ù'
                | 'Ú'
                | 'Ý'
                | 'ă'
                | 'Ă'
                | 'ơ'
                | 'Ơ'
                | 'ư'
                | 'Ư'
                | 'đ'
                | 'Đ'
        )
}

fn buffer_has_vietnamese_unlock(display: &[char]) -> bool {
    display.iter().copied().any(is_vietnamese_composed_char)
}

/// Last whitespace-delimited segment (falls back to full buffer).
fn last_token(display: &[char]) -> &[char] {
    display
        .iter()
        .rposition(|&c| c.is_whitespace())
        .map(|i| &display[i + 1..])
        .unwrap_or(display)
}

fn is_ascii_letters_only(chars: &[char]) -> bool {
    !chars.is_empty() && chars.iter().all(|c| c.is_ascii_alphabetic())
}

/// Path separators, Rust/URL-ish operators — strong *not Vietnamese prose* signal.
fn looks_code_like(chars: &[char]) -> bool {
    let s: String = chars.iter().collect();
    s.contains("::")
        || s.contains("->")
        || s.contains("=>")
        || s.contains("//")
        || s.contains("/*")
        || s.contains("*/")
        || s.contains("${")
        || s.contains('`')
        || s.contains('%')
        || s.contains('\\')
        || (s.contains('/') && chars.len() >= 3)
        || (s.contains('_') && chars.iter().any(|c| c.is_ascii_digit()))
}

fn is_two_char_strict_alternation(chars: &[char]) -> bool {
    if chars.len() < 2 {
        return true;
    }
    let a = chars[0];
    let b = chars[1];
    if a == b {
        return false;
    }
    chars
        .iter()
        .enumerate()
        .all(|(i, &c)| c == if i % 2 == 0 { a } else { b })
}

/// True if either alternating letter is a Telex tone trigger (`s`/`f`/`r`/`x`/`j`/`z`).
/// This keeps `xox` / `xoxoxo` blocked (retro `oo` / tone mis-fires) while allowing
/// natural CV-CV prefixes like `toto` → `totos` → `tốt`.
fn alternation_includes_telex_tone_letter(a: char, b: char) -> bool {
    const TONE: &str = "sfrxjzSFRXJZ";
    TONE.contains(a) || TONE.contains(b)
}

/// Detects `xox` / `xoxoxo` style input: strict two-letter ASCII alternation **and**
/// at least one of the two letters is a Telex tone key (so `toto` / `coco` are not
/// treated as spam — length ≥ 3 is enough for Telex to mis-fire via **retroactive**
/// `oo`/`aa`/… on a vowel that is not the last display character (e.g. `xox` + `o` → `xôx`).
fn is_ascii_alternation_spam_token(token: &[char]) -> bool {
    if token.len() < 3 || !is_ascii_letters_only(token) || !is_two_char_strict_alternation(token) {
        return false;
    }
    let a = token[0];
    let b = token[1];
    alternation_includes_telex_tone_letter(a, b)
}

/// If appending `tone_key` would continue a strict `ababab` ASCII alternation of
/// length ≥ 3, suppress Telex tone (e.g. `xo` + `x` → `xox` literal, not `xõ`).
fn would_tone_break_ascii_alternation_spam(display: &[char], tone_key: char) -> bool {
    if buffer_has_vietnamese_unlock(display) || !telex_is_tone_key(tone_key) {
        return false;
    }
    let mut v = display.to_vec();
    v.push(tone_key);
    if v.len() < 3 || !is_ascii_letters_only(&v) || !is_two_char_strict_alternation(&v) {
        return false;
    }
    let a = v[0];
    let b = v[1];
    alternation_includes_telex_tone_letter(a, b)
}

/// Same-letter run length ≥ `min` anywhere in the token.
fn has_long_identical_run(chars: &[char], min: usize) -> bool {
    if chars.len() < min {
        return false;
    }
    const VOWELS: &str = "aeiouyAEIOUY";
    let mut run = 1usize;
    let mut prev = chars[0];
    for &c in &chars[1..] {
        if c == prev {
            if VOWELS.contains(prev) {
                run += 1;
                if run >= min {
                    return true;
                }
            }
        } else {
            prev = c;
            run = 1;
        }
    }
    false
}

/// Detects `xoxoxoxo`-style spam: a short pattern (period 2..=6) repeated many times
/// at the end of an ASCII-only token.
fn has_repeating_short_pattern_suffix(chars: &[char], min_repeats: usize) -> bool {
    let n = chars.len();
    if n < 8 {
        return false;
    }
    let tail_len = n.min(48);
    let start = n - tail_len;
    let tail = &chars[start..];

    for period in 2..=6usize {
        if tail.len() < period * min_repeats {
            continue;
        }
        let pat = &tail[tail.len() - period * min_repeats..];
        let chunk = &pat[0..period];
        // Ignore patterns like `nnnn` (single-letter padding); real spam uses ≥2 letters.
        if chunk.iter().min() == chunk.iter().max() {
            continue;
        }
        let mut ok = true;
        for r in 1..min_repeats {
            let lo = r * period;
            if pat[lo..lo + period] != *chunk {
                ok = false;
                break;
            }
        }
        if ok {
            return true;
        }
    }
    false
}

/// ASCII-only token that looks like random/spam typing, not Vietnamese prose.
fn ascii_token_looks_like_noise(token: &[char]) -> bool {
    if token.is_empty() {
        return false;
    }
    // `foo_bar1`, `snake_0`, … — identifier-ish, not Vietnamese prose.
    if token.iter().all(|c| c.is_ascii_alphanumeric() || *c == '_') && looks_code_like(token) {
        return true;
    }
    if !is_ascii_letters_only(token) {
        return false;
    }
    if looks_code_like(token) {
        return true;
    }
    if has_long_identical_run(token, 5) {
        return true;
    }
    if has_repeating_short_pattern_suffix(token, 4) {
        return true;
    }
    is_ascii_alternation_spam_token(token)
}

fn should_block_vietnamese_rules(display: &[char]) -> bool {
    if buffer_has_vietnamese_unlock(display) {
        return false;
    }
    let token = last_token(display);
    ascii_token_looks_like_noise(token)
}

fn telex_is_tone_key(ch: char) -> bool {
    matches!(ch, 's' | 'f' | 'r' | 'x' | 'j' | 'z')
}

/// Whether Telex `apply` should run for this keystroke.
pub fn telex_apply_allowed(display: &[char], new_key: char, modifiers: Modifiers) -> bool {
    if modifiers.altgr {
        return false;
    }
    if would_tone_break_ascii_alternation_spam(display, new_key) {
        return false;
    }
    !should_block_vietnamese_rules(display)
}

fn vni_is_special_digit(ch: char) -> bool {
    ch.is_ascii_digit()
}

/// Whether VNI `apply` should run for this keystroke.
pub fn vni_apply_allowed(display: &[char], new_key: char, modifiers: Modifiers) -> bool {
    let _ = modifiers;
    !(should_block_vietnamese_rules(display) && vni_is_special_digit(new_key))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tok(s: &str) -> Vec<char> {
        s.chars().collect()
    }

    #[test]
    fn xo_then_alternating_tone_x_blocked() {
        assert!(!telex_apply_allowed(&tok("xo"), 'x', Modifiers::none()));
    }

    #[test]
    fn xox_blocks_retroactive_oo() {
        assert!(ascii_token_looks_like_noise(&tok("xox")));
        assert!(!telex_apply_allowed(&tok("xox"), 'o', Modifiers::none()));
    }

    #[test]
    fn toto_is_not_alternation_spam_allows_totos() {
        assert!(!ascii_token_looks_like_noise(&tok("toto")));
        assert!(telex_apply_allowed(&tok("toto"), 's', Modifiers::none()));
    }

    #[test]
    fn xoxoxoxo_is_noise_ascii() {
        let t = tok("xoxoxoxo");
        assert!(ascii_token_looks_like_noise(&t));
        assert!(!telex_apply_allowed(&t, 's', Modifiers::none()));
    }

    #[test]
    fn viet_is_not_noise() {
        let t = tok("viet");
        assert!(!ascii_token_looks_like_noise(&t));
        assert!(telex_apply_allowed(&t, 's', Modifiers::none()));
    }

    #[test]
    fn unlock_after_vietnamese_char() {
        let t = tok("xoxoxoá");
        assert!(buffer_has_vietnamese_unlock(&t));
        assert!(telex_apply_allowed(&t, 's', Modifiers::none()));
    }

    #[test]
    fn code_like_underscore_digit() {
        let t = tok("foo_bar1");
        assert!(ascii_token_looks_like_noise(&t));
    }

    #[test]
    fn vni_blocks_digits_in_noise() {
        let t = tok("xoxoxoxo");
        assert!(!vni_apply_allowed(&t, '1', Modifiers::none()));
    }
}
