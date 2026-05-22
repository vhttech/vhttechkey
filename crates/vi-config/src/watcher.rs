use std::path::{Path, PathBuf};
use std::sync::Arc;

use notify::Watcher as _;
use tokio::sync::watch;

use crate::error::ConfigError;
use crate::io::{load, SharedConfig};

/// A live-reloading config handle.
///
/// Subscribers call `.receiver()` to get a `watch::Receiver<SharedConfig>`;
/// they can `.borrow()` for the current value or `.changed().await` to wait
/// for the next reload.
pub struct ConfigWatcher {
    rx: watch::Receiver<SharedConfig>,
    /// Kept alive so the OS watcher keeps running until this is dropped.
    _watcher: notify::RecommendedWatcher,
}

impl ConfigWatcher {
    /// Return a clone of the receiver suitable for subscribing to config updates.
    pub fn receiver(&self) -> watch::Receiver<SharedConfig> {
        self.rx.clone()
    }

    /// Borrow the current config value.
    pub fn current(&self) -> SharedConfig {
        Arc::clone(&self.rx.borrow())
    }
}

/// Start watching `path` for changes.
///
/// Loads the initial config synchronously, then spawns an OS-level file
/// watcher whose callback sends new configs over a `tokio::sync::watch`
/// channel whenever the file is modified.
pub fn watch(path: impl AsRef<Path>) -> Result<ConfigWatcher, ConfigError> {
    let path: PathBuf = path.as_ref().to_path_buf();
    let initial = Arc::new(load(&path)?);
    let (tx, rx) = watch::channel(initial);

    let watcher_path = path.clone();
    let mut watcher =
        notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
            if let Ok(event) = res {
                if event.kind.is_modify() {
                    match load(&watcher_path) {
                        Ok(cfg) => {
                            tx.send(Arc::new(cfg)).ok();
                        }
                        Err(e) => {
                            tracing::error!(path = %watcher_path.display(), "config reload failed: {e}");
                        }
                    }
                }
            }
        })
        .map_err(ConfigError::Watch)?;

    watcher
        .watch(&path, notify::RecursiveMode::NonRecursive)
        .map_err(ConfigError::Watch)?;

    Ok(ConfigWatcher { rx, _watcher: watcher })
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::io::save;
    use crate::schema::Config;

    #[tokio::test]
    async fn watch_loads_initial_config() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let cfg = Config::default();
        save(&path, &cfg).unwrap();

        let watcher = watch(&path).unwrap();
        let current = watcher.current();
        assert_eq!(current.active_profile, cfg.active_profile);
    }
}
