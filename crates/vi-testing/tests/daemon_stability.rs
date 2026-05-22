//! Daemon stability tests.
//!
//! Exercises the IPC server's panic/malformed-input recovery and socket
//! security properties without needing a live compositor or D-Bus service.

use std::os::unix::fs::MetadataExt;
use std::path::PathBuf;
use std::time::Duration;

use tempfile::TempDir;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use tokio::time::timeout;
use tokio_util::sync::CancellationToken;

use vi_daemon::ipc::{Request, Response};

fn unique_sock(dir: &TempDir, label: &str) -> PathBuf {
    dir.path().join(format!("{label}-{}.sock", std::process::id()))
}

/// Spawn an IPC server, send a malformed request and a panicking request, then
/// assert the daemon is still alive and answers a subsequent valid request.
#[tokio::test]
async fn test_ipc_survives_malformed_and_panicking_requests() {
    let dir = TempDir::new().expect("tempdir");
    let path = unique_sock(&dir, "stability");

    let handler = |req: Request| -> Response {
        match req {
            Request::Status => Response::Status {
                backend: "test".into(),
                method: "telex".into(),
                preedit: String::new(),
            },
            Request::SetMethod { ref method } if method == "panic" => {
                panic!("deliberate test panic")
            }
            _ => Response::Ok,
        }
    };

    let server_path = path.clone();
    tokio::spawn(async move {
        vi_daemon::ipc::serve(server_path, handler, Duration::from_secs(30)).await;
    });

    // Wait for the socket to appear.
    timeout(Duration::from_millis(500), async {
        loop {
            if path.exists() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("socket did not appear within 500ms");

    let stream = timeout(Duration::from_millis(500), UnixStream::connect(&path))
        .await
        .expect("connect timeout")
        .expect("connect");

    let (reader_half, mut writer_half) = stream.into_split();
    let mut lines = BufReader::new(reader_half).lines();

    // ── Malformed JSON ──────────────────────────────────────────────────────
    timeout(
        Duration::from_millis(200),
        writer_half.write_all(b"not json at all\n"),
    )
    .await
    .expect("write timeout (malformed)")
    .expect("write");

    let error_line = timeout(Duration::from_millis(500), lines.next_line())
        .await
        .expect("read timeout (malformed)")
        .expect("read ok")
        .expect("line");
    let error_resp: Response = serde_json::from_str(&error_line).expect("parse error response");
    assert!(
        matches!(error_resp, Response::Error { .. }),
        "malformed JSON must return an Error response"
    );

    // ── Panicking handler ───────────────────────────────────────────────────
    let panic_req =
        serde_json::to_string(&Request::SetMethod { method: "panic".into() }).unwrap() + "\n";
    timeout(
        Duration::from_millis(200),
        writer_half.write_all(panic_req.as_bytes()),
    )
    .await
    .expect("write timeout (panic req)")
    .expect("write");

    let panic_line = timeout(Duration::from_millis(500), lines.next_line())
        .await
        .expect("read timeout (panic resp)")
        .expect("read ok")
        .expect("line");
    let panic_resp: Response = serde_json::from_str(&panic_line).expect("parse panic response");
    assert!(
        matches!(panic_resp, Response::Error { .. }),
        "panicking handler must return an Error response"
    );

    // ── Daemon still alive ──────────────────────────────────────────────────
    let valid_req = serde_json::to_string(&Request::Status).unwrap() + "\n";
    timeout(
        Duration::from_millis(200),
        writer_half.write_all(valid_req.as_bytes()),
    )
    .await
    .expect("write timeout (valid req)")
    .expect("write");

    let ok_line = timeout(Duration::from_millis(500), lines.next_line())
        .await
        .expect("read timeout (valid resp)")
        .expect("read ok")
        .expect("line");
    let ok_resp: Response = serde_json::from_str(&ok_line).expect("parse valid response");
    assert!(
        matches!(ok_resp, Response::Status { .. }),
        "daemon must respond to a valid request after malformed and panicking requests"
    );
}

/// A task driven by a CancellationToken must exit within 100ms of cancellation.
#[tokio::test]
async fn test_watchdog_cancellation_exits_within_100ms() {
    let token = CancellationToken::new();
    let child = token.child_token();

    // Mimics the watchdog inner loop: wait on cancellation or a long sleep.
    let handle = tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = child.cancelled() => break,
                _ = tokio::time::sleep(Duration::from_secs(60)) => {}
            }
        }
    });

    token.cancel();

    timeout(Duration::from_millis(100), handle)
        .await
        .expect("task must exit within 100ms after cancellation")
        .expect("task panicked");
}

/// The socket file must have mode 0o600 immediately after the IPC server binds.
#[tokio::test]
async fn test_socket_file_mode_is_0o600() {
    let dir = TempDir::new().expect("tempdir");
    let path = unique_sock(&dir, "perms");

    let server_path = path.clone();
    tokio::spawn(async move {
        vi_daemon::ipc::serve(server_path, |_: Request| Response::Ok, Duration::from_secs(30))
            .await;
    });

    // Poll until the socket exists (permissions are set right after bind).
    timeout(Duration::from_millis(500), async {
        loop {
            if path.exists() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("socket did not appear within 500ms");

    let meta = std::fs::metadata(&path).expect("socket metadata");
    let mode = meta.mode() & 0o777;
    assert_eq!(mode, 0o600, "socket must be mode 0o600, got {mode:o}");
}
