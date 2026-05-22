//! Optional dictionary-assisted **commit** behaviour: if the NFC lowercase word is missing
//! from `vietnamese.cm.dict` but the buffer already contains Vietnamese letters, commit the
//! raw Telex/VNI key sequence instead of the composed Unicode.

use std::sync::Arc;

use crate::composition_gate::is_vietnamese_composed_char;
use crate::vietnamese_dict::VietnameseDict;

/// Per-engine spell / dictionary options (cheap to clone thanks to `Arc`).
#[derive(Clone)]
pub struct SpellOptions {
    /// Loaded `vietnamese.cm.dict` (or subset). When `None`, dictionary checks are skipped.
    pub dictionary: Option<Arc<VietnameseDict>>,
    /// When `true` and `dictionary` is set: on commit, use raw keys if lookup fails.
    pub commit_spell_check_dict: bool,
    /// `dd_freestyle`: if the composed buffer contains `đ`, never substitute raw keys.
    pub dd_freestyle: bool,
}

impl SpellOptions {
    pub fn disabled() -> Self {
        Self {
            dictionary: None,
            commit_spell_check_dict: false,
            dd_freestyle: true,
        }
    }
}

impl Default for SpellOptions {
    fn default() -> Self {
        Self {
            dictionary: None,
            commit_spell_check_dict: false,
            dd_freestyle: true,
        }
    }
}

/// True if any character is already “Vietnamese-shaped” for spell / fallback purposes.
pub fn buffer_contains_vietnamese(s: &str) -> bool {
    s.chars().any(is_vietnamese_composed_char)
}
