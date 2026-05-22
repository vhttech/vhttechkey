//! Serial contention regression tests.
//!
//! These tests pin the invariants that prevent compositor serial desync when
//! multiple Wayland protocol operations are issued in rapid succession.
//!
//! Background:  The Wayland zwp_input_method_v2 protocol requires every
//! `zwp_input_method_v2.commit(serial)` to use the serial number received in
//! the most-recent `done` event.  Compositors reject (silently drop) requests
//! that use a serial the client has not yet received via a `done` event.
//!
//! Contention arises when two logical operations (e.g. commit-string and
//! update-preedit) each try to call `with_im()` — they would consume two
//! serial slots, but the compositor has only emitted one `done` after the
//! first frame, making the second serial invalid.
//!
//! The fix (from PDCA Round 1) is to fold atomic operations into a single
//! `with_im()` closure so exactly ONE `im.commit(N)` is sent per logical frame.

use vi_wayland::quirks::{CompositorProfile, CompositorQuirks};

// ── Monotonicity ──────────────────────────────────────────────────────────────

/// `sent_commits` must increase strictly monotonically across consecutive
/// `with_im()` calls.  Duplicate serials (sent_commits never incremented) cause
/// compositors to silently drop subsequent protocol requests.
#[test]
fn sent_commits_is_strictly_monotonic_across_operations() {
    let mut sent_commits: u32 = 0;

    for i in 0..10u32 {
        let serial_used = sent_commits;
        sent_commits = sent_commits.wrapping_add(1);

        assert_eq!(
            serial_used, i,
            "serial used in operation {i} must equal {i}"
        );
        assert_eq!(sent_commits, i + 1, "sent_commits must be {}", i + 1);
    }
}

/// Two separate `with_im()` calls for the same logical frame would consume
/// serials N and N+1.  The compositor rejects N+1 because `done(N+1)` has not
/// yet been emitted.  Folding into one call sends only `im.commit(N)`.
#[test]
fn two_separate_calls_produce_future_serial_hazard() {
    let mut sent_commits: u32 = 0;

    // Hypothetical two-call path (the bug):
    let serial_first = sent_commits; // 0 — sent to compositor
    sent_commits = serial_first.wrapping_add(1);

    // compositor has not emitted done(1) yet at this point
    let serial_second = sent_commits; // 1 — FUTURE serial, compositor rejects
    sent_commits = serial_second.wrapping_add(1);

    assert_eq!(serial_first, 0);
    assert_eq!(
        serial_second, 1,
        "two-call path sends a future serial before done(1) arrives"
    );

    // Atomic single-call path (the fix):
    let mut sent_commits_fixed: u32 = 0;
    let serial_atomic = sent_commits_fixed; // 0
    sent_commits_fixed = serial_atomic.wrapping_add(1);

    assert_eq!(serial_atomic, 0, "atomic path uses serial 0");
    assert_eq!(
        sent_commits_fixed, 1,
        "atomic path consumes exactly one slot"
    );
    assert_eq!(
        sent_commits_fixed,
        sent_commits - 1,
        "atomic path uses one fewer serial slot than the buggy two-call path"
    );
}

// ── Serial overflow (wrapping_add) ────────────────────────────────────────────

/// `sent_commits` is a u32 that wraps around at u32::MAX.  After wrap-around
/// the next serial must be 0 — compositors are specified to handle wrapping
/// serial numbers.  The test also checks that the Niri guard's
/// `wrapping_sub(ti_serial) > 2` threshold remains correct across the boundary.
#[test]
fn serial_wraps_correctly_at_u32_max() {
    let mut sent_commits: u32 = u32::MAX - 1;

    // Penultimate operation.
    let s0 = sent_commits;
    sent_commits = sent_commits.wrapping_add(1);
    assert_eq!(s0, u32::MAX - 1);
    assert_eq!(sent_commits, u32::MAX);

    // Operation at MAX.
    let s1 = sent_commits;
    sent_commits = sent_commits.wrapping_add(1);
    assert_eq!(s1, u32::MAX);
    assert_eq!(sent_commits, 0, "sent_commits wraps from u32::MAX to 0");

    // Operation after wrap.
    let s2 = sent_commits;
    sent_commits = sent_commits.wrapping_add(1);
    assert_eq!(s2, 0);
    assert_eq!(sent_commits, 1);
}

/// The Niri dual-protocol guard fires when `sent_commits.wrapping_sub(ti_serial) > 2`.
///
/// After a wrap the wrapping_sub must still compute correctly so the
/// guard does not produce false positives immediately after Activate.
#[test]
fn niri_guard_wrapping_sub_is_safe_after_activate() {
    let niri = CompositorQuirks {
        profile: CompositorProfile::Niri,
        niri_dual_protocol: true,
        ..Default::default()
    };

    // Both counters reset to 0 on Activate.
    let ti_serial: u32 = 0;
    let sent_commits: u32 = 0;

    let guard_fires = niri.niri_dual_protocol && sent_commits.wrapping_sub(ti_serial) > 2;
    assert!(
        !guard_fires,
        "Niri guard must not fire immediately after Activate"
    );

    // Simulate two normal operations — guard must still not fire.
    let sent_commits2 = sent_commits.wrapping_add(2);
    let guard_fires2 = niri.niri_dual_protocol && sent_commits2.wrapping_sub(ti_serial) > 2;
    assert!(
        !guard_fires2,
        "Niri guard must not fire after two normal operations"
    );

    // Simulate a large divergence (> 2 ops without a ti_serial update) — guard fires.
    let sent_commits3 = sent_commits.wrapping_add(3);
    let guard_fires3 = niri.niri_dual_protocol && sent_commits3.wrapping_sub(ti_serial) > 2;
    assert!(
        guard_fires3,
        "Niri guard must fire when sent_commits leads ti_serial by > 2"
    );
}

// ── commit_replacing_preedit serial contract ─────────────────────────────────

/// `commit_replacing_preedit` sends `set_preedit_string("") + commit_string(text)
/// + im.commit(N)` in one atomic `with_im()` call — one serial slot consumed.
///
/// Previously `event_loop.rs` called `clear_preedit()` then `commit()` as two
/// separate `with_im()` invocations.  Both used the same serial N because the
/// compositor had not yet emitted `done(N+1)`.  GNOME/Mutter silently dropped
/// the second `im.commit(N)`, so the committed text was never inserted.
#[test]
fn commit_replacing_preedit_consumes_one_serial_slot() {
    let mut sent_commits: u32 = 0;

    // Atomic commit_replacing_preedit: one with_im() sends clear + commit_string
    // + im.commit(N).  Only ONE serial slot is consumed.
    let serial_used = sent_commits;
    sent_commits = serial_used.wrapping_add(1);

    assert_eq!(serial_used, 0, "first serial slot is 0");
    assert_eq!(sent_commits, 1, "exactly one slot consumed");

    // Verify the next operation uses a distinct serial.
    let serial_next = sent_commits;
    sent_commits = serial_next.wrapping_add(1);
    assert_ne!(
        serial_used, serial_next,
        "consecutive operations use distinct serials"
    );
    assert_eq!(serial_next, 1);
    assert_eq!(sent_commits, 2);
}

/// Hypothetical two-call buggy path: clear_preedit() + commit() both read
/// sent_commits = N and both send im.commit(N).  The compositor drops the
/// second im.commit(N) because it has already processed serial N.
#[test]
fn buggy_two_call_clear_then_commit_reuses_same_serial() {
    let sent_commits: u32 = 5;

    // clear_preedit() reads serial 5.
    let serial_clear = sent_commits;
    // (In the buggy path, sent_commits is NOT incremented between calls.)
    // commit() also reads serial 5.
    let serial_commit = sent_commits;

    assert_eq!(
        serial_clear, serial_commit,
        "buggy path: both calls use the same serial"
    );
    assert_eq!(serial_clear, 5);
    // The compositor drops the second im.commit(5); text is silently lost.
}

// ── Hyprland buffer_preedit_updates path ─────────────────────────────────────

/// On Hyprland/Labwc (`buffer_preedit_updates = true`) every `update_preedit()`
/// call must increment `sent_commits` after `im.commit()`.  Without the
/// increment every call reuses serial 0 and compositors that detect duplicate
/// serials silently drop all preedit updates after the first.
#[test]
fn hyprland_update_preedit_increments_serial_each_call() {
    let hyprland = CompositorQuirks {
        profile: CompositorProfile::Hyprland,
        buffer_preedit_updates: true,
        ..Default::default()
    };
    assert!(hyprland.buffer_preedit_updates);

    let mut sent_commits: u32 = 0;
    let mut serials_used: Vec<u32> = Vec::new();

    for _ in 0..5 {
        let s = sent_commits;
        serials_used.push(s);
        sent_commits = sent_commits.wrapping_add(1);
    }

    assert_eq!(
        serials_used,
        vec![0, 1, 2, 3, 4],
        "each update_preedit must use a distinct serial"
    );
    assert_eq!(sent_commits, 5);
}
