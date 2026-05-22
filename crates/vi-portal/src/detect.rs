/// The sandbox or managed-runtime environment detected for the current process.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SandboxKind {
    /// Running inside a Flatpak sandbox (`/.flatpak-info` is present, or
    /// `/run/host/etc/os-release` is bind-mounted by the Flatpak runtime).
    Flatpak,
    /// Running inside a Snap package (`$SNAP` is set).
    Snap,
    /// Running inside an AppImage bundle (`$APPIMAGE` is set).
    AppImage,
    /// Running inside an Electron/Chromium renderer process.
    Electron,
    /// No recognised sandbox environment detected.
    None,
}

/// Detect the sandbox or managed-runtime environment for the current process.
///
/// Checks, in order:
/// 1. Flatpak  — `/.flatpak-info` exists, or `/run/host/etc/os-release` is
///    bind-mounted there by the Flatpak runtime.
/// 2. Snap     — `$SNAP` environment variable is set.
/// 3. AppImage — `$APPIMAGE` environment variable is set.
/// 4. Electron — `$ELECTRON_RUN_AS_NODE` is set, or the executable path
///    contains "electron".
pub fn detect_sandbox() -> SandboxKind {
    if std::path::Path::new("/.flatpak-info").exists()
        || std::path::Path::new("/run/host/etc/os-release").exists()
    {
        return SandboxKind::Flatpak;
    }
    if std::env::var_os("SNAP").is_some() {
        return SandboxKind::Snap;
    }
    if std::env::var_os("APPIMAGE").is_some() {
        return SandboxKind::AppImage;
    }
    if is_electron() {
        return SandboxKind::Electron;
    }
    SandboxKind::None
}

/// Returns `true` when the process is running inside a known sandbox or
/// managed runtime that restricts direct compositor socket access.
///
/// Checks, in order:
/// * Flatpak  – `/.flatpak-info` is present or `/run/host/etc/os-release`
///   is bind-mounted there by the Flatpak runtime.
/// * Snap     – the `$SNAP` environment variable is set.
/// * AppImage – the `$APPIMAGE` environment variable is set.
/// * Electron – the `$ELECTRON_RUN_AS_NODE` environment variable is set, or
///   the process executable path contains "electron".
pub fn is_sandboxed() -> bool {
    detect_sandbox() != SandboxKind::None
}

/// Returns `true` when the current process appears to be an Electron app.
///
/// Electron embeds Chromium and uses a custom IPC bridge for IME; direct
/// Wayland/X11 compositor connections may be unavailable or unreliable.
pub fn is_electron() -> bool {
    if std::env::var_os("ELECTRON_RUN_AS_NODE").is_some() {
        return true;
    }
    // Resolve the process executable and check whether it is an Electron binary.
    // This covers packaged apps that do not set ELECTRON_RUN_AS_NODE.
    if let Ok(exe) = std::fs::read_link("/proc/self/exe") {
        if exe.to_string_lossy().to_lowercase().contains("electron") {
            return true;
        }
    }
    false
}
