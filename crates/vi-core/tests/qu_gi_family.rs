#![allow(clippy::unwrap_used)]
//! Regression tests for qu-family and gi-family syllables.
//!
//! Covers the bug where the scan-back and retroactive scan incorrectly
//! treated 'u' after 'q' (the "qu" onset) and 'i' after 'g' before a vowel
//! (the "gi" onset) as vowel nucleus candidates.

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

// ── qu-family: 'u' after 'q' must not be treated as a vowel nucleus ──────────

#[test]
fn quaw_gives_qua_breve() {
    // Before fix: scan-back turned 'u' into 'ư' → "qưa"; correct: "quă"
    assert_eq!(type_and_commit("quaw"), "quă");
}

#[test]
fn quawj_gives_qua_breve_nang() {
    assert_eq!(type_and_commit("quawj"), "quặ");
}

#[test]
fn quawngj_gives_quang_nang() {
    // Longer word: quaw → "quă", then ng, then j(nặng) on 'ă'
    assert_eq!(type_and_commit("quawngj"), "quặng");
}

#[test]
fn quoos_gives_quo_circ_sac() {
    // quoo → quô (oo rule), s → sắc on ô → "quố"
    assert_eq!(type_and_commit("quoos"), "quố");
}

#[test]
fn quaif_gives_quai_huyen() {
    // tone on 'a' (penult of final glide 'i'), not on medial 'u'
    assert_eq!(type_and_commit("quaif"), "quài");
}

#[test]
fn quays_gives_quay_sac() {
    // tone on 'a' (penult of final glide 'y'), not on medial 'u'
    assert_eq!(type_and_commit("quays"), "quáy");
}

// ── Non-regression: 'u' NOT after 'q' must still trigger the scan-back ───────

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

// ── gi-family: 'i' after 'g' before a vowel must not be nucleus ──────────────

#[test]
fn giar_gives_gia_hoi() {
    // Before fix: tone would land on 'i' via Rule 2 (ia → tone on first); correct: 'a'
    assert_eq!(type_and_commit("giar"), "giả");
}

#[test]
fn giasn_gives_gian_sac() {
    // s=sắc on 'a' (not 'i'), then 'n' appended without spurious nucleus close
    assert_eq!(type_and_commit("giasn"), "gián");
}

#[test]
fn giarn_gives_gian_hoi() {
    // r=hỏi on 'a', then 'n' appended without spurious nucleus close → "giản"
    assert_eq!(type_and_commit("giarn"), "giản");
}

#[test]
fn giaof_gives_giao_huyen() {
    // tone on 'a' (penult of final glide 'o') → "giào"
    assert_eq!(type_and_commit("giaof"), "giào");
}

#[test]
fn giaf_gives_gia_huyen() {
    assert_eq!(type_and_commit("giaf"), "già");
}
