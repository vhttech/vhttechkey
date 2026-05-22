#![allow(clippy::unwrap_used)]
//! Regression tests for flexible tone placement.
//!
//! Vietnamese IME users often type tone marks at different positions within a
//! syllable — before a vowel-form key, after a doubled vowel, or before a
//! consonant coda.  All of these must produce the correct output; the tone mark
//! must migrate to the phonologically correct vowel nucleus regardless of when
//! it is typed.
//!
//! Three placement patterns tested here:
//!   A) Double-vowel-then-tone: type the doubled vowel key first (oo→ô), then
//!      the tone key (s=sắc → ố).
//!   B) Tone-before-vowel-cluster: type tone after the first vowel, then type
//!      the key that triggers the vowel form change (e.g. a+s+w → a→á→ắ).
//!   C) Tone-before-consonant-coda: type vowel+tone, then add a consonant coda
//!      (e.g. a+s+n → "án"); or add the coda first then the tone (a+n+s → "án").

use vi_core::{CompositionEngine, InputEvent, InputMethod, Key, Modifiers, StandardEngine, StateTransition};

fn telex() -> StandardEngine {
    StandardEngine::new(InputMethod::Telex)
}

fn key(c: char) -> InputEvent {
    InputEvent::KeyDown(Key::Char(c), Modifiers::none())
}

fn ret() -> InputEvent {
    InputEvent::KeyDown(Key::Return, Modifiers::none())
}

fn type_and_commit(keys: &str) -> String {
    let mut e = telex();
    for ch in keys.chars() {
        e.process(&key(ch)).unwrap();
        let _ = e.process(&InputEvent::KeyUp(Key::Char(ch)));
    }
    match e.process(&ret()).unwrap() {
        StateTransition::CommitAndClear(c) => c.as_str().to_owned(),
        StateTransition::PassThrough => e.preedit().as_str().to_owned(),
        other => panic!("unexpected transition for {:?}: {other:?}", keys),
    }
}

// ── A: Double-vowel-then-tone ────────────────────────────────────────────────
//
// The vowel-form rule fires on the doubled key (oo→ô, ee→ê, aa→â).
// The tone key is typed AFTER the vowel form is already set.

#[test]
fn double_vowel_oo_then_sac()   { assert_eq!(type_and_commit("oos"), "ố"); }
#[test]
fn double_vowel_oo_then_huyen() { assert_eq!(type_and_commit("oof"), "ồ"); }
#[test]
fn double_vowel_oo_then_hoi()   { assert_eq!(type_and_commit("oor"), "ổ"); }
#[test]
fn double_vowel_oo_then_nga()   { assert_eq!(type_and_commit("oox"), "ỗ"); }
#[test]
fn double_vowel_oo_then_nang()  { assert_eq!(type_and_commit("ooj"), "ộ"); }

#[test]
fn double_vowel_ee_then_sac()   { assert_eq!(type_and_commit("ees"), "ế"); }
#[test]
fn double_vowel_ee_then_huyen() { assert_eq!(type_and_commit("eef"), "ề"); }

#[test]
fn double_vowel_aa_then_sac()   { assert_eq!(type_and_commit("aas"), "ấ"); }
#[test]
fn double_vowel_aa_then_huyen() { assert_eq!(type_and_commit("aaf"), "ầ"); }

/// uw→ư is a vowel-form doubling (u+w); tone applied after.
#[test]
fn double_vowel_uw_then_sac()   { assert_eq!(type_and_commit("uws"), "ứ"); }

// ── B: Tone-before-vowel-cluster ─────────────────────────────────────────────
//
// Tone is typed BEFORE the second key that completes the vowel form.
// The 'w' key triggers the form change (a→ă, o→ơ, u→ư); the tone mark that
// was placed on the pre-form vowel must migrate to the post-form vowel.

#[test]
fn tone_before_aw_sac()   { assert_eq!(type_and_commit("asw"), "ắ"); }
#[test]
fn tone_before_aw_huyen() { assert_eq!(type_and_commit("afw"), "ằ"); }
#[test]
fn tone_before_aw_hoi()   { assert_eq!(type_and_commit("arw"), "ẳ"); }
#[test]
fn tone_before_aw_nga()   { assert_eq!(type_and_commit("axw"), "ẵ"); }
#[test]
fn tone_before_aw_nang()  { assert_eq!(type_and_commit("ajw"), "ặ"); }

#[test]
fn tone_before_ow_sac()   { assert_eq!(type_and_commit("osw"), "ớ"); }
#[test]
fn tone_before_ow_huyen() { assert_eq!(type_and_commit("ofw"), "ờ"); }

#[test]
fn tone_before_uw_sac()   { assert_eq!(type_and_commit("usw"), "ứ"); }
#[test]
fn tone_before_uw_huyen() { assert_eq!(type_and_commit("ufw"), "ừ"); }

/// Second 'e' transforms e→ê; tone typed between the two 'e' presses.
#[test]
fn tone_before_ee_cluster_sac()   { assert_eq!(type_and_commit("ese"), "ế"); }
#[test]
fn tone_before_ee_cluster_huyen() { assert_eq!(type_and_commit("efe"), "ề"); }

// ── C: Tone-before-consonant-coda ────────────────────────────────────────────
//
// After placing the tone on the nucleus, a consonant coda is appended.
// The tone mark must remain on the vowel even though more characters follow.

#[test]
fn tone_before_coda_a_sac_n() {
    // a→á (sắc), then 'n' appended as coda → "án"
    assert_eq!(type_and_commit("asn"), "án");
}

#[test]
fn tone_before_coda_a_sac_ng() {
    // a→á (sắc), then 'n'+'g' appended as coda → "áng"
    assert_eq!(type_and_commit("asng"), "áng");
}

#[test]
fn tone_before_coda_oo_sac_m() {
    // oo→ô, s=sắc → "ố", then 'm' appended as coda → "ốm"
    assert_eq!(type_and_commit("oosm"), "ốm");
}

// ── D: Coda-then-tone ("post-coda tone") ─────────────────────────────────────
//
// Consonant coda typed FIRST, then the tone key is applied.
// The tone mark must reach the vowel nucleus even though a consonant separates
// the tone key from the vowel in the key sequence.

#[test]
fn coda_first_then_tone_an_s() {
    // a + n (coda) + s (tone) → tone lands on 'a' (the only vowel) → "án"
    assert_eq!(type_and_commit("ans"), "án");
}

#[test]
fn coda_first_then_tone_oon_s() {
    // oo→ô + n (coda) + s (tone) → tone lands on 'ô' → "ốn"
    assert_eq!(type_and_commit("oons"), "ốn");
}

#[test]
fn coda_first_then_tone_oom_s() {
    // oo→ô + m (coda) + s (tone) → tone lands on 'ô' → "ốm"
    assert_eq!(type_and_commit("ooms"), "ốm");
}

// ── E: Tone switch then vowel form ────────────────────────────────────────────
//
// The user types one tone key, then switches to another, then applies a vowel
// form change.  The final vowel form must carry the last tone.

#[test]
fn tone_switch_then_vowel_form_asfw() {
    // a→á (s) → à (f switches sắc to huyền) → ằ (w applies ă-form, preserving huyền)
    assert_eq!(type_and_commit("asfw"), "ằ");
}

// ── G: Retroactive vowel assignment ("nữa" family) ───────────────────────────
//
// When direct application of a vowel-form key creates an invalid Vietnamese
// nucleus (e.g. "uă"), the engine must retroactively apply the key to an
// earlier vowel that yields a valid nucleus (e.g. "ưa").

#[test]
fn nuawx_gives_nua_nga() {
    assert_eq!(type_and_commit("nuawx"), "nữa");
}
#[test]
fn nuaw_gives_nua_level() {
    assert_eq!(type_and_commit("nuaw"), "nưa");
}
#[test]
fn buawx_gives_bua_nga() {
    assert_eq!(type_and_commit("buawx"), "bữa");
}
#[test]
fn luawf_gives_lua_huyen() {
    assert_eq!(type_and_commit("luawf"), "lừa");
}
#[test]
fn luaw_gives_lua_level() {
    assert_eq!(type_and_commit("luaw"), "lưa");
}
// "mưa" (rain): muaw → "mưa"
#[test]
fn muaw_gives_mua_level() {
    assert_eq!(type_and_commit("muaw"), "mưa");
}
// nuaxw → nữa (tone typed on 'u' first, then 'w' transforms u→ư, carrying tone)
#[test]
fn scan_back_nuaxw_gives_nuwa_nga() {
    assert_eq!(type_and_commit("nuaxw"), "nữa");
}
// nuawf → nừa (huyền)
#[test]
fn scan_back_nuawf_gives_nuwa_huyen() {
    assert_eq!(type_and_commit("nuawf"), "nừa");
}
// Confirm no regression: isolated 'aw' still gives 'ă'
#[test]
fn aw_still_gives_a_breve() {
    assert_eq!(type_and_commit("aw"), "ă");
}

// ── H: Multi-consonant prefix retroactive scan ───────────────────────────────
//
// The ua+w scan-back must work even when the consonant onset is multi-char
// (th, ch, etc.), since the penultimate char is still 'u' regardless of what
// precedes it.

#[test]
fn thuaw_gives_thua_level() { assert_eq!(type_and_commit("thuaw"), "thưa"); }
#[test]
fn chuawx_gives_chua_nga() { assert_eq!(type_and_commit("chuawx"), "chữa"); }
#[test]
fn thuaws_gives_thua_sac() { assert_eq!(type_and_commit("thuaws"), "thứa"); }

// nuaxw pattern (tone on u BEFORE w transform)
#[test]
fn nuaxw_nga_before_w() { assert_eq!(type_and_commit("nuaxw"), "nữa"); }

// nướng full word via uow path (u + ow → ươ with consonant onset)
#[test]
fn nuowsng_gives_nuong_sac() { assert_eq!(type_and_commit("nuowsng"), "nướng"); }

// được (dduwowcj) — same encoding as the existing Telex unit test
#[test]
fn duoc_nang() { assert_eq!(type_and_commit("dduwowcj"), "được"); }

// Common words
#[test]
fn mua_level() { assert_eq!(type_and_commit("muaw"), "mưa"); }
#[test]
fn bua_nga() { assert_eq!(type_and_commit("buawx"), "bữa"); }
#[test]
fn cua_level() { assert_eq!(type_and_commit("cuaw"), "cưa"); }

