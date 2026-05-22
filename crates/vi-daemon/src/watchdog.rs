//! systemd watchdog integration.
//!
//! Sends `READY=1` to the systemd notification socket when the daemon is fully
//! initialised, then sends `WATCHDOG=1` heartbeats at the interval specified
//! by `$WATCHDOG_USEC` (half the watchdog timeout to stay well under the
//! deadline).

use std::os::unix::io::AsRawFd;
use std::time::Duration;

use tokio::time::sleep;
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, warn};

/// Notify systemd that the service is ready (`READY=1`).
///
/// Does nothing if `$NOTIFY_SOCKET` is not set.
pub fn notify_ready() {
    sd_notify("READY=1");
}

/// Spawn a background task that sends `WATCHDOG=1` heartbeats.
///
/// Does nothing if `$WATCHDOG_USEC` is not set.
pub fn spawn_watchdog(token: CancellationToken) {
    let watchdog_usec: u64 = match std::env::var("WATCHDOG_USEC")
        .ok()
        .and_then(|v| v.parse().ok())
    {
        Some(u) => u,
        None => return,
    };
    let interval = Duration::from_micros((watchdog_usec / 2).max(1));
    tokio::spawn(async move {
        let mut consecutive_failures: u32 = 0;
        loop {
            tokio::select! {
                _ = token.cancelled() => break,
                _ = sleep(interval) => {
                    debug!("systemd watchdog heartbeat");
                    if sd_notify("WATCHDOG=1") {
                        consecutive_failures = 0;
                    } else {
                        consecutive_failures += 1;
                        if consecutive_failures >= 3 {
                            warn!("watchdog: 3 consecutive sd_notify failures; systemd is likely dead");
                            break;
                        }
                    }
                }
            }
        }
    });
}

/// Spawn a background task that calls `reconnect()` in an exponential-backoff
/// loop, intended to restore the backend D-Bus connection after ibus-daemon or
/// fcitx5 restarts.
///
/// When `reconnect()` resolves to `true` the delay resets to its initial value
/// (100 ms) and the loop waits again.  When it resolves to `false` the delay
/// doubles (capped at 30 s) before the next attempt.
///
/// Preedit is preserved transparently because the composition engine lives
/// independently of the backend connection.
pub fn spawn_reconnect_loop<F, Fut>(name: &'static str, reconnect: F)
where
    F: Fn() -> Fut + Send + Sync + 'static,
    Fut: std::future::Future<Output = bool> + Send + 'static,
{
    tokio::spawn(async move {
        let mut delay = Duration::from_millis(100);
        let max_delay = Duration::from_secs(30);
        loop {
            tokio::time::sleep(delay).await;
            if reconnect().await {
                info!("{name}: backend reconnected");
                delay = Duration::from_millis(100);
            } else {
                warn!("{name}: reconnect attempt failed; retrying in {delay:?}");
                delay = (delay * 2).min(max_delay);
            }
        }
    });
}

fn sd_notify(msg: &str) -> bool {
    let socket_path = match std::env::var("NOTIFY_SOCKET") {
        Ok(p) => p,
        Err(_) => return true,
    };
    // Abstract sockets (starting with '@') require raw socket calls.
    // For path-based sockets we use UnixDatagram directly.
    if let Some(abstract_path) = socket_path.strip_prefix('@') {
        return sd_notify_abstract(msg, abstract_path);
    }
    use std::os::unix::net::UnixDatagram;
    match UnixDatagram::unbound() {
        Ok(sock) => {
            if let Err(e) = sock.send_to(msg.as_bytes(), &socket_path) {
                warn!("sd_notify send failed: {e}");
                false
            } else {
                true
            }
        }
        Err(e) => {
            warn!("sd_notify socket creation failed: {e}");
            false
        }
    }
}

/// Send a systemd notification to an abstract Unix socket.
///
/// Uses `nix` to construct the abstract socket address (a null-byte prefix
/// is not representable in Rust's `std::path::Path`).
fn sd_notify_abstract(msg: &str, abstract_name: &str) -> bool {
    use nix::sys::socket::{self, AddressFamily, SockFlag, SockType, UnixAddr};
    let Ok(fd) = socket::socket(
        AddressFamily::Unix,
        SockType::Datagram,
        SockFlag::empty(),
        None,
    ) else {
        warn!("sd_notify: failed to create abstract datagram socket");
        return false;
    };
    let Ok(addr) = UnixAddr::new_abstract(abstract_name.as_bytes()) else {
        warn!("sd_notify: invalid abstract socket name");
        return false;
    };
    if let Err(e) =
        socket::sendto(fd.as_raw_fd(), msg.as_bytes(), &addr, socket::MsgFlags::empty())
    {
        warn!("sd_notify abstract send failed: {e}");
        false
    } else {
        true
    }
}
