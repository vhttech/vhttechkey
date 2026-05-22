#![allow(clippy::unwrap_used)]
//! Comprehensive real-word regression tests for Telex input → Vietnamese output.
//!
//! Covers retroactive scan-back ("ưa" family), multi-consonant onset clusters,
//! flexible tone ordering (tone before form, tone before coda, tone
//! switch then form, coda then tone, triphthong), and backspace rollback in
//! complex sequences.

use vi_core::{
    CompositionEngine, InputEvent, InputMethod, Key, Modifiers, StandardEngine, StateTransition,
};

fn telex() -> StandardEngine {
    StandardEngine::new(InputMethod::Telex)
}

fn key(c: char) -> InputEvent {
    InputEvent::KeyDown(Key::Char(c), Modifiers::none())
}

fn bs() -> InputEvent {
    InputEvent::KeyDown(Key::Backspace, Modifiers::none())
}

fn ret() -> InputEvent {
    InputEvent::KeyDown(Key::Return, Modifiers::none())
}

fn preedit(e: &StandardEngine) -> String {
    e.preedit().as_str().to_owned()
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

// ── ưa-family: retroactive u→ư scan-back ─────────────────────────────────────

#[test]
fn nuawx_nua_nga() {
    assert_eq!(type_and_commit("nuawx"), "nữa");
}

#[test]
fn buawx_bua_nga() {
    assert_eq!(type_and_commit("buawx"), "bữa");
}

#[test]
fn luawf_lua_huyen() {
    assert_eq!(type_and_commit("luawf"), "lừa");
}

#[test]
fn muaw_mua_level() {
    assert_eq!(type_and_commit("muaw"), "mưa");
}

// ── Multi-consonant real-word sequences ───────────────────────────────────────

// nướng: uow path (u+ow→ơ) with sắc before coda ng
#[test]
fn nuowsng_nuong_sac() {
    assert_eq!(type_and_commit("nuowsng"), "nướng");
}

// người: ng-onset, u(medial)+ow→ơ retroactively promoting u→ư, i-glide, f=huyền
#[test]
fn nguowif_nguoi_huyen() {
    assert_eq!(type_and_commit("nguowif"), "người");
}

// được: dd→đ, uw→ư, ow→ơ, c, j=nặng on ơ→ợ
#[test]
fn dduwowcj_duoc_nang() {
    assert_eq!(type_and_commit("dduwowcj"), "được");
}

// trường: tr-onset, u(medial)+ow→ơ (u retroactively→ư), ng-coda, f=huyền
#[test]
fn truowngf_truong_huyen() {
    assert_eq!(type_and_commit("truowngf"), "trường");
}

// hoặng: h-onset, o(medial), aw→ă nucleus, ng-coda, j=nặng — exercises 'oă' nucleus
#[test]
fn hoawngj_hoang_nang() {
    assert_eq!(type_and_commit("hoawngj"), "hoặng");
}

// ── Common vocabulary ──────────────────────────────────────────────────────────

// toán: t-onset, oa-diphthong (o=medial, a=nucleus), s=sắc on a, n-coda
#[test]
fn toasn_toan_sac() {
    assert_eq!(type_and_commit("toasn"), "toán");
}

// việt: vi-onset, ee→ê, j=nặng on ê, t-coda
#[test]
fn vieejt_viet_nang() {
    assert_eq!(type_and_commit("vieejt"), "việt");
}

// tôi: t-onset, oo→ô, i-glide coda
#[test]
fn tooi_toi_level() {
    assert_eq!(type_and_commit("tooi"), "tôi");
}

// muốn: m-onset, u(medial), oo→ô, s=sắc on ô, n-coda
#[test]
fn muoosn_muon_sac() {
    assert_eq!(type_and_commit("muoosn"), "muốn");
}

// hòa: h-onset, oa-diphthong, f=huyền; EstdToneStyle places tone on 'o'
#[test]
fn hoaf_hoa_huyen() {
    assert_eq!(type_and_commit("hoaf"), "hòa");
}

// hoài: h-onset, oa-diphthong, i-glide coda, f=huyền on a
#[test]
fn hoaif_hoai_huyen() {
    assert_eq!(type_and_commit("hoaif"), "hoài");
}

// chiều: ch-onset, i+ee→ê triphthong iêu, u-glide coda, f=huyền on ê
#[test]
fn chieeuf_chieu_huyen() {
    assert_eq!(type_and_commit("chieeuf"), "chiều");
}

// ── qu-family ─────────────────────────────────────────────────────────────────

// quặng: qu-onset, aw→ă nucleus, ng-coda, j=nặng
#[test]
fn quawngj_quang_nang() {
    assert_eq!(type_and_commit("quawngj"), "quặng");
}

// quộ: qu-onset, oo→ô, s=sắc, j switches sắc→nặng → "quộ"
#[test]
fn quoosj_quo_nang() {
    assert_eq!(type_and_commit("quoosj"), "quộ");
}

// ── gi-family ─────────────────────────────────────────────────────────────────

// gián: gi-onset, a-nucleus, s=sắc (acute) on a, n-coda → "gián"
#[test]
fn giasn_gian_sac() {
    assert_eq!(type_and_commit("giasn"), "gián");
}

// giào: gi-onset, a-nucleus, o-glide coda, f=huyền on a
#[test]
fn giaof_giao_huyen() {
    assert_eq!(type_and_commit("giaof"), "giào");
}

// ── Flexible: tone before vowel form ───────────────────────────────────────────

// oosf: oo→ô, s=sắc→ố, f switches tone sắc→huyền → "ồ"
#[test]
fn oosf_o_circ_huyen() {
    assert_eq!(type_and_commit("oosf"), "ồ");
}

// asw: a, s=sắc→á, w applies ă-form carrying the sắc → "ắ"
#[test]
fn asw_a_breve_sac() {
    assert_eq!(type_and_commit("asw"), "ắ");
}

// usw: u, s=sắc→ú, w applies ư-form carrying the sắc → "ứ"
#[test]
fn usw_u_horn_sac() {
    assert_eq!(type_and_commit("usw"), "ứ");
}

// ── Flexible: tone before coda ─────────────────────────────────────────────────

// asn: a, s=sắc→á, n appended as coda → "án"
#[test]
fn asn_a_sac_n() {
    assert_eq!(type_and_commit("asn"), "án");
}

// oosi: oo→ô, s=sắc→ố, i appended as glide coda → "ối"
#[test]
fn oosi_o_circ_sac_i() {
    assert_eq!(type_and_commit("oosi"), "ối");
}

// ── Flexible: tone switch then vowel form ──────────────────────────────────────

// asfw: a→á(s=sắc) → à(f switches to huyền) → ằ(w applies ă-form, huyền preserved)
#[test]
fn asfw_a_breve_huyen() {
    assert_eq!(type_and_commit("asfw"), "ằ");
}

// ── Flexible: coda first then tone ─────────────────────────────────────────────

// oons: oo→ô, n-coda appended, s=sắc retroactively on ô → "ốn"
#[test]
fn oons_o_circ_sac_n() {
    assert_eq!(type_and_commit("oons"), "ốn");
}

// ooms: oo→ô, m-coda appended, s=sắc retroactively on ô → "ốm"
#[test]
fn ooms_o_circ_sac_m() {
    assert_eq!(type_and_commit("ooms"), "ốm");
}

// ── Flexible: triphthong ───────────────────────────────────────────────────────

// uowis: u+ow→ơ scan-back promotes u→ư; i(glide) + s=sắc on ơ → "ưới"
#[test]
fn uowis_uoi_sac() {
    assert_eq!(type_and_commit("uowis"), "ưới");
}

// uowif: same scan-back u→ư; i(glide) + f=huyền on ơ → "ười"
#[test]
fn uowif_uoi_huyen() {
    assert_eq!(type_and_commit("uowif"), "ười");
}

// ── Backspace rollback ────────────────────────────────────────────────────────

// nuawx → "nữa"; backspace removes the last visual char.
// Snapshots for "nưa" and "nua" both have 3 chars (same as "nữa"), so they are
// skipped; the first shorter snapshot is "nu" (2 chars).
#[test]
fn nuawx_backspace_removes_a() {
    let mut e = telex();
    for ch in "nuawx".chars() {
        e.process(&key(ch)).unwrap();
        let _ = e.process(&InputEvent::KeyUp(Key::Char(ch)));
    }
    assert_eq!(preedit(&e), "nữa");

    e.process(&bs()).unwrap();
    assert_eq!(
        preedit(&e),
        "nu",
        "BS: remove last char (a + ư modification)"
    );
}

// asw → "ắ" (1 char); one backspace empties the buffer entirely.
#[test]
fn asw_backspace_clears() {
    let mut e = telex();
    for ch in "asw".chars() {
        e.process(&key(ch)).unwrap();
        let _ = e.process(&InputEvent::KeyUp(Key::Char(ch)));
    }
    assert_eq!(preedit(&e), "ắ");

    e.process(&bs()).unwrap();
    assert_eq!(preedit(&e), "", "BS: single composed char fully removed");
}
