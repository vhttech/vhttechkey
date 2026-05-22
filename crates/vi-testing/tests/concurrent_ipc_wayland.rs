//! Concurrent stress test: simultaneous IPC SetMethod calls and simulated
//! Wayland compositor activate / deactivate / input cycles.
//!
//! Task A: ~100 SetMethod IPC requests/sec for 5 seconds (alternating Telex/VNI).
//! Task B: ~200 compositor event cycles/sec for 5 seconds (FocusIn → keys → FocusOut).
//!
//! Assertions:
//!   - No panics or deadlocks under concurrent load.
//!   - Every committed string is valid NFC.
//!   - Preedit is empty after all tasks complete.

#![allow(clippy::unwrap_used)]

use std::sync::{Arc, Mutex};
use std::time::Duration;

use tempfile::TempDir;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use tokio::time::timeout;
use unicode_normalization::UnicodeNormalization;
use vi_core::{
    CompositionEngine, InputEvent, InputMethod, Key, Modifiers, StandardEngine, StateTransition,
    TransitionResult,
};
use vi_daemon::ipc::{Request, Response};

fn collect_commits(tr: TransitionResult, buf: &Mutex<Vec<String>>) {
    match tr {
        Ok(StateTransition::Commit(c)) | Ok(StateTransition::CommitAndClear(c)) => {
            buf.lock().unwrap().push(c.as_str().to_owned());
        }
        Ok(StateTransition::CommitThenPreedit(c, _)) => {
            buf.lock().unwrap().push(c.as_str().to_owned());
        }
        _ => {}
    }
}

/// Concurrent stress test: IPC method-switching races with simulated compositor
/// activate / input / deactivate cycles.
///
/// Task A (~100 req/sec): IPC client alternates `SetMethod` Telex ↔ VNI.
/// Task B (~200 cycles/sec): simulates FocusIn → key presses → FocusOut.
///
/// The test is wrapped in a 30-second timeout so CI does not hang on deadlock.
#[tokio::test]
async fn test_concurrent_ipc_and_compositor_events() {
    let engine = Arc::new(Mutex::new(StandardEngine::new(InputMethod::Telex)));
    let committed: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));

    let dir = TempDir::new().unwrap();
    let sock_path =
        dir.path().join(format!("concurrent-ipc-{}.sock", std::process::id()));

    // IPC server: SetMethod requests switch the input method on the shared engine.
    let engine_ipc = Arc::clone(&engine);
    let server_path = sock_path.clone();
    tokio::spawn(async move {
        vi_daemon::ipc::serve(
            server_path,
            move |req: Request| match req {
                Request::SetMethod { method } => {
                    let m = match method.as_str() {
                        "vni" => InputMethod::Vni,
                        _ => InputMethod::Telex,
                    };
                    engine_ipc.lock().unwrap().set_method(m);
                    Response::Ok
                }
                _ => Response::Ok,
            },
            Duration::from_secs(30),
        )
        .await;
    });

    // Wait for the socket file to appear before connecting.
    timeout(Duration::from_millis(500), async {
        loop {
            if sock_path.exists() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("IPC socket did not appear within 500 ms");

    // Task A: IPC client; alternates SetMethod Telex / VNI at ~100 req/sec.
    let sock_for_a = sock_path.clone();
    let task_a = tokio::spawn(async move {
        let stream =
            UnixStream::connect(&sock_for_a).await.expect("connect to IPC socket");
        let (reader_half, mut writer_half) = stream.into_split();
        let mut lines = BufReader::new(reader_half).lines();
        let methods = ["telex", "vni"];
        for i in 0u32..500 {
            let method = methods[(i as usize) % 2];
            let req =
                serde_json::to_string(&Request::SetMethod { method: method.into() }).unwrap()
                    + "\n";
            writer_half.write_all(req.as_bytes()).await.unwrap();
            // Drain the response so the server's write buffer does not fill up.
            let _ = lines.next_line().await;
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    });

    // Task B: simulates Wayland compositor activate / input / deactivate cycles.
    // Runs in a blocking thread so std::thread::sleep does not stall the executor.
    let engine_b = Arc::clone(&engine);
    let committed_b = Arc::clone(&committed);
    let task_b = tokio::task::spawn_blocking(move || {
        let keys: &[char] = &['t', 'o', 'i', 'a', 'e', 'u', 'h', 'n'];
        let mut seq = 0usize;
        let start = std::time::Instant::now();
        loop {
            // Activate (compositor sends text-input enable / activate).
            let _ = engine_b.lock().unwrap().process(&InputEvent::FocusIn);

            // Type 2–3 characters (compositor relays key events to the IM).
            let n = 2 + (seq % 2);
            for k in 0..n {
                let ch = keys[(seq + k) % keys.len()];
                let tr = engine_b
                    .lock()
                    .unwrap()
                    .process(&InputEvent::KeyDown(Key::Char(ch), Modifiers::none()));
                collect_commits(tr, &committed_b);
            }

            // Deactivate (window-switch or focus-loss) — flushes preedit as a commit.
            let tr = engine_b.lock().unwrap().process(&InputEvent::FocusOut);
            collect_commits(tr, &committed_b);

            seq = seq.wrapping_add(1);
            std::thread::sleep(Duration::from_millis(5));

            if start.elapsed() >= Duration::from_secs(5) {
                break;
            }
        }
    });

    // Both tasks must complete without panic or deadlock within 30 seconds.
    timeout(Duration::from_secs(30), async {
        task_a.await.expect("Task A (IPC client) panicked");
        task_b.await.expect("Task B (compositor simulation) panicked");
    })
    .await
    .expect("test timed out after 30 s — possible deadlock");

    // Every committed string must be valid NFC.
    let commits = committed.lock().unwrap().clone();
    for (i, s) in commits.iter().enumerate() {
        let nfc: String = s.nfc().collect();
        assert_eq!(s, &nfc, "commit[{i}] is not NFC: {s:?}");
    }

    // No stuck preedit should remain after all tasks have finished.
    let preedit = engine.lock().unwrap().preedit().as_str().to_owned();
    assert!(preedit.is_empty(), "preedit not cleared after test: {preedit:?}");
}
