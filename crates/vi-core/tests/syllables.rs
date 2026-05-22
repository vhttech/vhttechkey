#![allow(clippy::unwrap_used)]
//! Tests for all Vietnamese syllable+tone combinations via Telex.
//!
//! Each vowel base × 6 tones (level, sắc, huyền, hỏi, ngã, nặng).
//! Tone keys: (none), s, f, r, x, j

use vi_core::{InputEvent, InputMethod, Key, Modifiers, StandardEngine, CompositionEngine, StateTransition};

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
        other => panic!("unexpected transition: {other:?}"),
    }
}

// ── a ────────────────────────────────────────────────────────────────────────
#[test] fn a_level()  { assert_eq!(type_and_commit("a"),  "a");  }
#[test] fn a_sac()    { assert_eq!(type_and_commit("as"), "á");  }
#[test] fn a_huyen()  { assert_eq!(type_and_commit("af"), "à");  }
#[test] fn a_hoi()    { assert_eq!(type_and_commit("ar"), "ả");  }
#[test] fn a_nga()    { assert_eq!(type_and_commit("ax"), "ã");  }
#[test] fn a_nang()   { assert_eq!(type_and_commit("aj"), "ạ");  }

// ── ă ────────────────────────────────────────────────────────────────────────
#[test] fn aw_level() { assert_eq!(type_and_commit("aw"),  "ă");  }
#[test] fn aw_sac()   { assert_eq!(type_and_commit("aws"), "ắ");  }
#[test] fn aw_huyen() { assert_eq!(type_and_commit("awf"), "ằ");  }
#[test] fn aw_hoi()   { assert_eq!(type_and_commit("awr"), "ẳ");  }
#[test] fn aw_nga()   { assert_eq!(type_and_commit("awx"), "ẵ");  }
#[test] fn aw_nang()  { assert_eq!(type_and_commit("awj"), "ặ");  }

// ── â ────────────────────────────────────────────────────────────────────────
#[test] fn aa_level() { assert_eq!(type_and_commit("aa"),  "â");  }
#[test] fn aa_sac()   { assert_eq!(type_and_commit("aas"), "ấ");  }
#[test] fn aa_huyen() { assert_eq!(type_and_commit("aaf"), "ầ");  }
#[test] fn aa_hoi()   { assert_eq!(type_and_commit("aar"), "ẩ");  }
#[test] fn aa_nga()   { assert_eq!(type_and_commit("aax"), "ẫ");  }
#[test] fn aa_nang()  { assert_eq!(type_and_commit("aaj"), "ậ");  }

// ── e ────────────────────────────────────────────────────────────────────────
#[test] fn e_level()  { assert_eq!(type_and_commit("e"),  "e");  }
#[test] fn e_sac()    { assert_eq!(type_and_commit("es"), "é");  }
#[test] fn e_huyen()  { assert_eq!(type_and_commit("ef"), "è");  }
#[test] fn e_hoi()    { assert_eq!(type_and_commit("er"), "ẻ");  }
#[test] fn e_nga()    { assert_eq!(type_and_commit("ex"), "ẽ");  }
#[test] fn e_nang()   { assert_eq!(type_and_commit("ej"), "ẹ");  }

// ── ê ────────────────────────────────────────────────────────────────────────
#[test] fn ee_level() { assert_eq!(type_and_commit("ee"),  "ê");  }
#[test] fn ee_sac()   { assert_eq!(type_and_commit("ees"), "ế");  }
#[test] fn ee_huyen() { assert_eq!(type_and_commit("eef"), "ề");  }
#[test] fn ee_hoi()   { assert_eq!(type_and_commit("eer"), "ể");  }
#[test] fn ee_nga()   { assert_eq!(type_and_commit("eex"), "ễ");  }
#[test] fn ee_nang()  { assert_eq!(type_and_commit("eej"), "ệ");  }

// ── i ────────────────────────────────────────────────────────────────────────
#[test] fn i_level()  { assert_eq!(type_and_commit("i"),  "i");  }
#[test] fn i_sac()    { assert_eq!(type_and_commit("is"), "í");  }
#[test] fn i_huyen()  { assert_eq!(type_and_commit("if"), "ì");  }
#[test] fn i_hoi()    { assert_eq!(type_and_commit("ir"), "ỉ");  }
#[test] fn i_nga()    { assert_eq!(type_and_commit("ix"), "ĩ");  }
#[test] fn i_nang()   { assert_eq!(type_and_commit("ij"), "ị");  }

// ── o ────────────────────────────────────────────────────────────────────────
#[test] fn o_level()  { assert_eq!(type_and_commit("o"),  "o");  }
#[test] fn o_sac()    { assert_eq!(type_and_commit("os"), "ó");  }
#[test] fn o_huyen()  { assert_eq!(type_and_commit("of"), "ò");  }
#[test] fn o_hoi()    { assert_eq!(type_and_commit("or"), "ỏ");  }
#[test] fn o_nga()    { assert_eq!(type_and_commit("ox"), "õ");  }
#[test] fn o_nang()   { assert_eq!(type_and_commit("oj"), "ọ");  }

// ── ô ────────────────────────────────────────────────────────────────────────
#[test] fn oo_level() { assert_eq!(type_and_commit("oo"),  "ô");  }
#[test] fn oo_sac()   { assert_eq!(type_and_commit("oos"), "ố");  }
#[test] fn oo_huyen() { assert_eq!(type_and_commit("oof"), "ồ");  }
#[test] fn oo_hoi()   { assert_eq!(type_and_commit("oor"), "ổ");  }
#[test] fn oo_nga()   { assert_eq!(type_and_commit("oox"), "ỗ");  }
#[test] fn oo_nang()  { assert_eq!(type_and_commit("ooj"), "ộ");  }

// ── ơ ────────────────────────────────────────────────────────────────────────
#[test] fn ow_level() { assert_eq!(type_and_commit("ow"),  "ơ");  }
#[test] fn ow_sac()   { assert_eq!(type_and_commit("ows"), "ớ");  }
#[test] fn ow_huyen() { assert_eq!(type_and_commit("owf"), "ờ");  }
#[test] fn ow_hoi()   { assert_eq!(type_and_commit("owr"), "ở");  }
#[test] fn ow_nga()   { assert_eq!(type_and_commit("owx"), "ỡ");  }
#[test] fn ow_nang()  { assert_eq!(type_and_commit("owj"), "ợ");  }

// ── u ────────────────────────────────────────────────────────────────────────
#[test] fn u_level()  { assert_eq!(type_and_commit("u"),  "u");  }
#[test] fn u_sac()    { assert_eq!(type_and_commit("us"), "ú");  }
#[test] fn u_huyen()  { assert_eq!(type_and_commit("uf"), "ù");  }
#[test] fn u_hoi()    { assert_eq!(type_and_commit("ur"), "ủ");  }
#[test] fn u_nga()    { assert_eq!(type_and_commit("ux"), "ũ");  }
#[test] fn u_nang()   { assert_eq!(type_and_commit("uj"), "ụ");  }

// ── ư ────────────────────────────────────────────────────────────────────────
#[test] fn uw_level() { assert_eq!(type_and_commit("uw"),  "ư");  }
#[test] fn uw_sac()   { assert_eq!(type_and_commit("uws"), "ứ");  }
#[test] fn uw_huyen() { assert_eq!(type_and_commit("uwf"), "ừ");  }
#[test] fn uw_hoi()   { assert_eq!(type_and_commit("uwr"), "ử");  }
#[test] fn uw_nga()   { assert_eq!(type_and_commit("uwx"), "ữ");  }
#[test] fn uw_nang()  { assert_eq!(type_and_commit("uwj"), "ự");  }

// ── y ────────────────────────────────────────────────────────────────────────
#[test] fn y_level()  { assert_eq!(type_and_commit("y"),  "y");  }
#[test] fn y_sac()    { assert_eq!(type_and_commit("ys"), "ý");  }
#[test] fn y_huyen()  { assert_eq!(type_and_commit("yf"), "ỳ");  }
#[test] fn y_hoi()    { assert_eq!(type_and_commit("yr"), "ỷ");  }
#[test] fn y_nga()    { assert_eq!(type_and_commit("yx"), "ỹ");  }
#[test] fn y_nang()   { assert_eq!(type_and_commit("yj"), "ỵ");  }
