//! Deprecated: use the vime-* equivalent crate instead.

pub mod vi_engine;
pub mod commit_engine;
pub mod composition_gate;
pub mod engine;
pub mod error;
pub mod keyboard;
pub mod methods;
pub mod preedit_buffer;
pub mod spell;
pub mod syllable;
pub mod types;
pub mod unicode_pipeline;
pub mod vietnamese_dict;

pub use engine::{CompositionEngine, EngineSnapshot, StandardEngine};
pub use error::CompositionError;
pub use methods::InputMethod;
pub use types::{
    CommittedText, InputEvent, Key, Modifiers, NfcString, PreeditText, StateTransition,
    TransitionResult,
};
pub use spell::SpellOptions;
pub use unicode_pipeline::UnicodePipeline;
pub use vietnamese_dict::VietnameseDict;
