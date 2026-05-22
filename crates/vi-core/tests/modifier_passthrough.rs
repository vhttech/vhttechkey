#![allow(clippy::unwrap_used)]
//! Tests for modifier-key and dead-key behaviour during composition.
//!
//! Requirements audited:
//!  • Ctrl+char — commits pending preedit; Ctrl+char is not swallowed into
//!    composition state.
//!  • Super+char — same.
//!  • Alt+char (no AltGr flag) — commits pending preedit.
//!  • AltGr flag set — commits pending preedit.
//!  • Bare modifier Keysym (Shift, Ctrl, Alt, Super, Hyper, Compose, …) pressed
//!    alone — must NOT commit or disturb in-progress composition context.
//!    (Regression: the catch-all KeyDown arm used to fire on these.)
//!  • Navigation / non-character keys (Left, Right, Delete, Keysym) — commit
//!    pending preedit so the text is not silently lost.

use vi_core::{
    CompositionEngine, InputEvent, InputMethod, Key, Modifiers, StandardEngine, StateTransition,
};

fn telex() -> StandardEngine {
    StandardEngine::new(InputMethod::Telex)
}

fn kd(c: char) -> InputEvent {
    InputEvent::KeyDown(Key::Char(c), Modifiers::none())
}

fn type_str(engine: &mut StandardEngine, s: &str) {
    for ch in s.chars() {
        engine.process(&kd(ch)).unwrap();
        let _ = engine.process(&InputEvent::KeyUp(Key::Char(ch)));
    }
}

fn preedit(e: &StandardEngine) -> String {
    e.preedit().as_str().to_owned()
}

fn is_commit(t: &StateTransition) -> bool {
    matches!(
        t,
        StateTransition::Commit(_) | StateTransition::CommitAndClear(_)
    )
}

fn committed_text(t: &StateTransition) -> Option<&str> {
    match t {
        StateTransition::Commit(c) | StateTransition::CommitAndClear(c) => Some(c.as_str()),
        _ => None,
    }
}

// ── Ctrl + char ───────────────────────────────────────────────────────────────

#[test]
fn ctrl_char_commits_preedit() {
    let mut e = telex();
    type_str(&mut e, "too"); // preedit = "tô"
    assert_eq!(preedit(&e), "tô");

    let t = e.process(&InputEvent::KeyDown(
        Key::Char('c'),
        Modifiers { ctrl: true, ..Modifiers::none() },
    )).unwrap();

    assert!(is_commit(&t), "Ctrl+char must commit preedit; got {t:?}");
    assert_eq!(committed_text(&t), Some("tô"));
    assert!(e.preedit().is_empty(), "preedit must be empty after Ctrl+char");
}

#[test]
fn ctrl_char_passthrough_when_preedit_empty() {
    let mut e = telex();
    let t = e.process(&InputEvent::KeyDown(
        Key::Char('c'),
        Modifiers { ctrl: true, ..Modifiers::none() },
    )).unwrap();
    assert_eq!(t, StateTransition::PassThrough, "Ctrl+char with empty preedit must PassThrough");
}

// ── Super + char ──────────────────────────────────────────────────────────────

#[test]
fn super_char_commits_preedit() {
    let mut e = telex();
    type_str(&mut e, "aa"); // preedit = "â"

    let t = e.process(&InputEvent::KeyDown(
        Key::Char('l'),
        Modifiers { super_key: true, ..Modifiers::none() },
    )).unwrap();

    assert!(is_commit(&t), "Super+char must commit preedit; got {t:?}");
    assert_eq!(committed_text(&t), Some("â"));
    assert!(e.preedit().is_empty());
}

#[test]
fn super_char_passthrough_when_preedit_empty() {
    let mut e = telex();
    let t = e.process(&InputEvent::KeyDown(
        Key::Char('l'),
        Modifiers { super_key: true, ..Modifiers::none() },
    )).unwrap();
    assert_eq!(t, StateTransition::PassThrough);
}

// ── Alt + char (AltGr workaround path) ───────────────────────────────────────

#[test]
fn alt_char_commits_preedit() {
    let mut e = telex();
    type_str(&mut e, "uw"); // preedit = "ư"

    let t = e.process(&InputEvent::KeyDown(
        Key::Char('o'),
        Modifiers { alt: true, ..Modifiers::none() },
    )).unwrap();

    assert!(is_commit(&t), "Alt+char must commit preedit; got {t:?}");
    assert_eq!(committed_text(&t), Some("ư"));
    assert!(e.preedit().is_empty());
}

#[test]
fn alt_char_passthrough_when_preedit_empty() {
    let mut e = telex();
    let t = e.process(&InputEvent::KeyDown(
        Key::Char('o'),
        Modifiers { alt: true, ..Modifiers::none() },
    )).unwrap();
    assert_eq!(t, StateTransition::PassThrough);
}

// ── AltGr (dedicated flag) ────────────────────────────────────────────────────

#[test]
fn altgr_commits_preedit() {
    let mut e = telex();
    type_str(&mut e, "ee"); // preedit = "ê"

    let t = e.process(&InputEvent::KeyDown(
        Key::Char('e'),
        Modifiers::altgr(),
    )).unwrap();

    assert!(is_commit(&t), "AltGr must commit preedit; got {t:?}");
    assert_eq!(committed_text(&t), Some("ê"));
    assert!(e.preedit().is_empty());
}

#[test]
fn altgr_passthrough_when_preedit_empty() {
    let mut e = telex();
    let t = e.process(&InputEvent::KeyDown(Key::Char('e'), Modifiers::altgr())).unwrap();
    assert_eq!(t, StateTransition::PassThrough);
}

// ── Bare modifier Keysyms must NOT disturb composition ─────────────────────────
//
// Regression: before the is_modifier_keysym fix, pressing a lone modifier key
// (e.g. Shift_L while composing) hit the catch-all KeyDown arm and committed the
// preedit unexpectedly.

const XK_SHIFT_L:   u32 = 0xffe1;
const XK_SHIFT_R:   u32 = 0xffe2;
const XK_CONTROL_L: u32 = 0xffe3;
const XK_CONTROL_R: u32 = 0xffe4;
const XK_CAPS_LOCK: u32 = 0xffe5;
const XK_ALT_L:     u32 = 0xffe9;
const XK_ALT_R:     u32 = 0xffea;
const XK_SUPER_L:   u32 = 0xffeb;
const XK_SUPER_R:   u32 = 0xffec;
const XK_ALTGR:     u32 = 0xfe03; // ISO_Level3_Shift
const XK_MULTI_KEY: u32 = 0xff20; // Compose
const XK_NUM_LOCK:  u32 = 0xff7f;

fn modifier_keysyms() -> &'static [u32] {
    &[
        XK_SHIFT_L, XK_SHIFT_R, XK_CONTROL_L, XK_CONTROL_R, XK_CAPS_LOCK,
        XK_ALT_L, XK_ALT_R, XK_SUPER_L, XK_SUPER_R, XK_ALTGR, XK_MULTI_KEY, XK_NUM_LOCK,
    ]
}

#[test]
fn bare_modifier_keysym_does_not_commit_preedit() {
    for &sym in modifier_keysyms() {
        let mut e = telex();
        type_str(&mut e, "too"); // preedit = "tô"
        let pre = preedit(&e);
        assert_eq!(pre, "tô");

        let t = e.process(&InputEvent::KeyDown(Key::Keysym(sym), Modifiers::none())).unwrap();

        assert_eq!(
            t,
            StateTransition::PassThrough,
            "bare modifier keysym {sym:#010x} must PassThrough, got {t:?}"
        );
        assert_eq!(
            preedit(&e), "tô",
            "bare modifier keysym {sym:#010x} must not alter preedit"
        );
    }
}

#[test]
fn bare_modifier_keysym_does_not_reset_last_char_state() {
    // After a rule fires (last_rule_fired = true), a bare modifier key
    // pressed while holding Shift (to start a chord) must not clear that flag.
    // This is verified indirectly: after the modifier key, a KeyRepeat of the
    // rule-triggering char must still be handled as a raw push (not re-fire the rule).
    let mut e = telex();
    type_str(&mut e, "oo"); // "oo"→"ô", last_rule_fired = true
    assert_eq!(preedit(&e), "ô");

    // Bare Shift_L (e.g. user holding Shift before pressing another key).
    e.process(&InputEvent::KeyDown(Key::Keysym(XK_SHIFT_L), Modifiers::none())).unwrap();
    assert_eq!(preedit(&e), "ô", "modifier key must not clear preedit");

    // KeyRepeat of 'o' should append raw 'o', not re-fire oo→ô.
    e.process(&InputEvent::KeyRepeat(Key::Char('o'), Modifiers::none())).unwrap();
    assert_eq!(preedit(&e), "ôo", "KeyRepeat after modifier must extend preedit raw");
}

#[test]
fn bare_modifier_keysym_passthrough_when_preedit_empty() {
    for &sym in modifier_keysyms() {
        let mut e = telex();
        let t = e.process(&InputEvent::KeyDown(Key::Keysym(sym), Modifiers::none())).unwrap();
        assert_eq!(
            t,
            StateTransition::PassThrough,
            "modifier keysym {sym:#010x} with empty preedit must PassThrough"
        );
    }
}

// ── Navigation / non-character keys commit preedit ────────────────────────────

#[test]
fn left_arrow_commits_preedit() {
    let mut e = telex();
    type_str(&mut e, "as"); // "á"
    let t = e.process(&InputEvent::KeyDown(Key::Left, Modifiers::none())).unwrap();
    assert!(is_commit(&t), "Left arrow must commit preedit; got {t:?}");
    assert_eq!(committed_text(&t), Some("á"));
    assert!(e.preedit().is_empty());
}

#[test]
fn right_arrow_commits_preedit() {
    let mut e = telex();
    type_str(&mut e, "oo");
    let t = e.process(&InputEvent::KeyDown(Key::Right, Modifiers::none())).unwrap();
    assert!(is_commit(&t), "Right arrow must commit preedit; got {t:?}");
    assert_eq!(committed_text(&t), Some("ô"));
}

#[test]
fn delete_key_commits_preedit() {
    let mut e = telex();
    type_str(&mut e, "uw"); // "ư"
    let t = e.process(&InputEvent::KeyDown(Key::Delete, Modifiers::none())).unwrap();
    assert!(is_commit(&t), "Delete must commit preedit; got {t:?}");
    assert_eq!(committed_text(&t), Some("ư"));
}

#[test]
fn home_key_commits_preedit() {
    let mut e = telex();
    type_str(&mut e, "ee"); // "ê"
    let t = e.process(&InputEvent::KeyDown(Key::Home, Modifiers::none())).unwrap();
    assert!(is_commit(&t), "Home must commit preedit; got {t:?}");
    assert_eq!(committed_text(&t), Some("ê"));
}

#[test]
fn non_modifier_keysym_commits_preedit() {
    // XK_F1 = 0xffbe — a function key, not a modifier — must commit.
    const XK_F1: u32 = 0xffbe;
    let mut e = telex();
    type_str(&mut e, "aa"); // "â"
    let t = e.process(&InputEvent::KeyDown(Key::Keysym(XK_F1), Modifiers::none())).unwrap();
    assert!(is_commit(&t), "F1 Keysym must commit preedit; got {t:?}");
    assert_eq!(committed_text(&t), Some("â"));
}

#[test]
fn non_modifier_keysym_passthrough_when_preedit_empty() {
    const XK_F1: u32 = 0xffbe;
    let mut e = telex();
    let t = e.process(&InputEvent::KeyDown(Key::Keysym(XK_F1), Modifiers::none())).unwrap();
    assert_eq!(t, StateTransition::PassThrough);
}

// ── All three methods behave the same for modifier keys ───────────────────────

#[test]
fn ctrl_char_behaviour_all_methods() {
    for method in [InputMethod::Telex, InputMethod::Vni, InputMethod::Viqr] {
        let mut e = StandardEngine::new(method);
        // 'a' is safe for all methods.
        e.process(&InputEvent::KeyDown(Key::Char('a'), Modifiers::none())).unwrap();
        assert!(!e.preedit().is_empty(), "{method:?}: preedit must be non-empty after 'a'");

        let t = e.process(&InputEvent::KeyDown(
            Key::Char('c'),
            Modifiers { ctrl: true, ..Modifiers::none() },
        )).unwrap();

        assert!(is_commit(&t), "{method:?}: Ctrl+c must commit preedit; got {t:?}");
        assert!(e.preedit().is_empty(), "{method:?}: preedit must be empty after Ctrl+c");
    }
}

#[test]
fn bare_modifier_behaviour_all_methods() {
    for method in [InputMethod::Telex, InputMethod::Vni, InputMethod::Viqr] {
        let mut e = StandardEngine::new(method);
        e.process(&InputEvent::KeyDown(Key::Char('a'), Modifiers::none())).unwrap();

        let t = e.process(&InputEvent::KeyDown(
            Key::Keysym(XK_SHIFT_L),
            Modifiers::none(),
        )).unwrap();

        assert_eq!(
            t,
            StateTransition::PassThrough,
            "{method:?}: bare Shift_L must PassThrough during composition"
        );
        assert!(!e.preedit().is_empty(), "{method:?}: preedit must survive a bare modifier key");
    }
}
