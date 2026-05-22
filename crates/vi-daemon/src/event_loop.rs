use std::sync::{Arc, Mutex};

use tokio::sync::{broadcast, mpsc};
use vi_core::{
    engine::StandardEngine,
    types::{InputEvent, Key, StateTransition},
    CompositionEngine,
};
use vi_platform::{CharCursor, ImeBackend};

pub type EngineHandle = Arc<Mutex<StandardEngine>>;
pub type BackendHandle = Arc<dyn ImeBackend>;

pub async fn run_event_loop(
    engine: EngineHandle,
    backend: BackendHandle,
    mut rx: mpsc::Receiver<InputEvent>,
    mut shutdown: broadcast::Receiver<()>,
) {
    loop {
        tokio::select! {
            Some(event) = rx.recv() => {
                let (transition, snapshot) = {
                    let mut eng = match engine.lock() {
                        Ok(g) => g,
                        Err(poisoned) => {
                            tracing::error!("engine mutex poisoned, resetting state");
                            let mut g = poisoned.into_inner();
                            g.reset();
                            g
                        }
                    };
                    let snap = eng.snapshot();
                    let t = eng.process(&event);
                    (t, snap)
                    // Lock is dropped here, before the async dispatch below.
                };
                if let Err(e) = dispatch_transition(transition, &event, &backend, &engine).await {
                    tracing::error!("dispatch failed, rolling back engine state: {e}");
                    match engine.lock() {
                        Ok(mut g) => g.restore(snapshot),
                        Err(poisoned) => {
                            tracing::error!("engine mutex poisoned, resetting state");
                            let mut g = poisoned.into_inner();
                            g.reset();
                        }
                    }
                }
            }
            _ = shutdown.recv() => break,
        }
    }
}

async fn dispatch_transition(
    transition: vi_core::types::TransitionResult,
    event: &InputEvent,
    backend: &BackendHandle,
    engine: &EngineHandle,
) -> Result<(), vi_platform::PlatformError> {
    match transition {
        Ok(StateTransition::PreeditUpdated(p)) => {
            tracing::trace!(
                preedit_chars = p.as_str().chars().count(),
                "dispatch: update_preedit"
            );
            backend.update_preedit(&p, CharCursor(p.cursor_byte_offset))
        }
        Ok(StateTransition::CommitThenPassThrough(c)) => {
            tracing::trace!(
                commit_chars = c.as_str().chars().count(),
                "dispatch: commit_replacing_preedit + forward_key"
            );
            backend.commit_replacing_preedit(c.as_nfc())?;
            backend.forward_key(event)
        }
        Ok(StateTransition::Commit(c)) | Ok(StateTransition::CommitAndClear(c)) => {
            tracing::trace!(
                commit_chars = c.as_str().chars().count(),
                "dispatch: commit_replacing_preedit"
            );
            backend.commit_replacing_preedit(c.as_nfc())?;
            if let InputEvent::KeyDown(Key::Return | Key::Tab, _) = event {
                backend.forward_key(event).ok();
            }
            Ok(())
        }
        Ok(StateTransition::CommitThenPreedit(c, p)) => {
            tracing::trace!(
                commit_chars = c.as_str().chars().count(),
                preedit_chars = p.as_str().chars().count(),
                "dispatch: commit_then_update_preedit"
            );
            backend.commit_then_update_preedit(c.as_nfc(), &p, CharCursor(p.cursor_byte_offset))
        }
        Ok(StateTransition::Cleared) => {
            tracing::trace!("dispatch: clear_preedit");
            backend.clear_preedit()
        }
        // Forward the original key to the application so it isn't silently dropped.
        // The IBus backend already returned true (consumed) for this key event, so
        // we must use forward_key to deliver it; returning Ok(()) would eat the key.
        Ok(StateTransition::PassThrough) => {
            tracing::trace!("dispatch: forward_key");
            backend.forward_key(event)
        }
        Ok(StateTransition::Consumed) => {
            tracing::trace!("dispatch: consumed");
            Ok(())
        }
        Err(e) => {
            tracing::error!("composition error: {e}");
            match engine.lock() {
                Ok(mut g) => g.reset(),
                Err(poisoned) => poisoned.into_inner().reset(),
            }
            Err(vi_platform::PlatformError::DBus(format!(
                "composition error: {e}"
            )))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex as StdMutex;
    use vi_core::{
        CommittedText, CompositionEngine, CompositionError, InputMethod, Key, Modifiers, NfcString,
        PreeditText, StandardEngine, UnicodePipeline,
    };
    use vi_platform::{Capabilities, PlatformError, Result, SurroundingText};

    struct NullBackend;
    impl ImeBackend for NullBackend {
        fn commit(&self, _: &NfcString) -> Result<()> {
            Ok(())
        }
        fn update_preedit(&self, _: &PreeditText, _: CharCursor) -> Result<()> {
            Ok(())
        }
        fn clear_preedit(&self) -> Result<()> {
            Ok(())
        }
        fn forward_key(&self, _: &InputEvent) -> Result<()> {
            Ok(())
        }
        fn surrounding_text(&self) -> Result<Option<SurroundingText>> {
            Ok(None)
        }
        fn capabilities(&self) -> Capabilities {
            Capabilities::default()
        }
    }

    /// Backend that records which methods were called and in what order.
    /// Optionally injects a failure on `update_preedit` to test rollback.
    struct RecordingBackend {
        calls: StdMutex<Vec<&'static str>>,
        fail_update_preedit: bool,
    }

    impl RecordingBackend {
        fn new() -> Self {
            Self {
                calls: StdMutex::new(vec![]),
                fail_update_preedit: false,
            }
        }

        fn new_failing() -> Self {
            Self {
                calls: StdMutex::new(vec![]),
                fail_update_preedit: true,
            }
        }

        fn recorded(&self) -> Vec<&'static str> {
            self.calls.lock().unwrap().clone()
        }
    }

    impl ImeBackend for RecordingBackend {
        fn commit(&self, _: &NfcString) -> Result<()> {
            self.calls.lock().unwrap().push("commit");
            Ok(())
        }
        fn update_preedit(&self, _: &PreeditText, _: CharCursor) -> Result<()> {
            self.calls.lock().unwrap().push("update_preedit");
            if self.fail_update_preedit {
                Err(PlatformError::DBus("injected failure".to_string()))
            } else {
                Ok(())
            }
        }
        fn clear_preedit(&self) -> Result<()> {
            self.calls.lock().unwrap().push("clear_preedit");
            Ok(())
        }
        fn forward_key(&self, _: &InputEvent) -> Result<()> {
            self.calls.lock().unwrap().push("forward_key");
            Ok(())
        }
        fn commit_replacing_preedit(&self, _: &NfcString) -> Result<()> {
            self.calls.lock().unwrap().push("commit_replacing_preedit");
            Ok(())
        }
        fn surrounding_text(&self) -> Result<Option<SurroundingText>> {
            Ok(None)
        }
        fn capabilities(&self) -> Capabilities {
            Capabilities::default()
        }
    }

    /// Backend that records calls but does NOT override `commit_replacing_preedit`,
    /// so the trait default runs: `clear_preedit().ok(); commit(text)`.
    /// Used to verify that the default impl calls `clear_preedit` before `commit`.
    struct DefaultImplRecordingBackend {
        calls: StdMutex<Vec<&'static str>>,
    }

    impl DefaultImplRecordingBackend {
        fn new() -> Self {
            Self {
                calls: StdMutex::new(vec![]),
            }
        }

        fn recorded(&self) -> Vec<&'static str> {
            self.calls.lock().unwrap().clone()
        }
    }

    impl ImeBackend for DefaultImplRecordingBackend {
        fn commit(&self, _: &NfcString) -> Result<()> {
            self.calls.lock().unwrap().push("commit");
            Ok(())
        }
        fn update_preedit(&self, _: &PreeditText, _: CharCursor) -> Result<()> {
            self.calls.lock().unwrap().push("update_preedit");
            Ok(())
        }
        fn clear_preedit(&self) -> Result<()> {
            self.calls.lock().unwrap().push("clear_preedit");
            Ok(())
        }
        fn forward_key(&self, _: &InputEvent) -> Result<()> {
            self.calls.lock().unwrap().push("forward_key");
            Ok(())
        }
        fn surrounding_text(&self) -> Result<Option<SurroundingText>> {
            Ok(None)
        }
        fn capabilities(&self) -> Capabilities {
            Capabilities::default()
        }
        // commit_replacing_preedit intentionally NOT overridden → default impl
    }

    fn make_engine() -> EngineHandle {
        Arc::new(Mutex::new(StandardEngine::new(InputMethod::Telex)))
    }

    fn make_backend() -> BackendHandle {
        Arc::new(NullBackend)
    }

    #[tokio::test]
    async fn composition_error_propagates_as_err() {
        let engine = make_engine();
        let backend = make_backend();
        let dummy = InputEvent::KeyDown(Key::Char('a'), Modifiers::none());
        let result = dispatch_transition(
            Err(CompositionError::OrphanedCombiningMark(0x0301)),
            &dummy,
            &backend,
            &engine,
        )
        .await;
        assert!(
            result.is_err(),
            "composition error must not be silently swallowed"
        );
    }

    #[tokio::test]
    async fn engine_is_reset_after_composition_error() {
        let engine = make_engine();
        let backend = make_backend();
        let dummy = InputEvent::KeyDown(Key::Char('a'), Modifiers::none());

        {
            let mut eng = engine.lock().unwrap();
            let _ = eng.process(&dummy);
            assert!(
                !eng.preedit().is_empty(),
                "preedit must have content after 'a'"
            );
        }

        let _ = dispatch_transition(
            Err(CompositionError::OrphanedCombiningMark(0x0301)),
            &dummy,
            &backend,
            &engine,
        )
        .await;

        {
            let eng = engine.lock().unwrap();
            assert!(
                eng.preedit().is_empty(),
                "engine preedit must be empty after composition error reset"
            );
        }

        let (tx, rx) = mpsc::channel(16);
        let (shutdown_tx, shutdown_rx) = broadcast::channel(1);
        tx.send(InputEvent::KeyDown(Key::Char('b'), Modifiers::none()))
            .await
            .unwrap();
        shutdown_tx.send(()).unwrap();
        run_event_loop(Arc::clone(&engine), Arc::clone(&backend), rx, shutdown_rx).await;
    }

    // ── RecordingBackend dispatch tests ───────────────────────────────────────

    #[tokio::test]
    async fn preedit_updated_calls_update_preedit_only() {
        let engine = make_engine();
        let recording = Arc::new(RecordingBackend::new());
        let backend: BackendHandle = Arc::clone(&recording) as Arc<dyn ImeBackend>;

        let event = InputEvent::KeyDown(Key::Char('a'), Modifiers::none());
        let transition = Ok(StateTransition::PreeditUpdated(PreeditText::new(
            "a".to_string(),
        )));

        dispatch_transition(transition, &event, &backend, &engine)
            .await
            .unwrap();

        assert_eq!(
            recording.recorded(),
            vec!["update_preedit"],
            "PreeditUpdated must call only update_preedit"
        );
    }

    #[tokio::test]
    async fn commit_and_clear_calls_commit_replacing_preedit() {
        let engine = make_engine();
        let recording = Arc::new(RecordingBackend::new());
        let backend: BackendHandle = Arc::clone(&recording) as Arc<dyn ImeBackend>;

        // Use the engine to generate a real CommitAndClear transition.
        // Ctrl+char on a non-empty buffer produces CommitAndClear.
        let mut helper = StandardEngine::new(InputMethod::Telex);
        let _ = helper.process(&InputEvent::KeyDown(Key::Char('a'), Modifiers::none()));
        let event = InputEvent::KeyDown(
            Key::Char('c'),
            Modifiers {
                ctrl: true,
                ..Modifiers::none()
            },
        );
        let transition = helper.process(&event);

        dispatch_transition(transition, &event, &backend, &engine)
            .await
            .unwrap();

        assert_eq!(
            recording.recorded(),
            vec!["commit_replacing_preedit"],
            "CommitAndClear must use commit_replacing_preedit (atomic), not separate clear_preedit + commit"
        );
    }

    #[tokio::test]
    async fn passthrough_calls_forward_key_only() {
        let engine = make_engine();
        let recording = Arc::new(RecordingBackend::new());
        let backend: BackendHandle = Arc::clone(&recording) as Arc<dyn ImeBackend>;

        let event = InputEvent::KeyDown(Key::Char('q'), Modifiers::none());
        let transition = Ok(StateTransition::PassThrough);

        dispatch_transition(transition, &event, &backend, &engine)
            .await
            .unwrap();

        assert_eq!(
            recording.recorded(),
            vec!["forward_key"],
            "PassThrough must call only forward_key"
        );
    }

    #[tokio::test]
    async fn commit_then_passthrough_calls_commit_replacing_preedit_then_forward_key() {
        let engine = make_engine();
        let recording = Arc::new(RecordingBackend::new());
        let backend: BackendHandle = Arc::clone(&recording) as Arc<dyn ImeBackend>;

        let mut helper = StandardEngine::new(InputMethod::Telex);
        let _ = helper.process(&InputEvent::KeyDown(Key::Char('a'), Modifiers::none()));
        let event = InputEvent::KeyDown(Key::Char(' '), Modifiers::none());
        let transition = helper.process(&event);

        dispatch_transition(transition, &event, &backend, &engine)
            .await
            .unwrap();

        assert_eq!(
            recording.recorded(),
            vec!["commit_replacing_preedit", "forward_key"],
            "CommitThenPassThrough must call commit_replacing_preedit then forward_key"
        );
    }

    #[tokio::test]
    async fn update_preedit_error_propagates_from_dispatch() {
        let engine = make_engine();
        let failing = Arc::new(RecordingBackend::new_failing());
        let backend: BackendHandle = Arc::clone(&failing) as Arc<dyn ImeBackend>;

        let event = InputEvent::KeyDown(Key::Char('a'), Modifiers::none());
        let transition = Ok(StateTransition::PreeditUpdated(PreeditText::new(
            "a".to_string(),
        )));

        let result = dispatch_transition(transition, &event, &backend, &engine).await;

        assert!(
            result.is_err(),
            "update_preedit failure must propagate as Err from dispatch"
        );
        assert_eq!(failing.recorded(), vec!["update_preedit"]);
    }

    #[tokio::test]
    async fn full_vietnamese_sequence_uses_commit_replacing_preedit() {
        // Regression: CommitAndClear must route through commit_replacing_preedit
        // (atomic), not a separate clear_preedit + commit pair.
        // Sequence: t-o-o-i builds preedit "tôi"; Return commits it.
        // KeyUp events between the two 'o' presses prevent the repeat guard
        // from suppressing the second keypress.
        let engine = make_engine();
        let recording = Arc::new(RecordingBackend::new());
        let backend: BackendHandle = Arc::clone(&recording) as Arc<dyn ImeBackend>;

        let events: Vec<InputEvent> = vec![
            InputEvent::KeyDown(Key::Char('t'), Modifiers::none()),
            InputEvent::KeyUp(Key::Char('t')),
            InputEvent::KeyDown(Key::Char('o'), Modifiers::none()),
            InputEvent::KeyUp(Key::Char('o')),
            InputEvent::KeyDown(Key::Char('o'), Modifiers::none()), // oo → ô
            InputEvent::KeyUp(Key::Char('o')),
            InputEvent::KeyDown(Key::Char('i'), Modifiers::none()),
            InputEvent::KeyDown(Key::Return, Modifiers::none()),
        ];
        for event in &events {
            let transition = engine.lock().unwrap().process(event);
            dispatch_transition(transition, event, &backend, &engine)
                .await
                .unwrap();
        }

        let calls = recording.recorded();

        assert!(
            !calls.contains(&"clear_preedit"),
            "clear_preedit must not be called separately; full call list: {calls:?}"
        );

        // forward_key is expected for KeyUp pass-throughs and the Return forwarding;
        // filter to the preedit/commit calls that define the regression.
        let relevant: Vec<&str> = calls
            .iter()
            .copied()
            .filter(|&c| c == "update_preedit" || c == "commit_replacing_preedit")
            .collect();
        assert_eq!(
            relevant,
            [
                "update_preedit",
                "update_preedit",
                "update_preedit",
                "update_preedit",
                "commit_replacing_preedit",
            ],
            "expected 4 update_preedit then 1 commit_replacing_preedit; full call list: {calls:?}"
        );
    }

    /// Regression: `CommitThenPassThrough` must route through
    /// `commit_replacing_preedit`, and the trait default must call
    /// `clear_preedit` **before** `commit` (not after).  Swapping that order
    /// would clear text that was just committed.
    #[tokio::test]
    async fn commit_then_passthrough_uses_commit_replacing_preedit() {
        let engine = make_engine();
        let recording = Arc::new(DefaultImplRecordingBackend::new());
        let backend: BackendHandle = Arc::clone(&recording) as Arc<dyn ImeBackend>;

        // Telex: 'a' opens preedit, space flushes it as CommitThenPassThrough.
        let mut helper = StandardEngine::new(InputMethod::Telex);
        let _ = helper.process(&InputEvent::KeyDown(Key::Char('a'), Modifiers::none()));
        let event = InputEvent::KeyDown(Key::Char(' '), Modifiers::none());
        let transition = helper.process(&event);

        dispatch_transition(transition, &event, &backend, &engine)
            .await
            .unwrap();

        assert_eq!(
            recording.recorded(),
            vec!["clear_preedit", "commit", "forward_key"],
            "default commit_replacing_preedit must call clear_preedit before commit; \
             forward_key must follow"
        );
    }

    #[tokio::test]
    async fn commit_then_preedit_dispatches_commit_then_update_preedit() {
        let engine = make_engine();
        let recording = Arc::new(RecordingBackend::new());
        let backend: BackendHandle = Arc::clone(&recording) as Arc<dyn ImeBackend>;

        let event = InputEvent::KeyDown(Key::Char('a'), Modifiers::none());
        let committed = CommittedText::new(UnicodePipeline::normalize_only("a"));
        let preedit = PreeditText::new("b".to_string());
        let transition = Ok(StateTransition::CommitThenPreedit(committed, preedit));

        dispatch_transition(transition, &event, &backend, &engine)
            .await
            .unwrap();

        assert_eq!(
            recording.recorded(),
            vec!["commit", "update_preedit"],
            "CommitThenPreedit must call commit then update_preedit in order"
        );
    }

    #[tokio::test]
    async fn engine_rolled_back_when_update_preedit_fails() {
        // Pre-populate the engine with preedit "a".
        let engine = make_engine();
        {
            let mut eng = engine.lock().unwrap();
            let _ = eng.process(&InputEvent::KeyDown(Key::Char('a'), Modifiers::none()));
            assert_eq!(eng.preedit().as_str(), "a");
        }

        // Failing backend: update_preedit always errors.
        let backend: BackendHandle = Arc::new(RecordingBackend::new_failing());

        let (tx, rx) = mpsc::channel(16);
        let (shutdown_tx, shutdown_rx) = broadcast::channel(1);

        // Send 'b' then shutdown. The event loop either:
        //   (a) processes 'b' first → dispatch fails → rolls back to snapshot
        //       (preedit "a"), or
        //   (b) receives shutdown first → preedit unchanged ("a").
        // Either way the assertion holds.
        tx.send(InputEvent::KeyDown(Key::Char('b'), Modifiers::none()))
            .await
            .unwrap();
        shutdown_tx.send(()).unwrap();
        run_event_loop(Arc::clone(&engine), backend, rx, shutdown_rx).await;

        let eng = engine.lock().unwrap();
        assert_eq!(
            eng.preedit().as_str(),
            "a",
            "engine must be rolled back to the pre-event snapshot on dispatch failure"
        );
    }
}
