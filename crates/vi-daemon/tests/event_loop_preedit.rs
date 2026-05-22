//! Regression tests: daemon event loop drives the ImeBackend correctly.
//!
//! Verifies that the dispatch layer calls `update_preedit` once per character
//! keystroke and defers `commit` / `commit_replacing_preedit` until a
//! word-boundary event (space) is received.
//!
//! Tests drive the engine + backend synchronously without `run_event_loop` to
//! avoid flaky async-timing issues — the dispatch logic is deterministic and
//! does not require the channel-based event loop to exercise it.
#![allow(clippy::unwrap_used)]

use std::sync::Mutex;

use vi_daemon::{
    core::{
        CompositionEngine, InputEvent, InputMethod, Key, Modifiers, NfcString, PreeditText,
        StandardEngine, StateTransition,
    },
    platform::{Capabilities, CharCursor, ImeBackend, Result as PResult, SurroundingText},
};

// ── Mock backend ──────────────────────────────────────────────────────────────

struct CountingBackend {
    update_preedit: Mutex<u32>,
    commit: Mutex<u32>,
    commit_replacing_preedit: Mutex<u32>,
    forward_key: Mutex<u32>,
    clear_preedit: Mutex<u32>,
}

impl CountingBackend {
    fn new() -> Self {
        Self {
            update_preedit: Mutex::new(0),
            commit: Mutex::new(0),
            commit_replacing_preedit: Mutex::new(0),
            forward_key: Mutex::new(0),
            clear_preedit: Mutex::new(0),
        }
    }

    fn update_preedit_count(&self) -> u32 {
        *self.update_preedit.lock().unwrap()
    }

    fn commit_count(&self) -> u32 {
        *self.commit.lock().unwrap()
    }

    fn commit_replacing_preedit_count(&self) -> u32 {
        *self.commit_replacing_preedit.lock().unwrap()
    }

    fn any_commit_count(&self) -> u32 {
        self.commit_count() + self.commit_replacing_preedit_count()
    }
}

impl ImeBackend for CountingBackend {
    fn commit(&self, _: &NfcString) -> PResult<()> {
        *self.commit.lock().unwrap() += 1;
        Ok(())
    }

    fn update_preedit(&self, _: &PreeditText, _: CharCursor) -> PResult<()> {
        *self.update_preedit.lock().unwrap() += 1;
        Ok(())
    }

    fn clear_preedit(&self) -> PResult<()> {
        *self.clear_preedit.lock().unwrap() += 1;
        Ok(())
    }

    fn forward_key(&self, _: &InputEvent) -> PResult<()> {
        *self.forward_key.lock().unwrap() += 1;
        Ok(())
    }

    fn commit_replacing_preedit(&self, _: &NfcString) -> PResult<()> {
        *self.commit_replacing_preedit.lock().unwrap() += 1;
        Ok(())
    }

    fn surrounding_text(&self) -> PResult<Option<SurroundingText>> {
        Ok(None)
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities::default()
    }
}

// ── Dispatch helper ───────────────────────────────────────────────────────────

/// Replicates the routing logic from `event_loop::dispatch_transition`.
/// Drives the backend synchronously — no async, no timing, no flakiness.
fn dispatch(
    backend: &CountingBackend,
    transition: vi_daemon::core::TransitionResult,
    event: &InputEvent,
) {
    match transition.expect("engine must not error") {
        StateTransition::PreeditUpdated(p) => {
            backend
                .update_preedit(&p, CharCursor(p.cursor_byte_offset))
                .unwrap();
        }
        StateTransition::CommitThenPassThrough(c) => {
            backend.commit_replacing_preedit(c.as_nfc()).unwrap();
            backend.forward_key(event).unwrap();
        }
        StateTransition::Commit(c) | StateTransition::CommitAndClear(c) => {
            backend.commit_replacing_preedit(c.as_nfc()).unwrap();
        }
        StateTransition::CommitThenPreedit(c, p) => {
            backend.commit(c.as_nfc()).unwrap();
            backend
                .update_preedit(&p, CharCursor(p.cursor_byte_offset))
                .unwrap();
        }
        StateTransition::Cleared => {
            backend.clear_preedit().unwrap();
        }
        StateTransition::PassThrough => {
            backend.forward_key(event).unwrap();
        }
        StateTransition::Consumed => {}
    }
}

fn key_down(ch: char) -> InputEvent {
    InputEvent::KeyDown(Key::Char(ch), Modifiers::none())
}

fn key_up(ch: char) -> InputEvent {
    InputEvent::KeyUp(Key::Char(ch))
}

// ── Tests ─────────────────────────────────────────────────────────────────────

/// Ten character keystrokes must produce exactly ten `update_preedit` calls
/// and zero commit calls.
#[test]
fn ten_char_keystrokes_produce_ten_update_preedit_calls() {
    let mut engine = StandardEngine::new(InputMethod::Telex);
    let backend = CountingBackend::new();

    for ch in ['a', 'b', 'c', 'd', 'e', 'f', 'g', 'h', 'i', 'j'] {
        let ev = key_down(ch);
        let transition = engine.process(&ev);
        dispatch(&backend, transition, &ev);
        let _ = engine.process(&key_up(ch));
    }

    assert_eq!(
        backend.update_preedit_count(),
        10,
        "10 char keystrokes must produce exactly 10 update_preedit calls"
    );
    assert_eq!(
        backend.commit_count(),
        0,
        "commit must not be called before a word-boundary event"
    );
    assert_eq!(
        backend.commit_replacing_preedit_count(),
        0,
        "commit_replacing_preedit must not be called before a word-boundary event"
    );
}

/// `commit()` and `commit_replacing_preedit()` must both be zero until a space
/// event is received, then exactly one commit must be issued.
#[test]
fn commit_zero_until_space_event() {
    let mut engine = StandardEngine::new(InputMethod::Telex);
    let backend = CountingBackend::new();

    // Type 10 chars — all produce PreeditUpdated, no commit.
    for ch in ['a', 'b', 'c', 'd', 'e', 'f', 'g', 'h', 'i', 'j'] {
        let ev = key_down(ch);
        let transition = engine.process(&ev);
        dispatch(&backend, transition, &ev);
        let _ = engine.process(&key_up(ch));
    }

    assert_eq!(
        backend.any_commit_count(),
        0,
        "no commit expected after 10 chars; update_preedit={}, commit={}+{}",
        backend.update_preedit_count(),
        backend.commit_count(),
        backend.commit_replacing_preedit_count(),
    );

    // Space triggers CommitThenPassThrough → commit_replacing_preedit + forward_key.
    let space = key_down(' ');
    let transition = engine.process(&space);
    dispatch(&backend, transition, &space);

    assert_eq!(
        backend.commit_replacing_preedit_count(),
        1,
        "space must produce exactly 1 commit_replacing_preedit call"
    );
    assert_eq!(
        backend.commit_count(),
        0,
        "direct commit() must not be called on space (commit_replacing_preedit is used)"
    );
}

/// The mock backend's `update_preedit` receives progressively longer preedit
/// text as characters are typed.  This tests that the engine builds up preedit
/// incrementally rather than committing mid-sequence.
///
/// Uses consonants that do not trigger any Telex composition rules (tone marks
/// 's','f','r','x','j' and vowel modifiers 'a','e','o','u','w' are avoided).
#[test]
fn preedit_grows_incrementally_across_ten_keystrokes() {
    let mut engine = StandardEngine::new(InputMethod::Telex);

    // Pure consonants — none trigger Telex tone or vowel-form rules.
    let chars = ['b', 'c', 'd', 'g', 'h', 'k', 'l', 'm', 'n', 'p'];
    for (i, ch) in chars.iter().enumerate() {
        let ev = key_down(*ch);
        let transition = engine.process(&ev).expect("engine must not error");
        let _ = engine.process(&key_up(*ch));

        assert!(
            matches!(transition, StateTransition::PreeditUpdated(_)),
            "char '{ch}' (index {i}) must produce PreeditUpdated"
        );
        let preedit_len = engine.preedit().as_str().chars().count();
        assert_eq!(
            preedit_len,
            i + 1,
            "preedit must have {} char(s) after typing '{ch}', got {:?}",
            i + 1,
            engine.preedit().as_str()
        );
    }

    // Preedit must contain exactly these 10 consonants in order.
    assert_eq!(
        engine.preedit().as_str(),
        "bcdghklmnp",
        "preedit must accumulate all 10 consonant chars in order"
    );
}

/// After a space event the preedit buffer must be empty: the text was committed.
#[test]
fn preedit_empty_after_space_commit() {
    let mut engine = StandardEngine::new(InputMethod::Telex);
    let backend = CountingBackend::new();

    for ch in ['v', 'i', 'e', 't'] {
        let ev = key_down(ch);
        let t = engine.process(&ev);
        dispatch(&backend, t, &ev);
        let _ = engine.process(&key_up(ch));
    }
    assert!(
        !engine.preedit().is_empty(),
        "preedit must be non-empty before space"
    );

    let space = key_down(' ');
    let t = engine.process(&space);
    dispatch(&backend, t, &space);

    assert!(
        engine.preedit().is_empty(),
        "preedit must be empty after space commits the word"
    );
    assert_eq!(
        backend.any_commit_count(),
        1,
        "exactly one commit after space"
    );
}

/// `update_preedit` is never called for non-composition keystrokes such as
/// modifier-only or navigation keys.
#[test]
fn non_composition_keys_do_not_call_update_preedit() {
    let mut engine = StandardEngine::new(InputMethod::Telex);
    let backend = CountingBackend::new();

    // These keys must not extend preedit on an empty buffer.
    let non_compose = [
        InputEvent::KeyDown(Key::Left, Modifiers::none()),
        InputEvent::KeyDown(Key::Right, Modifiers::none()),
        InputEvent::KeyDown(Key::Home, Modifiers::none()),
        InputEvent::KeyDown(Key::End, Modifiers::none()),
    ];

    for ev in &non_compose {
        let t = engine.process(ev);
        dispatch(&backend, t, ev);
    }

    assert_eq!(
        backend.update_preedit_count(),
        0,
        "navigation keys on empty preedit must not call update_preedit; \
         calls recorded: {}",
        backend.update_preedit_count()
    );
}

/// Backspace while preedit has content calls `update_preedit` (shrinks preedit),
/// not `commit` or `forward_key`.
#[test]
fn backspace_mid_word_calls_update_preedit_not_commit() {
    let mut engine = StandardEngine::new(InputMethod::Telex);
    let backend = CountingBackend::new();

    // Build up 4-char preedit.
    for ch in ['a', 'b', 'c', 'd'] {
        let ev = key_down(ch);
        let t = engine.process(&ev);
        dispatch(&backend, t, &ev);
        let _ = engine.process(&key_up(ch));
    }
    let preedit_calls_before = backend.update_preedit_count();

    // Backspace: preedit shrinks → update_preedit.
    let bs = InputEvent::KeyDown(Key::Backspace, Modifiers::none());
    let t = engine.process(&bs);
    dispatch(&backend, t, &bs);

    assert_eq!(
        backend.update_preedit_count(),
        preedit_calls_before + 1,
        "backspace on non-empty preedit must call update_preedit once more"
    );
    assert_eq!(backend.commit_count(), 0, "backspace must not call commit");
    assert_eq!(
        backend.commit_replacing_preedit_count(),
        0,
        "backspace must not call commit_replacing_preedit"
    );
}
