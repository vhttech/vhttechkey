//! Integration tests for the XDG portal IME backend.
//!
//! Sandbox detection tests run without any external dependencies.
//! D-Bus proxy tests spin up a mock `org.freedesktop.portal.InputMethod`
//! service on the session bus and verify that committed text arrives
//! NFC-normalised.  They skip gracefully when no session bus is available.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use zbus::interface;

// ── Mock portal service ───────────────────────────────────────────────────────

#[derive(Default)]
struct MockPortalService {
    committed: Arc<Mutex<Vec<String>>>,
    preedit: Arc<Mutex<Vec<(String, u32, u32)>>>,
}

#[interface(name = "org.freedesktop.portal.InputMethod")]
impl MockPortalService {
    async fn commit_string(&self, text: String) {
        self.committed
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(text);
    }

    /// Receives the `UpdatePreeditString` D-Bus method call (which the proxy
    /// exposes as `set_preedit_string`).
    async fn update_preedit_string(&self, text: String, cursor_begin: u32, cursor_end: u32) {
        self.preedit
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push((text, cursor_begin, cursor_end));
    }
}

// ── Sandbox detection tests ───────────────────────────────────────────────────

#[test]
fn test_sandbox_kind_is_sandboxed_consistent() {
    // detect_sandbox() and is_sandboxed() must agree on whether we are inside
    // a sandbox.  This invariant must hold in every environment.
    use vi_portal::detect::{detect_sandbox, SandboxKind};
    let kind = detect_sandbox();
    let sandboxed = vi_portal::is_sandboxed();
    assert_eq!(sandboxed, kind != SandboxKind::None);
}

#[test]
fn test_sandbox_kind_debug_formatting() {
    use vi_portal::detect::SandboxKind;
    // Verify all variants format without panicking.
    for kind in [
        SandboxKind::Flatpak,
        SandboxKind::Snap,
        SandboxKind::AppImage,
        SandboxKind::Electron,
        SandboxKind::None,
    ] {
        let _ = format!("{kind:?}");
    }
}

#[test]
fn test_sandbox_kind_none_not_sandboxed() {
    use vi_portal::detect::SandboxKind;
    assert_ne!(SandboxKind::Flatpak, SandboxKind::None);
    assert_ne!(SandboxKind::Snap, SandboxKind::None);
    assert_ne!(SandboxKind::AppImage, SandboxKind::None);
    assert_ne!(SandboxKind::Electron, SandboxKind::None);
}

// ── D-Bus proxy tests ─────────────────────────────────────────────────────────

/// Try to start a session bus connection.  Returns `None` when no session bus
/// is available so callers can skip the test gracefully.
async fn try_session_conn() -> Option<zbus::Connection> {
    zbus::Connection::session().await.ok()
}

#[tokio::test]
async fn test_portal_proxy_commit_nfc_normalised_vietnamese() {
    let Some(_sentinel) = try_session_conn().await else {
        // No session D-Bus — skip.
        return;
    };

    let committed = Arc::new(Mutex::new(Vec::<String>::new()));
    let mock = MockPortalService {
        committed: committed.clone(),
        preedit: Arc::new(Mutex::new(Vec::new())),
    };

    // Register the mock under a test-unique well-known name so that it does
    // not conflict with a real portal that may be running on the same bus.
    let service_name = "org.vi_portal.Test.CommitNfc";
    let service_path = "/org/vi_portal/test";

    let service_conn = match zbus::ConnectionBuilder::session()
        .and_then(|b| b.name(service_name))
        .and_then(|b| b.serve_at(service_path, mock))
        .map_err(|e| e.to_string())
    {
        Ok(b) => match b.build().await {
            Ok(conn) => conn,
            Err(_) => return, // bus unavailable or name already taken
        },
        Err(_) => return,
    };

    let client_conn = match zbus::Connection::session().await {
        Ok(c) => c,
        Err(_) => return,
    };

    let proxy = vi_portal::proxy::InputMethodPortalProxy::builder(&client_conn)
        .destination(service_name)
        .unwrap()
        .path(service_path)
        .unwrap()
        .build()
        .await
        .unwrap();

    // "việt" in NFC (precomposed form) — the engine guarantees NFC before
    // calling ImeBackend::commit(), so the portal receives it as-is.
    let nfc_viet = "vi\u{1EC7}t"; // "việt" NFC: ệ = U+1EC7
    proxy.commit_string(nfc_viet).await.unwrap();

    tokio::time::sleep(Duration::from_millis(80)).await;

    let calls = committed.lock().unwrap_or_else(|e| e.into_inner());
    assert_eq!(
        calls.as_slice(),
        &[nfc_viet],
        "committed text must be NFC-normalised Vietnamese"
    );

    drop(service_conn);
}

#[tokio::test]
async fn test_portal_proxy_preedit_update() {
    let Some(_sentinel) = try_session_conn().await else {
        return;
    };

    let preedit = Arc::new(Mutex::new(Vec::<(String, u32, u32)>::new()));
    let mock = MockPortalService {
        committed: Arc::new(Mutex::new(Vec::new())),
        preedit: preedit.clone(),
    };

    let service_name = "org.vi_portal.Test.Preedit";
    let service_path = "/org/vi_portal/test";

    let service_conn = match zbus::ConnectionBuilder::session()
        .and_then(|b| b.name(service_name))
        .and_then(|b| b.serve_at(service_path, mock))
        .map_err(|e| e.to_string())
    {
        Ok(b) => match b.build().await {
            Ok(conn) => conn,
            Err(_) => return,
        },
        Err(_) => return,
    };

    let client_conn = match zbus::Connection::session().await {
        Ok(c) => c,
        Err(_) => return,
    };

    let proxy = vi_portal::proxy::InputMethodPortalProxy::builder(&client_conn)
        .destination(service_name)
        .unwrap()
        .path(service_path)
        .unwrap()
        .build()
        .await
        .unwrap();

    // Preedit "vi" with cursor at the end (2 Unicode scalars).
    proxy.set_preedit_string("vi", 2, 2).await.unwrap();

    tokio::time::sleep(Duration::from_millis(80)).await;

    let calls = preedit.lock().unwrap_or_else(|e| e.into_inner());
    assert_eq!(
        calls.as_slice(),
        &[("vi".to_string(), 2u32, 2u32)],
        "preedit update must arrive with correct cursor positions"
    );

    drop(service_conn);
}

#[tokio::test]
async fn test_portal_proxy_clear_preedit_sends_empty_string() {
    let Some(_sentinel) = try_session_conn().await else {
        return;
    };

    let preedit = Arc::new(Mutex::new(Vec::<(String, u32, u32)>::new()));
    let mock = MockPortalService {
        committed: Arc::new(Mutex::new(Vec::new())),
        preedit: preedit.clone(),
    };

    let service_name = "org.vi_portal.Test.ClearPreedit";
    let service_path = "/org/vi_portal/test";

    let service_conn = match zbus::ConnectionBuilder::session()
        .and_then(|b| b.name(service_name))
        .and_then(|b| b.serve_at(service_path, mock))
        .map_err(|e| e.to_string())
    {
        Ok(b) => match b.build().await {
            Ok(conn) => conn,
            Err(_) => return,
        },
        Err(_) => return,
    };

    let client_conn = match zbus::Connection::session().await {
        Ok(c) => c,
        Err(_) => return,
    };

    let proxy = vi_portal::proxy::InputMethodPortalProxy::builder(&client_conn)
        .destination(service_name)
        .unwrap()
        .path(service_path)
        .unwrap()
        .build()
        .await
        .unwrap();

    // Clearing preedit sends an empty string with zero cursors.
    proxy.set_preedit_string("", 0, 0).await.unwrap();

    tokio::time::sleep(Duration::from_millis(80)).await;

    let calls = preedit.lock().unwrap_or_else(|e| e.into_inner());
    assert_eq!(
        calls.as_slice(),
        &[("".to_string(), 0u32, 0u32)],
        "clear_preedit must send an empty string with zero cursor positions"
    );

    drop(service_conn);
}
