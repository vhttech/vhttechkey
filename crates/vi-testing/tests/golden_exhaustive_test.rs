//! Extend the 216 golden cases with proptest: arbitrary lowercase prefix
//! followed by the golden key sequence must still produce the expected output.
#![allow(clippy::unwrap_used)]

use proptest::prelude::*;
use vi_core::{CompositionEngine, InputEvent, Key, Modifiers, StandardEngine, StateTransition};
use vi_testing::golden::all_216;

fn key(c: char) -> InputEvent {
    InputEvent::KeyDown(Key::Char(c), Modifiers::none())
}

fn ret() -> InputEvent {
    InputEvent::KeyDown(Key::Return, Modifiers::none())
}

proptest! {
    /// Type an arbitrary lowercase prefix, commit it, then type the golden
    /// key sequence and commit again.  The second commit must equal the golden
    /// expected output.
    #[test]
    fn golden_with_arbitrary_prefix(prefix in "[a-z]{0,5}") {
        let cases = all_216();

        for case in &cases {
            let mut engine = StandardEngine::new(case.method);

            // 1. Type the prefix.
            for ch in prefix.chars() {
                engine.process(&key(ch)).unwrap();
                let _ = engine.process(&InputEvent::KeyUp(Key::Char(ch)));
            }
            // 2. Commit (or flush) the prefix so the buffer is clean.
            let _ = engine.process(&ret());

            // 3. Type the golden key sequence.
            for ch in case.input.chars() {
                engine.process(&key(ch)).unwrap();
                let _ = engine.process(&InputEvent::KeyUp(Key::Char(ch)));
            }

            // 4. Commit and check against the golden expected output.
            match engine.process(&ret()).unwrap() {
                StateTransition::CommitAndClear(c) | StateTransition::Commit(c) => {
                    prop_assert_eq!(
                        c.as_str(),
                        case.expected,
                        "golden case '{}' failed with prefix {:?}: input={:?}",
                        case.description,
                        prefix,
                        case.input,
                    );
                }
                StateTransition::PassThrough => {
                    // Empty golden input after flat-tone vowel: check preedit is empty.
                    prop_assert!(
                        engine.preedit().is_empty(),
                        "preedit unexpectedly non-empty after PassThrough for '{}' with prefix {:?}",
                        case.description,
                        prefix,
                    );
                }
                other => {
                    prop_assert!(
                        false,
                        "unexpected transition {:?} for '{}' with prefix {:?}",
                        other,
                        case.description,
                        prefix,
                    );
                }
            }
        }
    }
}
