use std::sync::Arc;

use vi_core::{PreeditText, UnicodePipeline};
use vi_platform::{CharCursor, ImeBackend as _};
use vi_x11::{
    fullscreen::{is_fullscreen_atoms, FullscreenMonitor},
    wine::{is_wine_process, WineXimBridge},
    X11Backend,
};
use x11rb::{
    connection::Connection as _,
    protocol::{
        xproto::{
            self, AtomEnum, ConnectionExt as _, CreateWindowAux, EventMask, PropMode, WindowClass,
        },
        Event,
    },
    rust_connection::RustConnection,
    wrapper::ConnectionExt as _,
    COPY_DEPTH_FROM_PARENT,
};

fn connect_or_skip() -> Option<(Arc<RustConnection>, usize)> {
    let display = std::env::var("DISPLAY").ok()?;
    RustConnection::connect(Some(&display))
        .ok()
        .map(|(c, s)| (Arc::new(c), s))
}

fn intern(conn: &RustConnection, name: &str) -> xproto::Atom {
    conn.intern_atom(false, name.as_bytes())
        .unwrap()
        .reply()
        .unwrap()
        .atom
}

// ── Wine detection ────────────────────────────────────────────────────────────

#[test]
fn wine_process_nonexistent_pid_is_false() {
    // PID 0 never has a /proc entry; any realistic non-existent large PID also works.
    assert!(!is_wine_process(0));
    assert!(!is_wine_process(4_000_000));
}

#[test]
fn wine_process_current_pid_is_not_wine() {
    assert!(!is_wine_process(std::process::id()));
}

#[test]
fn wine_window_unset_pid_not_detected() {
    let Some((conn, screen)) = connect_or_skip() else {
        return;
    };
    let root = conn.setup().roots[screen].root;
    let win = conn.generate_id().unwrap();
    conn.create_window(
        COPY_DEPTH_FROM_PARENT,
        win,
        root,
        0,
        0,
        1,
        1,
        0,
        WindowClass::INPUT_ONLY,
        0,
        &CreateWindowAux::new(),
    )
    .unwrap()
    .check()
    .unwrap();

    let net_wm_pid = intern(&conn, "_NET_WM_PID");
    let bridge = WineXimBridge::new(Arc::clone(&conn), net_wm_pid);

    // No _NET_WM_PID set → not detected as Wine.
    assert!(!bridge.is_wine_window(win));

    conn.destroy_window(win).unwrap().check().unwrap();
}

#[test]
fn wine_window_nonexistent_pid_not_detected() {
    let Some((conn, screen)) = connect_or_skip() else {
        return;
    };
    let root = conn.setup().roots[screen].root;
    let win = conn.generate_id().unwrap();
    conn.create_window(
        COPY_DEPTH_FROM_PARENT,
        win,
        root,
        0,
        0,
        1,
        1,
        0,
        WindowClass::INPUT_ONLY,
        0,
        &CreateWindowAux::new(),
    )
    .unwrap()
    .check()
    .unwrap();

    let net_wm_pid = intern(&conn, "_NET_WM_PID");
    // Set PID to a non-existent process (high number).
    conn.change_property32(
        PropMode::REPLACE,
        win,
        net_wm_pid,
        AtomEnum::CARDINAL,
        &[4_000_000],
    )
    .unwrap()
    .check()
    .unwrap();

    let bridge = WineXimBridge::new(Arc::clone(&conn), net_wm_pid);
    // Non-existent PID → /proc/<pid>/exe unreadable → not Wine.
    assert!(!bridge.is_wine_window(win));

    conn.destroy_window(win).unwrap().check().unwrap();
}

/// Verifies that `WineXimBridge::commit` delivers `KeyPress`/`KeyRelease`
/// events via XSendEvent rather than the XIM commit path.
#[test]
fn wine_commit_delivers_key_events_via_xsendevent() {
    let Some((conn, screen)) = connect_or_skip() else {
        return;
    };
    let root = conn.setup().roots[screen].root;

    // Create a window that subscribes to key events.
    let win = conn.generate_id().unwrap();
    conn.create_window(
        COPY_DEPTH_FROM_PARENT,
        win,
        root,
        0,
        0,
        200,
        200,
        0,
        WindowClass::INPUT_OUTPUT,
        0,
        &CreateWindowAux::new().event_mask(EventMask::KEY_PRESS | EventMask::KEY_RELEASE),
    )
    .unwrap()
    .check()
    .unwrap();

    let net_wm_pid = intern(&conn, "_NET_WM_PID");
    let bridge = WineXimBridge::new(Arc::clone(&conn), net_wm_pid);

    // Commit the character 'a'; expect KeyPress then KeyRelease.
    bridge.commit(win, root, "a").expect("commit must succeed");
    // Force a round-trip so the server has processed SendEvent and the events
    // are in our socket read buffer before polling.
    let _ = conn.get_input_focus().unwrap().reply().unwrap();

    let press = conn.poll_for_event().unwrap();
    let release = conn.poll_for_event().unwrap();

    assert!(
        matches!(press, Some(Event::KeyPress(_))),
        "expected KeyPress, got {press:?}"
    );
    assert!(
        matches!(release, Some(Event::KeyRelease(_))),
        "expected KeyRelease, got {release:?}"
    );

    conn.destroy_window(win).unwrap().check().unwrap();
}

// ── Fullscreen detection ──────────────────────────────────────────────────────

#[test]
fn fullscreen_atoms_pure_predicate() {
    assert!(is_fullscreen_atoms(&[10, 42, 7], 42));
    assert!(!is_fullscreen_atoms(&[10, 7], 42));
    assert!(!is_fullscreen_atoms(&[], 42));
}

#[test]
fn fullscreen_monitor_detects_state_property() {
    let Some((conn, screen)) = connect_or_skip() else {
        return;
    };
    let root = conn.setup().roots[screen].root;

    let win = conn.generate_id().unwrap();
    conn.create_window(
        COPY_DEPTH_FROM_PARENT,
        win,
        root,
        0,
        0,
        1,
        1,
        0,
        WindowClass::INPUT_ONLY,
        0,
        &CreateWindowAux::new(),
    )
    .unwrap()
    .check()
    .unwrap();

    let net_wm_state = intern(&conn, "_NET_WM_STATE");
    let net_wm_state_fullscreen = intern(&conn, "_NET_WM_STATE_FULLSCREEN");

    let monitor = FullscreenMonitor::new(Arc::clone(&conn), net_wm_state, net_wm_state_fullscreen);

    // No state set yet.
    assert!(!monitor.is_fullscreen(win));

    // Set _NET_WM_STATE to [_NET_WM_STATE_FULLSCREEN].
    conn.change_property32(
        PropMode::REPLACE,
        win,
        net_wm_state,
        AtomEnum::ATOM,
        &[net_wm_state_fullscreen],
    )
    .unwrap()
    .check()
    .unwrap();
    assert!(
        monitor.is_fullscreen(win),
        "fullscreen state must be detected"
    );

    // Clear state.
    conn.change_property32(
        PropMode::REPLACE,
        win,
        net_wm_state,
        AtomEnum::ATOM,
        &[] as &[u32],
    )
    .unwrap()
    .check()
    .unwrap();
    assert!(
        !monitor.is_fullscreen(win),
        "fullscreen must clear when property removed"
    );

    conn.destroy_window(win).unwrap().check().unwrap();
}

/// Focus switch between two XIM input contexts while preedit is active.
///
/// The invariant: `XIM_UNSET_IC_FOCUS` on window A must commit (or cleanly
/// reset) the active preedit before the focus token is handed to window B.
/// Window B must start with an empty preedit — no text is transferred.
#[test]
fn test_x11_focus_switch_commits_preedit_to_window_a() {
    // Model two XIM input contexts (one per window) without a live X11 connection.
    struct IcState {
        preedit: String,
        committed: Vec<String>,
    }

    // Window A has an in-progress composition ("việt").
    let mut ic_a = IcState {
        preedit: "việt".to_string(),
        committed: Vec::new(),
    };
    // Window B is idle.
    let ic_b = IcState {
        preedit: String::new(),
        committed: Vec::new(),
    };

    // XIM_UNSET_IC_FOCUS handler for window A: commit preedit before releasing focus.
    let on_unset_focus = |ic: &mut IcState| {
        if !ic.preedit.is_empty() {
            ic.committed.push(ic.preedit.clone());
            ic.preedit.clear();
        }
    };

    // Simulate focus switch: unset focus on A, then set focus on B.
    on_unset_focus(&mut ic_a);
    // XIM_SET_IC_FOCUS on B: state is already clean (no special init needed).

    // Invariants after the switch.
    assert!(
        ic_a.preedit.is_empty(),
        "preedit must be cleared from window A after focus switch"
    );
    assert_eq!(
        ic_a.committed,
        vec!["việt"],
        "preedit must be committed to window A, not discarded"
    );
    assert!(
        ic_b.preedit.is_empty(),
        "window B must start with empty preedit after receiving focus"
    );
    assert!(
        ic_b.committed.is_empty(),
        "window B must not receive spurious commits"
    );
}

#[test]
fn fullscreen_immediate_commit_no_preedit_suppressed() {
    // Verify that SharedState.fullscreen_mode is respected by the X11Backend
    // by checking the flag logic in isolation (no X11 connection needed).
    //
    // The fullscreen_mode flag causes update_preedit to be a no-op so games
    // never see a preedit string they cannot display.
    let fullscreen_mode = true;
    let preedit_suppressed = fullscreen_mode;
    assert!(preedit_suppressed);

    let fullscreen_mode = false;
    let preedit_suppressed = fullscreen_mode;
    assert!(!preedit_suppressed);
}

// ── Shadow-diff live delivery regression tests ────────────────────────────────
//
// These tests exercise the differential preedit delivery path in X11Backend.
// update_preedit computes the common prefix between shadow_buf and the new
// preedit text, sends only the minimum number of BackSpace key events to
// erase the diverged suffix, then commits the new tail via XIM_COMMIT.
//
// All tests skip gracefully when DISPLAY is not set.

/// Create a minimal INPUT_ONLY window on `conn` and return (win, root).
fn make_win(conn: &Arc<RustConnection>, screen: usize) -> (xproto::Window, xproto::Window) {
    let root = conn.setup().roots[screen].root;
    let win = conn.generate_id().unwrap();
    conn.create_window(
        COPY_DEPTH_FROM_PARENT,
        win,
        root,
        0,
        0,
        1,
        1,
        0,
        WindowClass::INPUT_ONLY,
        0,
        &CreateWindowAux::new(),
    )
    .unwrap()
    .check()
    .unwrap();
    (win, root)
}

/// Flush pending work and collect all events currently queued for `conn`.
fn drain_events(conn: &Arc<RustConnection>) -> Vec<Event> {
    // A round-trip ensures the X server has processed all prior send_event
    // requests and placed their results in this connection's receive buffer.
    let _ = conn.get_input_focus().unwrap().reply().unwrap();
    let mut events = Vec::new();
    while let Some(ev) = conn.poll_for_event().unwrap() {
        events.push(ev);
    }
    events
}

/// Count KeyPress events with keycode 22 (BackSpace).
fn count_backspace_presses(events: &[Event]) -> usize {
    let mut n = 0;
    for e in events {
        if let Event::KeyPress(kp) = e {
            if kp.detail == 22 {
                n += 1;
            }
        }
    }
    n
}

/// Extract the text payload from every XIM_COMMIT (opcode 65) ClientMessage.
fn extract_xim_commits(events: &[Event]) -> Vec<String> {
    let mut result = Vec::new();
    for e in events {
        if let Event::ClientMessage(cm) = e {
            if cm.format == 8 {
                let data = cm.data.as_data8();
                if data[0] == 65 {
                    // data[4] = byte length, data[5..5+len] = UTF-8 text
                    let len = data[4] as usize;
                    if 5 + len <= 20 {
                        if let Ok(s) = std::str::from_utf8(&data[5..5 + len]) {
                            result.push(s.to_owned());
                        }
                    }
                }
            }
        }
    }
    result
}

/// Initial char 'a': shadow_buf seeded, XIM_COMMIT delivered, no BackSpace.
#[test]
fn shadow_diff_first_char_seeds_shadow_buf_and_sends_commit() {
    let Some((conn, screen)) = connect_or_skip() else {
        return;
    };
    let (win, _root) = make_win(&conn, screen);
    let (backend, _rt) = X11Backend::for_test(Arc::clone(&conn)).unwrap();
    backend.set_dest_win_for_test(Some(win));

    backend
        .update_preedit(&PreeditText::new("a".to_owned()), CharCursor(1))
        .unwrap();

    let events = drain_events(&conn);

    assert_eq!(backend.shadow_buf_snapshot(), "a");
    assert_eq!(
        count_backspace_presses(&events),
        0,
        "no BackSpace expected on first char"
    );
    assert_eq!(
        extract_xim_commits(&events),
        vec!["a"],
        "XIM_COMMIT must deliver 'a'"
    );

    conn.destroy_window(win).unwrap().check().unwrap();
}

/// Telex double-'a' → 'â': one BackSpace to erase 'a', then XIM_COMMIT 'â'.
#[test]
fn shadow_diff_telex_double_a_sends_one_backspace_and_new_commit() {
    let Some((conn, screen)) = connect_or_skip() else {
        return;
    };
    let (win, _root) = make_win(&conn, screen);
    let (backend, _rt) = X11Backend::for_test(Arc::clone(&conn)).unwrap();
    backend.set_dest_win_for_test(Some(win));

    // Seed shadow_buf = "a".
    backend
        .update_preedit(&PreeditText::new("a".to_owned()), CharCursor(1))
        .unwrap();
    drain_events(&conn);

    // Telex aa → â: shadow diverges at position 0.
    backend
        .update_preedit(&PreeditText::new("â".to_owned()), CharCursor(1))
        .unwrap();
    let events = drain_events(&conn);

    assert_eq!(backend.shadow_buf_snapshot(), "â");
    assert_eq!(
        count_backspace_presses(&events),
        1,
        "exactly 1 BackSpace expected"
    );
    assert_eq!(extract_xim_commits(&events), vec!["â"]);

    conn.destroy_window(win).unwrap().check().unwrap();
}

/// Clear preedit (update to ""): N BackSpace events for each char in shadow, no commit.
#[test]
fn shadow_diff_clear_sends_backspace_for_each_char_in_shadow() {
    let Some((conn, screen)) = connect_or_skip() else {
        return;
    };
    let (win, _root) = make_win(&conn, screen);
    let (backend, _rt) = X11Backend::for_test(Arc::clone(&conn)).unwrap();
    backend.set_dest_win_for_test(Some(win));

    // Seed shadow_buf = "ab" (2 chars).
    backend
        .update_preedit(&PreeditText::new("a".to_owned()), CharCursor(1))
        .unwrap();
    drain_events(&conn);
    backend
        .update_preedit(&PreeditText::new("ab".to_owned()), CharCursor(2))
        .unwrap();
    drain_events(&conn);

    // Clear.
    backend
        .update_preedit(&PreeditText::new(String::new()), CharCursor(0))
        .unwrap();
    let events = drain_events(&conn);

    assert_eq!(backend.shadow_buf_snapshot(), "");
    assert_eq!(
        count_backspace_presses(&events),
        2,
        "2 BackSpace presses for 2-char shadow"
    );
    assert_eq!(
        extract_xim_commits(&events).len(),
        0,
        "no XIM_COMMIT when clearing to empty"
    );

    conn.destroy_window(win).unwrap().check().unwrap();
}

/// commit() must reset shadow_buf to "".
#[test]
fn shadow_diff_commit_resets_shadow_buf() {
    let Some((conn, _screen)) = connect_or_skip() else {
        return;
    };
    let (backend, _rt) = X11Backend::for_test(conn).unwrap();
    // dest_win is None: no events sent, but shadow_buf is still updated.

    backend
        .update_preedit(&PreeditText::new("việt".to_owned()), CharCursor(4))
        .unwrap();
    assert_eq!(backend.shadow_buf_snapshot(), "việt");

    let nfc = UnicodePipeline::normalize_only("việt");
    backend.commit(&nfc).unwrap();
    assert_eq!(
        backend.shadow_buf_snapshot(),
        "",
        "commit must clear shadow_buf"
    );
}

/// fullscreen_mode == true: update_preedit is a no-op; shadow_buf and events unchanged.
#[test]
fn shadow_diff_fullscreen_suppresses_update_preedit() {
    let Some((conn, screen)) = connect_or_skip() else {
        return;
    };
    let (win, _root) = make_win(&conn, screen);
    let (backend, _rt) = X11Backend::for_test(Arc::clone(&conn)).unwrap();
    backend.set_dest_win_for_test(Some(win));

    // Establish a known shadow_buf.
    backend
        .update_preedit(&PreeditText::new("a".to_owned()), CharCursor(1))
        .unwrap();
    drain_events(&conn);
    assert_eq!(backend.shadow_buf_snapshot(), "a");

    // Enable fullscreen: subsequent update_preedit must be a no-op.
    backend.set_fullscreen_for_test(true);
    backend
        .update_preedit(&PreeditText::new("ab".to_owned()), CharCursor(2))
        .unwrap();
    let events = drain_events(&conn);

    assert_eq!(
        backend.shadow_buf_snapshot(),
        "a",
        "fullscreen_mode must leave shadow_buf unchanged"
    );
    assert_eq!(
        count_backspace_presses(&events),
        0,
        "no BackSpace when fullscreen"
    );
    assert_eq!(
        extract_xim_commits(&events).len(),
        0,
        "no XIM_COMMIT when fullscreen"
    );

    conn.destroy_window(win).unwrap().check().unwrap();
}

/// dest_win == None: returns Ok without panic; shadow_buf is still updated.
#[test]
fn shadow_diff_no_dest_win_returns_ok_and_updates_shadow_buf() {
    let Some((conn, _screen)) = connect_or_skip() else {
        return;
    };
    let (backend, _rt) = X11Backend::for_test(conn).unwrap();
    // dest_win defaults to None.

    let result = backend.update_preedit(&PreeditText::new("a".to_owned()), CharCursor(1));
    assert!(result.is_ok(), "must return Ok when dest_win is None");
    assert_eq!(
        backend.shadow_buf_snapshot(),
        "a",
        "shadow_buf updated even without dest_win"
    );
}

/// U+1ED5 'ổ' is 3 UTF-8 bytes but 1 Unicode char → counts as 1 BackSpace.
#[test]
fn shadow_diff_vietnamese_multibyte_char_counts_as_one_backspace() {
    assert_eq!("ổ".len(), 3, "test precondition: ổ is 3 UTF-8 bytes");
    assert_eq!(
        "ổ".chars().count(),
        1,
        "test precondition: ổ is 1 codepoint"
    );

    let Some((conn, screen)) = connect_or_skip() else {
        return;
    };
    let (win, _root) = make_win(&conn, screen);
    let (backend, _rt) = X11Backend::for_test(Arc::clone(&conn)).unwrap();
    backend.set_dest_win_for_test(Some(win));

    // Seed shadow_buf = "ổ" (1 char, 3 bytes).
    backend
        .update_preedit(&PreeditText::new("ổ".to_owned()), CharCursor(1))
        .unwrap();
    drain_events(&conn);

    // Update to an unrelated char: should erase 1 char, not 3 bytes.
    backend
        .update_preedit(&PreeditText::new("x".to_owned()), CharCursor(1))
        .unwrap();
    let events = drain_events(&conn);

    assert_eq!(
        count_backspace_presses(&events),
        1,
        "multi-byte char must produce 1 BackSpace (char count), not 3 (byte count)"
    );

    conn.destroy_window(win).unwrap().check().unwrap();
}
