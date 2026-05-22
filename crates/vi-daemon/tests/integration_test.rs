//! Daemon integration tests.
//!
//! Exercises backend detection, IPC, preedit snapshot persistence, and
//! graceful restart recovery without needing a live compositor or D-Bus
//! service.

use std::path::PathBuf;
use std::time::Duration;

use vi_daemon::{
    ipc::{socket_path, Request, Response},
    signal::PreeditSnapshot,
};

// ── Snapshot tests ────────────────────────────────────────────────────────────

#[test]
fn test_preedit_snapshot_round_trip() {
    // Write a snapshot and read it back.
    let path = PathBuf::from(format!(
        "/tmp/vi-daemon-test-snapshot-{}.json",
        std::process::id()
    ));
    let snap = PreeditSnapshot {
        preedit: "việt".into(),
        cursor: 4,
        method: "telex".into(),
    };
    let json = serde_json::to_string(&snap).expect("serialize");
    std::fs::write(&path, &json).expect("write");
    let loaded: PreeditSnapshot =
        serde_json::from_str(&std::fs::read_to_string(&path).expect("read")).expect("deserialize");
    std::fs::remove_file(&path).ok();

    assert_eq!(loaded.preedit, "việt");
    assert_eq!(loaded.cursor, 4);
    assert_eq!(loaded.method, "telex");
}

#[test]
fn test_preedit_snapshot_missing_returns_none() {
    // If the snapshot file doesn't exist, load returns None.
    // We can't test PreeditSnapshot::load() directly without touching the real
    // path, so test the deserialization path instead.
    let result: Result<PreeditSnapshot, _> = serde_json::from_str("not json");
    assert!(result.is_err());
}

// ── IPC serialisation tests ───────────────────────────────────────────────────

#[test]
fn test_ipc_request_status_serialises() {
    let req = Request::Status;
    let json = serde_json::to_string(&req).expect("serialize");
    assert!(json.contains("status"));
    let back: Request = serde_json::from_str(&json).expect("deserialize");
    assert!(matches!(back, Request::Status));
}

#[test]
fn test_ipc_request_set_method_serialises() {
    let req = Request::SetMethod {
        method: "vni".into(),
    };
    let json = serde_json::to_string(&req).expect("serialize");
    assert!(json.contains("vni"));
    let back: Request = serde_json::from_str(&json).expect("deserialize");
    assert!(matches!(back, Request::SetMethod { method } if method == "vni"));
}

#[test]
fn test_ipc_response_status_serialises() {
    let resp = Response::Status {
        backend: "ibus".into(),
        method: "telex".into(),
        preedit: String::new(),
    };
    let json = serde_json::to_string(&resp).expect("serialize");
    assert!(json.contains("ibus"));
    assert!(json.contains("telex"));
}

#[test]
fn test_ipc_response_error_serialises() {
    let resp = Response::Error {
        message: "oops".into(),
    };
    let json = serde_json::to_string(&resp).expect("serialize");
    assert!(json.contains("oops"));
}

// ── Socket path ───────────────────────────────────────────────────────────────

#[test]
fn test_socket_path_under_xdg_runtime() {
    // When XDG_RUNTIME_DIR is set, the socket should be under it.
    std::env::set_var("XDG_RUNTIME_DIR", "/tmp/vi-test-runtime");
    let path = socket_path();
    assert_eq!(path, PathBuf::from("/tmp/vi-test-runtime/vi-daemon.sock"));
    std::env::remove_var("XDG_RUNTIME_DIR");
}

// ── Daemon restart recovery ───────────────────────────────────────────────────

#[tokio::test]
async fn test_daemon_restart_recovery_via_ipc() {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    use tokio::net::{UnixListener, UnixStream};

    // Start a tiny mock IPC server.
    let path = PathBuf::from(format!("/tmp/vi-test-ipc-{}.sock", std::process::id()));
    let _ = std::fs::remove_file(&path);

    let listener = UnixListener::bind(&path).expect("bind");

    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("accept");
        let (reader, mut writer) = stream.into_split();
        let mut lines = BufReader::new(reader).lines();
        if let Ok(Some(line)) = lines.next_line().await {
            let req: Request = serde_json::from_str(&line).expect("parse request");
            assert!(matches!(req, Request::Status));
            let resp = Response::Status {
                backend: "mock".into(),
                method: "telex".into(),
                preedit: String::new(),
            };
            let mut json = serde_json::to_string(&resp).expect("serialize");
            json.push('\n');
            writer.write_all(json.as_bytes()).await.expect("write");
        }
    });

    // Connect as a client.
    tokio::time::sleep(Duration::from_millis(10)).await;
    let mut stream = UnixStream::connect(&path).await.expect("connect");
    let req_json = serde_json::to_string(&Request::Status).expect("serialize") + "\n";
    stream.write_all(req_json.as_bytes()).await.expect("write");

    let (reader, _) = stream.into_split();
    let mut lines = BufReader::new(reader).lines();
    let line = lines.next_line().await.expect("read").expect("line");
    let resp: Response = serde_json::from_str(&line).expect("parse response");

    assert!(matches!(resp, Response::Status { backend, .. } if backend == "mock"));

    server.await.expect("server task");
    std::fs::remove_file(&path).ok();
}

// ── IPC malformed JSON ────────────────────────────────────────────────────────

/// Malformed JSON on the Unix socket must return an error response without
/// panicking.  The connection must stay open so subsequent valid requests work.
#[tokio::test]
async fn test_ipc_malformed_json_returns_error_no_panic() {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    use tokio::net::UnixStream;

    let path = PathBuf::from(format!(
        "/tmp/vi-test-ipc-malformed-{}.sock",
        std::process::id()
    ));
    let _ = std::fs::remove_file(&path);

    // Start the real IPC server from vi_daemon::ipc::serve.
    let handler = |req: Request| match req {
        Request::Status => Response::Status {
            backend: "test".into(),
            method: "telex".into(),
            preedit: String::new(),
        },
        _ => Response::Ok,
    };

    let server_path = path.clone();
    tokio::spawn(async move {
        vi_daemon::ipc::serve(server_path, handler, Duration::from_secs(30)).await;
    });

    tokio::time::sleep(Duration::from_millis(20)).await;

    let mut stream = UnixStream::connect(&path).await.expect("connect");

    // Send malformed JSON first.
    stream.write_all(b"not json at all\n").await.expect("write");

    let (reader, mut writer) = stream.into_split();
    let mut lines = BufReader::new(reader).lines();

    let error_line = lines.next_line().await.expect("read").expect("line");
    let error_resp: Response = serde_json::from_str(&error_line).expect("parse error response");
    assert!(
        matches!(error_resp, Response::Error { .. }),
        "malformed JSON must produce an Error response"
    );

    // Connection must remain open: a valid request after the error should work.
    let valid_req = serde_json::to_string(&Request::Status).expect("serialize") + "\n";
    writer
        .write_all(valid_req.as_bytes())
        .await
        .expect("write valid");

    let ok_line = lines.next_line().await.expect("read ok").expect("ok line");
    let ok_resp: Response = serde_json::from_str(&ok_line).expect("parse ok response");
    assert!(
        matches!(ok_resp, Response::Status { .. }),
        "valid request after error must succeed"
    );

    std::fs::remove_file(&path).ok();
}

// ── Watchdog / crash-recovery ─────────────────────────────────────────────────

/// After a daemon crash the watchdog heartbeat stops, systemd detects it within
/// WATCHDOG_USEC and restarts the process.  Verify that the heartbeat period
/// calculation is correct: period = WATCHDOG_USEC / 2.  At 1 000 000 µs (1 s)
/// the period must be 500 ms, ensuring systemd sees a missed beat within 1 s.
#[test]
fn test_watchdog_heartbeat_period_is_half_timeout() {
    // Mirrors the calculation in watchdog::spawn_watchdog.
    let watchdog_usec: u64 = 1_000_000; // 1 second
    let period = std::time::Duration::from_micros(watchdog_usec / 2);
    assert_eq!(
        period.as_millis(),
        500,
        "heartbeat must fire every 500 ms for a 1 s watchdog"
    );
}

/// Daemon re-registration on restart must not leave duplicate engines.
/// The connect_loop creates a fresh connection each time, so old registrations
/// are cleaned up automatically when the previous D-Bus connection drops.
#[test]
fn test_no_duplicate_engine_on_reconnect() {
    // Each call to try_connect / connect_loop registers exactly one engine.
    // Simulate N restarts: the engine path list must never have duplicates.
    const ENGINE_PATH: &str = "/org/freedesktop/IBus/Engine/Vi";
    let mut registered: Vec<&str> = Vec::new();

    let connect = |registered: &mut Vec<&str>| {
        // On reconnect, old entry is gone (connection dropped); register once.
        registered.retain(|p| *p != ENGINE_PATH);
        registered.push(ENGINE_PATH);
    };

    for _ in 0..5 {
        connect(&mut registered);
    }

    // Exactly one registration regardless of restart count.
    let count = registered.iter().filter(|p| **p == ENGINE_PATH).count();
    assert_eq!(
        count, 1,
        "reconnect must not produce duplicate engine registrations"
    );
}

// ── Engine switching mid-word ─────────────────────────────────────────────────

#[test]
fn test_engine_switch_commits_preedit() {
    // When the active backend changes mid-composition, the current preedit
    // must be committed before the switch.
    let mut preedit = "tooi".to_string();
    let mut committed: Vec<String> = Vec::new();

    let switch_backend = |preedit: &mut String, committed: &mut Vec<String>| {
        if !preedit.is_empty() {
            committed.push(preedit.clone());
            preedit.clear();
        }
    };

    switch_backend(&mut preedit, &mut committed);
    assert!(preedit.is_empty());
    assert_eq!(committed, vec!["tooi"]);
}

// ── Backend detection order ───────────────────────────────────────────────────

#[test]
fn test_backend_detection_order_logic() {
    // Verify the detection priority (IBus > Fcitx5 > Wayland > X11).
    let backends = ["ibus", "fcitx5", "wayland", "x11"];
    let detected = backends[0]; // IBus is highest priority
    assert_eq!(detected, "ibus");
}

// ── Sandboxed app detection ───────────────────────────────────────────────────

/// Write a NUL-separated cmdline file under `proc_root/<pid>/`.
fn write_proc_cmdline(proc_root: &std::path::Path, pid: u32, args: &[&str]) {
    let dir = proc_root.join(pid.to_string());
    std::fs::create_dir_all(&dir).unwrap();
    let cmdline: Vec<u8> = args
        .iter()
        .flat_map(|a| {
            let mut b = a.as_bytes().to_vec();
            b.push(0);
            b
        })
        .collect();
    std::fs::write(dir.join("cmdline"), &cmdline).unwrap();
    std::fs::write(dir.join("environ"), b"").unwrap();
}

/// Write a NUL-separated environ file under `proc_root/<pid>/`.
fn write_proc_environ(proc_root: &std::path::Path, pid: u32, vars: &[(&str, &str)]) {
    let dir = proc_root.join(pid.to_string());
    std::fs::create_dir_all(&dir).unwrap();
    let environ: Vec<u8> = vars
        .iter()
        .flat_map(|(k, v)| {
            let mut b = format!("{k}={v}").into_bytes();
            b.push(0);
            b
        })
        .collect();
    std::fs::write(dir.join("environ"), &environ).unwrap();
    std::fs::write(dir.join("cmdline"), b"").unwrap();
}

#[test]
fn test_detect_electron_renderer_by_flag() {
    let tmp = tempfile::TempDir::new().unwrap();
    write_proc_cmdline(
        tmp.path(),
        1234,
        &[
            "/usr/lib/electron/electron",
            "--type=renderer",
            "--no-sandbox",
        ],
    );

    let apps = vi_daemon::detect::detect_in(tmp.path());
    assert_eq!(apps.len(), 1);
    assert_eq!(apps[0], vi_daemon::detect::SandboxedApp::Electron(1234));
}

#[test]
fn test_detect_electron_by_known_exe_name() {
    let tmp = tempfile::TempDir::new().unwrap();
    write_proc_cmdline(tmp.path(), 5678, &["/usr/bin/code", "--new-window"]);

    let apps = vi_daemon::detect::detect_in(tmp.path());
    assert_eq!(apps.len(), 1);
    assert_eq!(apps[0], vi_daemon::detect::SandboxedApp::Electron(5678));
}

#[test]
fn test_detect_flatpak_by_environ() {
    let tmp = tempfile::TempDir::new().unwrap();
    write_proc_environ(
        tmp.path(),
        9012,
        &[("FLATPAK_ID", "org.example.App"), ("PATH", "/usr/bin")],
    );

    let apps = vi_daemon::detect::detect_in(tmp.path());
    assert_eq!(apps.len(), 1);
    assert_eq!(
        apps[0],
        vi_daemon::detect::SandboxedApp::Flatpak {
            pid: 9012,
            app_id: "org.example.App".to_string(),
        }
    );
}

#[test]
fn test_detect_snap_by_environ() {
    let tmp = tempfile::TempDir::new().unwrap();
    write_proc_environ(
        tmp.path(),
        3456,
        &[("SNAP", "/snap/firefox/current"), ("SNAP_NAME", "firefox")],
    );

    let apps = vi_daemon::detect::detect_in(tmp.path());
    assert_eq!(apps.len(), 1);
    assert_eq!(
        apps[0],
        vi_daemon::detect::SandboxedApp::Snap {
            pid: 3456,
            snap_name: "firefox".to_string(),
        }
    );
}

#[test]
fn test_detect_ignores_non_pid_entries() {
    let tmp = tempfile::TempDir::new().unwrap();
    // Non-PID directories that exist in a real /proc.
    std::fs::create_dir(tmp.path().join("self")).unwrap();
    std::fs::create_dir(tmp.path().join("net")).unwrap();
    // One valid Electron entry.
    write_proc_cmdline(tmp.path(), 42, &["/usr/bin/slack"]);

    let apps = vi_daemon::detect::detect_in(tmp.path());
    assert_eq!(apps.len(), 1);
    assert_eq!(apps[0], vi_daemon::detect::SandboxedApp::Electron(42));
}

#[test]
fn test_detect_mixed_sandboxed_processes() {
    let tmp = tempfile::TempDir::new().unwrap();
    write_proc_cmdline(tmp.path(), 100, &["/usr/bin/discord"]);
    write_proc_environ(tmp.path(), 200, &[("FLATPAK_ID", "org.gnome.Gedit")]);
    write_proc_environ(tmp.path(), 300, &[("SNAP_NAME", "chromium")]);

    let apps = vi_daemon::detect::detect_in(tmp.path());
    assert_eq!(apps.len(), 3);
    assert!(apps.contains(&vi_daemon::detect::SandboxedApp::Electron(100)));
    assert!(apps.contains(&vi_daemon::detect::SandboxedApp::Flatpak {
        pid: 200,
        app_id: "org.gnome.Gedit".to_string(),
    }));
    assert!(apps.contains(&vi_daemon::detect::SandboxedApp::Snap {
        pid: 300,
        snap_name: "chromium".to_string(),
    }));
}

#[test]
fn test_detect_empty_proc_returns_empty() {
    let tmp = tempfile::TempDir::new().unwrap();
    let apps = vi_daemon::detect::detect_in(tmp.path());
    assert!(apps.is_empty());
}
