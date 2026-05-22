//! Unix signal handling: SIGTERM triggers a graceful shutdown with preedit
//! serialization to /tmp so the daemon can resume after a restart.

use std::{fs::OpenOptions, path::PathBuf, time::Duration};

use serde::{Deserialize, Serialize};
use tokio::signal::unix::{signal, SignalKind};
use tracing::info;

/// RAII guard that removes the lock file on drop.
struct LockGuard(PathBuf);

impl Drop for LockGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// Preedit snapshot written to disk on SIGTERM for graceful restart.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PreeditSnapshot {
    pub preedit: String,
    /// Cursor position expressed as a Unicode scalar count (not a byte offset).
    pub cursor: usize,
    pub method: String,
}

impl PreeditSnapshot {
    pub fn snapshot_path() -> PathBuf {
        let uid = nix::unistd::getuid().as_raw();
        PathBuf::from(format!("/tmp/vi-daemon-{uid}-preedit.json"))
    }

    pub fn save(&self) {
        let path = Self::snapshot_path();
        let lock_path = PathBuf::from(format!("{}.lock", path.display()));

        // Acquire exclusive write access. create_new(true) uses O_CREAT|O_EXCL so
        // it fails with AlreadyExists if another daemon instance is already writing.
        let _guard = match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&lock_path)
        {
            Ok(_f) => LockGuard(lock_path),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                tracing::warn!(
                    "SIGTERM: skipping snapshot, another daemon instance holds the lock"
                );
                return;
            }
            Err(e) => {
                tracing::warn!("SIGTERM: failed to create lock file: {e}");
                return;
            }
        };

        let tmp_path = PathBuf::from(format!("{}.tmp", path.display()));
        match serde_json::to_string(self) {
            Ok(json) => {
                if let Err(e) = std::fs::write(&tmp_path, &json) {
                    tracing::warn!("SIGTERM: failed to write tmp snapshot: {e}");
                    return;
                }
                // rename(2) is atomic on Linux: readers never see a partial write.
                match std::fs::rename(&tmp_path, &path) {
                    Ok(()) => info!("SIGTERM: preedit snapshot written to {path:?}"),
                    Err(e) => {
                        tracing::warn!("SIGTERM: failed to rename snapshot: {e}");
                        let _ = std::fs::remove_file(&tmp_path);
                    }
                }
            }
            Err(e) => {
                tracing::warn!("SIGTERM: failed to serialize preedit snapshot: {e}");
            }
        }
    }

    pub fn load() -> Option<Self> {
        let path = Self::snapshot_path();
        let json = std::fs::read_to_string(&path).ok()?;
        // Remove after reading so it doesn't persist across non-restarts.
        let _ = std::fs::remove_file(&path);
        let snap: PreeditSnapshot = serde_json::from_str(&json).ok()?;
        let char_count = snap.preedit.chars().count();
        if snap.cursor > char_count {
            tracing::warn!(
                "SIGTERM: discarding snapshot with out-of-range cursor {} (preedit char count {})",
                snap.cursor,
                char_count,
            );
            return None;
        }
        Some(snap)
    }
}

/// Wait for SIGTERM, then return. Caller handles snapshot and shutdown.
pub async fn wait_for_sigterm() {
    let mut sigterm =
        signal(SignalKind::terminate()).expect("SIGTERM handler registration should not fail");
    sigterm.recv().await;
    info!("Received SIGTERM — shutting down gracefully");
}

/// Write `snap` to disk with a 2-second deadline.
///
/// On timeout or I/O failure the error is logged and shutdown proceeds normally.
pub async fn save_snapshot(snap: PreeditSnapshot) {
    match tokio::time::timeout(
        Duration::from_secs(2),
        tokio::task::spawn_blocking(move || snap.save()),
    )
    .await
    {
        Ok(Ok(())) => {}
        Ok(Err(e)) => tracing::error!("SIGTERM: snapshot save task failed: {e}"),
        Err(_) => {
            tracing::error!("SIGTERM: snapshot write timed out after 2 s, proceeding with shutdown")
        }
    }
}
