use serde::{Deserialize, Serialize};
use unicode_normalization::UnicodeNormalization;

use crate::error::CompositionError;

/// A `String` guaranteed to be in NFC form. Only constructable via `UnicodePipeline`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct NfcString(String);

impl NfcString {
    /// Internal constructor — only `UnicodePipeline` should call this.
    pub(crate) fn new_unchecked(s: String) -> Self {
        Self(s)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for NfcString {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl AsRef<str> for NfcString {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

/// Text currently being composed; shown inline in the application.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct PreeditText {
    pub(crate) text: String,
    /// UTF-8 byte offset of the insertion cursor within `text`.
    ///
    /// Computed by walking `str::char_indices()` to the end of the NFC-normalised
    /// preedit string.  Equals `text.len()` when the cursor is at the end, which
    /// is always the case during normal composition.
    pub cursor_byte_offset: usize,
}

impl PreeditText {
    /// Construct a `PreeditText` with the cursor at the end of `text`.
    ///
    /// `cursor_byte_offset` is derived by walking `char_indices()` rather than
    /// using `text.len()` directly, so it always reflects the correct UTF-8
    /// byte boundary even after NFC normalisation changes the string length.
    pub fn new(text: String) -> Self {
        let cursor_byte_offset = text
            .char_indices()
            .last()
            .map(|(i, c)| i + c.len_utf8())
            .unwrap_or(0);
        Self { text, cursor_byte_offset }
    }

    pub fn as_str(&self) -> &str {
        &self.text
    }

    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    pub fn nfc_chars(&self) -> impl Iterator<Item = char> + '_ {
        self.text.nfc()
    }
}

impl std::fmt::Display for PreeditText {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.text.fmt(f)
    }
}

/// Text that has been finalized and sent to the application.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommittedText(NfcString);

impl CommittedText {
    pub fn new(nfc: NfcString) -> Self {
        Self(nfc)
    }

    pub fn as_nfc(&self) -> &NfcString {
        &self.0
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl std::fmt::Display for CommittedText {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

/// Modifiers held when a key event fires.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct Modifiers {
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
    pub super_key: bool,
    pub caps_lock: bool,
    /// AltGr (right-alt / ISO_Level3_Shift). When set, the engine flushes
    /// any pending preedit and forwards the key without attempting composition.
    pub altgr: bool,
}

impl Modifiers {
    pub fn none() -> Self {
        Self::default()
    }

    pub fn shift() -> Self {
        Self { shift: true, ..Default::default() }
    }

    pub fn altgr() -> Self {
        Self { altgr: true, ..Default::default() }
    }
}

/// A logical key symbol, independent of physical key position.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Key {
    Char(char),
    Backspace,
    Delete,
    Return,
    Escape,
    Tab,
    Left,
    Right,
    Up,
    Down,
    Home,
    End,
    /// A dead key waiting to combine with the next keypress.
    ///
    /// The `char` is a conventional ASCII identifier for the combining diacritic:
    /// `'^'` = circumflex, `'('` = breve, `'+'` = horn, `'d'` = stroke, etc.
    DeadKey(char),
    /// The Compose / Multi_key, initiating a compose sequence.
    ComposeKey,
    /// Raw keysym for keys not otherwise covered.
    Keysym(u32),
}

impl std::fmt::Display for Key {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Key::Char(c) => write!(f, "'{c}'"),
            Key::Backspace => write!(f, "<BS>"),
            Key::Delete => write!(f, "<DEL>"),
            Key::Return => write!(f, "<RET>"),
            Key::Escape => write!(f, "<ESC>"),
            Key::Tab => write!(f, "<TAB>"),
            Key::Left => write!(f, "<LEFT>"),
            Key::Right => write!(f, "<RIGHT>"),
            Key::Up => write!(f, "<UP>"),
            Key::Down => write!(f, "<DOWN>"),
            Key::Home => write!(f, "<HOME>"),
            Key::End => write!(f, "<END>"),
            Key::DeadKey(c) => write!(f, "<dead:'{c}'>"),
            Key::ComposeKey => write!(f, "<compose>"),
            Key::Keysym(k) => write!(f, "<keysym:{k:#010x}>"),
        }
    }
}

/// All events the composition engine can receive.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum InputEvent {
    KeyDown(Key, Modifiers),
    KeyUp(Key),
    KeyRepeat(Key, Modifiers),
    FocusIn,
    FocusOut,
    Reset,
    /// Text surrounding the cursor as reported by the application.
    ///
    /// `cursor` is a UTF-8 byte offset into `text` indicating the position of
    /// the insertion cursor.  Emitted by backends that support surrounding-text
    /// queries (Wayland text-input-v3, XIM SET_IC_VALUES extension).
    SurroundingText { text: String, cursor: usize },
}

/// What the engine decided to do after processing one event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StateTransition {
    /// Preedit text has changed — update the candidate window.
    PreeditUpdated(PreeditText),
    /// Text is ready to be inserted into the application.
    Commit(CommittedText),
    /// Preedit was committed then cleared.
    CommitAndClear(CommittedText),
    /// Preedit buffer overflowed: prior content was committed and composition
    /// restarted with the triggering character as new preedit.
    CommitThenPreedit(CommittedText, PreeditText),
    /// Preedit was committed AND the triggering key must still reach the application.
    /// Used for space, punctuation, and other word-boundary characters that end
    /// Vietnamese composition but should also be inserted by the app.
    CommitThenPassThrough(CommittedText),
    /// Nothing to do for this event.
    Consumed,
    /// Engine did not handle the event; pass it through to the application.
    PassThrough,
    /// Preedit is now empty (cleared without committing).
    Cleared,
}

/// Result of a single state-machine step.
pub type TransitionResult = Result<StateTransition, CompositionError>;
