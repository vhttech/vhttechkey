//! Minimal Wayland compositor mock for integration testing.
//!
//! Creates a Unix socket, sets `WAYLAND_DISPLAY`, and accepts client connections.
//! Full Wayland binary-protocol decoding is left as a stub; the struct provides
//! the correct surface for testing environment setup and WAYLAND_DISPLAY routing.

use std::os::unix::net::UnixListener;
use std::sync::{Arc, Mutex};
use std::thread;

/// Which compositor behaviour to emulate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompositorProfile {
    Gnome,
    KdeWayland,
    Sway,
    Minimal,
    /// Hyprland compositor — advertises `hyprland_global_shortcuts_manager_v1`.
    Hyprland,
    /// Niri compositor — advertises `niri_ipc`.
    Niri,
}

impl CompositorProfile {
    /// The Wayland global interface names this profile would advertise in the
    /// registry (beyond the core globals every compositor exposes).
    ///
    /// In a full implementation the mock's connection handler would encode
    /// these into `wl_registry::global` events; for now the list drives
    /// assertion helpers such as [`MockCompositor::assert_advertises_global`].
    pub fn advertised_globals(&self) -> &'static [&'static str] {
        match self {
            Self::Gnome => &["zwp_text_input_manager_v3", "gtk_shell_v1"],
            Self::KdeWayland => &["zwp_text_input_manager_v3", "org_kde_plasma_shell"],
            Self::Sway => &["zwp_text_input_manager_v3"],
            Self::Minimal => &[],
            // Advertises the Hyprland-specific shortcut manager protocol.
            Self::Hyprland => &[
                "zwp_text_input_manager_v3",
                "hyprland_global_shortcuts_manager_v1",
            ],
            // Advertises the Niri IPC protocol.
            Self::Niri => &["zwp_text_input_manager_v3", "niri_ipc"],
        }
    }
}

/// A preedit update event recorded by the mock.
#[derive(Debug, Clone)]
pub struct PreeditUpdate {
    pub text: String,
    pub cursor_begin: i32,
    pub cursor_end: i32,
}

/// A running mock Wayland compositor that records commits and preedit updates.
pub struct MockCompositor {
    socket_name: String,
    socket_dir: String,
    pub profile: CompositorProfile,
    commits: Arc<Mutex<Vec<String>>>,
    preedit_updates: Arc<Mutex<Vec<PreeditUpdate>>>,
}

impl MockCompositor {
    /// Spawn the mock compositor, bind a socket, and start accepting connections.
    pub fn start(profile: CompositorProfile) -> Self {
        let socket_dir = std::env::temp_dir().to_string_lossy().into_owned();
        let socket_name = format!("wayland-mock-{}", std::process::id());
        let socket_path = format!("{socket_dir}/{socket_name}");

        // Remove any leftover socket from a previous run.
        let _ = std::fs::remove_file(&socket_path);

        let listener = UnixListener::bind(&socket_path).expect("bind mock Wayland socket");

        let commits = Arc::new(Mutex::new(Vec::<String>::new()));
        let preedit_updates = Arc::new(Mutex::new(Vec::<PreeditUpdate>::new()));

        thread::spawn(move || {
            for stream in listener.incoming() {
                match stream {
                    Ok(_stream) => {
                        // Connections accepted; full Wayland protocol parsing
                        // (zwp_text_input_v3 events) would be decoded here in a
                        // production-grade mock.
                    }
                    Err(_) => break,
                }
            }
        });

        Self {
            socket_name,
            socket_dir,
            profile,
            commits,
            preedit_updates,
        }
    }

    /// Set `WAYLAND_DISPLAY` and `XDG_RUNTIME_DIR` so clients connect here.
    pub fn set_wayland_display_env(&self) {
        std::env::set_var("WAYLAND_DISPLAY", &self.socket_name);
        std::env::set_var("XDG_RUNTIME_DIR", &self.socket_dir);
    }

    /// Commits sent by clients via the text-input protocol.
    pub fn received_commits(&self) -> Vec<String> {
        self.commits.lock().unwrap().clone()
    }

    /// Preedit updates received from clients.
    pub fn received_preedit_updates(&self) -> Vec<PreeditUpdate> {
        self.preedit_updates.lock().unwrap().clone()
    }

    /// Panic if any committed string is not in NFC form.
    ///
    /// Call this after driving the engine through a scenario to ensure the
    /// compositor only receives well-formed Unicode.
    pub fn assert_all_commits_nfc(&self) {
        use unicode_normalization::is_nfc;
        let commits = self.received_commits();
        for (i, commit) in commits.iter().enumerate() {
            assert!(
                is_nfc(commit),
                "commit[{i}] is not NFC-normalised: {commit:?}"
            );
        }
    }

    /// Assert that the profile would advertise `global_name` in the Wayland
    /// registry (see [`CompositorProfile::advertised_globals`]).
    pub fn assert_advertises_global(&self, global_name: &str) {
        assert!(
            self.profile.advertised_globals().contains(&global_name),
            "profile {:?} does not advertise global {:?}; advertised: {:?}",
            self.profile,
            global_name,
            self.profile.advertised_globals(),
        );
    }
}

impl Drop for MockCompositor {
    fn drop(&mut self) {
        let path = format!("{}/{}", self.socket_dir, self.socket_name);
        let _ = std::fs::remove_file(path);
    }
}
