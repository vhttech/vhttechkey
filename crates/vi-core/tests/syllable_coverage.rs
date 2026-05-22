#![allow(clippy::unwrap_used)]
//! Coverage for complex Vietnamese syllable tone placement.
//!
//! These tests exercise find_tone_position() paths with multi-char onset
//! clusters (ng-, th-, tr-, qu-, h-) that the 72-syllable golden tests miss.
//! Each test asserts the preedit mid-composition (after vowel formation, before
//! Return) and the committed string after KeyDown(Return).

use vi_core::{
    CompositionEngine, InputEvent, InputMethod, Key, Modifiers, StandardEngine, StateTransition,
};

fn telex() -> StandardEngine {
    StandardEngine::new(InputMethod::Telex)
}

fn key(c: char) -> InputEvent {
    InputEvent::KeyDown(Key::Char(c), Modifiers::none())
}

fn ret() -> InputEvent {
    InputEvent::KeyDown(Key::Return, Modifiers::none())
}

fn process_str(e: &mut StandardEngine, s: &str) {
    for ch in s.chars() {
        e.process(&key(ch)).unwrap();
        let _ = e.process(&InputEvent::KeyUp(Key::Char(ch)));
    }
}

fn commit(t: StateTransition) -> String {
    match t {
        StateTransition::CommitAndClear(c) => c.as_str().to_owned(),
        StateTransition::Commit(c) => c.as_str().to_owned(),
        StateTransition::CommitThenPreedit(c, _) => c.as_str().to_owned(),
        other => panic!("unexpected transition: {other:?}"),
    }
}

/// uy cluster — final-glide rule: tone lands on y (ề), not the leading u.
/// Telex: nguyeen → nguyên, then f (huyền) → nguyền.
#[test]
fn nguyeenf_nguyen_huyen() {
    let mut e = telex();
    process_str(&mut e, "nguyeen");
    assert_eq!(e.preedit().as_str(), "nguyên");
    e.process(&key('f')).unwrap();
    assert_eq!(e.preedit().as_str(), "nguyền");
    assert_eq!(commit(e.process(&ret()).unwrap()), "nguyền");
}

/// Modified-vowel ươ — two Telex styles (same result):
/// - One `w` after medial `uo`: `thuowng` (Case B: consonant + u + o + w → ươ).
/// - Two `w` (uw then ow): `thuwowng` — matches habits from some IMEs.
/// Tone on ơ → ợ (j = nặng).
#[test]
fn thuowngj_thuong_nang_one_w_after_uo() {
    let mut e = telex();
    process_str(&mut e, "thuowng");
    assert_eq!(e.preedit().as_str(), "thương");
    e.process(&key('j')).unwrap();
    assert_eq!(e.preedit().as_str(), "thượng");
    assert_eq!(commit(e.process(&ret()).unwrap()), "thượng");
}

#[test]
fn thuwowngj_thuong_nang() {
    let mut e = telex();
    process_str(&mut e, "thuwowng");
    assert_eq!(e.preedit().as_str(), "thương");
    e.process(&key('j')).unwrap();
    assert_eq!(e.preedit().as_str(), "thượng");
    assert_eq!(commit(e.process(&ret()).unwrap()), "thượng");
}

/// Modified-vowel ươ with tr- onset, tone lands on ơ → ờ.
/// Telex: truwowng → trương, then f (huyền) → trường.
#[test]
fn truwowngf_truong_huyen() {
    let mut e = telex();
    process_str(&mut e, "truwowng");
    assert_eq!(e.preedit().as_str(), "trương");
    e.process(&key('f')).unwrap();
    assert_eq!(e.preedit().as_str(), "trường");
    assert_eq!(commit(e.process(&ret()).unwrap()), "trường");
}

/// Modified-vowel ô (oo) inside qu- syllable, tone lands on ô → ộ.
/// Telex: quooc → quôc, then j (nặng) → quộc.
#[test]
fn quoocj_quoc_nang() {
    let mut e = telex();
    process_str(&mut e, "quooc");
    assert_eq!(e.preedit().as_str(), "quôc");
    e.process(&key('j')).unwrap();
    assert_eq!(e.preedit().as_str(), "quộc");
    assert_eq!(commit(e.process(&ret()).unwrap()), "quộc");
}

/// oa cluster — tone lands on a (the nucleus), not on o (the glide).
/// Telex: hoang → hoang, then f (huyền) → hoàng.
#[test]
fn hoangf_hoang_huyen() {
    let mut e = telex();
    process_str(&mut e, "hoang");
    assert_eq!(e.preedit().as_str(), "hoang");
    e.process(&key('f')).unwrap();
    assert_eq!(e.preedit().as_str(), "hoàng");
    assert_eq!(commit(e.process(&ret()).unwrap()), "hoàng");
}

/// Leading-vowel cluster uyê, tone lands on ê → ể.
/// Telex: uyeen → uyên, then r (hỏi) → uyển.
#[test]
fn uyeenr_uyen_hoi() {
    let mut e = telex();
    process_str(&mut e, "uyeen");
    assert_eq!(e.preedit().as_str(), "uyên");
    e.process(&key('r')).unwrap();
    assert_eq!(e.preedit().as_str(), "uyển");
    assert_eq!(commit(e.process(&ret()).unwrap()), "uyển");
}

// ── VALID_NUCLEI Step-1 spot-checks ──────────────────────────────────────────

/// uê nucleus (medial u + ê): tone lands on ê (Rule 1 — ê is form-marked).
/// Telex: uees → uế
#[test]
fn uees_ue_sac() {
    let mut e = telex();
    process_str(&mut e, "uees");
    assert_eq!(commit(e.process(&ret()).unwrap()), "uế");
}

/// uy nucleus (u + y final-glide): tone lands on u (Rule 3 — y is final glide,
/// penultimate vowel = u).
/// Telex: uys → úy
#[test]
fn uys_uy_sac() {
    let mut e = telex();
    process_str(&mut e, "uys");
    assert_eq!(commit(e.process(&ret()).unwrap()), "úy");
}

// ── oa-family: EstdToneStyle places tone on 'o' (first vowel) ─────────────────
// Note: for "oa"/"hoa"/"toa" without a consonant coda, vhttechkey follows the
// EstdToneStyle rule that puts the tone mark on the first vowel 'o' rather than
// on the nucleus 'a'.  This matches the vhttechkey reference behaviour.

/// hoas → "hóa"  (sắc on o)
#[test]
fn hoas_hoa_sac() {
    let mut e = telex();
    process_str(&mut e, "hoas");
    assert_eq!(commit(e.process(&ret()).unwrap()), "hóa");
}

/// hoaj → "họa"  (nặng on o)
#[test]
fn hoaj_hoa_nang() {
    let mut e = telex();
    process_str(&mut e, "hoaj");
    assert_eq!(commit(e.process(&ret()).unwrap()), "họa");
}

/// hoar → "hỏa"  (hỏi on o)
#[test]
fn hoar_hoa_hoi() {
    let mut e = telex();
    process_str(&mut e, "hoar");
    assert_eq!(commit(e.process(&ret()).unwrap()), "hỏa");
}

// ── oe-family: EstdToneStyle places tone on 'o' ─────────────────────────────────

/// xoes → "xóe"  (sắc on o)
#[test]
fn xoes_xoe_sac() {
    let mut e = telex();
    process_str(&mut e, "xoes");
    assert_eq!(commit(e.process(&ret()).unwrap()), "xóe");
}

// ── Medial-onset 'toa': EstdToneStyle places tone on 'o' ────────────────────────

/// toas → "tóa"  (sắc on o)
#[test]
fn toas_toa_sac() {
    let mut e = telex();
    process_str(&mut e, "toas");
    assert_eq!(commit(e.process(&ret()).unwrap()), "tóa");
}

/// toaf → "tòa"  (huyền on o)
#[test]
fn toaf_toa_huyen() {
    let mut e = telex();
    process_str(&mut e, "toaf");
    assert_eq!(commit(e.process(&ret()).unwrap()), "tòa");
}

/// toar → "tỏa"  (hỏi on o)
#[test]
fn toar_toa_hoi() {
    let mut e = telex();
    process_str(&mut e, "toar");
    assert_eq!(commit(e.process(&ret()).unwrap()), "tỏa");
}

/// toax → "tõa"  (ngã on o)
#[test]
fn toax_toa_nga() {
    let mut e = telex();
    process_str(&mut e, "toax");
    assert_eq!(commit(e.process(&ret()).unwrap()), "tõa");
}

/// toaj → "tọa"  (nặng on o)
#[test]
fn toaj_toa_nang() {
    let mut e = telex();
    process_str(&mut e, "toaj");
    assert_eq!(commit(e.process(&ret()).unwrap()), "tọa");
}
