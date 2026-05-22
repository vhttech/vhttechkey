//! Focus-switch stress helpers and tests.
//!
//! Verifies that rapid focus-in / focus-out cycles produce no stuck preedit,
//! no double-commit, and that the engine state stays consistent.

use vi_core::{
    CompositionEngine, InputEvent, InputMethod, Key, Modifiers, StandardEngine, StateTransition,
};

/// Run `iterations` cycles of: type one character, FocusIn, FocusOut.
///
/// Returns the total number of commits emitted.  On every cycle a single
/// character is in the preedit before FocusOut, so each FocusOut must
/// produce exactly one commit.  The caller may assert `result == iterations`.
pub fn focus_stress_run(method: InputMethod, iterations: usize) -> usize {
    let mut engine = StandardEngine::new(method);
    let mut commits = 0usize;

    for _ in 0..iterations {
        let _ = engine.process(&InputEvent::KeyDown(Key::Char('a'), Modifiers::none()));
        let _ = engine.process(&InputEvent::FocusIn);
        match engine
            .process(&InputEvent::FocusOut)
            .unwrap_or(StateTransition::Consumed)
        {
            StateTransition::Commit(_) | StateTransition::CommitAndClear(_) => {
                commits += 1;
            }
            _ => {}
        }
        debug_assert!(
            engine.preedit().is_empty(),
            "preedit must be clear after FocusOut"
        );
    }

    commits
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn focus_stress_1000_telex() {
        assert_eq!(focus_stress_run(InputMethod::Telex, 1_000), 1_000);
    }

    #[test]
    fn focus_stress_1000_vni() {
        assert_eq!(focus_stress_run(InputMethod::Vni, 1_000), 1_000);
    }

    #[test]
    fn focus_stress_1000_viqr() {
        assert_eq!(focus_stress_run(InputMethod::Viqr, 1_000), 1_000);
    }

    /// FocusOut on an already-empty preedit must return Consumed, not a commit.
    #[test]
    fn focus_out_on_empty_preedit_is_consumed() {
        let mut engine = StandardEngine::new(InputMethod::Telex);
        for _ in 0..1_000 {
            let r = engine
                .process(&InputEvent::FocusOut)
                .expect("must not fail");
            assert_eq!(
                r,
                StateTransition::Consumed,
                "FocusOut on empty must be Consumed"
            );
        }
    }

    /// FocusIn on an idle engine must always return Consumed.
    #[test]
    fn focus_in_on_idle_engine_is_consumed() {
        let mut engine = StandardEngine::new(InputMethod::Telex);
        for _ in 0..1_000 {
            let r = engine.process(&InputEvent::FocusIn).expect("must not fail");
            assert_eq!(
                r,
                StateTransition::Consumed,
                "FocusIn on idle must be Consumed"
            );
        }
    }

    /// Multi-char preedit (Telex "too" → "tô"): FocusOut commits exactly once
    /// per cycle and leaves preedit empty.
    #[test]
    fn multi_char_preedit_focus_out_commits_once() {
        let mut engine = StandardEngine::new(InputMethod::Telex);
        let mut total = 0usize;

        for _ in 0..1_000 {
            for ch in ['t', 'o', 'o'] {
                engine
                    .process(&InputEvent::KeyDown(Key::Char(ch), Modifiers::none()))
                    .unwrap();
            }
            match engine.process(&InputEvent::FocusOut).unwrap() {
                StateTransition::Commit(_) | StateTransition::CommitAndClear(_) => total += 1,
                _ => {}
            }
            assert!(
                engine.preedit().is_empty(),
                "preedit must be clear after FocusOut"
            );
        }

        assert_eq!(
            total, 1_000,
            "expected one commit per FocusOut on non-empty preedit"
        );
    }
}
