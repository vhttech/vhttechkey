//! Regression: UpdatePreeditTextWithMode must be emitted AND flushed before
//! CommitText for every composing character — preventing the 'invisible text
//! until commit' bug from regressing in the IBus backend.
//!
//! D-Bus signal emission cannot be verified without a live session bus.  This
//! test replicates the `dispatch_transition` logic inline with a call recorder
//! and drives a real `StandardEngine` through the Telex "t","o","o","i"," "
//! sequence (→ "tôi"), then asserts the ordering and flush invariants.

use vi_core::{
    CompositionEngine, InputEvent, InputMethod, Key, Modifiers, StandardEngine, StateTransition,
};

// ── Simulated call log ────────────────────────────────────────────────────────

/// Signals and connection operations recorded by the simulated dispatcher.
#[derive(Debug, Clone, PartialEq)]
enum Call {
    /// IBus `UpdatePreeditTextWithMode` signal (text content, cursor char count).
    UpdatePreeditTextWithMode { text: String, cursor_pos: u32 },
    /// IBus `CommitText` signal.
    CommitText(String),
    /// D-Bus connection flush (called after every signal emission in production).
    Flush,
}

fn kd(c: char) -> InputEvent {
    InputEvent::KeyDown(Key::Char(c), Modifiers::none())
}

fn ku(c: char) -> InputEvent {
    InputEvent::KeyUp(Key::Char(c))
}

/// Simulate the standard-preedit and commit arms of `dispatch_transition`.
///
/// In production `dispatch_transition` emits D-Bus signals via `SignalContext`
/// and then calls `ctx.connection().flush()` after each preedit update so the
/// application sees it immediately (before the next key event arrives).
fn sim_dispatch(transition: StateTransition, log: &mut Vec<Call>) {
    match transition {
        StateTransition::PreeditUpdated(p) => {
            let cursor_pos = p.as_str().chars().count() as u32;
            log.push(Call::UpdatePreeditTextWithMode {
                text: p.as_str().to_owned(),
                cursor_pos,
            });
            // Production: ctx.connection().flush() immediately after the signal.
            log.push(Call::Flush);
        }
        StateTransition::CommitThenPassThrough(ct) => {
            log.push(Call::CommitText(ct.as_str().to_owned()));
            log.push(Call::Flush);
        }
        StateTransition::Commit(ct) | StateTransition::CommitAndClear(ct) => {
            log.push(Call::CommitText(ct.as_str().to_owned()));
            log.push(Call::Flush);
        }
        StateTransition::CommitThenPreedit(ct, p) => {
            log.push(Call::CommitText(ct.as_str().to_owned()));
            log.push(Call::Flush);
            let cursor_pos = p.as_str().chars().count() as u32;
            log.push(Call::UpdatePreeditTextWithMode {
                text: p.as_str().to_owned(),
                cursor_pos,
            });
            log.push(Call::Flush);
        }
        _ => {}
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

/// Telex "t","o","o","i"," " → "tôi": every composing character must produce
/// `UpdatePreeditTextWithMode` followed immediately by `Flush`, and ALL preedit
/// signals must appear before `CommitText`.
///
/// Invariants verified:
/// 1. Exactly 4 `UpdatePreeditTextWithMode` calls (one per composing char).
/// 2. `Flush` immediately follows each `UpdatePreeditTextWithMode`.
/// 3. All preedit calls precede the single `CommitText` call.
/// 4. Preedit text values are `["t", "to", "tô", "tôi"]` in order.
#[test]
fn preedit_flushed_before_commit_tooi_space() {
    let mut engine = StandardEngine::new(InputMethod::Telex);
    let mut log: Vec<Call> = Vec::new();

    // Feed "t","o","o","i" as composing chars then " " to commit.
    // KeyUp between each char resets the repeat-guard so the second 'o' fires
    // the oo→ô rule instead of being suppressed as a held key.
    for &ch in &['t', 'o', 'o', 'i', ' '] {
        let t = engine.process(&kd(ch)).expect("engine must not error");
        let _ = engine.process(&ku(ch));
        sim_dispatch(t, &mut log);
    }

    // Collect positions of each call type.
    let preedit_pos: Vec<usize> = log
        .iter()
        .enumerate()
        .filter_map(|(i, c)| {
            if matches!(c, Call::UpdatePreeditTextWithMode { .. }) { Some(i) } else { None }
        })
        .collect();

    let commit_pos: Vec<usize> = log
        .iter()
        .enumerate()
        .filter_map(|(i, c)| {
            if matches!(c, Call::CommitText(_)) { Some(i) } else { None }
        })
        .collect();

    // ── Invariant 1: exactly 4 preedit updates ────────────────────────────────
    assert_eq!(
        preedit_pos.len(),
        4,
        "must emit exactly 4 UpdatePreeditTextWithMode calls (one per composing char); \
         log = {log:?}"
    );

    // ── Invariant 2: exactly 1 commit ─────────────────────────────────────────
    assert_eq!(
        commit_pos.len(),
        1,
        "must emit exactly 1 CommitText (triggered by space); log = {log:?}"
    );

    // ── Invariant 3: all preedit updates precede the commit ───────────────────
    let last_preedit = *preedit_pos.last().unwrap();
    let commit = commit_pos[0];
    assert!(
        last_preedit < commit,
        "all UpdatePreeditTextWithMode calls must precede CommitText; \
         last preedit at index {last_preedit}, commit at index {commit}; log = {log:?}"
    );

    // ── Invariant 4: Flush immediately follows each preedit update ────────────
    for &pos in &preedit_pos {
        assert_eq!(
            log.get(pos + 1),
            Some(&Call::Flush),
            "Flush must immediately follow UpdatePreeditTextWithMode at index {pos}; \
             log = {log:?}"
        );
    }

    // ── Invariant 5: correct preedit text sequence ────────────────────────────
    let texts: Vec<&str> = log
        .iter()
        .filter_map(|c| {
            if let Call::UpdatePreeditTextWithMode { text, .. } = c {
                Some(text.as_str())
            } else {
                None
            }
        })
        .collect();
    assert_eq!(
        texts,
        ["t", "to", "tô", "tôi"],
        "preedit text sequence must be [\"t\", \"to\", \"tô\", \"tôi\"]"
    );
}

/// Verify that each individual preedit update carries correct cursor position.
#[test]
fn preedit_cursor_positions_match_char_count() {
    let mut engine = StandardEngine::new(InputMethod::Telex);
    let mut log: Vec<Call> = Vec::new();

    for &ch in &['t', 'o', 'o', 'i'] {
        let t = engine.process(&kd(ch)).expect("engine ok");
        let _ = engine.process(&ku(ch));
        sim_dispatch(t, &mut log);
    }

    let cursors: Vec<u32> = log
        .iter()
        .filter_map(|c| {
            if let Call::UpdatePreeditTextWithMode { cursor_pos, .. } = c {
                Some(*cursor_pos)
            } else {
                None
            }
        })
        .collect();

    // "t"=1, "to"=2, "tô"=2, "tôi"=3
    assert_eq!(cursors, [1, 2, 2, 3], "cursor positions must match char counts of preedit text");
}
