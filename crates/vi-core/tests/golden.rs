#![allow(clippy::unwrap_used)]
//! Regression golden-file tests for Telex, VNI, and VIQR key sequences.

use vi_core::{CompositionEngine, InputEvent, InputMethod, Key, Modifiers, StandardEngine, StateTransition};

fn engine(method: InputMethod) -> StandardEngine {
    StandardEngine::new(method)
}

fn key(c: char) -> InputEvent {
    InputEvent::KeyDown(Key::Char(c), Modifiers::none())
}

fn ret() -> InputEvent {
    InputEvent::KeyDown(Key::Return, Modifiers::none())
}

fn type_and_commit(method: InputMethod, keys: &str) -> String {
    let mut e = engine(method);
    for ch in keys.chars() {
        e.process(&key(ch)).unwrap();
        let _ = e.process(&InputEvent::KeyUp(Key::Char(ch)));
    }
    match e.process(&ret()).unwrap() {
        StateTransition::CommitAndClear(c) => c.as_str().to_owned(),
        StateTransition::PassThrough => String::new(),
        other => panic!("unexpected: {other:?}"),
    }
}

// ── Telex golden sequences ────────────────────────────────────────────────────

#[test]
fn telex_toi() {
    assert_eq!(type_and_commit(InputMethod::Telex, "tooi"), "tôi");
}

#[test]
fn telex_duoc() {
    // dd→đ, uw→ư, ow→ơ, c, j→nặng on ơ→ợ
    assert_eq!(type_and_commit(InputMethod::Telex, "dduwowcj"), "được");
}

#[test]
fn telex_viet_nam() {
    assert_eq!(type_and_commit(InputMethod::Telex, "vieejt"), "việt");
}

#[test]
fn telex_xin_chao() {
    assert_eq!(type_and_commit(InputMethod::Telex, "xinf"), "xìn");
}

#[test]
fn telex_ha_noi() {
    // H, aa→â, f→grave on â→ầ
    assert_eq!(type_and_commit(InputMethod::Telex, "Haaf"), "Hầ");
}

#[test]
fn telex_nguoi() {
    // tone (huyền) must land on ơ, not on the trailing glide i → "người"
    assert_eq!(type_and_commit(InputMethod::Telex, "nguwowif"), "người");
}

// ── VNI golden sequences ──────────────────────────────────────────────────────

#[test]
fn vni_toi() {
    assert_eq!(type_and_commit(InputMethod::Vni, "to6i"), "tôi");
}

#[test]
fn vni_duoc() {
    // d9→đ, u7→ư, o7→ơ, c, 5→nặng on ơ→ợ
    assert_eq!(type_and_commit(InputMethod::Vni, "d9u7o7c5"), "được");
}

#[test]
fn vni_viet() {
    assert_eq!(type_and_commit(InputMethod::Vni, "vie65t"), "việt");
}

#[test]
fn vni_a_breve_acute() {
    assert_eq!(type_and_commit(InputMethod::Vni, "a81"), "ắ");
}

// ── VIQR golden sequences ─────────────────────────────────────────────────────

#[test]
fn viqr_toi() {
    assert_eq!(type_and_commit(InputMethod::Viqr, "to^i"), "tôi");
}

#[test]
fn viqr_duoc() {
    // dd→đ, u+→ư, o+→ơ, c, .→nặng on ơ→ợ
    assert_eq!(type_and_commit(InputMethod::Viqr, "ddu+o+c."), "được");
}

#[test]
fn viqr_a_breve_acute() {
    assert_eq!(type_and_commit(InputMethod::Viqr, "a('"), "ắ");
}

#[test]
fn viqr_acute() {
    assert_eq!(type_and_commit(InputMethod::Viqr, "a'"), "á");
}

#[test]
fn viqr_grave() {
    assert_eq!(type_and_commit(InputMethod::Viqr, "a`"), "à");
}

#[test]
fn english_words_commit_correctly() {
    // "message" is excluded: "me"+'s'→"mé" (valid Vietnamese syllable, no raw fallback),
    // then second 's' triggers the double-tone undo → "mes" (no Vietnamese char).
    // Continuation after the undo uses the composition output ("mesa"…) rather than raw
    // keys ("messa"…), so "message" yields "mesage". This matches ibus-lotus behaviour.
    for word in &["permission", "monitor", "server", "system", "network", "process", "resource",
                  "access", "session", "address", "protocol", "request", "response"] {
        let mut e = engine(InputMethod::Telex);
        for ch in word.chars() {
            e.process(&key(ch)).unwrap();
            e.process(&InputEvent::KeyUp(Key::Char(ch))).ok();
        }
        let r = e.process(&ret()).unwrap();
        let committed = match r {
            StateTransition::CommitAndClear(c) => c.as_str().to_owned(),
            StateTransition::PassThrough => e.preedit().as_str().to_owned(),
            _ => format!("{r:?}"),
        };
        assert_eq!(committed, *word, "English word '{}' was mangled to '{}'", word, committed);
    }
}

#[test]
fn telex_message_double_s_undo_known_limitation() {
    // "me"+"s" → "mé" (valid Vietnamese, no fallback); second "s" double-undo → "mes";
    // continuation uses composition output (not raw keys), so "message" → "mesage".
    // ibus-lotus has the same behaviour — accepted trade-off for fixing "perrm"→"perm".
    let mut e = engine(InputMethod::Telex);
    for ch in "message".chars() {
        e.process(&key(ch)).unwrap();
        e.process(&InputEvent::KeyUp(Key::Char(ch))).ok();
    }
    let r = e.process(&ret()).unwrap();
    let committed = match r {
        StateTransition::CommitAndClear(c) => c.as_str().to_owned(),
        StateTransition::PassThrough => e.preedit().as_str().to_owned(),
        _ => format!("{r:?}"),
    };
    assert_eq!(committed, "mesage");
}
