//! Composition engine types (port from github.com/BambooEngine/bamboo-core, MIT).

#![allow(dead_code)]

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum EffectType {
    Appending = 0,
    MarkTransformation = 1,
    ToneTransformation = 2,
    Replacing = 3,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum Mark {
    None = 0,
    Hat = 1,
    Breve = 2,
    Horn = 3,
    Dash = 4,
    Raw = 5,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum Tone {
    None = 0,
    Grave = 1,
    Acute = 2,
    Hook = 3,
    Tilde = 4,
    Dot = 5,
}

impl From<u8> for Tone {
    fn from(v: u8) -> Self {
        match v {
            1 => Tone::Grave,
            2 => Tone::Acute,
            3 => Tone::Hook,
            4 => Tone::Tilde,
            5 => Tone::Dot,
            _ => Tone::None,
        }
    }
}

/// Engine mode bitmask (`1 << iota` order in Go).
pub(crate) type EngineMode = u32;

pub(crate) const VIETNAMESE_MODE: EngineMode = 1 << 0;
pub(crate) const ENGLISH_MODE: EngineMode = 1 << 1;
pub(crate) const TONE_LESS: EngineMode = 1 << 2;
pub(crate) const MARK_LESS: EngineMode = 1 << 3;
pub(crate) const LOWERCASE_MODE: EngineMode = 1 << 4;
pub(crate) const FULL_TEXT: EngineMode = 1 << 5;
pub(crate) const PUNCTUATION_MODE: EngineMode = 1 << 6;
pub(crate) const IN_REVERSE_ORDER: EngineMode = 1 << 7;

pub(crate) const EFREE_TONE_MARKING: u32 = 1 << 0;
pub(crate) const ESTANDARD_TONE_STYLE: u32 = 1 << 1;
pub(crate) const EAUTO_CORRECT_ENABLED: u32 = 1 << 2;
pub(crate) const ESTDFLAGS: u32 = EFREE_TONE_MARKING | ESTANDARD_TONE_STYLE | EAUTO_CORRECT_ENABLED;

#[derive(Debug, Clone)]
pub(crate) struct Rule {
    pub key: char,
    pub effect: u8,
    pub effect_type: EffectType,
    pub effect_on: char,
    pub result: char,
    pub appended_rules: Vec<Rule>,
}

#[derive(Debug)]
pub(crate) struct TransInner {
    pub rule: Rule,
    pub target: Option<Trans>,
    pub is_upper_case: bool,
}

pub(crate) type Trans = std::sync::Arc<parking_lot::RwLock<TransInner>>;

#[derive(Debug, Clone)]
pub(crate) struct ParsedInputMethod {
    pub name: String,
    pub rules: Vec<Rule>,
    pub super_keys: Vec<char>,
    pub tone_keys: Vec<char>,
    pub appending_keys: Vec<char>,
    pub keys: Vec<char>,
}
