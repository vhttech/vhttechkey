//! Unix domain socket IPC between the daemon and the UI / helper tools.
//!
//! Protocol: newline-delimited JSON over a Unix socket at
//! `$XDG_RUNTIME_DIR/vi-daemon.sock` (or `/tmp/vi-daemon-{uid}.sock`).
//!
//! Each message is a JSON object followed by `\n`.

use std::collections::HashMap;
use std::fs::Permissions;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::{UnixListener, UnixStream},
};
use tracing::{error, info, warn};
use vi_config::Config;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "cmd", rename_all = "snake_case")]
pub enum Request {
    /// Query daemon status.
    Status,
    /// Switch the active input method.
    SetMethod { method: String },
    /// Update the global key bindings.
    SetKeyBindings { bindings: HashMap<String, String> },
    /// Gracefully shut down the daemon.
    Shutdown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Response {
    Ok,
    Status {
        backend: String,
        method: String,
        preedit: String,
    },
    Error {
        message: String,
    },
}

/// Compute the Unix socket path.
pub fn socket_path() -> PathBuf {
    let uid = nix::unistd::getuid().as_raw();
    if let Ok(dir) = std::env::var("XDG_RUNTIME_DIR") {
        PathBuf::from(dir).join("vi-daemon.sock")
    } else {
        PathBuf::from(format!("/tmp/vi-daemon-{uid}.sock"))
    }
}

/// Bind the Unix socket and accept client connections.
///
/// `read_timeout` is how long the server waits for a complete line from a
/// client before closing the connection (prevents hung clients blocking tasks).
pub async fn serve<F>(path: PathBuf, handler: F, read_timeout: Duration)
where
    F: Fn(Request) -> Response + Send + Sync + 'static,
{
    // Remove stale socket from a previous run.
    match std::fs::remove_file(&path) {
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => warn!("IPC: failed to remove stale socket {path:?}: {e}"),
    }
    let listener = match UnixListener::bind(&path) {
        Ok(l) => l,
        Err(e) => {
            warn!("IPC: failed to bind {path:?}: {e}");
            return;
        }
    };
    if let Err(e) = std::fs::set_permissions(&path, Permissions::from_mode(0o600)) {
        warn!("IPC: failed to set socket permissions on {path:?}: {e}");
    }
    info!("IPC: listening at {path:?}");
    let handler = std::sync::Arc::new(handler);
    let mut handles: Vec<tokio::task::JoinHandle<()>> = Vec::new();
    loop {
        handles.retain(|h| !h.is_finished());
        match listener.accept().await {
            Ok((stream, _)) => {
                // Reject connections from other users via SO_PEERCRED.
                #[cfg(target_os = "linux")]
                {
                    use nix::sys::socket::{getsockopt, sockopt::PeerCredentials};
                    match getsockopt(&stream, PeerCredentials) {
                        Ok(cred) if cred.uid() != nix::unistd::getuid().as_raw() => {
                            warn!("IPC: rejecting connection from uid {}", cred.uid());
                            continue;
                        }
                        Err(e) => {
                            warn!("IPC: getsockopt(SO_PEERCRED) failed: {e}; rejecting connection");
                            continue;
                        }
                        Ok(_) => {}
                    }
                }
                let h = std::sync::Arc::clone(&handler);
                handles.push(tokio::spawn(handle_connection(stream, h, read_timeout)));
            }
            Err(e) => {
                warn!("IPC: accept error: {e}");
                break;
            }
        }
    }
    for handle in handles {
        handle.abort();
    }
}

async fn handle_connection<F>(
    stream: UnixStream,
    handler: std::sync::Arc<F>,
    read_timeout: Duration,
) where
    F: Fn(Request) -> Response + Send + Sync + 'static,
{
    let (reader, mut writer) = stream.into_split();
    let mut lines = BufReader::new(reader).lines();
    loop {
        let line = match tokio::time::timeout(read_timeout, lines.next_line()).await {
            Err(_elapsed) => {
                warn!("IPC: client read timed out after {read_timeout:?}; closing connection");
                break;
            }
            Ok(Ok(Some(line))) => line,
            Ok(Ok(None)) | Ok(Err(_)) => break,
        };
        let request: Request = match serde_json::from_str(&line) {
            Ok(r) => r,
            Err(e) => {
                let resp = Response::Error {
                    message: format!("parse error: {e}"),
                };
                let _ = write_response(&mut writer, &resp).await;
                continue;
            }
        };
        let h = std::sync::Arc::clone(&handler);
        let response = tokio::task::spawn_blocking(move || {
            match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| h(request))) {
                Ok(resp) => resp,
                Err(_) => {
                    tracing::error!("IPC: handler panicked; returning error to client");
                    Response::Error {
                        message: "internal error".to_string(),
                    }
                }
            }
        })
        .await
        .unwrap_or_else(|e| {
            error!("IPC: handler task failed: {e}");
            Response::Error {
                message: "internal error".to_string(),
            }
        });
        if let Err(e) = write_response(&mut writer, &response).await {
            warn!("IPC: failed to write response: {e}");
            break;
        }
    }
}

async fn write_response<W: AsyncWriteExt + Unpin>(
    w: &mut W,
    resp: &Response,
) -> std::io::Result<()> {
    let mut json = serde_json::to_string(resp)
        .unwrap_or_else(|_| r#"{"type":"error","message":"serialization failed"}"#.into());
    json.push('\n');
    w.write_all(json.as_bytes()).await
}

/// Validate `bindings` and store them in `config`, returning the appropriate
/// [`Response`].  Extracted so it can be unit-tested without a running socket.
pub fn apply_key_bindings(
    config: &Arc<RwLock<Config>>,
    bindings: HashMap<String, String>,
) -> Response {
    for (action, key) in &bindings {
        if action.trim().is_empty() || key.trim().is_empty() {
            return Response::Error {
                message: "empty action or key".into(),
            };
        }
    }
    let mut guard = match config.write() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    };
    guard.set_key_bindings(bindings);
    Response::Ok
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_config() -> Arc<RwLock<Config>> {
        Arc::new(RwLock::new(Config::default()))
    }

    #[test]
    fn set_key_bindings_ok() {
        let config = make_config();
        let bindings: HashMap<String, String> = [("Toggle IME".into(), "ctrl+space".into())]
            .into_iter()
            .collect();
        let resp = apply_key_bindings(&config, bindings.clone());
        assert!(matches!(resp, Response::Ok));
        assert_eq!(config.read().unwrap().key_bindings, bindings);
    }

    #[test]
    fn set_key_bindings_rejects_empty_action() {
        let config = make_config();
        let bindings: HashMap<String, String> =
            [("".into(), "ctrl+space".into())].into_iter().collect();
        let resp = apply_key_bindings(&config, bindings);
        assert!(matches!(resp, Response::Error { .. }));
        assert!(config.read().unwrap().key_bindings.is_empty());
    }

    #[test]
    fn set_key_bindings_rejects_empty_binding() {
        let config = make_config();
        let bindings: HashMap<String, String> =
            [("Toggle IME".into(), "".into())].into_iter().collect();
        let resp = apply_key_bindings(&config, bindings);
        assert!(matches!(resp, Response::Error { .. }));
        assert!(config.read().unwrap().key_bindings.is_empty());
    }
}
