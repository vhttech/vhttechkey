//! Golden tests: intermediate preedit states for typing 'xin chao' in Telex.
//!
//! Documents the expected preedit value after each character and the expected
//! commit value when space terminates a word.  These golden values serve as
//! a regression baseline: if any intermediate preedit state changes, the
//! corresponding assertion fails and must be reviewed.
//!
//! Sequence: x → i → n → <space> → c → h → a → o
//!   "xin" is committed on space; "chao" accumulates in preedit.
#![allow(clippy::unwrap_used)]

use vi_core::{
    CompositionEngine, InputEvent, InputMethod, Key, Modifiers, StandardEngine, StateTransition,
};

fn key_down(ch: char) -> InputEvent {
    InputEvent::KeyDown(Key::Char(ch), Modifiers::none())
}

fn key_up(ch: char) -> InputEvent {
    InputEvent::KeyUp(Key::Char(ch))
}

/// Type a character, reset the repeat guard with KeyUp, and return the transition.
fn type_char(engine: &mut StandardEngine, ch: char) -> StateTransition {
    let t = engine
        .process(&key_down(ch))
        .expect("engine must not error");
    let _ = engine.process(&key_up(ch));
    t
}

// ── Golden: "xin" — preedit builds incrementally ─────────────────────────────

#[test]
fn golden_x_preedit_is_x() {
    let mut engine = StandardEngine::new(InputMethod::Telex);
    let t = type_char(&mut engine, 'x');

    // 'x' is the Telex ngã tone mark, but on an empty buffer it is just pushed
    // to preedit as a literal character (no vowel to tone).
    assert!(
        matches!(t, StateTransition::PreeditUpdated(_)),
        "typing 'x' on empty preedit must produce PreeditUpdated; got {t:?}"
    );
    assert_eq!(
        engine.preedit().as_str(),
        "x",
        "preedit after 'x' must be \"x\""
    );
}

#[test]
fn golden_xi_preedit_is_xi() {
    let mut engine = StandardEngine::new(InputMethod::Telex);
    type_char(&mut engine, 'x');
    let t = type_char(&mut engine, 'i');

    assert!(
        matches!(t, StateTransition::PreeditUpdated(_)),
        "typing 'i' after 'x' must produce PreeditUpdated; got {t:?}"
    );
    assert_eq!(
        engine.preedit().as_str(),
        "xi",
        "preedit after 'x','i' must be \"xi\""
    );
}

#[test]
fn golden_xin_preedit_is_xin() {
    let mut engine = StandardEngine::new(InputMethod::Telex);
    type_char(&mut engine, 'x');
    type_char(&mut engine, 'i');
    let t = type_char(&mut engine, 'n');

    assert!(
        matches!(t, StateTransition::PreeditUpdated(_)),
        "typing 'n' after 'xi' must produce PreeditUpdated; got {t:?}"
    );
    assert_eq!(
        engine.preedit().as_str(),
        "xin",
        "preedit after 'x','i','n' must be \"xin\""
    );
}

// ── Golden: space commits "xin" ───────────────────────────────────────────────

#[test]
fn golden_space_commits_xin() {
    let mut engine = StandardEngine::new(InputMethod::Telex);
    type_char(&mut engine, 'x');
    type_char(&mut engine, 'i');
    type_char(&mut engine, 'n');

    let space = InputEvent::KeyDown(Key::Char(' '), Modifiers::none());
    let t = engine
        .process(&space)
        .expect("engine must not error on space");

    assert!(
        matches!(t, StateTransition::CommitThenPassThrough(_)),
        "space after 'xin' must produce CommitThenPassThrough; got {t:?}"
    );

    if let StateTransition::CommitThenPassThrough(committed) = t {
        assert_eq!(
            committed.as_str(),
            "xin",
            "space must commit \"xin\", not {:?}",
            committed.as_str()
        );
    }

    assert!(
        engine.preedit().is_empty(),
        "preedit must be empty after commit on space"
    );
}

// ── Golden: "chao" — second word builds incrementally ────────────────────────

#[test]
fn golden_c_preedit_is_c() {
    let mut engine = StandardEngine::new(InputMethod::Telex);
    type_char(&mut engine, 'x');
    type_char(&mut engine, 'i');
    type_char(&mut engine, 'n');
    // Commit "xin" with space, then start fresh composition.
    engine
        .process(&InputEvent::KeyDown(Key::Char(' '), Modifiers::none()))
        .unwrap();

    let t = type_char(&mut engine, 'c');
    assert!(
        matches!(t, StateTransition::PreeditUpdated(_)),
        "typing 'c' after space must produce PreeditUpdated; got {t:?}"
    );
    assert_eq!(
        engine.preedit().as_str(),
        "c",
        "preedit after second-word 'c' must be \"c\""
    );
}

#[test]
fn golden_ch_preedit_is_ch() {
    let mut engine = StandardEngine::new(InputMethod::Telex);
    type_char(&mut engine, 'x');
    type_char(&mut engine, 'i');
    type_char(&mut engine, 'n');
    engine
        .process(&InputEvent::KeyDown(Key::Char(' '), Modifiers::none()))
        .unwrap();

    type_char(&mut engine, 'c');
    let t = type_char(&mut engine, 'h');

    assert!(
        matches!(t, StateTransition::PreeditUpdated(_)),
        "typing 'h' after 'c' must produce PreeditUpdated; got {t:?}"
    );
    assert_eq!(
        engine.preedit().as_str(),
        "ch",
        "preedit after 'c','h' must be \"ch\""
    );
}

#[test]
fn golden_cha_preedit_is_cha() {
    let mut engine = StandardEngine::new(InputMethod::Telex);
    type_char(&mut engine, 'x');
    type_char(&mut engine, 'i');
    type_char(&mut engine, 'n');
    engine
        .process(&InputEvent::KeyDown(Key::Char(' '), Modifiers::none()))
        .unwrap();

    type_char(&mut engine, 'c');
    type_char(&mut engine, 'h');
    let t = type_char(&mut engine, 'a');

    assert!(
        matches!(t, StateTransition::PreeditUpdated(_)),
        "typing 'a' after 'ch' must produce PreeditUpdated; got {t:?}"
    );
    assert_eq!(
        engine.preedit().as_str(),
        "cha",
        "preedit after 'c','h','a' must be \"cha\""
    );
}

#[test]
fn golden_chao_preedit_is_chao() {
    let mut engine = StandardEngine::new(InputMethod::Telex);
    type_char(&mut engine, 'x');
    type_char(&mut engine, 'i');
    type_char(&mut engine, 'n');
    engine
        .process(&InputEvent::KeyDown(Key::Char(' '), Modifiers::none()))
        .unwrap();

    type_char(&mut engine, 'c');
    type_char(&mut engine, 'h');
    type_char(&mut engine, 'a');
    let t = type_char(&mut engine, 'o');

    // In Telex, 'ao' is not a diphthong rule — 'o' after 'cha' is appended raw.
    assert!(
        matches!(t, StateTransition::PreeditUpdated(_)),
        "typing 'o' after 'cha' must produce PreeditUpdated; got {t:?}"
    );
    assert_eq!(
        engine.preedit().as_str(),
        "chao",
        "preedit after 'c','h','a','o' must be \"chao\""
    );
}

// ── Full sequence integration: "xin chao" in one harness ─────────────────────

/// Drive the full "xin chao" sequence and collect every intermediate preedit
/// value and every commit value.  Verify the complete golden record.
#[test]
fn golden_full_xin_chao_sequence() {
    let mut engine = StandardEngine::new(InputMethod::Telex);
    let mut preedit_history: Vec<String> = Vec::new();
    let mut commit_history: Vec<String> = Vec::new();

    // Type "xin".
    for ch in ['x', 'i', 'n'] {
        let t = type_char(&mut engine, ch);
        if let StateTransition::PreeditUpdated(ref p) = t {
            preedit_history.push(p.as_str().to_owned());
        }
    }

    // Space commits "xin".
    let space = InputEvent::KeyDown(Key::Char(' '), Modifiers::none());
    match engine.process(&space).expect("space must not error") {
        StateTransition::CommitThenPassThrough(c) => {
            commit_history.push(c.as_str().to_owned());
        }
        other => panic!("space after 'xin' must produce CommitThenPassThrough; got {other:?}"),
    }

    // Type "chao".
    for ch in ['c', 'h', 'a', 'o'] {
        let t = type_char(&mut engine, ch);
        if let StateTransition::PreeditUpdated(ref p) = t {
            preedit_history.push(p.as_str().to_owned());
        }
    }

    // Golden: preedit after each keystroke.
    assert_eq!(
        preedit_history,
        ["x", "xi", "xin", "c", "ch", "cha", "chao"],
        "preedit history for 'xin chao' must match golden sequence"
    );

    // Golden: exactly one commit ("xin") on the space.
    assert_eq!(
        commit_history,
        ["xin"],
        "commit history must be [\"xin\"] — one commit on space"
    );

    // Golden: "chao" remains in preedit at end of sequence.
    assert_eq!(
        engine.preedit().as_str(),
        "chao",
        "preedit must hold \"chao\" at end of 'xin chao' sequence"
    );
}

// ── All keystrokes in "xin chao" produce PreeditUpdated (not PassThrough) ────

#[test]
fn all_xin_chao_letters_produce_preedit_updated() {
    let mut engine = StandardEngine::new(InputMethod::Telex);

    for ch in ['x', 'i', 'n'] {
        let t = type_char(&mut engine, ch);
        assert!(
            matches!(t, StateTransition::PreeditUpdated(_)),
            "'{}' in 'xin' must produce PreeditUpdated; got {t:?}",
            ch
        );
    }

    // Space produces CommitThenPassThrough, not PreeditUpdated.
    engine
        .process(&InputEvent::KeyDown(Key::Char(' '), Modifiers::none()))
        .unwrap();

    for ch in ['c', 'h', 'a', 'o'] {
        let t = type_char(&mut engine, ch);
        assert!(
            matches!(t, StateTransition::PreeditUpdated(_)),
            "'{}' in 'chao' must produce PreeditUpdated; got {t:?}",
            ch
        );
    }
}
