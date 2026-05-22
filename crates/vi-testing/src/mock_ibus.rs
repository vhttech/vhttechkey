//! Minimal IBus daemon mock for integration testing.
//!
//! Registers `org.freedesktop.IBus` on the session D-Bus and captures every
//! `CommitText` and `UpdatePreeditText` method call made by the engine backend.

use std::sync::{Arc, Mutex};
use std::time::Duration;
use zbus::{interface, Connection};

/// A method call captured by the mock daemon.
#[derive(Debug, Clone)]
pub enum IbusCall {
    CommitText(String),
    UpdatePreedit { text: String, cursor: u32, visible: bool },
}

// ── D-Bus interface implementation ───────────────────────────────────────────

struct MockEngineServer {
    calls: Arc<Mutex<Vec<IbusCall>>>,
}

#[interface(name = "org.freedesktop.IBus.Engine1")]
impl MockEngineServer {
    fn commit_text(&self, text: String) {
        self.calls.lock().unwrap().push(IbusCall::CommitText(text));
    }

    fn update_preedit_text(&self, text: String, cursor: u32, visible: bool) {
        self.calls
            .lock()
            .unwrap()
            .push(IbusCall::UpdatePreedit { text, cursor, visible });
    }
}

// ── Public API ────────────────────────────────────────────────────────────────

/// A running mock IBus daemon that records calls made to it over D-Bus.
pub struct MockIbusDaemon {
    calls: Arc<Mutex<Vec<IbusCall>>>,
    /// Wrapped in Option so `simulate_restart` can take and drop the connection.
    conn: Arc<Mutex<Option<Connection>>>,
}

impl MockIbusDaemon {
    /// Register `org.freedesktop.IBus` on the session bus and start listening.
    /// Returns `None` when the name is already taken (i.e. real IBus is running)
    /// — callers should skip the test in that case.
    pub async fn start() -> Option<Self> {
        let calls = Arc::new(Mutex::new(Vec::<IbusCall>::new()));
        match Self::make_connection(Arc::clone(&calls)).await {
            Ok(conn) => Some(Self { calls, conn: Arc::new(Mutex::new(Some(conn))) }),
            Err(e) => {
                eprintln!("SKIP MockIbusDaemon: {e}");
                None
            }
        }
    }

    async fn make_connection(calls: Arc<Mutex<Vec<IbusCall>>>) -> Result<Connection, String> {
        let conn = Connection::session()
            .await
            .map_err(|e| format!("session D-Bus unavailable: {e}"))?;

        conn.object_server()
            .at("/org/freedesktop/IBus", MockEngineServer { calls })
            .await
            .map_err(|e| format!("register IBus object: {e}"))?;

        conn.request_name("org.freedesktop.IBus")
            .await
            .map_err(|e| format!("acquire org.freedesktop.IBus name (NameTaken — run with dbus-run-session): {e}"))?;

        Ok(conn)
    }

    /// Simulate a daemon restart: drop the current connection, wait 50 ms, then
    /// reconnect under the same well-known name.  The call log is preserved.
    pub async fn simulate_restart(&self) {
        // Take and drop the connection without holding the lock across the await.
        let old = self.conn.lock().unwrap().take();
        drop(old);
        tokio::time::sleep(Duration::from_millis(50)).await;
        let new_conn = Self::make_connection(Arc::clone(&self.calls))
            .await
            .expect("reconnect after simulated restart");
        *self.conn.lock().unwrap() = Some(new_conn);
    }

    /// Return a snapshot of all calls captured so far.
    pub fn calls(&self) -> Vec<IbusCall> {
        self.calls.lock().unwrap().clone()
    }

    /// Clear the captured call log.
    pub fn clear_calls(&self) {
        self.calls.lock().unwrap().clear();
    }
}
