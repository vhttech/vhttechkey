#![allow(clippy::unwrap_used)]
//! Input method switching tests: switching between Telex, VNI, and VIQR
//! mid-word must reset state cleanly and leave no residue in the preedit.

use vi_core::{
    CompositionEngine, InputEvent, InputMethod, Key, Modifiers, StandardEngine, StateTransition,
};

fn kd(c: char) -> InputEvent {
    InputEvent::KeyDown(Key::Char(c), Modifiers::none())
}

fn type_str(engine: &mut StandardEngine, s: &str) {
    for ch in s.chars() {
        engine.process(&kd(ch)).unwrap();
        let _ = engine.process(&InputEvent::KeyUp(Key::Char(ch)));
    }
}

fn commit_str(engine: &mut StandardEngine) -> String {
    match engine.process(&InputEvent::KeyDown(Key::Return, Modifiers::none())).unwrap() {
        StateTransition::CommitAndClear(c) | StateTransition::Commit(c) => c.as_str().to_owned(),
        StateTransition::PassThrough => String::new(),
        other => panic!("unexpected transition on Return: {other:?}"),
    }
}

/// Type a partial word with `from`, reset, then compose a full word with `to`
/// and verify the committed text matches `expected`.
fn assert_clean_switch(
    from: InputMethod,
    from_keys: &str,
    to: InputMethod,
    to_keys: &str,
    expected: &str,
) {
    let mut from_engine = StandardEngine::new(from);
    type_str(&mut from_engine, from_keys);
    assert!(
        !from_engine.preedit().is_empty(),
        "preedit must be non-empty after {from_keys:?} with {from:?}"
    );

    // Simulate method switch: discard in-progress composition.
    from_engine.reset();
    assert!(
        from_engine.preedit().is_empty(),
        "preedit must be empty after reset for {from:?}"
    );

    let mut to_engine = StandardEngine::new(to);
    type_str(&mut to_engine, to_keys);
    let committed = commit_str(&mut to_engine);
    assert_eq!(committed, expected, "wrong output after {from:?}→{to:?} switch");
}

#[test]
fn telex_to_vni() {
    assert_clean_switch(InputMethod::Telex, "too", InputMethod::Vni, "to6i", "tôi");
}

#[test]
fn vni_to_telex() {
    assert_clean_switch(InputMethod::Vni, "to6", InputMethod::Telex, "tooi", "tôi");
}

#[test]
fn telex_to_viqr() {
    assert_clean_switch(InputMethod::Telex, "vie", InputMethod::Viqr, "to^i", "tôi");
}

#[test]
fn viqr_to_telex() {
    assert_clean_switch(InputMethod::Viqr, "to^", InputMethod::Telex, "tooi", "tôi");
}

#[test]
fn vni_to_viqr() {
    assert_clean_switch(InputMethod::Vni, "to6", InputMethod::Viqr, "to^i", "tôi");
}

#[test]
fn viqr_to_vni() {
    assert_clean_switch(InputMethod::Viqr, "to^", InputMethod::Vni, "to6i", "tôi");
}

/// Cycling through all three methods 50 times each — reset leaves no residue.
#[test]
fn cycle_all_three_methods_no_residue() {
    let cases: &[(InputMethod, &str)] = &[
        (InputMethod::Telex, "too"),
        (InputMethod::Vni, "to6"),
        (InputMethod::Viqr, "to^"),
    ];
    for _ in 0..50 {
        for &(method, keys) in cases {
            let mut engine = StandardEngine::new(method);
            type_str(&mut engine, keys);
            assert!(!engine.preedit().is_empty());
            engine.reset();
            assert!(
                engine.preedit().is_empty(),
                "residual preedit after reset for {method:?}"
            );
        }
    }
}

/// Reset event (InputEvent::Reset) must clear preedit without committing,
/// matching the same guarantee as calling engine.reset().
#[test]
fn reset_event_clears_without_commit() {
    for method in [InputMethod::Telex, InputMethod::Vni, InputMethod::Viqr] {
        let mut engine = StandardEngine::new(method);
        type_str(&mut engine, "aaa");
        assert!(!engine.preedit().is_empty());

        let r = engine.process(&InputEvent::Reset).unwrap();
        assert!(
            matches!(r, StateTransition::Cleared),
            "Reset event must return Cleared for {method:?}, got {r:?}"
        );
        assert!(engine.preedit().is_empty(), "preedit not cleared after Reset event for {method:?}");
    }
}

/// Switch via FocusOut: old method commits its preedit; new engine starts clean.
#[test]
fn switch_via_focus_out_then_new_engine() {
    let mut telex = StandardEngine::new(InputMethod::Telex);
    type_str(&mut telex, "af"); // "af" → "à" (huyền tone on 'a')
    let committed = match telex.process(&InputEvent::FocusOut).unwrap() {
        StateTransition::CommitAndClear(c) | StateTransition::Commit(c) => c.as_str().to_owned(),
        other => panic!("FocusOut must commit non-empty preedit: {other:?}"),
    };
    assert_eq!(committed, "à");

    // New VNI engine starts with empty preedit.
    let mut vni = StandardEngine::new(InputMethod::Vni);
    assert!(vni.preedit().is_empty(), "new engine must start empty");
    vni.process(&InputEvent::FocusIn).unwrap();
    assert!(vni.preedit().is_empty());

    type_str(&mut vni, "a2"); // "a2" → "à" in VNI
    assert_eq!(commit_str(&mut vni), "à");
}
