//! Regression tests: cursor positions must be char counts, not byte lengths.

#[cfg(test)]
mod tests {
    use vi_platform::char_cursor;

    /// "được" is 4 Unicode scalar values but 8 UTF-8 bytes (đ=2, ư=2, ợ=3, c=1).
    /// Before the fix, `String::len()` returned 8; the correct char count is 4.
    #[test]
    fn char_cursor_duoc_is_4_not_byte_length() {
        let s = "được";
        let cursor = char_cursor(s);
        assert_eq!(
            cursor.0,
            4,
            "char_cursor must return char count (4), not byte length ({})",
            s.len()
        );
        assert_ne!(
            s.len(),
            cursor.0,
            "sanity: byte length must differ from char count"
        );
    }

    #[test]
    fn char_cursor_ascii_matches_len() {
        let s = "hello";
        assert_eq!(char_cursor(s).0, s.len());
    }

    #[test]
    fn char_cursor_empty() {
        assert_eq!(char_cursor("").0, 0);
    }
}
