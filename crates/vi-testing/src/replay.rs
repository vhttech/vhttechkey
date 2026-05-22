//! Record and replay InputEvent streams for regression testing.

use serde::{Deserialize, Serialize};
use vi_core::{
    CompositionEngine, InputEvent, InputMethod, Key, StandardEngine, StateTransition,
};

/// A recorded input session: the sequence of events and the text committed
/// during replay.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplaySession {
    /// Human-readable label.
    pub name: String,
    /// Input method active during the session.
    pub method: InputMethod,
    /// Events in order.
    pub events: Vec<InputEvent>,
    /// The committed strings expected when replaying these events.
    pub expected_commits: Vec<String>,
}

impl ReplaySession {
    /// Serialize to bincode bytes (the `.vi-replay` wire format).
    pub fn to_bytes(&self) -> Vec<u8> {
        bincode::serialize(self).expect("ReplaySession serialization must not fail")
    }

    /// Deserialize from bincode bytes.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, bincode::Error> {
        bincode::deserialize(bytes)
    }
}

/// Replay `session` through a fresh engine and return the list of committed
/// strings produced.
pub fn replay_session(session: &ReplaySession) -> Vec<String> {
    let mut engine = StandardEngine::new(session.method);
    let mut commits: Vec<String> = Vec::new();

    for event in &session.events {
        match engine.process(event).expect("engine must not fail during replay") {
            StateTransition::Commit(c) | StateTransition::CommitAndClear(c) => {
                commits.push(c.as_str().to_owned());
            }
            _ => {}
        }
        // Inject an implicit KeyUp after every KeyDown(Char) so that the
        // repeat-dedup guard is cleared between consecutive same-character
        // presses.  Fixture event lists do not include KeyUp events.
        if let InputEvent::KeyDown(Key::Char(ch), _) = event {
            let _ = engine.process(&InputEvent::KeyUp(Key::Char(*ch)));
        }
    }

    commits
}

/// Assert that replaying `session` produces the expected committed text.
pub fn assert_replay(session: &ReplaySession) {
    let actual = replay_session(session);
    assert_eq!(
        actual, session.expected_commits,
        "replay mismatch for session '{}': expected {:?}, got {:?}",
        session.name, session.expected_commits, actual
    );
}
