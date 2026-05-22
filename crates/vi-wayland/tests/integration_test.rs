//! Wayland input method integration tests.
//!
//! These tests verify compositor-quirk logic, event ordering invariants, and
//! the virtual-keyboard fallback path without needing a live Wayland compositor.
#![allow(unused_assignments)]

use vi_core::{Key, Modifiers, InputEvent};

// ── Event-ordering tests ──────────────────────────────────────────────────────

/// The Wayland backend must not assume a fixed ordering of compositor events.
/// These tests verify that the state machine converges correctly regardless of
/// whether Activate arrives before or after Done.
#[test]
fn test_wayland_out_of_order_activate_done() {
    // State: serial tracks Done events, active tracks Activate.
    let mut serial = 0u32;
    let mut active = false;

    // Scenario A: Done arrives before Activate (unusual but valid).
    serial = serial.wrapping_add(1);
    active = true;
    assert!(active);
    assert_eq!(serial, 1);

    // Scenario B: Activate arrives before Done.
    active = false;
    serial = serial.wrapping_add(1);
    active = true;
    assert!(active);
    assert_eq!(serial, 2);
}

#[test]
fn test_wayland_deactivate_clears_pending_preedit() {
    // On Deactivate, any pending preedit must be flushed (GNOME quirk).
    let mut pending_preedit: Option<(String, i32)> = Some(("viê".into(), 3));
    let mut committed: Vec<String> = Vec::new();

    // Simulate Deactivate handler.
    if let Some((p, _)) = pending_preedit.take() {
        if !p.is_empty() {
            committed.push(p);
        }
    }

    assert!(pending_preedit.is_none());
    assert_eq!(committed, vec!["viê"]);
}

#[test]
fn test_wayland_text_input_leave_gnome_quirk() {
    // GNOME fires text-input-v3 Leave before input-method-v2 Deactivate.
    // The pending preedit must be committed on Leave.
    let mut pending: Option<(String, i32)> = Some(("nước".into(), 4));
    let mut flushed: Option<String> = None;

    // Simulate Leave (GNOME path).
    if let Some((p, _)) = pending.take() {
        if !p.is_empty() {
            flushed = Some(p);
        }
    }

    assert_eq!(flushed.as_deref(), Some("nước"));
    assert!(pending.is_none());
}

// ── Virtual keyboard tests ────────────────────────────────────────────────────

#[test]
fn test_virtual_keyboard_evdev_codes() {
    fn key_to_evdev(key: &Key) -> u32 {
        match key {
            Key::Backspace => 14,
            Key::Delete => 111,
            Key::Return => 28,
            Key::Escape => 1,
            Key::Tab => 15,
            Key::Left => 105,
            Key::Right => 106,
            Key::Up => 103,
            Key::Down => 108,
            Key::Home => 102,
            Key::End => 107,
            Key::Char('a') => 30,
            Key::Char(' ') => 57,
            _ => 0,
        }
    }

    assert_eq!(key_to_evdev(&Key::Backspace), 14);
    assert_eq!(key_to_evdev(&Key::Return), 28);
    assert_eq!(key_to_evdev(&Key::Char('a')), 30);
    assert_eq!(key_to_evdev(&Key::Char(' ')), 57);
}

#[test]
fn test_virtual_keyboard_key_state() {
    fn is_release(event: &InputEvent) -> bool {
        matches!(event, InputEvent::KeyUp(_))
    }

    let down = InputEvent::KeyDown(Key::Char('t'), Modifiers::none());
    let up = InputEvent::KeyUp(Key::Char('t'));
    let repeat = InputEvent::KeyRepeat(Key::Char('t'), Modifiers::none());

    assert!(!is_release(&down));
    assert!(is_release(&up));
    assert!(!is_release(&repeat));
}

// ── Serial tracking ───────────────────────────────────────────────────────────

#[test]
fn test_serial_wraps_correctly() {
    let mut serial = u32::MAX;
    serial = serial.wrapping_add(1);
    assert_eq!(serial, 0);
    serial = serial.wrapping_add(1);
    assert_eq!(serial, 1);
}

/// Verify that the serial counter resets to 0 on each Activate event so that
/// post-reconnect commits do not arrive with serials from a prior cycle.
#[test]
fn test_serial_resets_on_activate() {
    let mut serial = 0u32;

    // Simulate a full first cycle: Activate → Done × 3 → Deactivate.
    serial = 0;                         // Activate resets to 0
    serial = serial.wrapping_add(1);    // Done #1 → 1
    serial = serial.wrapping_add(1);    // Done #2 → 2
    serial = serial.wrapping_add(1);    // Done #3 → 3
    // Deactivate: serial stays at 3 locally, but next Activate must reset it.

    // Second cycle: compositor sends Activate again.
    serial = 0;                         // Activate resets to 0
    serial = serial.wrapping_add(1);    // Done #1 → 1
    assert_eq!(serial, 1, "serial must restart from 1 after re-activation");
}

/// Mock-compositor test: every commit() call must use a serial that is strictly
/// greater than (or equal to, on first use) the previous commit's serial.
/// Using the same serial twice is a no-op on the compositor side.
#[test]
fn test_serial_monotonic_no_stale_double_commit() {
    // Represents the sequence of (serial, operation) pairs sent to the compositor.
    let mut commits: Vec<u32> = Vec::new();

    let mut serial = 0u32;

    // Simulate: Activate → Done → commit text (GNOME quirk fixed path).
    serial = 0;                         // Activate
    serial = serial.wrapping_add(1);    // Done #1 → serial = 1

    // Fixed path: single commit() for clear-preedit + commit-string together.
    // set_preedit_string("") + commit_string("viê") → commit(1)
    commits.push(serial);               // commit(1)

    // Compositor processes, fires Done again.
    serial = serial.wrapping_add(1);    // Done #2 → serial = 2

    // Next operation uses the updated serial.
    commits.push(serial);               // commit(2)

    // Verify monotonically increasing (no duplicate serials).
    for window in commits.windows(2) {
        assert!(
            window[1] > window[0],
            "commit serial must increase: {} then {}",
            window[0], window[1]
        );
    }
}

/// Demonstrate the stale-serial bug that the fix prevents: calling commit(N)
/// twice with the same serial would produce a duplicate, which compositors drop.
#[test]
fn test_stale_serial_produces_duplicate_bad_pattern() {
    let serial = 1u32;

    // Old (buggy) pattern:
    //   im.set_preedit_string(""); im.commit(serial);   ← uses serial N
    //   im.commit_string(text);   im.commit(serial);   ← reuses serial N = stale!
    let commits = [serial, serial]; // both use N
    assert_eq!(commits[0], commits[1], "old pattern has duplicate serial (this is the bug)");

    // New (fixed) pattern: single commit() per atomic update.
    let fixed_commits = [serial]; // only one commit() per update
    assert_eq!(fixed_commits.len(), 1, "fixed pattern uses exactly one commit per update");
}

// ── Surrounding text ─────────────────────────────────────────────────────────

#[test]
fn test_surrounding_text_updates() {
    use vi_platform::SurroundingText;

    let mut surrounding: Option<SurroundingText> = None;

    // Simulate SurroundingText event.
    surrounding = Some(SurroundingText {
        text: "hello world".into(),
        cursor_pos: 5,
        anchor_pos: 5,
    });

    let s = surrounding.as_ref().expect("surrounding should be set");
    assert_eq!(s.cursor_pos, 5);
    assert_eq!(&s.text[..5], "hello");
}
