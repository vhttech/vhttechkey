//! Insta snapshot tests for preedit display at each composition step.
//!
//! Run `cargo insta review` after changing engine behaviour to update snapshots.
#![allow(clippy::unwrap_used)]

use vi_core::{CompositionEngine, InputEvent, InputMethod, Key, Modifiers, StandardEngine};

fn key(c: char) -> InputEvent {
    InputEvent::KeyDown(Key::Char(c), Modifiers::none())
}

fn step(engine: &mut StandardEngine, ch: char) -> String {
    engine.process(&key(ch)).unwrap();
    let _ = engine.process(&InputEvent::KeyUp(Key::Char(ch)));
    engine.preedit().as_str().to_owned()
}

// ── Telex composition steps ───────────────────────────────────────────────────

#[test]
fn snapshot_telex_toi_steps() {
    let mut e = StandardEngine::new(InputMethod::Telex);
    insta::assert_snapshot!(step(&mut e, 't'),   @"t");
    insta::assert_snapshot!(step(&mut e, 'o'),   @"to");
    insta::assert_snapshot!(step(&mut e, 'o'),   @"tô");
    insta::assert_snapshot!(step(&mut e, 'i'),   @"tôi");
}

#[test]
fn snapshot_telex_a_sac_steps() {
    let mut e = StandardEngine::new(InputMethod::Telex);
    insta::assert_snapshot!(step(&mut e, 'a'), @"a");
    insta::assert_snapshot!(step(&mut e, 's'), @"á");
}

#[test]
fn snapshot_telex_a_huyen_steps() {
    let mut e = StandardEngine::new(InputMethod::Telex);
    insta::assert_snapshot!(step(&mut e, 'a'), @"a");
    insta::assert_snapshot!(step(&mut e, 'f'), @"à");
}

#[test]
fn snapshot_telex_dd_d_stroke() {
    let mut e = StandardEngine::new(InputMethod::Telex);
    insta::assert_snapshot!(step(&mut e, 'd'), @"d");
    insta::assert_snapshot!(step(&mut e, 'd'), @"đ");
}

#[test]
fn snapshot_telex_o_circumflex_steps() {
    let mut e = StandardEngine::new(InputMethod::Telex);
    insta::assert_snapshot!(step(&mut e, 'o'), @"o");
    insta::assert_snapshot!(step(&mut e, 'o'), @"ô");
}

// ── VNI composition steps ─────────────────────────────────────────────────────

#[test]
fn snapshot_vni_toi_steps() {
    let mut e = StandardEngine::new(InputMethod::Vni);
    insta::assert_snapshot!(step(&mut e, 't'), @"t");
    insta::assert_snapshot!(step(&mut e, 'o'), @"to");
    insta::assert_snapshot!(step(&mut e, '6'), @"tô");
    insta::assert_snapshot!(step(&mut e, 'i'), @"tôi");
}

#[test]
fn snapshot_vni_a_acute_steps() {
    let mut e = StandardEngine::new(InputMethod::Vni);
    insta::assert_snapshot!(step(&mut e, 'a'), @"a");
    insta::assert_snapshot!(step(&mut e, '1'), @"á");
}

// ── VIQR composition steps ────────────────────────────────────────────────────

#[test]
fn snapshot_viqr_toi_steps() {
    let mut e = StandardEngine::new(InputMethod::Viqr);
    insta::assert_snapshot!(step(&mut e, 't'), @"t");
    insta::assert_snapshot!(step(&mut e, 'o'), @"to");
    insta::assert_snapshot!(step(&mut e, '^'), @"tô");
    insta::assert_snapshot!(step(&mut e, 'i'), @"tôi");
}

#[test]
fn snapshot_viqr_a_acute_steps() {
    let mut e = StandardEngine::new(InputMethod::Viqr);
    insta::assert_snapshot!(step(&mut e, 'a'),  @"a");
    insta::assert_snapshot!(step(&mut e, '\''), @"á");
}

// ── Backspace rollback ────────────────────────────────────────────────────────

#[test]
fn snapshot_backspace_rollback() {
    let mut e = StandardEngine::new(InputMethod::Telex);
    step(&mut e, 't');
    step(&mut e, 'o');
    step(&mut e, 'o'); // preedit = "tô"
    e.process(&InputEvent::KeyDown(Key::Backspace, Modifiers::none()))
        .unwrap();
    insta::assert_snapshot!(e.preedit().as_str(), @"t");
}
