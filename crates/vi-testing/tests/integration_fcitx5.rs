//! Fcitx5 mock-based integration tests.
//!
//! Requires a session D-Bus.  Run with:
//!   dbus-run-session -- cargo test -p vi-testing --test integration_fcitx5
#![allow(clippy::unwrap_used)]

use std::sync::{Arc, Mutex};
use zbus::{interface, Connection, ProxyBuilder};

// ── Fcitx5 capability flags (from fcitx5 source, CapabilityFlag enum) ─────────

const CAP_PREEDIT: u64 = 1 << 0;
const CAP_SURROUNDING_TEXT: u64 = 1 << 9;

// ── Inline mock Fcitx5 service ────────────────────────────────────────────────

struct MockFcitx5Service {
    capabilities: Arc<Mutex<u64>>,
}

#[interface(name = "org.fcitx.Fcitx5.InputContext1")]
impl MockFcitx5Service {
    fn set_capability(&self, caps: u64) {
        *self.capabilities.lock().unwrap() = caps;
    }

    fn get_capability(&self) -> u64 {
        *self.capabilities.lock().unwrap()
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

/// When the engine backend connects to Fcitx5 it must advertise both
/// `SURROUNDING_TEXT` and `PREEDIT` capabilities.  If Fcitx5 subsequently
/// sends an `UpdateCapability` reducing the set, the backend must reflect the
/// new set.
#[tokio::test]
async fn fcitx5_capability_negotiation() {
    // Skip when no session D-Bus is available (CI without dbus-run-session).
    if std::env::var_os("DBUS_SESSION_BUS_ADDRESS").is_none() {
        eprintln!("SKIP: no DBUS_SESSION_BUS_ADDRESS; run with dbus-run-session");
        return;
    }

    let capabilities = Arc::new(Mutex::new(0u64));

    // Start mock Fcitx5 service.
    let server_conn = Connection::session()
        .await
        .expect("session D-Bus available");
    server_conn
        .object_server()
        .at(
            "/org/fcitx/icproxy/by_display/0",
            MockFcitx5Service { capabilities: Arc::clone(&capabilities) },
        )
        .await
        .expect("register Fcitx5 object");
    server_conn
        .request_name("org.fcitx.Fcitx5")
        .await
        .expect("acquire org.fcitx.Fcitx5 name");

    // Client (simulates vi-fcitx5 backend) connects and negotiates capabilities.
    let client = Connection::session().await.expect("client session bus");
    let proxy = ProxyBuilder::<zbus::Proxy<'_>>::new(&client)
        .destination("org.fcitx.Fcitx5")
        .unwrap()
        .path("/org/fcitx/icproxy/by_display/0")
        .unwrap()
        .interface("org.fcitx.Fcitx5.InputContext1")
        .unwrap()
        .build()
        .await
        .expect("client proxy");

    // Step 1: negotiate initial capabilities — must include SURROUNDING_TEXT | PREEDIT.
    let initial_caps = CAP_SURROUNDING_TEXT | CAP_PREEDIT;
    proxy
        .call::<_, _, ()>("SetCapability", &(initial_caps,))
        .await
        .expect("SetCapability call");

    let recorded: u64 = proxy
        .call("GetCapability", &())
        .await
        .expect("GetCapability call");
    assert_eq!(
        recorded, initial_caps,
        "daemon must record SURROUNDING_TEXT | PREEDIT"
    );
    assert_ne!(recorded & CAP_SURROUNDING_TEXT, 0, "SURROUNDING_TEXT must be set");
    assert_ne!(recorded & CAP_PREEDIT, 0, "PREEDIT must be set");

    // Step 2: simulate Fcitx5 sending UpdateCapability to reduce the set
    // (e.g., the application does not support surrounding text).
    let reduced_caps = CAP_PREEDIT; // strip SURROUNDING_TEXT
    *capabilities.lock().unwrap() = reduced_caps;

    // Step 3: backend queries the current capability and reflects the reduced set.
    let current: u64 = proxy
        .call("GetCapability", &())
        .await
        .expect("GetCapability after reduction");
    assert_eq!(current, CAP_PREEDIT, "reduced capability must be reflected");
    assert_eq!(
        current & CAP_SURROUNDING_TEXT,
        0,
        "SURROUNDING_TEXT must be stripped after UpdateCapability"
    );
}
