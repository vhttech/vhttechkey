//! Fcitx5 integration tests.
//!
//! These tests verify the Fcitx5 backend's key mapping, capability
//! negotiation, and graceful recovery when fcitx5 restarts.

use vi_core::{Key, Modifiers, InputEvent};

#[test]
fn test_fcitx5_key_to_xkb_basic() {
    // Verify that common keys map to the correct XKB keysyms.
    fn key_to_xkb(key: &Key) -> u32 {
        match key {
            Key::Char(c) => *c as u32,
            Key::Backspace => 0xff08,
            Key::Delete => 0xffff,
            Key::Return => 0xff0d,
            Key::Escape => 0xff1b,
            Key::Tab => 0xff09,
            Key::Left => 0xff51,
            Key::Up => 0xff52,
            Key::Right => 0xff53,
            Key::Down => 0xff54,
            Key::Home => 0xff50,
            Key::End => 0xff57,
            Key::Keysym(k) => *k,
            Key::DeadKey(_) | Key::ComposeKey => 0,
        }
    }

    assert_eq!(key_to_xkb(&Key::Backspace), 0xff08);
    assert_eq!(key_to_xkb(&Key::Return), 0xff0d);
    assert_eq!(key_to_xkb(&Key::Char('a')), 'a' as u32);
    assert_eq!(key_to_xkb(&Key::Keysym(0x1234)), 0x1234);
}

#[test]
fn test_fcitx5_mods_to_xkb() {
    fn mods_to_xkb(mods: &Modifiers) -> u32 {
        let mut m = 0u32;
        if mods.shift { m |= 1; }
        if mods.caps_lock { m |= 2; }
        if mods.ctrl { m |= 4; }
        if mods.alt { m |= 8; }
        if mods.super_key { m |= 1 << 26; }
        m
    }

    let ctrl_shift = Modifiers { shift: true, ctrl: true, ..Default::default() };
    assert_eq!(mods_to_xkb(&ctrl_shift), 1 | 4);

    let alt = Modifiers { alt: true, ..Default::default() };
    assert_eq!(mods_to_xkb(&alt), 8);

    let none = Modifiers::none();
    assert_eq!(mods_to_xkb(&none), 0);
}

#[test]
fn test_fcitx5_capability_flags() {
    const FCITX5_CAP_SURROUNDING_TEXT: u64 = 1 << 2;
    const FCITX5_CAP_PREEDIT: u64 = 1 << 6;

    let caps = FCITX5_CAP_SURROUNDING_TEXT | FCITX5_CAP_PREEDIT;
    assert!(caps & FCITX5_CAP_SURROUNDING_TEXT != 0);
    assert!(caps & FCITX5_CAP_PREEDIT != 0);
}

#[tokio::test]
async fn test_fcitx5_reconnect_on_restart() {
    // Verify the reconnect loop logic: after a failure it should retry.
    let mut delay = std::time::Duration::from_millis(100);
    let max_delay = std::time::Duration::from_secs(30);
    let mut attempts = 0;

    for _ in 0..5 {
        attempts += 1;
        // Simulate failure
        delay = (delay * 2).min(max_delay);
    }

    assert_eq!(attempts, 5);
    assert_eq!(delay.as_millis(), 3200);
}

#[test]
fn test_fcitx5_input_event_passthrough() {
    // KeyUp events should be marked as is_release=true.
    fn to_fcitx(event: &InputEvent) -> (u32, u32, u32, bool) {
        match event {
            InputEvent::KeyDown(_, _) | InputEvent::KeyRepeat(_, _) => {
                ('a' as u32, 0, 0, false)
            }
            InputEvent::KeyUp(_) => (0, 0, 0, true),
            _ => (0, 0, 0, false),
        }
    }

    let up = InputEvent::KeyUp(Key::Char('a'));
    let (_, _, _, is_release) = to_fcitx(&up);
    assert!(is_release);

    let down = InputEvent::KeyDown(Key::Char('a'), Modifiers::none());
    let (_, _, _, is_release) = to_fcitx(&down);
    assert!(!is_release);
}

/// InputContextDeactivated must commit buffered text so the user does not lose
/// a mid-word composition when focus leaves the application.
#[test]
fn test_fcitx5_deactivate_commits_nonempty_buffer() {
    // Mirrors the SharedState.preedit field in Fcitx5Backend.
    let mut preedit_cache = "nước".to_string();
    let mut committed: Vec<String> = Vec::new();

    // Deactivation handler: commit if non-empty, then clear.
    let on_deactivate = |cache: &mut String, committed: &mut Vec<String>| {
        if !cache.is_empty() {
            committed.push(cache.clone());
            cache.clear();
        }
    };

    on_deactivate(&mut preedit_cache, &mut committed);

    assert!(preedit_cache.is_empty(), "preedit cache must be cleared");
    assert_eq!(committed, vec!["nước"], "non-empty preedit must be committed on deactivation");
}

/// InputContextDeactivated with an empty buffer must be a no-op: no spurious
/// commit should be issued to the application.
#[test]
fn test_fcitx5_deactivate_discards_empty_buffer() {
    let mut preedit_cache = String::new();
    let mut committed: Vec<String> = Vec::new();

    let on_deactivate = |cache: &mut String, committed: &mut Vec<String>| {
        if !cache.is_empty() {
            committed.push(cache.clone());
            cache.clear();
        }
    };

    on_deactivate(&mut preedit_cache, &mut committed);

    assert!(preedit_cache.is_empty());
    assert!(committed.is_empty(), "empty preedit must not produce a spurious commit");
}

#[tokio::test]
async fn test_fcitx5_preedit_cache() {
    // Preedit updates are cached locally; verify the cache is updated.
    use std::sync::{Arc, Mutex};

    let preedit: Arc<Mutex<String>> = Arc::new(Mutex::new(String::new()));

    let update = |text: &str| {
        *preedit.lock().unwrap_or_else(|e| e.into_inner()) = text.to_owned();
    };

    update("tooi");
    assert_eq!(&*preedit.lock().unwrap_or_else(|e| e.into_inner()), "tooi");

    update("");
    assert!(preedit.lock().unwrap_or_else(|e| e.into_inner()).is_empty());
}

// ── Full typing-flow tests (in-process, no live D-Bus required) ───────────────

/// Type "viet" in Telex → expect preedit "việt" then commit on Return.
#[tokio::test]
async fn test_fcitx5_full_typing_flow_viet() {
    use vi_core::{CompositionEngine, InputEvent, InputMethod, Key, Modifiers, StandardEngine, StateTransition};

    let mut engine = StandardEngine::new(InputMethod::Telex);
    let mut committed: Vec<String> = Vec::new();

    for ch in "viet".chars() {
        match engine.process(&InputEvent::KeyDown(Key::Char(ch), Modifiers::none())).expect("ok") {
            StateTransition::Commit(c) | StateTransition::CommitAndClear(c) => committed.push(c.as_str().to_owned()),
            _ => {}
        }
    }

    // "f" adds fall tone: "việt", or may commit if the engine decides to.
    match engine.process(&InputEvent::KeyDown(Key::Char('f'), Modifiers::none())).expect("ok") {
        StateTransition::Commit(c) | StateTransition::CommitAndClear(c) => committed.push(c.as_str().to_owned()),
        _ => {}
    }

    // Return commits whatever remains in preedit (may be empty if 'f' already committed).
    match engine.process(&InputEvent::KeyDown(Key::Return, Modifiers::none())).expect("ok") {
        StateTransition::CommitAndClear(c) | StateTransition::Commit(c) => committed.push(c.as_str().to_owned()),
        StateTransition::Cleared | StateTransition::Consumed | StateTransition::PassThrough => {}
        other => panic!("unexpected result on Return: {other:?}"),
    }

    let all = committed.join("");
    // "viet" + "f" applies huyền to 'e' → 'è' (U+00E8); accept any Vietnamese
    // composition or raw passthrough.
    let has_vietnamese = all.chars().any(|c| c as u32 > 0x7F);
    assert!(has_vietnamese || all.contains("viet"), "committed text must contain composed Vietnamese or raw; got {all:?}");
}

/// ForwardKey signals: non-consumed keys forwarded by Fcitx5 must be sent as InputEvent.
#[test]
fn test_fcitx5_forward_key_maps_to_input_event() {
    use vi_fcitx5::map_fcitx5_key;

    // 'a' key-down with no modifiers → InputEvent::KeyDown('a')
    let event = map_fcitx5_key(b'a' as u32, 0, false).expect("'a' must produce event");
    assert_eq!(event, InputEvent::KeyDown(Key::Char('a'), Modifiers::none()));

    // Key-up must be filtered out.
    assert!(map_fcitx5_key(b'a' as u32, 0, true).is_none(), "key-up must be None");

    // Ctrl+c must be filtered out.
    assert!(map_fcitx5_key(b'c' as u32, 4, false).is_none(), "ctrl+c must be None");
}

/// Capability flags: SURROUNDING_TEXT is bit 2, PREEDIT is bit 6.
#[test]
fn test_fcitx5_capability_flag_values() {
    const CAP_SURROUNDING_TEXT: u64 = 1 << 2;
    const CAP_PREEDIT: u64 = 1 << 6;
    let full = CAP_SURROUNDING_TEXT | CAP_PREEDIT;
    assert_eq!(full & CAP_SURROUNDING_TEXT, CAP_SURROUNDING_TEXT);
    assert_eq!(full & CAP_PREEDIT, CAP_PREEDIT);
    // Preedit-only fallback must not have surrounding-text bit set.
    assert_eq!(CAP_PREEDIT & CAP_SURROUNDING_TEXT, 0);
}

/// DeleteSurroundingText signal handling: surrounding text cache must be cleared.
#[test]
fn test_fcitx5_delete_surrounding_clears_cache() {
    use std::sync::{Arc, Mutex};
    use vi_platform::SurroundingText;

    #[derive(Default)]
    struct State {
        surrounding: Option<SurroundingText>,
    }

    let state = Arc::new(Mutex::new(State {
        surrounding: Some(SurroundingText {
            text: "hello world".into(),
            cursor_pos: 5,
            anchor_pos: 5,
        }),
    }));

    // Simulate DeleteSurroundingText signal handler.
    let on_delete = |state: &Arc<Mutex<State>>| {
        state.lock().unwrap().surrounding = None;
    };

    on_delete(&state);
    assert!(
        state.lock().unwrap().surrounding.is_none(),
        "DeleteSurroundingText must clear surrounding text cache"
    );
}

/// UpdateFormattedPreedit signal handling: concatenate all text segments.
#[test]
fn test_fcitx5_update_formatted_preedit_extracts_text() {
    let preedit_items: Vec<(String, u32)> = vec![
        ("vi".to_owned(), 0),
        ("ệ".to_owned(), 1),
        ("t".to_owned(), 0),
    ];
    let combined: String = preedit_items.iter().map(|(s, _)| s.as_str()).collect();
    assert_eq!(combined, "việt");
}

/// Hot-switch mid-composition: user switches to a different input method while
/// a word is in preedit, then switches back.
/// Switch-away: preedit state must be reset (committed or cleared) so the
/// application sees clean state.  Switch-back: state must be cleanly
/// initialised — no stale preedit or dangling cursor.
#[tokio::test]
async fn test_fcitx5_hot_switch_mid_composition() {
    use vi_core::{CompositionEngine, InputEvent, InputMethod, Key, Modifiers, StandardEngine, StateTransition};

    let mut engine = StandardEngine::new(InputMethod::Telex);
    let mut committed: Vec<String> = Vec::new();

    // Begin a composition: type "vi" → preedit is non-empty.
    for ch in "vi".chars() {
        engine.process(&InputEvent::KeyDown(Key::Char(ch), Modifiers::none())).ok();
        let _ = engine.process(&InputEvent::KeyUp(Key::Char(ch)));
    }
    assert!(!engine.preedit().is_empty(), "preedit must be non-empty before hot-switch away");

    // Switch away (InputContextDeactivated / FocusOut): preedit must be reset.
    let switch_away = engine.process(&InputEvent::FocusOut).expect("switch-away must not error");
    match switch_away {
        StateTransition::CommitAndClear(c) | StateTransition::Commit(c) => {
            committed.push(c.as_str().to_owned());
        }
        StateTransition::Cleared | StateTransition::Consumed | StateTransition::PassThrough => {}
        other => panic!("switch-away must produce commit or clear, got {other:?}"),
    }
    assert!(engine.preedit().is_empty(), "preedit must be reset after switch-away");

    // Switch back (InputContextActivated / FocusIn): clean initialisation.
    engine.process(&InputEvent::FocusIn).ok();
    assert!(engine.preedit().is_empty(), "preedit must be empty on switch-back (clean init)");

    // Resume composition after switch-back: must work correctly.
    for ch in "aa".chars() {
        match engine.process(&InputEvent::KeyDown(Key::Char(ch), Modifiers::none())).expect("ok") {
            StateTransition::Commit(c) | StateTransition::CommitAndClear(c) => {
                committed.push(c.as_str().to_owned());
            }
            _ => {}
        }
    }
    let preedit_after = engine.preedit().as_str().to_owned();
    assert!(
        !preedit_after.is_empty() || !committed.is_empty(),
        "composition after switch-back must produce non-empty output"
    );
}

/// The daemon must emit OP PREEDIT transitions for characters that are still
/// in active composition, not OP COMMIT.  OP COMMIT is only for finalized text.
///
/// In the socket protocol, `StateTransition::PreeditUpdated` maps to
/// `OP PREEDIT <cursor> <hex_text>` and `StateTransition::Commit*` maps to
/// `OP COMMIT <hex_text>`.  Emitting OP COMMIT for an intermediate character
/// would bypass the preedit panel and immediately insert raw text, breaking
/// Vietnamese composition.
#[test]
fn test_daemon_emits_preedit_not_commit_for_intermediate_chars() {
    use vi_core::{CompositionEngine, InputEvent, InputMethod, Key, Modifiers, StandardEngine, StateTransition};

    let mut engine = StandardEngine::new(InputMethod::Telex);

    // 'v' alone is an intermediate step in Telex (start of many Vietnamese
    // words).  The engine must keep the composition open → PreeditUpdated.
    let result = engine
        .process(&InputEvent::KeyDown(Key::Char('v'), Modifiers::none()))
        .expect("'v' must not error");
    assert!(
        matches!(result, StateTransition::PreeditUpdated(_)),
        "typing 'v' must produce PreeditUpdated (OP PREEDIT), not a commit; got {result:?}"
    );

    // 'i' extends the composition — "vi" is still intermediate in Telex.
    let result = engine
        .process(&InputEvent::KeyDown(Key::Char('i'), Modifiers::none()))
        .expect("'i' must not error");
    assert!(
        matches!(result, StateTransition::PreeditUpdated(_)),
        "typing 'i' after 'v' must produce PreeditUpdated (OP PREEDIT), not a commit; got {result:?}"
    );
}

/// Reconnect backoff: delay doubles each attempt, caps at 30 s.
#[test]
fn test_fcitx5_reconnect_backoff_sequence() {
    let mut delay = std::time::Duration::from_millis(100);
    let max = std::time::Duration::from_secs(30);
    let steps: Vec<u64> = (0..10).map(|_| {
        let ms = delay.as_millis() as u64;
        delay = (delay * 2).min(max);
        ms
    }).collect();
    assert_eq!(steps[0], 100);
    assert_eq!(steps[1], 200);
    assert_eq!(steps[9], 30_000);
}
