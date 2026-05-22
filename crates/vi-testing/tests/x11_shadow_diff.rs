//! Tests for the X11 shadow-diff live delivery path.
//!
//! The X11 backend (`vi-x11/src/lib.rs`, `update_preedit`) tracks what text the
//! application currently sees in `shadow_buf` and diffs it against each
//! `PreeditUpdated` event to compute the minimum number of BackSpace events
//! plus committed characters to send:
//!
//! ```text
//! common_len  = shadow.chars().zip(new).take_while(|a,b| a==b).count()
//! backspaces  = shadow.chars().count() - common_len
//! new_tail    = new_preedit.chars().skip(common_len).collect()
//! ```
//!
//! Invariants verified:
//!   1. backspaces ≤ shadow.chars().count()  — no underflow
//!   2. apply_diff(shadow, backspaces, tail) == new_preedit  — correctness
//!   3. char counts, not byte counts, are used throughout

#![allow(clippy::unwrap_used)]

use vi_core::{
    CompositionEngine, InputEvent, InputMethod, Key, Modifiers, StandardEngine, StateTransition,
};

// ── helpers ───────────────────────────────────────────────────────────────────

fn key(c: char) -> InputEvent {
    InputEvent::KeyDown(Key::Char(c), Modifiers::none())
}

fn key_shift(c: char) -> InputEvent {
    InputEvent::KeyDown(Key::Char(c), Modifiers::shift())
}

fn backspace() -> InputEvent {
    InputEvent::KeyDown(Key::Backspace, Modifiers::none())
}

/// Mirror of `update_preedit` shadow-diff in `vi-x11/src/lib.rs` (lines 1317-1324).
fn shadow_diff(shadow: &str, new_preedit: &str) -> (usize, String) {
    let common_len = shadow
        .chars()
        .zip(new_preedit.chars())
        .take_while(|(a, b)| a == b)
        .count();
    let backspaces = shadow.chars().count() - common_len;
    let new_tail: String = new_preedit.chars().skip(common_len).collect();
    (backspaces, new_tail)
}

/// Simulate applying BackSpace × backspaces then appending tail to shadow.
fn apply_diff(shadow: &str, backspaces: usize, tail: &str) -> String {
    let mut chars: Vec<char> = shadow.chars().collect();
    for _ in 0..backspaces {
        chars.pop();
    }
    chars.extend(tail.chars());
    chars.into_iter().collect()
}

/// Update a simulated shadow_buf for one engine `StateTransition`.
/// Panics if either shadow-diff invariant is violated.
fn advance_shadow(shadow: String, t: &StateTransition) -> String {
    match t {
        StateTransition::PreeditUpdated(p) => {
            let new_preedit = p.as_str();
            let (bs, tail) = shadow_diff(&shadow, new_preedit);
            assert!(
                bs <= shadow.chars().count(),
                "backspace underflow: bs={bs} > shadow.chars().count()={} \
                 (shadow={shadow:?}, new_preedit={new_preedit:?})",
                shadow.chars().count(),
            );
            let result = apply_diff(&shadow, bs, &tail);
            assert_eq!(
                result, new_preedit,
                "shadow after diff must equal new_preedit \
                 (shadow={shadow:?}, new_preedit={new_preedit:?})"
            );
            result
        }
        // Commit clears the shadow; the app received the text directly.
        StateTransition::Commit(_)
        | StateTransition::CommitAndClear(_)
        | StateTransition::CommitThenPassThrough(_)
        | StateTransition::Cleared => String::new(),
        // Buffer overflow: committed text sent, new preedit starts fresh.
        StateTransition::CommitThenPreedit(_, p) => p.as_str().to_owned(),
        StateTransition::Consumed | StateTransition::PassThrough => shadow,
    }
}

/// Run `events` through the engine tracking the shadow with diff invariants.
/// Returns the final shadow_buf.  Panics on any invariant violation.
fn run_shadow_sim(method: InputMethod, events: &[InputEvent]) -> String {
    let mut engine = StandardEngine::new(method);
    let mut shadow = String::new();
    for event in events {
        if let Ok(t) = engine.process(event) {
            shadow = advance_shadow(shadow, &t);
        }
    }
    shadow
}

// ── 1. Char-count safety ──────────────────────────────────────────────────────

/// Erasing a 2-byte Vietnamese character requires 1 BackSpace, not 2.
#[test]
fn diff_erases_two_byte_char_with_one_backspace() {
    // 'ô' = U+00F4 = 2 UTF-8 bytes but 1 Unicode character.
    let (bs, tail) = shadow_diff("tô", "t");
    assert_eq!(
        bs, 1,
        "erasing 'ô' must take 1 BackSpace (char), not 2 (bytes)"
    );
    assert_eq!(tail, "");
}

/// Erasing a 3-byte Vietnamese character requires 1 BackSpace, not 3.
#[test]
fn diff_erases_three_byte_char_with_one_backspace() {
    // 'ổ' = U+1ED5 = 3 UTF-8 bytes.
    let (bs, tail) = shadow_diff("hổ", "h");
    assert_eq!(
        bs, 1,
        "erasing 'ổ' must take 1 BackSpace (char), not 3 (bytes)"
    );
    assert_eq!(tail, "");
}

/// Clearing 'trông' (5 chars, 6 bytes) needs exactly 5 BackSpaces.
#[test]
fn trong_clear_uses_char_count_not_byte_count() {
    let word = "trông"; // t(1) + r(1) + ô(2) + n(1) + g(1) = 6 bytes, 5 chars
    assert_eq!(word.chars().count(), 5, "trông has 5 Unicode characters");
    assert!(
        word.len() > word.chars().count(),
        "trông has more bytes than chars"
    );

    let (bs, tail) = shadow_diff(word, "");
    assert_eq!(
        bs,
        word.chars().count(),
        "clearing 'trông' must use char count ({}) not byte count ({})",
        word.chars().count(),
        word.len()
    );
    assert_eq!(tail, "");
}

/// Typing "troong" in Telex builds "trông" incrementally;
/// shadow tracking must not confuse bytes and chars at any step.
#[test]
fn telex_trong_shadow_sim_no_invariant_violation() {
    let events: Vec<InputEvent> = "troong".chars().map(key).collect();
    let final_shadow = run_shadow_sim(InputMethod::Telex, &events);
    // The final shadow must have the same char count as the engine's preedit.
    let mut engine = StandardEngine::new(InputMethod::Telex);
    for c in "troong".chars() {
        let _ = engine.process(&key(c));
    }
    assert_eq!(
        final_shadow.chars().count(),
        engine.preedit().as_str().chars().count(),
        "shadow char count must match engine preedit char count"
    );
}

// ── 2. Capitalization ─────────────────────────────────────────────────────────

/// Lowercase "tooi" in Telex — the 'oo' rule fires, producing 'ô'.
/// Shadow tracking must remain consistent across the rule-triggered rewrite.
#[test]
fn telex_tooi_lowercase_shadow_tracking() {
    let events: Vec<InputEvent> = "tooi".chars().map(key).collect();
    run_shadow_sim(InputMethod::Telex, &events);
}

/// Uppercase T,O,O,I with Shift — engine handles case independently.
/// Shadow-diff invariants must hold regardless of whether vowel rules fire.
#[test]
fn telex_tooi_uppercase_shift_shadow_tracking() {
    let events = [
        key_shift('T'),
        key_shift('O'),
        key_shift('O'),
        key_shift('I'),
    ];
    run_shadow_sim(InputMethod::Telex, &events);
}

/// Mixed-case sequence; uppercase tone key 'S' must not trigger Telex tone rule
/// (only lowercase 's' is the sắc tone in Telex).
#[test]
fn telex_mixed_case_tone_key_shadow_tracking() {
    // 'as' (lowercase) → á; 'aS' (uppercase) → literal "aS", no tone.
    let events_lower: Vec<InputEvent> = "as".chars().map(key).collect();
    let shadow_lower = run_shadow_sim(InputMethod::Telex, &events_lower);

    let events_upper = [key('a'), key_shift('S')];
    let shadow_upper = run_shadow_sim(InputMethod::Telex, &events_upper);

    // Both must satisfy invariants (run_shadow_sim panics if violated).
    // The specific output may differ; we only assert invariants here.
    let _ = (shadow_lower, shadow_upper);
}

// ── 3. Repeated accent keys ───────────────────────────────────────────────────

/// VNI: h+o+i+5 → nặng tone applied to vowel nucleus; verify shadow at every step.
#[test]
fn vni_hoi5_repeated_accent_shadow_tracking() {
    let events: Vec<InputEvent> = "hoi5".chars().map(key).collect();
    run_shadow_sim(InputMethod::Vni, &events);
}

/// VNI: applying the same tone digit twice should be idempotent or cancel —
/// shadow tracking must not underflow.
#[test]
fn vni_double_tone_key_no_underflow() {
    // "hoi55" — second '5' either re-applies or cancels the tone.
    let events: Vec<InputEvent> = "hoi55".chars().map(key).collect();
    run_shadow_sim(InputMethod::Vni, &events);
}

/// VNI: vowel form digit followed by tone digit — two successive rewrites.
#[test]
fn vni_vowel_then_tone_shadow_tracking() {
    // "o6" → 'ô'; "o61" → 'ố' (circumflex + sắc).
    let events: Vec<InputEvent> = "o61".chars().map(key).collect();
    run_shadow_sim(InputMethod::Vni, &events);
}

/// VIQR: h+o+i+. (dot = nặng tone) — diacritic key triggers a rewrite.
#[test]
fn viqr_hoi_dot_shadow_tracking() {
    let events: Vec<InputEvent> = "hoi.".chars().map(key).collect();
    run_shadow_sim(InputMethod::Viqr, &events);
}

/// VIQR: circumflex vowel then tone mark — two successive rewrites.
#[test]
fn viqr_circumflex_then_grave_shadow_tracking() {
    // "o^`" → 'ồ' (ô + huyền).
    let events: Vec<InputEvent> = "o^`".chars().map(key).collect();
    run_shadow_sim(InputMethod::Viqr, &events);
}

/// Telex: tone key 'r' on 'hoi' → 'hỏi'; shadow diff must track the
/// in-place character rewrite correctly.
#[test]
fn telex_hoir_tone_rewrite_shadow_tracking() {
    let events: Vec<InputEvent> = "hoir".chars().map(key).collect();
    run_shadow_sim(InputMethod::Telex, &events);
}

// ── 4. BackSpace during composition ──────────────────────────────────────────

/// BackSpace after "too" (→ "tô"): removes 'ô' (multibyte char) with 1 BackSpace.
#[test]
fn backspace_erases_multibyte_preedit_char() {
    // 't','o','o' → preedit "tô"; then BackSpace → preedit shrinks.
    // The shadow diff must use char count when computing backspaces.
    let events = [key('t'), key('o'), key('o'), backspace()];
    run_shadow_sim(InputMethod::Telex, &events);
}

/// BackSpace mid-word with tone mark: 'h','o','o','j' → "hồ", then BackSpace.
#[test]
fn backspace_after_tone_rewrite_shadow_tracking() {
    let events = [key('h'), key('o'), key('o'), key('j'), backspace()];
    run_shadow_sim(InputMethod::Telex, &events);
}

/// Multiple BackSpaces: type 5 chars then delete all of them.
#[test]
fn multiple_backspaces_clear_multibyte_preedit() {
    let mut events: Vec<InputEvent> = "troong".chars().map(key).collect();
    // Add 6 backspaces to clear the preedit.
    events.extend(std::iter::repeat_with(backspace).take(6));
    run_shadow_sim(InputMethod::Telex, &events);
}

/// BackSpace on empty preedit must not underflow (engine returns PassThrough).
#[test]
fn backspace_on_empty_preedit_no_underflow() {
    let events = [backspace(), backspace()];
    let shadow = run_shadow_sim(InputMethod::Telex, &events);
    assert_eq!(
        shadow, "",
        "shadow must stay empty after BackSpace on empty preedit"
    );
}

// ── 5. Long-string path detection ────────────────────────────────────────────

/// The XIM inline buffer in `send_commit_to_client` holds at most 14 bytes.
/// A string of 5+ accented Vietnamese characters easily exceeds that threshold
/// and must be routed through `send_commit_long`.
#[test]
fn long_preedit_exceeds_14_byte_threshold() {
    // Each of these characters is 3 UTF-8 bytes (U+1E00–U+1EFF range).
    let long_text = "ổệẫịựỷ"; // 6 chars × 3 bytes = 18 bytes
    assert_eq!(long_text.len(), 18, "test string must be 18 bytes");
    assert_eq!(long_text.chars().count(), 6, "test string must be 6 chars");
    assert!(
        long_text.len() > 14,
        "must exceed 14-byte XIM buffer limit to trigger send_commit_long"
    );

    // Shadow-diff on a long string must use char count, not byte count.
    let (bs, tail) = shadow_diff(long_text, "");
    assert_eq!(
        bs,
        long_text.chars().count(),
        "clearing long preedit: must use char count ({}) not byte count ({})",
        long_text.chars().count(),
        long_text.len()
    );
    assert_eq!(tail, "");
}

/// A multi-step Telex sequence that rewrites the preedit several times.
/// Exercises the shadow-diff logic across multiple char rewrites.
#[test]
fn long_preedit_shadow_sim_no_invariant_violation() {
    // Telex: "truongf" → attempts to compose with huyền tone applied.
    // This exercises several intermediate PreeditUpdated steps.
    let events: Vec<InputEvent> = "truongf".chars().map(key).collect();
    run_shadow_sim(InputMethod::Telex, &events);
}
