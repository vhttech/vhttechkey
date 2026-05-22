use thiserror::Error;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum CompositionError {
    #[error("surrogate codepoint U+{0:04X} is not valid Unicode")]
    SurrogateCodepoint(u32),
    #[error("input byte sequence is not valid UTF-8")]
    InvalidUtf8,
    #[error("combining mark U+{0:04X} has no base character")]
    OrphanedCombiningMark(u32),
    #[error("unknown key symbol: {0}")]
    UnknownKey(String),
    #[error("composition buffer overflow (max {max} codepoints)")]
    BufferOverflow { max: usize },
    #[error("invalid state transition from {from} on input {input}")]
    InvalidTransition { from: String, input: String },
    /// Codepoint is in the C1 control range (U+0080–U+009F), which indicates
    /// text that was decoded from a legacy 8-bit encoding (TCVN3, VPS, VISCII)
    /// as if it were Latin-1 before being stored as UTF-8.  These sequences
    /// cannot be reliably converted to Unicode without an explicit encoding
    /// table, so the pipeline rejects them.
    #[error("codepoint U+{0:04X} indicates legacy 8-bit encoding (TCVN3/VPS/VISCII); re-encode the source document as UTF-8")]
    LegacyEncoding(u32),
}
