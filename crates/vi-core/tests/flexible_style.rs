#![allow(clippy::unwrap_used)]
//! Integration tests for Telex tone-then-vowel-form compositions (flexible tone ordering).
//!
//! These tests verify that typing a tone key before a vowel-form key correctly
//! preserves the tone when the base vowel changes form:
//!   a + s(ắc) → á, then w → ă-form → ắ

use vi_core::{CompositionEngine, InputEvent, InputMethod, Key, Modifiers, StandardEngine, StateTransition};

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
        StateTransition::PassThrough => String::new(),
        other => panic!("unexpected transition: {other:?}"),
    }
}

// a + sắc + w → ắ  (a → á → ắ)
#[test]
fn asw_a_breve_acute() {
    assert_eq!(type_and_commit("asw"), "ắ");
}

// e + huyền + e → ề  (e → è → ề)
#[test]
fn efe_e_circumflex_grave() {
    assert_eq!(type_and_commit("efe"), "ề");
}

// u + huyền + w → ừ  (u → ù → ừ)
#[test]
fn ufw_u_horn_grave() {
    assert_eq!(type_and_commit("ufw"), "ừ");
}

// o + sắc + w → ớ  (o → ó → ớ)
#[test]
fn osw_o_horn_acute() {
    assert_eq!(type_and_commit("osw"), "ớ");
}

// Tone switching: a + sắc + huyền → à  (á switches to à)
#[test]
fn asf_tone_switch_sac_to_huyen() {
    assert_eq!(type_and_commit("asf"), "à");
}

// Tone switching on compound vowel: oo→ô, j→ộ, s→ố
#[test]
fn oojs_compound_vowel_tone_switch() {
    assert_eq!(type_and_commit("oojs"), "ố");
}

// Uppercase tone keys must be passed through unchanged
#[test]
fn uppercase_s_no_tone_applied() {
    assert_eq!(type_and_commit("aS"), "aS");
}

#[test]
fn uppercase_f_no_tone_applied() {
    assert_eq!(type_and_commit("aF"), "aF");
}

// Tone switch then vowel form: e + huyền + sắc + e → ế
// (è switches to é via 's', then second 'e' fires ee→ê carrying the existing sắc)
#[test]
fn efse_tone_switch_then_e_circumflex() {
    assert_eq!(type_and_commit("efse"), "ế");
}

// Compound vowel with two-step tone switch: oo→ô, sắc→ố, huyền→ồ
// ('s' first applies sắc to ô, 'f' then switches to huyền on the same ô nucleus)
#[test]
fn oosf_compound_vowel_double_tone_switch() {
    assert_eq!(type_and_commit("oosf"), "ồ");
}

// Multi-char word with tone-before-vowelform: thoai + huyền → thoài
// (tone lands on 'a' — 'i' is a coda glide in the 'oai' triphthong nucleus)
#[test]
fn thoaif_multi_char_huyen_on_nucleus_a() {
    assert_eq!(type_and_commit("thoaif"), "thoài");
}

// Backspace on a single composed char deletes it entirely (vhttechkey style).
// "asw" → "ắ" (1 displayed char); one backspace empties the buffer.
#[test]
fn asw_backspace_clears() {
    let mut e = telex();
    for ch in "asw".chars() {
        e.process(&key(ch)).unwrap();
        let _ = e.process(&InputEvent::KeyUp(Key::Char(ch)));
    }
    assert_eq!(preedit(&e), "ắ");

    e.process(&bs()).unwrap();
    assert_eq!(preedit(&e), "", "bs removes the whole composed char");
}
