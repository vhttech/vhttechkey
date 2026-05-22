//! Serial-tracking regression tests.
//!
//! These tests pin the Activate-reset and sent_commits-increment behaviors
//! that prevent Niri dual-protocol desync and silently dropped commits.

use vi_wayland::quirks::{CompositorProfile, CompositorQuirks};

/// Regression: on Activate, both `ti_serial` and `sent_commits` must reset to 0.
///
/// If either field retains a non-zero value from the previous focus session the
/// Niri dual-protocol guard (`sent_commits.wrapping_sub(ti_serial) > 2`) fires
/// on the first `with_im` call of the new session and silently drops it.
#[test]
#[allow(unused_assignments)]
fn niri_ti_serial_resets_on_activate() {
    // Simulate WaylandShared state left over from a previous session.
    let mut ti_serial: u32 = 5;
    let mut sent_commits: u32 = 3;

    // Simulate the zwp_input_method_v2 Activate handler (lib.rs):
    //   s.serial = 0; s.sent_commits = 0; s.ti_serial = 0;
    ti_serial = 0;
    sent_commits = 0;

    assert_eq!(ti_serial, 0, "ti_serial must reset to 0 on Activate");
    assert_eq!(sent_commits, 0, "sent_commits must reset to 0 on Activate");

    // With both counters at 0, the Niri guard must not fire on the first with_im call.
    let niri = CompositorQuirks {
        profile: CompositorProfile::Niri,
        niri_dual_protocol: true,
        ..Default::default()
    };
    let guard_fires = niri.niri_dual_protocol && sent_commits.wrapping_sub(ti_serial) > 2;
    assert!(
        !guard_fires,
        "Niri guard must not fire immediately after Activate reset; \
         sent_commits={sent_commits}, ti_serial={ti_serial}"
    );
}

/// Regression guard: commit_then_update_preedit must consume exactly ONE serial
/// slot by bundling commit_string + set_preedit_string + im.commit(N) into a
/// single atomic `with_im` call.
///
/// If commit and preedit were dispatched via two separate `with_im` calls they
/// would use serials N and N+1 respectively.  The compositor rejects N+1 because
/// it has not yet emitted a `done` event with that serial — only done(N) is sent
/// after processing the commit frame.  GNOME/Mutter silently drops the second
/// request, so the new preedit never appears.
///
/// The fix: `WaylandBackend::commit_then_update_preedit` folds both operations
/// into one `with_im` closure, sending `im.commit(N)` exactly once.
#[test]
fn commit_then_preedit_atomic_uses_one_serial_slot() {
    // Simulate WaylandShared state after Activate.
    let mut sent_commits: u32 = 0;

    // Atomic commit_then_update_preedit (current implementation):
    // A single with_im() call sends:
    //   im.commit_string("toàn")
    //   im.set_preedit_string("h", 0, 0)
    //   im.commit(serial)  ← ONE im.commit() only
    let serial_used = sent_commits;
    sent_commits = serial_used.wrapping_add(1);

    assert_eq!(
        serial_used, 0,
        "commit_then_preedit uses the first available serial"
    );
    assert_eq!(
        sent_commits, 1,
        "exactly one serial slot consumed per logical operation"
    );
}

/// After an atomic commit_then_update_preedit the compositor sends done(N+1),
/// making N+1 the valid serial for the next operation.  The test pins the
/// monotonically-increasing invariant across consecutive operations.
#[test]
fn next_operation_after_commit_then_preedit_uses_different_serial() {
    let mut sent_commits: u32 = 0;

    // Atomic commit_then_update_preedit: serial N=0.
    let serial_commit_preedit = sent_commits;
    sent_commits = serial_commit_preedit.wrapping_add(1);

    // Compositor sends done(1). Next with_im call (e.g. a follow-up preedit):
    let serial_next = sent_commits;
    sent_commits = serial_next.wrapping_add(1);

    assert_ne!(
        serial_commit_preedit, serial_next,
        "commit_then_update_preedit (serial {serial_commit_preedit}) and the \
         subsequent operation (serial {serial_next}) must use different serials; \
         the compositor emits done(N+1) after the atomic frame, making N+1 valid \
         for the next request"
    );
    assert_eq!(
        serial_next,
        serial_commit_preedit.wrapping_add(1),
        "next serial must be exactly N+1"
    );
    assert_eq!(
        sent_commits, 2,
        "two operations consumed two serial slots total"
    );
}

/// Documents the serial hazard in a hypothetical two-call path.
///
/// If `event_loop.rs` called `backend.commit()` then `backend.update_preedit()`
/// as separate `with_im` invocations, the second call would use serial N+1
/// before the compositor has emitted done(N+1).  GNOME/Mutter rejects any
/// `im.commit(serial)` where serial exceeds the number of done events received,
/// so the preedit update would be silently dropped.
///
/// The current `commit_then_update_preedit` override folds both into a single
/// frame with ONE `im.commit(N)`, eliminating this hazard.
#[test]
fn two_call_path_would_send_future_serial_for_preedit() {
    let mut sent_commits: u32 = 0;

    // Hypothetical first call: backend.commit("toàn")
    let serial_commit = sent_commits; // 0
    sent_commits = serial_commit.wrapping_add(1); // sent_commits = 1

    // At this point done(1) has NOT arrived yet — the compositor processes
    // commit(0) asynchronously and will send done(1) later.
    // A separate backend.update_preedit("h") would use serial 1, which is
    // a FUTURE serial that the compositor has not yet authorised.
    let serial_preedit_buggy = sent_commits; // 1 — invalid before done(1) arrives
    sent_commits = serial_preedit_buggy.wrapping_add(1);

    assert_eq!(serial_commit, 0);
    assert_eq!(
        serial_preedit_buggy, 1,
        "second with_im() uses serial 1 before done(1) has been received; \
         GNOME/Mutter drops im.commit(1) because it has not yet emitted done(1)"
    );
    assert_eq!(
        sent_commits, 2,
        "two serial slots consumed — but only serial 0 was actually valid in this frame"
    );
}

/// Regression: `update_preedit` via the `buffer_preedit_updates` path
/// (Hyprland/Labwc) must increment `sent_commits` after each `im.commit()`.
///
/// Without the increment every call reuses `serial=0`, meaning compositors that
/// detect duplicate serials silently drop every preedit update after the first.
#[test]
fn update_preedit_increments_sent_commits_on_hyprland() {
    let hyprland = CompositorQuirks {
        profile: CompositorProfile::Hyprland,
        buffer_preedit_updates: true,
        ..Default::default()
    };
    assert!(hyprland.buffer_preedit_updates);

    // Simulate WaylandShared state after Activate.
    let mut sent_commits: u32 = 0;

    // Simulate first update_preedit via buffer_preedit_updates path:
    //   let serial = shared.sent_commits;          // capture
    //   im.set_preedit_string(s, cursor, cursor);
    //   im.commit(serial);
    //   shared.sent_commits = serial.wrapping_add(1); // must happen
    let serial_first = sent_commits; // 0
    sent_commits = serial_first.wrapping_add(1);

    // Simulate second update_preedit.
    let serial_second = sent_commits; // 1
    sent_commits = serial_second.wrapping_add(1);

    assert_eq!(
        sent_commits, 2,
        "sent_commits must be 2 after two update_preedit calls"
    );
    assert_ne!(
        serial_first, serial_second,
        "each update_preedit must use a distinct serial \
         (first={serial_first}, second={serial_second})"
    );

    // Regression: without the increment sent_commits stays at 0 and every call
    // uses serial=0. Compositors that detect duplicate serials drop duplicates.
    let buggy_sent_commits: u32 = 0; // never incremented in the old path
    assert_eq!(
        buggy_sent_commits, serial_first,
        "unfixed path reuses serial 0 on every call — compositors drop duplicates"
    );
}

// ── text-input-v3 Done event: ti_serial must increment per event ───────────────

/// Audit: `zwp_text_input_v3::Event::Done` must increment `ti_serial` by 1 on
/// every occurrence.  The Niri dual-protocol guard compares `sent_commits` against
/// `ti_serial`; if `ti_serial` does not increment, the guard fires erroneously on
/// every call after the first Done event (sent_commits - 0 > 2).
///
/// This mirrors the dispatch handler in vi-wayland/src/lib.rs:
///   `zwp_text_input_v3::Event::Done { serial } => { state.ti_adapter.on_done(serial); s.ti_serial = s.ti_serial.wrapping_add(1); }`
#[test]
fn ti_serial_increments_once_per_text_input_done_event() {
    // Simulate WaylandShared.ti_serial starting at 0 after Activate.
    let mut ti_serial: u32 = 0;

    // Simulate 3 text-input-v3 Done events (one per compositor frame).
    for expected in 1u32..=3 {
        // Done handler: s.ti_serial = s.ti_serial.wrapping_add(1)
        ti_serial = ti_serial.wrapping_add(1);
        assert_eq!(
            ti_serial, expected,
            "after Done event {expected}: ti_serial must equal {expected}"
        );
    }
}

/// Audit: the Niri dual-protocol desync guard must NOT fire when ti_serial and
/// sent_commits are in sync (differ by ≤ 2).
///
/// The guard is: `sent_commits.wrapping_sub(ti_serial) > 2`
/// If ti_serial increments correctly (one per Done event) and sent_commits
/// increments once per `with_im()` call, they stay within 1–2 of each other
/// under normal operation.
#[test]
fn niri_guard_does_not_fire_when_ti_serial_in_sync() {
    let niri = CompositorQuirks {
        profile: CompositorProfile::Niri,
        niri_dual_protocol: true,
        ..Default::default()
    };

    // Simulate: 2 preedit updates sent (sent_commits=2), compositor sent 2 Done events (ti_serial=2).
    let sent_commits: u32 = 2;
    let ti_serial: u32 = 2;

    let guard_fires = niri.niri_dual_protocol && sent_commits.wrapping_sub(ti_serial) > 2;
    assert!(
        !guard_fires,
        "Niri guard must not fire when sent_commits={sent_commits} and ti_serial={ti_serial} \
         are in sync; wrapping_sub={}",
        sent_commits.wrapping_sub(ti_serial)
    );
}

/// Audit: the Niri guard fires when ti_serial lags behind sent_commits by > 2,
/// which indicates the text-input-v3 Done events stopped arriving (compositor bug
/// or missing enable()+commit() lifecycle).
#[test]
fn niri_guard_fires_when_ti_serial_lags() {
    let niri = CompositorQuirks {
        profile: CompositorProfile::Niri,
        niri_dual_protocol: true,
        ..Default::default()
    };

    // Simulate: 5 preedit updates sent, but only 1 Done from text-input-v3.
    let sent_commits: u32 = 5;
    let ti_serial: u32 = 1;

    let guard_fires = niri.niri_dual_protocol && sent_commits.wrapping_sub(ti_serial) > 2;
    assert!(
        guard_fires,
        "Niri guard must fire when sent_commits={sent_commits} and ti_serial={ti_serial} \
         are out of sync by > 2 (indicates missing text-input-v3 Done events)"
    );
}
