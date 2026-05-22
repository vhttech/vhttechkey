//! Dictionary-assisted commit behaviour.

use std::sync::Arc;

use vi_core::{
    CompositionEngine, InputEvent, InputMethod, Key, Modifiers, SpellOptions, StandardEngine,
    StateTransition, VietnameseDict,
};

fn key_char(c: char) -> InputEvent {
    InputEvent::KeyDown(Key::Char(c), Modifiers::none())
}

fn key_return() -> InputEvent {
    InputEvent::KeyDown(Key::Return, Modifiers::none())
}

fn process_str(engine: &mut StandardEngine, s: &str) {
    for ch in s.chars() {
        engine.process(&key_char(ch)).unwrap();
        let _ = engine.process(&InputEvent::KeyUp(Key::Char(ch)));
    }
}

fn committed(t: Result<StateTransition, vi_core::CompositionError>) -> Option<String> {
    match t.unwrap() {
        StateTransition::CommitAndClear(c) => Some(c.as_str().to_owned()),
        StateTransition::Commit(c) => Some(c.as_str().to_owned()),
        StateTransition::CommitThenPassThrough(c) => Some(c.as_str().to_owned()),
        StateTransition::CommitThenPreedit(c, _) => Some(c.as_str().to_owned()),
        _ => None,
    }
}

#[test]
fn commits_raw_telex_when_word_missing_from_dict() {
    let dict = VietnameseDict::load_from_reader(std::io::Cursor::new("zzzunused\n")).unwrap();
    let spell = SpellOptions {
        dictionary: Some(Arc::new(dict)),
        commit_spell_check_dict: true,
        dd_freestyle: true,
    };
    let mut e = StandardEngine::with_spell(InputMethod::Telex, spell);
    process_str(&mut e, "tooi");
    assert_eq!(e.preedit().as_str(), "tôi");
    let out = committed(e.process(&key_return()));
    assert_eq!(out.as_deref(), Some("tooi"));
}

#[test]
fn commits_composed_when_word_present_in_dict() {
    let dict = VietnameseDict::load_from_reader(std::io::Cursor::new("tôi\n")).unwrap();
    let spell = SpellOptions {
        dictionary: Some(Arc::new(dict)),
        commit_spell_check_dict: true,
        dd_freestyle: true,
    };
    let mut e = StandardEngine::with_spell(InputMethod::Telex, spell);
    process_str(&mut e, "tooi");
    let out = committed(e.process(&key_return()));
    assert_eq!(out.as_deref(), Some("tôi"));
}

#[test]
fn dd_freestyle_keeps_composed_even_if_not_in_dict() {
    let dict = VietnameseDict::load_from_reader(std::io::Cursor::new("zzz\n")).unwrap();
    let spell = SpellOptions {
        dictionary: Some(Arc::new(dict)),
        commit_spell_check_dict: true,
        dd_freestyle: true,
    };
    let mut e = StandardEngine::with_spell(InputMethod::Telex, spell);
    process_str(&mut e, "dd");
    assert_eq!(e.preedit().as_str(), "đ");
    let out = committed(e.process(&key_return()));
    assert_eq!(out.as_deref(), Some("đ"));
}

#[test]
fn without_dict_flag_commits_as_before() {
    let dict = VietnameseDict::load_from_reader(std::io::Cursor::new("zzz\n")).unwrap();
    let spell = SpellOptions {
        dictionary: Some(Arc::new(dict)),
        commit_spell_check_dict: false,
        dd_freestyle: true,
    };
    let mut e = StandardEngine::with_spell(InputMethod::Telex, spell);
    process_str(&mut e, "tooi");
    let out = committed(e.process(&key_return()));
    assert_eq!(out.as_deref(), Some("tôi"));
}
