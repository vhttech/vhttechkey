//! Input method routing. Composition logic lives in `vi_engine`.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum InputMethod {
    #[default]
    Telex,
    Vni,
    Viqr,
}

impl InputMethod {
    /// True if `ch` is a composition trigger character for this method and
    /// must therefore never be treated as a word-boundary pass-through.
    pub fn is_composition_char(&self, ch: char) -> bool {
        match self {
            // VIQR uses ASCII punctuation for tone marks (', `, ?, ~, .)
            // and vowel form marks (^, (, +).
            Self::Viqr => matches!(ch, '\'' | '`' | '?' | '~' | '.' | '^' | '(' | '+'),
            _ => false,
        }
    }
}
