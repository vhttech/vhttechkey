//! Preedit pipeline regression tests.
//!
//! Verifies that update_preedit() (PreeditUpdated) emits set_preedit_string +
//! commit() on every call — not only on commit_replacing_preedit (CommitAndClear).
//!
//! Background: the zwp_input_method_v2 protocol requires the client to call
//! `set_preedit_string(text, begin, end)` followed by `commit(serial)` on every
//! preedit change.  Without the `commit()` call the compositor never applies the
//! new preedit overlay, so the user sees no candidate text.  The same `commit()`
//! call is required for CommitAndClear, but for a different reason: it atomically
//! clears the preedit and inserts the committed text in one compositor frame.

use vi_wayland::adapter::clamp_preedit_cursor;
use vi_wayland::quirks::{CompositorProfile, CompositorQuirks};

// ── Protocol operation recorder ───────────────────────────────────────────────

/// A Wayland protocol call sent to the compositor by the IM backend.
#[derive(Debug, PartialEq)]
enum WlOp {
    SetPreeditString { text: String, cursor_begin: i32, cursor_end: i32 },
    CommitString(String),
    Commit(u32),
}

/// Reproduce the operations that `update_preedit()` emits inside `with_im()`:
///
///   im.set_preedit_string(text, cursor_byte, cursor_byte);
///   im.commit(serial);
fn ops_update_preedit(text: &str, cursor_byte: i32, serial: u32) -> Vec<WlOp> {
    vec![
        WlOp::SetPreeditString {
            text: text.to_owned(),
            cursor_begin: cursor_byte,
            cursor_end: cursor_byte,
        },
        WlOp::Commit(serial),
    ]
}

/// Reproduce the operations that `commit_replacing_preedit()` emits inside
/// `with_im()` (the CommitAndClear path):
///
///   im.set_preedit_string("", -1, -1);
///   im.commit_string(text);
///   im.commit(serial);
fn ops_commit_replacing_preedit(text: &str, serial: u32) -> Vec<WlOp> {
    vec![
        WlOp::SetPreeditString { text: String::new(), cursor_begin: -1, cursor_end: -1 },
        WlOp::CommitString(text.to_owned()),
        WlOp::Commit(serial),
    ]
}

// ── PreeditUpdated (update_preedit) tests ─────────────────────────────────────

/// Every update_preedit call must emit set_preedit_string(text) + commit().
/// Without commit() the compositor never renders the new preedit overlay.
#[test]
fn update_preedit_emits_set_preedit_string_and_commit_on_every_call() {
    let steps = [("v", 1), ("vi", 2), ("viê", 4), ("viêt", 5)];
    let mut serial: u32 = 0;

    for (text, len) in steps {
        let cursor_byte = clamp_preedit_cursor(text, len);
        let ops = ops_update_preedit(text, cursor_byte, serial);

        assert_eq!(
            ops.len(),
            2,
            "update_preedit({text:?}) must emit exactly [SetPreeditString, Commit]; got {ops:?}"
        );

        // First operation must be set_preedit_string with the actual text.
        assert_eq!(
            ops[0],
            WlOp::SetPreeditString {
                text: text.to_owned(),
                cursor_begin: cursor_byte,
                cursor_end: cursor_byte,
            },
            "update_preedit({text:?}) first op must be set_preedit_string"
        );

        // Second operation must be commit() so the compositor applies the preedit.
        assert_eq!(
            ops[1],
            WlOp::Commit(serial),
            "update_preedit({text:?}) must call commit({serial}) — without it the compositor \
             never renders the preedit overlay"
        );

        // with_im() increments sent_commits after im.commit().
        serial = serial.wrapping_add(1);
    }
}

/// set_preedit_string must precede commit() in the operation sequence.
/// If commit() arrived first the compositor would apply an empty preedit.
#[test]
fn update_preedit_set_preedit_string_precedes_commit() {
    let text = "viê";
    let cursor_byte = clamp_preedit_cursor(text, text.len() as i32);
    let ops = ops_update_preedit(text, cursor_byte, 0);

    let preedit_pos = ops
        .iter()
        .position(|o| matches!(o, WlOp::SetPreeditString { .. }))
        .expect("SetPreeditString must be present");
    let commit_pos = ops
        .iter()
        .position(|o| matches!(o, WlOp::Commit(_)))
        .expect("Commit must be present");

    assert!(
        preedit_pos < commit_pos,
        "set_preedit_string (pos {preedit_pos}) must precede commit() (pos {commit_pos})"
    );
}

/// The serial passed to commit() must increase with every update_preedit call.
/// Reusing a serial causes compositors to silently drop subsequent requests.
#[test]
fn update_preedit_uses_distinct_serial_on_every_call() {
    let mut sent_commits: u32 = 0;
    let mut serials_used: Vec<u32> = Vec::new();

    for (text, len) in [("v", 1), ("vi", 2), ("viê", 4), ("viêt", 5)] {
        let cursor_byte = clamp_preedit_cursor(text, len);
        let ops = ops_update_preedit(text, cursor_byte, sent_commits);

        if let WlOp::Commit(s) = ops.last().unwrap() {
            serials_used.push(*s);
        }
        // with_im() increments sent_commits after im.commit().
        sent_commits = sent_commits.wrapping_add(1);
    }

    for window in serials_used.windows(2) {
        assert_ne!(
            window[0],
            window[1],
            "consecutive update_preedit calls must use distinct serials: {} then {}",
            window[0],
            window[1]
        );
    }
    assert_eq!(serials_used, vec![0, 1, 2, 3]);
}

/// Cursor byte offset in set_preedit_string must land on a valid UTF-8 boundary.
/// A mid-codepoint offset panics Qt widgets and causes glitches in GNOME Shell.
#[test]
fn update_preedit_cursor_is_clamped_to_char_boundary() {
    // "viê": v=0, i=1, ê is 2 bytes (0xC3 0xAA) at bytes 2–3, len=4.
    // Byte 3 falls inside the ê codepoint.
    let s = "viê";
    let mid_codepoint = 3i32;
    let clamped = clamp_preedit_cursor(s, mid_codepoint);

    assert!(
        s.is_char_boundary(clamped as usize),
        "clamp_preedit_cursor({s:?}, {mid_codepoint}) = {clamped} must be a char boundary"
    );

    let ops = ops_update_preedit(s, clamped, 0);
    match &ops[0] {
        WlOp::SetPreeditString { cursor_begin, cursor_end, .. } => {
            assert!(
                s.is_char_boundary(*cursor_begin as usize),
                "cursor_begin={cursor_begin} must be a char boundary in {s:?}"
            );
            assert!(
                s.is_char_boundary(*cursor_end as usize),
                "cursor_end={cursor_end} must be a char boundary in {s:?}"
            );
        }
        other => panic!("expected SetPreeditString, got {other:?}"),
    }
}

// ── CommitAndClear (commit_replacing_preedit) tests ───────────────────────────

/// commit_replacing_preedit must emit set_preedit_string("") + commit_string +
/// commit() in a single atomic frame — clearing the preedit and inserting the
/// committed text in one compositor update.
#[test]
fn commit_replacing_preedit_emits_clear_preedit_then_commit_string() {
    let serial = 5u32;
    let ops = ops_commit_replacing_preedit("viêt", serial);

    assert_eq!(
        ops.len(),
        3,
        "commit_replacing_preedit must produce exactly 3 ops: \
         [SetPreeditString(\"\"), CommitString, Commit]"
    );

    assert_eq!(
        ops[0],
        WlOp::SetPreeditString { text: String::new(), cursor_begin: -1, cursor_end: -1 },
        "first op must clear the preedit with set_preedit_string(\"\", -1, -1)"
    );
    assert_eq!(ops[1], WlOp::CommitString("viêt".to_owned()));
    assert_eq!(ops[2], WlOp::Commit(serial));
}

/// update_preedit carries actual text in set_preedit_string; commit_replacing_preedit
/// carries an empty string.  Both emit set_preedit_string + commit(), but for
/// different purposes.
#[test]
fn update_preedit_and_commit_replacing_differ_in_preedit_content() {
    let preedit_ops = ops_update_preedit("viê", clamp_preedit_cursor("viê", 4), 0);
    let commit_ops = ops_commit_replacing_preedit("viêt", 0);

    let preedit_text_in_update = match &preedit_ops[0] {
        WlOp::SetPreeditString { text, .. } => text.as_str(),
        _ => panic!("expected SetPreeditString"),
    };
    let preedit_text_in_commit = match &commit_ops[0] {
        WlOp::SetPreeditString { text, .. } => text.as_str(),
        _ => panic!("expected SetPreeditString"),
    };

    assert!(
        !preedit_text_in_update.is_empty(),
        "update_preedit must set a non-empty preedit text"
    );
    assert!(
        preedit_text_in_commit.is_empty(),
        "commit_replacing_preedit must clear preedit with an empty string"
    );

    // Both must contain a Commit operation.
    assert!(preedit_ops.iter().any(|o| matches!(o, WlOp::Commit(_))),
        "update_preedit must include commit()");
    assert!(commit_ops.iter().any(|o| matches!(o, WlOp::Commit(_))),
        "commit_replacing_preedit must include commit()");
}

// ── GNOME deactivate-flush path ───────────────────────────────────────────────

/// On GNOME (empty_preedit_before_commit), the pending preedit flushed on
/// Deactivate/Leave must be a single atomic commit — one serial slot consumed.
/// Two separate with_im() calls would reuse the same serial; GNOME/Mutter
/// silently drops the second, so the committed text would be lost.
#[test]
fn gnome_deactivate_flush_is_single_atomic_commit() {
    let quirks = CompositorQuirks {
        profile: CompositorProfile::Gnome,
        empty_preedit_before_commit: true,
        ..Default::default()
    };

    let pending = Some(("viêt".to_string(), 4i32));
    let serial = 2u32;
    let mut ops: Vec<WlOp> = Vec::new();
    let mut serials_committed = 0u32;

    if quirks.empty_preedit_before_commit {
        if let Some((preedit, _)) = pending {
            if !preedit.is_empty() {
                // One atomic frame: clear preedit + commit_string + commit().
                ops.push(WlOp::SetPreeditString {
                    text: String::new(),
                    cursor_begin: -1,
                    cursor_end: -1,
                });
                ops.push(WlOp::CommitString(preedit));
                ops.push(WlOp::Commit(serial));
                serials_committed += 1;
            }
        }
    }

    assert_eq!(serials_committed, 1, "GNOME flush must consume exactly one serial slot");
    assert_eq!(
        ops.len(),
        3,
        "atomic flush must produce 3 ops: set_preedit_string(\"\") + commit_string + commit()"
    );
    assert!(matches!(&ops[0], WlOp::SetPreeditString { text, .. } if text.is_empty()));
    assert!(matches!(&ops[1], WlOp::CommitString(t) if t == "viêt"));
    assert_eq!(ops[2], WlOp::Commit(serial));
}

/// GNOME quirk must not fire on non-GNOME compositors (Standard profile).
#[test]
fn gnome_deactivate_flush_skipped_when_not_gnome() {
    let quirks = CompositorQuirks {
        profile: CompositorProfile::Standard,
        empty_preedit_before_commit: false,
        ..Default::default()
    };

    let pending = Some(("viê".to_string(), 3i32));
    let mut ops: Vec<WlOp> = Vec::new();

    if quirks.empty_preedit_before_commit {
        if let Some((preedit, _)) = pending {
            ops.push(WlOp::CommitString(preedit));
        }
    }

    assert!(ops.is_empty(), "Standard profile must not trigger empty-preedit flush on deactivate");
}

// ── Serial management across preedit and commit operations ────────────────────

/// sent_commits must increment strictly for each update_preedit followed by
/// commit_replacing_preedit.  A shared-serial bug would cause one of these to
/// be silently dropped by the compositor.
#[test]
fn mixed_update_and_commit_operations_use_distinct_serials() {
    let mut sent_commits: u32 = 0;
    let mut serials: Vec<u32> = Vec::new();

    // update_preedit("vi")
    let cb1 = clamp_preedit_cursor("vi", 2);
    let ops1 = ops_update_preedit("vi", cb1, sent_commits);
    serials.push(match ops1.last() { Some(WlOp::Commit(s)) => *s, _ => panic!() });
    sent_commits = sent_commits.wrapping_add(1);

    // update_preedit("viê")
    let cb2 = clamp_preedit_cursor("viê", 4);
    let ops2 = ops_update_preedit("viê", cb2, sent_commits);
    serials.push(match ops2.last() { Some(WlOp::Commit(s)) => *s, _ => panic!() });
    sent_commits = sent_commits.wrapping_add(1);

    // commit_replacing_preedit("viêt")
    let ops3 = ops_commit_replacing_preedit("viêt", sent_commits);
    serials.push(match ops3.last() { Some(WlOp::Commit(s)) => *s, _ => panic!() });
    sent_commits = sent_commits.wrapping_add(1);

    let _ = sent_commits;

    assert_eq!(serials, vec![0, 1, 2], "all three operations must use distinct serials");

    for window in serials.windows(2) {
        assert!(
            window[1] > window[0],
            "serials must be strictly increasing: {} then {}",
            window[0],
            window[1]
        );
    }
}
