use std::collections::VecDeque;

use unicode_normalization::UnicodeNormalization;

use crate::types::PreeditText;
use thiserror::Error;

/// Maximum number of Unicode codepoints allowed in the preedit buffer.
const MAX_CODEPOINTS: usize = 64;

/// Maximum number of snapshots retained for rollback.
///
/// Matches MAX_CODEPOINTS so that a fully-filled buffer can be completely
/// undone one step at a time back to the empty state.
const MAX_HISTORY: usize = 64;

/// Immutable snapshot of the buffer at a point in time.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Snapshot {
    chars: Vec<char>,
    /// Raw key sequence that produced this snapshot (used for rollback).
    keys: Vec<char>,
}

/// An immutable-snapshot + delta model for preedit text.
///
/// Each mutating operation saves the current state before changing it, so that
/// `rollback()` (backspace) can restore exactly the previous visual state rather
/// than naively removing the last character.
#[derive(Debug, Clone)]
pub struct PreeditBuffer {
    /// Character sequence currently shown in preedit.
    chars: Vec<char>,
    /// The key characters typed so far (used by rules to determine context).
    keys: Vec<char>,
    /// Deque of snapshots for rollback (bounded at MAX_HISTORY; oldest evicted first).
    history: VecDeque<Snapshot>,
    /// Cached string form of `chars`, invalidated on every mutation.
    cached_display: Option<String>,
}

impl PreeditBuffer {
    pub fn new() -> Self {
        Self {
            chars: Vec::new(),
            keys: Vec::new(),
            history: VecDeque::new(),
            cached_display: None,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.chars.is_empty()
    }

    pub fn len(&self) -> usize {
        self.chars.len()
    }

    /// Current preedit content as a `String`.
    pub fn as_string(&self) -> String {
        let result: String = self.chars.iter().collect();
        debug_assert_eq!(
            result.chars().nfc().collect::<String>(),
            result,
            "preedit is not NFC"
        );
        result
    }

    pub fn to_preedit_text(&self) -> PreeditText {
        PreeditText::new(self.as_string())
    }

    /// The characters currently shown in preedit as a str (used for rule matching).
    ///
    /// The result is cached after the first call and invalidated by any mutation.
    pub fn display_chars(&mut self) -> &str {
        if self.cached_display.is_none() {
            self.cached_display = Some(self.chars.iter().collect());
        }
        self.cached_display.as_deref().unwrap_or("")
    }

    /// The raw key sequence typed (for rollback history; not for rule matching).
    pub fn key_sequence(&self) -> &[char] {
        &self.keys
    }

    /// Raw keystrokes as a string (English mode commit fallback).
    pub fn raw_keys_string(&self) -> String {
        self.keys.iter().collect()
    }

    /// Number of snapshots currently held (used in tests and diagnostics).
    pub fn history_len(&self) -> usize {
        self.history.len()
    }

    /// Replace the displayed characters with `new_chars`, pushing old state to history.
    pub fn set_display(&mut self, new_chars: Vec<char>, key: char) -> Result<(), BufferError> {
        // Check the *new* size, not old+1. When a rule fires at MAX_CODEPOINTS the
        // replacement is typically smaller (two chars → one), so the old+1 check was
        // a false positive that triggered an unwanted commit.
        if new_chars.len() > MAX_CODEPOINTS {
            return Err(BufferError::Overflow);
        }
        self.save_snapshot();
        self.chars = new_chars;
        self.keys.push(key);
        self.cached_display = None;
        Ok(())
    }

    /// Append a single character to both the key sequence and display.
    pub fn push(&mut self, ch: char) -> Result<(), BufferError> {
        if self.chars.len() >= MAX_CODEPOINTS {
            return Err(BufferError::Overflow);
        }
        self.save_snapshot();
        self.chars.push(ch);
        self.keys.push(ch);
        self.cached_display = None;
        Ok(())
    }

    /// Undo the last operation (keystroke-level). Returns `true` if there
    /// was something to undo.
    pub fn rollback(&mut self) -> bool {
        if let Some(snapshot) = self.history.pop_back() {
            self.chars = snapshot.chars;
            self.keys = snapshot.keys;
            self.cached_display = None;
            true
        } else {
            false
        }
    }

    /// Undo the last visual character (character-level delete). Pops snapshots until finding one with fewer displayed
    /// chars than the current state. If history is exhausted, clears to empty.
    pub fn rollback_to_shorter(&mut self) {
        let target_len = self.chars.len();
        while let Some(snapshot) = self.history.pop_back() {
            if snapshot.chars.len() < target_len {
                self.chars = snapshot.chars;
                self.keys = snapshot.keys;
                self.cached_display = None;
                return;
            }
        }
        self.chars.clear();
        self.keys.clear();
        self.cached_display = None;
    }

    /// Clear all state without committing.
    pub fn clear(&mut self) {
        self.chars.clear();
        self.keys.clear();
        self.history.clear();
        self.cached_display = None;
    }

    fn save_snapshot(&mut self) {
        self.history.push_back(Snapshot {
            chars: self.chars.clone(),
            keys: self.keys.clone(),
        });
        if self.history.len() > MAX_HISTORY {
            self.history.pop_front();
        }
    }
}

impl Default for PreeditBuffer {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum BufferError {
    #[error("preedit buffer overflow")]
    Overflow,
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn push_and_rollback() {
        let mut buf = PreeditBuffer::new();
        buf.push('t').unwrap();
        buf.push('o').unwrap();
        assert_eq!(buf.as_string(), "to");
        buf.rollback();
        assert_eq!(buf.as_string(), "t");
        buf.rollback();
        assert_eq!(buf.as_string(), "");
        assert!(!buf.rollback()); // nothing left
    }

    #[test]
    fn set_display_rollback() {
        let mut buf = PreeditBuffer::new();
        buf.push('t').unwrap();
        buf.push('o').unwrap();
        // Rule fires: "oo" → "ô"
        buf.set_display(vec!['t', 'ô'], 'o').unwrap();
        assert_eq!(buf.as_string(), "tô");
        buf.rollback();
        // Reverts to "to" state
        assert_eq!(buf.as_string(), "to");
    }

    #[test]
    fn clear_resets_history() {
        let mut buf = PreeditBuffer::new();
        buf.push('a').unwrap();
        buf.clear();
        assert!(buf.is_empty());
        assert!(!buf.rollback());
    }

    /// Regression: set_display used to check `self.chars.len() + 1 > MAX` which
    /// falsely returned Overflow when the buffer was full but the replacement was
    /// smaller (as happens when a composition rule fires at the size limit).
    #[test]
    fn set_display_no_false_overflow_when_replacement_shrinks() {
        let mut buf = PreeditBuffer::new();
        for _ in 0..MAX_CODEPOINTS {
            buf.push('n').unwrap();
        }
        assert_eq!(buf.len(), MAX_CODEPOINTS);
        // Replacement is one char shorter — a vowel rule reducing two chars to one.
        let shrunk: Vec<char> = std::iter::repeat_n('n', MAX_CODEPOINTS - 2)
            .chain(std::iter::once('ô'))
            .collect();
        let result = buf.set_display(shrunk, 'o');
        assert!(
            result.is_ok(),
            "shrinking set_display must succeed even at MAX_CODEPOINTS"
        );
        assert_eq!(buf.len(), MAX_CODEPOINTS - 1);
    }

    /// set_display must still reject replacements that exceed MAX_CODEPOINTS.
    #[test]
    fn set_display_rejects_oversized_replacement() {
        let mut buf = PreeditBuffer::new();
        buf.push('a').unwrap();
        let too_large: Vec<char> = std::iter::repeat_n('x', MAX_CODEPOINTS + 1).collect();
        let result = buf.set_display(too_large, 'x');
        assert_eq!(result, Err(BufferError::Overflow));
    }
}
