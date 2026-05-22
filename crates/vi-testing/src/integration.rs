//! Full-stack integration test harness.

use std::sync::{Arc, Mutex};

use unicode_normalization::UnicodeNormalization;
use vi_core::{
    CompositionEngine, InputEvent, InputMethod, Key, StandardEngine, StateTransition,
    TransitionResult,
};

/// Recording mock backend for integration tests.
#[derive(Default)]
pub struct MockBackend {
    pub committed: Vec<String>,
    pub preedit_history: Vec<String>,
    pub clears: usize,
    pub forwards: Vec<Key>,
}

fn dispatch_transition_mock(transition: TransitionResult, backend: &mut MockBackend) {
    match transition {
        Ok(StateTransition::PreeditUpdated(p)) => {
            backend.preedit_history.push(p.as_str().to_owned());
        }
        Ok(StateTransition::Commit(c)) | Ok(StateTransition::CommitAndClear(c)) => {
            let s = c.as_str();
            let nfc: String = s.nfc().collect();
            assert_eq!(s, nfc, "commit text not NFC normalized: {s:?}");
            backend.committed.push(s.to_owned());
            backend.clears += 1;
        }
        Ok(StateTransition::CommitThenPreedit(c, p)) => {
            let s = c.as_str();
            let nfc: String = s.nfc().collect();
            assert_eq!(s, nfc, "commit text not NFC normalized: {s:?}");
            backend.committed.push(s.to_owned());
            backend.preedit_history.push(p.as_str().to_owned());
        }
        Ok(StateTransition::Cleared) => {
            backend.clears += 1;
        }
        Ok(StateTransition::CommitThenPassThrough(c)) => {
            let s = c.as_str();
            let nfc: String = s.nfc().collect();
            assert_eq!(s, nfc, "commit text not NFC normalized: {s:?}");
            backend.committed.push(s.to_owned());
            backend.clears += 1;
        }
        Ok(StateTransition::PassThrough) | Ok(StateTransition::Consumed) => {}
        Err(e) => panic!("composition error in test: {e}"),
    }
}

/// Full-stack test harness wiring a `StandardEngine` to a `MockBackend`.
pub struct IntegrationHarness {
    pub engine: Arc<Mutex<StandardEngine>>,
    backend: Arc<Mutex<MockBackend>>,
}

impl IntegrationHarness {
    pub fn new(method: InputMethod) -> Self {
        Self {
            engine: Arc::new(Mutex::new(StandardEngine::new(method))),
            backend: Arc::new(Mutex::new(MockBackend::default())),
        }
    }

    pub async fn send_event(&self, event: InputEvent) {
        let transition = self.engine.lock().unwrap().process(&event);
        {
            let mut backend = self.backend.lock().unwrap();
            dispatch_transition_mock(transition, &mut backend);
        }
        // Inject an implicit KeyUp after every KeyDown(Char) so the repeat-dedup
        // guard is cleared between consecutive same-character presses.
        if let InputEvent::KeyDown(Key::Char(ch), _) = event {
            let _ = self
                .engine
                .lock()
                .unwrap()
                .process(&InputEvent::KeyUp(Key::Char(ch)));
        }
    }

    pub fn committed(&self) -> Vec<String> {
        self.backend.lock().unwrap().committed.clone()
    }

    /// Current preedit as reported by the engine (reflects `set_method` and
    /// backspace clears immediately, even before the next backend update).
    pub fn preedit(&self) -> String {
        self.engine.lock().unwrap().preedit().as_str().to_owned()
    }
}
