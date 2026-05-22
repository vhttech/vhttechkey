//! 20 pre-built regression fixture sessions.

use vi_core::{InputEvent, InputMethod, Key, Modifiers};

use crate::replay::ReplaySession;

fn k(c: char) -> InputEvent {
    InputEvent::KeyDown(Key::Char(c), Modifiers::none())
}

fn ret() -> InputEvent {
    InputEvent::KeyDown(Key::Return, Modifiers::none())
}

fn bs() -> InputEvent {
    InputEvent::KeyDown(Key::Backspace, Modifiers::none())
}

fn esc() -> InputEvent {
    InputEvent::KeyDown(Key::Escape, Modifiers::none())
}

/// Return all 20 regression fixture sessions.
pub fn all_fixtures() -> Vec<ReplaySession> {
    vec![
        // 01 — Telex: "tôi"
        ReplaySession {
            name: "telex_toi".to_string(),
            method: InputMethod::Telex,
            events: vec![k('t'), k('o'), k('o'), k('i'), ret()],
            expected_commits: vec!["tôi".to_string()],
        },
        // 02 — Telex: basic sắc on 'a'
        ReplaySession {
            name: "telex_a_sac".to_string(),
            method: InputMethod::Telex,
            events: vec![k('a'), k('s'), ret()],
            expected_commits: vec!["á".to_string()],
        },
        // 03 — Telex: huyền on 'a'
        ReplaySession {
            name: "telex_a_huyen".to_string(),
            method: InputMethod::Telex,
            events: vec![k('a'), k('f'), ret()],
            expected_commits: vec!["à".to_string()],
        },
        // 04 — Telex: backspace rollback
        ReplaySession {
            name: "telex_backspace".to_string(),
            method: InputMethod::Telex,
            events: vec![k('t'), k('o'), k('o'), bs(), ret()],
            expected_commits: vec!["t".to_string()],
        },
        // 05 — Telex: escape clears preedit (PassThrough on Return when empty)
        ReplaySession {
            name: "telex_escape_clears".to_string(),
            method: InputMethod::Telex,
            events: vec![k('t'), k('o'), esc()],
            expected_commits: vec![],
        },
        // 06 — VNI: "tôi"
        ReplaySession {
            name: "vni_toi".to_string(),
            method: InputMethod::Vni,
            events: vec![k('t'), k('o'), k('6'), k('i'), ret()],
            expected_commits: vec!["tôi".to_string()],
        },
        // 07 — VNI: sắc on 'a'
        ReplaySession {
            name: "vni_a_sac".to_string(),
            method: InputMethod::Vni,
            events: vec![k('a'), k('1'), ret()],
            expected_commits: vec!["á".to_string()],
        },
        // 08 — VNI: huyền on 'a'
        ReplaySession {
            name: "vni_a_huyen".to_string(),
            method: InputMethod::Vni,
            events: vec![k('a'), k('2'), ret()],
            expected_commits: vec!["à".to_string()],
        },
        // 09 — VNI: nặng on 'a'
        ReplaySession {
            name: "vni_a_nang".to_string(),
            method: InputMethod::Vni,
            events: vec![k('a'), k('5'), ret()],
            expected_commits: vec!["ạ".to_string()],
        },
        // 10 — VNI: ô circumflex
        ReplaySession {
            name: "vni_o_circ".to_string(),
            method: InputMethod::Vni,
            events: vec![k('o'), k('6'), ret()],
            expected_commits: vec!["ô".to_string()],
        },
        // 11 — VIQR: "tôi"
        ReplaySession {
            name: "viqr_toi".to_string(),
            method: InputMethod::Viqr,
            events: vec![k('t'), k('o'), k('^'), k('i'), ret()],
            expected_commits: vec!["tôi".to_string()],
        },
        // 12 — VIQR: sắc on 'a'
        ReplaySession {
            name: "viqr_a_sac".to_string(),
            method: InputMethod::Viqr,
            events: vec![k('a'), k('\''), ret()],
            expected_commits: vec!["á".to_string()],
        },
        // 13 — VIQR: huyền on 'a'
        ReplaySession {
            name: "viqr_a_huyen".to_string(),
            method: InputMethod::Viqr,
            events: vec![k('a'), k('`'), ret()],
            expected_commits: vec!["à".to_string()],
        },
        // 14 — VIQR: hỏi on 'a'
        ReplaySession {
            name: "viqr_a_hoi".to_string(),
            method: InputMethod::Viqr,
            events: vec![k('a'), k('?'), ret()],
            expected_commits: vec!["ả".to_string()],
        },
        // 15 — VIQR: ngã on 'a'
        ReplaySession {
            name: "viqr_a_nga".to_string(),
            method: InputMethod::Viqr,
            events: vec![k('a'), k('~'), ret()],
            expected_commits: vec!["ã".to_string()],
        },
        // 16 — VIQR: nặng on 'a'
        ReplaySession {
            name: "viqr_a_nang".to_string(),
            method: InputMethod::Viqr,
            events: vec![k('a'), k('.'), ret()],
            expected_commits: vec!["ạ".to_string()],
        },
        // 17 — Telex: FocusOut commits preedit
        ReplaySession {
            name: "telex_focus_out_commits".to_string(),
            method: InputMethod::Telex,
            events: vec![k('a'), k('a'), InputEvent::FocusOut],
            expected_commits: vec!["â".to_string()],
        },
        // 18 — Telex: consecutive words via Return
        ReplaySession {
            name: "telex_two_words".to_string(),
            method: InputMethod::Telex,
            events: vec![k('a'), k('s'), ret(), k('o'), k('o'), ret()],
            expected_commits: vec!["á".to_string(), "ô".to_string()],
        },
        // 19 — Telex: dd → đ
        ReplaySession {
            name: "telex_d_stroke".to_string(),
            method: InputMethod::Telex,
            events: vec![k('d'), k('d'), ret()],
            expected_commits: vec!["đ".to_string()],
        },
        // 20 — Telex: Reset event clears without commit
        ReplaySession {
            name: "telex_reset_no_commit".to_string(),
            method: InputMethod::Telex,
            events: vec![k('a'), k('a'), InputEvent::Reset],
            expected_commits: vec![],
        },
    ]
}

#[cfg(test)]
mod snapshot_tests {
    use super::all_fixtures;
    use crate::replay::replay_session;
    use vi_core::{InputMethod, StandardEngine, CompositionEngine, StateTransition, Key, InputEvent};

    /// Golden snapshot for every fixture session.
    ///
    /// Committed strings are joined with `|` so the snapshot is one readable
    /// line.  An empty commit list produces an empty snapshot.
    ///
    /// To regenerate: `INSTA_UPDATE=new cargo test -p vi-testing`
    #[test]
    fn snapshot_all_fixtures() {
        for session in all_fixtures() {
            let commits = replay_session(&session);
            let output = commits.join("|");
            insta::assert_snapshot!(format!("fixture_{}", session.name), output);
        }
    }

    /// Cross-engine golden regression: replay every fixture's key events
    /// through both the Telex and VNI engines, regardless of the fixture's
    /// original method.  Any change in output for either engine is caught
    /// immediately by the snapshot diff.
    ///
    /// To regenerate: `INSTA_UPDATE=new cargo test -p vi-testing`
    #[test]
    fn golden_cross_engine() {
        for session in all_fixtures() {
            for method in [InputMethod::Telex, InputMethod::Vni] {
                let method_name = match method {
                    InputMethod::Telex => "telex",
                    InputMethod::Vni => "vni",
                    InputMethod::Viqr => "viqr",
                };
                let mut engine = StandardEngine::new(method);
                let mut commits: Vec<String> = Vec::new();
                for event in &session.events {
                    match engine.process(event).expect("engine must not fail") {
                        StateTransition::Commit(c) | StateTransition::CommitAndClear(c) => {
                            commits.push(c.as_str().to_owned());
                        }
                        StateTransition::CommitThenPreedit(c, _) => {
                            commits.push(c.as_str().to_owned());
                        }
                        _ => {}
                    }
                    if let InputEvent::KeyDown(Key::Char(ch), _) = event {
                        let _ = engine.process(&InputEvent::KeyUp(Key::Char(*ch)));
                    }
                }
                let output = commits.join("|");
                insta::assert_snapshot!(
                    format!("cross_{}_{}", method_name, session.name),
                    output
                );
            }
        }
    }
}

#[cfg(test)]
mod fixture_tests {
    use vi_core::{
        CompositionEngine, InputEvent, InputMethod, Key, Modifiers, StandardEngine,
        StateTransition,
    };

    fn kd(c: char) -> InputEvent {
        InputEvent::KeyDown(Key::Char(c), Modifiers::none())
    }

    fn ret() -> InputEvent {
        InputEvent::KeyDown(Key::Return, Modifiers::none())
    }

    fn type_and_commit(method: InputMethod, keys: &str) -> String {
        let mut engine = StandardEngine::new(method);
        for ch in keys.chars() {
            engine.process(&kd(ch)).unwrap();
            let _ = engine.process(&InputEvent::KeyUp(Key::Char(ch)));
        }
        match engine.process(&ret()).unwrap() {
            StateTransition::CommitAndClear(c) | StateTransition::Commit(c) => {
                c.as_str().to_owned()
            }
            StateTransition::PassThrough => String::new(),
            other => panic!("unexpected transition on Return: {other:?}"),
        }
    }

    /// Fixture 21 — method_switch_mid_composition
    ///
    /// Semantic observed: **drop-on-switch** — `set_method` silently clears the
    /// in-flight preedit buffer without committing.  Any composition started
    /// with the previous method is lost; the new method starts with an empty
    /// buffer.
    ///
    /// Alternative semantics (not implemented):
    ///  - *flush-on-switch*: keep the buffer, switch the method — VNI would
    ///    then interpret '6' against the existing 'o' and produce "tôi".
    ///  - *clear-on-switch*: commit preedit first ("to"), then clear — VNI
    ///    would start fresh for the remaining keys.
    #[test]
    fn method_switch_mid_composition() {
        let mut engine = StandardEngine::new(InputMethod::Telex);
        engine.process(&kd('t')).unwrap();
        engine.process(&kd('o')).unwrap();
        assert_eq!(engine.preedit().as_str(), "to");

        // Switch mid-composition: in-flight "to" is dropped without committing.
        engine.set_method(InputMethod::Vni);
        assert!(engine.preedit().is_empty(), "set_method must clear preedit");

        engine.process(&kd('6')).unwrap(); // '6' alone has no VNI rule on empty buffer
        engine.process(&kd('i')).unwrap();
        let committed = match engine.process(&ret()).unwrap() {
            StateTransition::CommitAndClear(c) | StateTransition::Commit(c) => {
                c.as_str().to_owned()
            }
            StateTransition::PassThrough => String::new(),
            other => panic!("unexpected transition on Return: {other:?}"),
        };

        // drop-on-switch: "to" was lost; post-switch VNI sees an empty buffer so
        // '6' is pushed literally, giving "6i".
        assert_eq!(
            committed, "6i",
            "drop-on-switch: in-flight 'to' is discarded; VNI starts fresh with '6i'"
        );
    }

    /// Overflow fixture — 65 key presses exhaust the 64-codepoint preedit limit.
    ///
    /// On the 65th character `StateTransition::CommitThenPreedit` must fire:
    /// the full 64-char buffer is committed and the overflow character becomes
    /// the sole entry in the new preedit.
    #[test]
    fn overflow_65_chars_triggers_commit_then_preedit() {
        const LIMIT: usize = 64;
        let mut engine = StandardEngine::new(InputMethod::Telex);

        // 'n' is inert in Telex (no vowel/tone rules), so each press is a plain push.
        // KeyUp is required between presses to prevent the repeat-guard from
        // treating consecutive same-key events as ghost presses.
        for i in 0..LIMIT {
            let t = engine.process(&kd('n')).unwrap();
            assert!(
                matches!(t, StateTransition::PreeditUpdated(_)),
                "char {i} must return PreeditUpdated before overflow"
            );
            let _ = engine.process(&InputEvent::KeyUp(Key::Char('n')));
        }
        assert_eq!(engine.preedit().as_str().chars().count(), LIMIT);

        // 65th character triggers overflow.
        let t = engine.process(&kd('n')).unwrap();
        let (committed, new_preedit) = match t {
            StateTransition::CommitThenPreedit(c, p) => {
                (c.as_str().to_owned(), p.as_str().to_owned())
            }
            other => panic!("expected CommitThenPreedit on overflow; got {other:?}"),
        };

        assert_eq!(
            committed.chars().count(),
            LIMIT,
            "committed text must be exactly the {LIMIT}-char buffer"
        );
        assert_eq!(new_preedit, "n", "overflow residual preedit must be the 65th character");
        assert_eq!(
            engine.preedit().as_str(),
            "n",
            "engine preedit must equal the overflow residual after CommitThenPreedit"
        );
    }

    /// Flexible tone fixture: tone key before vowel-form key — asw → ắ
    ///
    /// Flexible tone ordering: apply sắc first (`s`→á), then the `w` vowel-form
    /// rule fires on the already-toned vowel and carries the tone through (á→ắ).
    #[test]
    fn flexible_asw_tone_before_vowelform() {
        assert_eq!(type_and_commit(InputMethod::Telex, "asw"), "ắ");
    }

    /// Flexible tone fixture: u+horn tone-before-vowelform — ufw → ừ
    ///
    /// `f` applies huyền (ù), `w` fires uw→ư carrying the existing huyền (ù→ừ).
    #[test]
    fn flexible_ufw_horn_preserves_huyen() {
        assert_eq!(type_and_commit(InputMethod::Telex, "ufw"), "ừ");
    }

    /// Flexible tone fixture: tone switch then vowel form — efse → ế
    ///
    /// Sequence: `e`(è via f) `s`(switches to é) `e`(ee→ê carries sắc → ế).
    /// Verifies that tone switching before a vowel-form trigger is handled correctly.
    #[test]
    fn flexible_efse_tone_switch_then_e_circumflex() {
        assert_eq!(type_and_commit(InputMethod::Telex, "efse"), "ế");
    }

    /// Flexible tone fixture: compound vowel with tone switch — oosf → ồ
    ///
    /// `oo`→ô, `s`→ố (sắc), `f`→ồ (switches sắc to huyền on the ô nucleus).
    #[test]
    fn flexible_oosf_compound_vowel_tone_switch() {
        assert_eq!(type_and_commit(InputMethod::Telex, "oosf"), "ồ");
    }

    /// Flexible tone fixture: multi-char word — thoaif → thoài
    ///
    /// `f` (huyền) targets the nucleus `a` in the triphthong `oai` (rule: final
    /// glide `i` cedes tone to the preceding vowel `a`).
    #[test]
    fn flexible_thoaif_huyen_on_triphthong_nucleus() {
        assert_eq!(type_and_commit(InputMethod::Telex, "thoaif"), "thoài");
    }

    /// Flexible tone fixture: complex syllable with form-marked vowel — uowij → ượi
    ///
    /// `u`+`o`→uo, `w`→ow fires (uơ shortcut + implicit ư), `i`→ượi, `j`(nặng) → ượi.
    #[test]
    fn flexible_uowij_nang_on_form_marked_o_horn() {
        assert_eq!(type_and_commit(InputMethod::Telex, "uowij"), "ượi");
    }
}
