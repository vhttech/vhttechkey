#![allow(clippy::unwrap_used)]
//! Proptest stress: 10 000 random key sequences verify the composition engine
//! never panics, always produces valid NFC output, and is deterministic
//! (identical input sequence → identical output sequence).

use proptest::prelude::*;
use unicode_normalization::UnicodeNormalization;
use vi_core::{
    CompositionEngine, InputEvent, InputMethod, Key, Modifiers, StandardEngine, StateTransition,
};

fn arb_key() -> impl Strategy<Value = Key> {
    prop_oneof![
        (0x20u32..=0x7Eu32).prop_map(|c| Key::Char(char::from_u32(c).unwrap())),
        Just(Key::Backspace),
        Just(Key::Return),
        Just(Key::Escape),
        Just(Key::Tab),
    ]
}

fn arb_event() -> impl Strategy<Value = InputEvent> {
    prop_oneof![
        (arb_key(), Just(Modifiers::none())).prop_map(|(k, m)| InputEvent::KeyDown(k, m)),
        arb_key().prop_map(|k| InputEvent::KeyRepeat(k, Modifiers::none())),
        Just(InputEvent::FocusIn),
        Just(InputEvent::FocusOut),
        Just(InputEvent::Reset),
    ]
}

fn arb_event_seq() -> impl Strategy<Value = Vec<InputEvent>> {
    prop::collection::vec(arb_event(), 1..=20)
}

/// Encode a method as a 0-2 integer so the strategy is simple and Copy-safe.
fn arb_method_idx() -> impl Strategy<Value = u8> {
    0u8..3u8
}

fn method_from_idx(idx: u8) -> InputMethod {
    match idx % 3 {
        0 => InputMethod::Telex,
        1 => InputMethod::Vni,
        _ => InputMethod::Viqr,
    }
}

fn assert_nfc(s: &str) {
    let nfc: String = s.nfc().collect();
    assert_eq!(s, nfc.as_str(), "string is not NFC: {s:?}");
}

fn run_engine(method: InputMethod, events: &[InputEvent]) -> Vec<String> {
    let mut engine = StandardEngine::new(method);
    let mut log = Vec::with_capacity(events.len());
    for event in events {
        let label = match engine.process(event) {
            Ok(StateTransition::PreeditUpdated(p)) => format!("preedit:{}", p.as_str()),
            Ok(StateTransition::CommitAndClear(c)) => format!("commit_clear:{}", c.as_str()),
            Ok(StateTransition::Commit(c)) => format!("commit:{}", c.as_str()),
            Ok(StateTransition::CommitThenPreedit(c, _p)) => {
                format!("commit_then_preedit:{}", c.as_str())
            }
            Ok(StateTransition::CommitThenPassThrough(c)) => {
                format!("commit_passthrough:{}", c.as_str())
            }
            Ok(StateTransition::Consumed) => "consumed".to_owned(),
            Ok(StateTransition::PassThrough) => "passthrough".to_owned(),
            Ok(StateTransition::Cleared) => "cleared".to_owned(),
            Err(_) => "error".to_owned(),
        };
        log.push(label);
    }
    log
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(10_000))]

    /// Every random event sequence must not panic, and every string the engine
    /// emits (preedit or committed) must be valid NFC.
    #[test]
    fn engine_never_panics_always_nfc(
        method_idx in arb_method_idx(),
        events in arb_event_seq(),
    ) {
        let method = method_from_idx(method_idx);
        let mut engine = StandardEngine::new(method);
        for event in &events {
            match engine.process(event) {
                Ok(StateTransition::PreeditUpdated(p)) => assert_nfc(p.as_str()),
                Ok(StateTransition::CommitAndClear(c) | StateTransition::Commit(c)) => {
                    assert_nfc(c.as_str());
                }
                Ok(_) | Err(_) => {}
            }
            assert_nfc(engine.preedit().as_str());
        }
    }

    /// Replaying the same event sequence on a fresh engine must produce the
    /// identical sequence of transitions — the engine is a pure state machine.
    #[test]
    fn engine_output_is_deterministic(
        method_idx in arb_method_idx(),
        events in arb_event_seq(),
    ) {
        let method = method_from_idx(method_idx);
        let first  = run_engine(method, &events);
        let second = run_engine(method, &events);
        prop_assert_eq!(first, second, "engine produced different output for identical input");
    }
}
