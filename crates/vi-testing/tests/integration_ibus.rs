//! IBus mock-based integration tests.
//!
//! Requires a session D-Bus.  Run with:
//!   dbus-run-session -- cargo test -p vi-testing --test integration_ibus
#![allow(clippy::unwrap_used)]

use vi_core::{
    CompositionEngine, InputEvent, InputMethod, Key, Modifiers, StandardEngine, StateTransition,
};
use vi_testing::mock_ibus::{IbusCall, MockIbusDaemon};
use zbus::{Connection, ProxyBuilder};

fn key(c: char) -> InputEvent {
    InputEvent::KeyDown(Key::Char(c), Modifiers::none())
}

/// Helper: call `CommitText` on the mock daemon via D-Bus.
async fn call_commit_text(conn: &Connection, text: &str) {
    let proxy = ProxyBuilder::<zbus::Proxy<'_>>::new(conn)
        .destination("org.freedesktop.IBus")
        .unwrap()
        .path("/org/freedesktop/IBus")
        .unwrap()
        .interface("org.freedesktop.IBus.Engine1")
        .unwrap()
        .build()
        .await
        .expect("build proxy");
    proxy
        .call::<_, _, ()>("CommitText", &(text.to_owned(),))
        .await
        .expect("CommitText call");
}

/// Helper: call `UpdatePreeditText` on the mock daemon via D-Bus.
async fn call_update_preedit(conn: &Connection, text: &str, cursor: u32, visible: bool) {
    let proxy = ProxyBuilder::<zbus::Proxy<'_>>::new(conn)
        .destination("org.freedesktop.IBus")
        .unwrap()
        .path("/org/freedesktop/IBus")
        .unwrap()
        .interface("org.freedesktop.IBus.Engine1")
        .unwrap()
        .build()
        .await
        .expect("build proxy");
    proxy
        .call::<_, _, ()>("UpdatePreeditText", &(text.to_owned(), cursor, visible))
        .await
        .expect("UpdatePreeditText call");
}

/// Type text through the engine, then channel the engine output to the mock
/// daemon via D-Bus.  Returns the committed text (empty if nothing was
/// committed).
async fn type_and_send(
    engine: &mut StandardEngine,
    client: &Connection,
    keys: &str,
) -> String {
    let mut committed = String::new();
    for ch in keys.chars() {
        match engine.process(&key(ch)).unwrap() {
            StateTransition::PreeditUpdated(p) => {
                call_update_preedit(client, p.as_str(), p.as_str().chars().count() as u32, true).await;
            }
            StateTransition::CommitAndClear(c) | StateTransition::Commit(c) => {
                call_commit_text(client, c.as_str()).await;
                committed.push_str(c.as_str());
            }
            _ => {}
        }
        let _ = engine.process(&InputEvent::KeyUp(Key::Char(ch)));
    }
    committed
}

// ── Tests ─────────────────────────────────────────────────────────────────────

/// Type text, commit, simulate a daemon restart, then type more text.
/// After the restart the daemon must still capture new calls.
#[tokio::test]
async fn ibus_daemon_restart_recovery() {
    if std::env::var_os("DBUS_SESSION_BUS_ADDRESS").is_none() {
        eprintln!("SKIP: no DBUS_SESSION_BUS_ADDRESS; run with dbus-run-session");
        return;
    }
    let Some(daemon) = MockIbusDaemon::start().await else {
        eprintln!("SKIP: org.freedesktop.IBus already registered; run with dbus-run-session");
        return;
    };
    let client = Connection::session().await.expect("client session bus");

    // Phase 1: type "aa" (Telex → "â"), then commit via Return.
    let mut engine = StandardEngine::new(InputMethod::Telex);
    type_and_send(&mut engine, &client, "aa").await;
    if let StateTransition::CommitAndClear(c) = engine.process(&InputEvent::KeyDown(Key::Return, Modifiers::none())).unwrap() {
        call_commit_text(&client, c.as_str()).await;
    }

    let calls_before = daemon.calls();
    assert!(
        calls_before.iter().any(|c| matches!(c, IbusCall::CommitText(t) if t == "â")),
        "CommitText('â') not captured before restart; calls: {calls_before:?}",
    );

    // Phase 2: simulate daemon restart.
    daemon.simulate_restart().await;

    // Phase 3: type "oo" (Telex → "ô"), commit.
    let mut engine2 = StandardEngine::new(InputMethod::Telex);
    type_and_send(&mut engine2, &client, "oo").await;
    if let StateTransition::CommitAndClear(c) = engine2.process(&InputEvent::KeyDown(Key::Return, Modifiers::none())).unwrap() {
        call_commit_text(&client, c.as_str()).await;
    }

    let calls_after = daemon.calls();
    assert!(
        calls_after.iter().any(|c| matches!(c, IbusCall::CommitText(t) if t == "ô")),
        "CommitText('ô') not captured after restart; calls: {calls_after:?}",
    );
}

/// FocusOut while composing must trigger a CommitText (IBUS_ENGINE_PREEDIT_COMMIT),
/// not silently discard the preedit.
#[tokio::test]
async fn ibus_focus_out_commits_preedit() {
    if std::env::var_os("DBUS_SESSION_BUS_ADDRESS").is_none() {
        eprintln!("SKIP: no DBUS_SESSION_BUS_ADDRESS; run with dbus-run-session");
        return;
    }
    let Some(daemon) = MockIbusDaemon::start().await else {
        eprintln!("SKIP: org.freedesktop.IBus already registered; run with dbus-run-session");
        return;
    };
    let client = Connection::session().await.expect("client session bus");

    let mut engine = StandardEngine::new(InputMethod::Telex);
    // Build preedit "tô" (type t, o, o).
    for ch in "too".chars() {
        engine.process(&key(ch)).unwrap();
        let _ = engine.process(&InputEvent::KeyUp(Key::Char(ch)));
    }
    assert_eq!(engine.preedit().as_str(), "tô");

    // FocusOut → engine commits the preedit.
    let result = engine.process(&InputEvent::FocusOut).unwrap();
    let committed = match result {
        StateTransition::CommitAndClear(c) => c.as_str().to_owned(),
        other => panic!("expected CommitAndClear on FocusOut, got {other:?}"),
    };
    assert_eq!(committed, "tô", "FocusOut must commit 'tô'");

    // Simulate what vi-ibus does: forward the committed text to the IBus daemon.
    call_commit_text(&client, &committed).await;

    // Daemon must have recorded exactly one CommitText("tô").
    let calls = daemon.calls();
    assert_eq!(calls.len(), 1, "expected exactly one call, got {calls:?}");
    assert!(
        matches!(&calls[0], IbusCall::CommitText(t) if t == "tô"),
        "expected CommitText('tô'), got {:?}",
        calls[0],
    );
}
