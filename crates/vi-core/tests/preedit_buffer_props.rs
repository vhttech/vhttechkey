#![allow(clippy::unwrap_used)]
//! Property tests for PreeditBuffer invariants.

use proptest::prelude::*;
use vi_core::preedit_buffer::PreeditBuffer;

proptest! {
    #![proptest_config(ProptestConfig::with_cases(500))]

    /// Typing up to 200 printable ASCII characters then backspacing 200 times
    /// must:
    ///   1. Never grow history beyond 64 entries (MAX_HISTORY = MAX_CODEPOINTS).
    ///   2. Leave the buffer completely empty at the end, since the 64-char
    ///      capacity limit causes subsequent pushes to fail, and the first
    ///      snapshot saved by push() captures the empty state.
    #[test]
    fn history_bounded_and_empty_after_full_cycle(
        chars in prop::collection::vec(0x20u32..=0x7eu32, 200),
    ) {
        let mut buf = PreeditBuffer::new();

        for &code in &chars {
            let ch = char::from_u32(code).unwrap();
            let _ = buf.push(ch);
            prop_assert!(
                buf.history_len() <= 64,
                "history grew to {} (limit 64) after push",
                buf.history_len(),
            );
        }

        for _ in 0..200 {
            buf.rollback();
            prop_assert!(
                buf.history_len() <= 64,
                "history grew to {} (limit 64) after rollback",
                buf.history_len(),
            );
        }

        prop_assert!(buf.is_empty(), "buffer must be empty after full push+rollback cycle");
    }
}
