//! egui-based settings UI for vhttechkey — UniKey-style compact dialog.

pub mod candidate;

use std::io::{BufRead, BufReader, Write as IoWrite};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use egui::{Color32, Frame, Margin, RichText, Ui};
use serde::{Deserialize, Serialize};

/// Renderer ưu tiên — wgpu tránh lỗi GLX trên X11 không có OpenGL thật.
pub fn preferred_renderer() -> eframe::Renderer {
    eframe::Renderer::Wgpu
}

/// `NativeOptions` cho cửa sổ cài đặt chính.
pub fn main_window_options() -> eframe::NativeOptions {
    eframe::NativeOptions {
        renderer: preferred_renderer(),
        viewport: egui::ViewportBuilder::default()
            .with_title("VHTTechKey")
            .with_inner_size([460.0, 520.0])
            .with_min_inner_size([380.0, 440.0])
            .with_resizable(true),
        ..Default::default()
    }
}

// ─── IPC ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "cmd", rename_all = "snake_case")]
enum IpcRequest {
    Status,
    SetMethod { method: String },
    SetCharset { charset: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum IpcResponse {
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

fn socket_path() -> PathBuf {
    let uid = nix::unistd::getuid().as_raw();
    match std::env::var("XDG_RUNTIME_DIR") {
        Ok(dir) => PathBuf::from(dir).join("vi-daemon.sock"),
        Err(_) => PathBuf::from(format!("/tmp/vi-daemon-{uid}.sock")),
    }
}

struct IpcClient {
    writer: Option<UnixStream>,
    reader: Option<BufReader<UnixStream>>,
    path: PathBuf,
    last_attempt: Instant,
    pub last_latency_ms: Option<f64>,
}

impl IpcClient {
    fn new() -> Self {
        Self {
            writer: None,
            reader: None,
            path: socket_path(),
            last_attempt: Instant::now()
                .checked_sub(Duration::from_secs(10))
                .unwrap_or_else(Instant::now),
            last_latency_ms: None,
        }
    }

    fn is_connected(&self) -> bool {
        self.writer.is_some()
    }

    fn try_connect(&mut self) {
        if self.is_connected() || self.last_attempt.elapsed() < Duration::from_secs(2) {
            return;
        }
        self.last_attempt = Instant::now();
        let Ok(stream) = UnixStream::connect(&self.path) else {
            return;
        };
        let Ok(read_clone) = stream.try_clone() else {
            return;
        };
        let _ = stream.set_write_timeout(Some(Duration::from_millis(500)));
        let _ = read_clone.set_read_timeout(Some(Duration::from_millis(300)));
        self.writer = Some(stream);
        self.reader = Some(BufReader::new(read_clone));
    }

    fn send(&mut self, req: &IpcRequest) -> Option<IpcResponse> {
        self.try_connect();
        let writer = self.writer.as_mut()?;
        let Ok(json) = serde_json::to_string(req) else {
            return None;
        };
        let t0 = Instant::now();
        if writer.write_all(format!("{json}\n").as_bytes()).is_err() {
            self.reset();
            return None;
        }
        let reader = self.reader.as_mut()?;
        let mut line = String::new();
        match reader.read_line(&mut line) {
            Ok(0) | Err(_) => {
                self.reset();
                None
            }
            Ok(_) => {
                self.last_latency_ms = Some(t0.elapsed().as_secs_f64() * 1000.0);
                serde_json::from_str(line.trim()).ok()
            }
        }
    }

    fn reset(&mut self) {
        self.writer = None;
        self.reader = None;
    }
}

// ─── Domain: Kiểu gõ ───────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputMethod {
    Telex,
    Vni,
    Viqr,
}

impl InputMethod {
    const ALL: &'static [Self] = &[Self::Telex, Self::Vni, Self::Viqr];

    fn label(self) -> &'static str {
        match self {
            Self::Telex => "Telex",
            Self::Vni => "VNI",
            Self::Viqr => "VIQR",
        }
    }

    fn ipc_name(self) -> &'static str {
        match self {
            Self::Telex => "telex",
            Self::Vni => "vni",
            Self::Viqr => "viqr",
        }
    }

    fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "telex" => Some(Self::Telex),
            "vni" => Some(Self::Vni),
            "viqr" => Some(Self::Viqr),
            _ => None,
        }
    }

    fn hint(self) -> &'static str {
        match self {
            Self::Telex => "aa→â  oo→ô  dd→đ  s/f/r/x/j dấu thanh",
            Self::Vni => "a6→â  o6→ô  d9→đ  1/2/3/4/5 dấu thanh",
            Self::Viqr => "a^→â  o^→ô  dd→đ  ' ` ? ~ .  dấu thanh",
        }
    }
}

// ─── Domain: Bảng mã ───────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputCharset {
    Unicode,
    Vni,
    Viqr,
    Tcvn3,
}

impl OutputCharset {
    const ALL: &'static [Self] = &[Self::Unicode, Self::Vni, Self::Viqr, Self::Tcvn3];

    fn label(self) -> &'static str {
        match self {
            Self::Unicode => "Unicode (UTF-8)",
            Self::Vni => "VNI Windows",
            Self::Viqr => "VIQR (ASCII)",
            Self::Tcvn3 => "TCVN3 (ABC)",
        }
    }

    fn ipc_name(self) -> &'static str {
        match self {
            Self::Unicode => "unicode",
            Self::Vni => "vni",
            Self::Viqr => "viqr",
            Self::Tcvn3 => "tcvn3",
        }
    }

    fn note(self) -> &'static str {
        match self {
            Self::Unicode => "Chuẩn hiện đại — NFC normalized",
            Self::Vni => "Tương thích VNI Windows cũ",
            Self::Viqr => "ASCII thuần — dùng cho terminal cũ",
            Self::Tcvn3 => "Bảng mã TCVN3 (kế thừa)",
        }
    }
}

// ─── Options ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
struct Options {
    enabled: bool,
    spell_check: bool,
    dd_freestyle: bool,
    restore_on_backspace: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            enabled: true,
            spell_check: false,
            dd_freestyle: true,
            restore_on_backspace: true,
        }
    }
}

// ─── Tab ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tab {
    Main,
    Setup,
    TypingTest,
    About,
}

// ─── Setup helpers ─────────────────────────────────────────────────────────

fn cmd_exists(cmd: &str) -> bool {
    std::process::Command::new("which")
        .arg(cmd)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn find_daemon_pid() -> Option<u32> {
    let out = std::process::Command::new("pgrep")
        .arg("-x")
        .arg("vi-daemon")
        .output()
        .ok()?;
    if out.status.success() {
        String::from_utf8(out.stdout)
            .ok()?
            .lines()
            .next()?
            .trim()
            .parse()
            .ok()
    } else {
        None
    }
}

fn ibus_component_registered() -> bool {
    let home = std::env::var("HOME").unwrap_or_default();
    PathBuf::from(home)
        .join(".local/share/ibus/component/vhttechkey.xml")
        .exists()
}

fn fcitx5_addon_installed() -> bool {
    let home = std::env::var("HOME").unwrap_or_default();
    PathBuf::from(home)
        .join(".local/share/fcitx5/addon/vhttechkey.conf")
        .exists()
}

fn autostart_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_default();
    PathBuf::from(home).join(".config/autostart/vhttechkey-ui.desktop")
}

fn daemon_bin_path() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?;
    let p = dir.join("vi-daemon");
    if p.exists() {
        Some(p)
    } else {
        None
    }
}

fn ui_bin_path() -> Option<PathBuf> {
    std::env::current_exe().ok()
}

// ─── IBus runtime helpers ───────────────────────────────────────────────────

fn ibus_get_current_engine() -> Option<String> {
    let out = std::process::Command::new("ibus")
        .arg("engine")
        .output()
        .ok()?;
    if out.status.success() {
        let s = String::from_utf8(out.stdout).ok()?;
        let name = s.trim().to_string();
        if name.is_empty() {
            None
        } else {
            Some(name)
        }
    } else {
        None
    }
}

fn gnome_has_vhttechkey() -> bool {
    let out = std::process::Command::new("gsettings")
        .args(["get", "org.gnome.desktop.input-sources", "sources"])
        .output();
    match out {
        Ok(o) => String::from_utf8_lossy(&o.stdout).contains("'vhttechkey'"),
        Err(_) => false,
    }
}

fn gnome_add_vhttechkey() -> Result<String, String> {
    let out = std::process::Command::new("gsettings")
        .args(["get", "org.gnome.desktop.input-sources", "sources"])
        .output()
        .map_err(|e| e.to_string())?;
    let current = String::from_utf8_lossy(&out.stdout).trim().to_string();

    if current.contains("'vhttechkey'") {
        return Ok("Đã có vhttechkey trong GNOME input sources".to_string());
    }

    let new_val = if current == "[]" || current.is_empty() || current == "@a(ss) []" {
        "[('ibus', 'vhttechkey')]".to_string()
    } else {
        // Insert before closing ]
        let base = current.trim_end_matches(']').trim_end();
        format!("{}, ('ibus', 'vhttechkey')]", base)
    };

    let set = std::process::Command::new("gsettings")
        .args([
            "set",
            "org.gnome.desktop.input-sources",
            "sources",
            &new_val,
        ])
        .output()
        .map_err(|e| e.to_string())?;
    if set.status.success() {
        Ok("Đã thêm vhttechkey vào GNOME input sources".to_string())
    } else {
        Err(String::from_utf8_lossy(&set.stderr).trim().to_string())
    }
}

fn gnome_remove_vhttechkey() -> Result<(), String> {
    let out = std::process::Command::new("gsettings")
        .args(["get", "org.gnome.desktop.input-sources", "sources"])
        .output()
        .map_err(|e| e.to_string())?;
    let current = String::from_utf8_lossy(&out.stdout).trim().to_string();

    // Remove the ('ibus', 'vhttechkey') tuple — handle leading comma or trailing comma
    let cleaned = current
        .replace(", ('ibus', 'vhttechkey')", "")
        .replace("('ibus', 'vhttechkey'), ", "")
        .replace("('ibus', 'vhttechkey')", "");

    let set = std::process::Command::new("gsettings")
        .args([
            "set",
            "org.gnome.desktop.input-sources",
            "sources",
            &cleaned,
        ])
        .output()
        .map_err(|e| e.to_string())?;
    if set.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&set.stderr).trim().to_string())
    }
}

// ─── Fcitx5 runtime helpers ─────────────────────────────────────────────────

fn fcitx5_current_im() -> Option<String> {
    let out = std::process::Command::new("fcitx5-remote")
        .arg("-n")
        .output()
        .ok()?;
    if out.status.success() {
        let s = String::from_utf8(out.stdout).ok()?;
        let name = s.trim().to_string();
        if name.is_empty() {
            None
        } else {
            Some(name)
        }
    } else {
        None
    }
}

fn fcitx5_profile_has_vhttechkey() -> bool {
    let home = std::env::var("HOME").unwrap_or_default();
    let path = PathBuf::from(home).join(".config/fcitx5/profile");
    std::fs::read_to_string(path)
        .map(|c| c.contains("Name=vhttechkey"))
        .unwrap_or(false)
}

fn fcitx5_profile_add() -> Result<(), String> {
    let home = std::env::var("HOME").unwrap_or_default();
    let path = PathBuf::from(&home).join(".config/fcitx5/profile");

    if path.exists() {
        let content = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
        if content.contains("Name=vhttechkey") {
            return Ok(());
        }

        // Count [Groups/0/Items/N] sections to find next index
        let count = content
            .lines()
            .filter(|l| l.starts_with("[Groups/0/Items/") && l.ends_with(']'))
            .count();
        let new_section = format!("\n[Groups/0/Items/{count}]\nName=vhttechkey\nLayout=\n");
        std::fs::write(&path, format!("{content}{new_section}")).map_err(|e| e.to_string())
    } else {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let content = "[Groups/0]\nName=Default\nDefault Layout=us\nDefaultIM=keyboard-us\n\
\n[Groups/0/Items/0]\nName=keyboard-us\nLayout=\n\
\n[Groups/0/Items/1]\nName=vhttechkey\nLayout=\n\
\n[GroupOrder]\n0=Default\n";
        std::fs::write(&path, content).map_err(|e| e.to_string())
    }
}

fn fcitx5_profile_remove() -> Result<(), String> {
    let home = std::env::var("HOME").unwrap_or_default();
    let path = PathBuf::from(&home).join(".config/fcitx5/profile");
    if !path.exists() {
        return Ok(());
    }

    let content = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    // Remove the [Groups/0/Items/N] block that has Name=vhttechkey
    let mut out_lines: Vec<&str> = Vec::new();
    let mut skip = false;
    for line in content.lines() {
        if line.starts_with("[Groups/0/Items/") && line.ends_with(']') {
            // peek ahead — we handle by two-pass
            skip = false;
            out_lines.push(line);
        } else if line.starts_with('[') {
            skip = false;
            out_lines.push(line);
        } else if line == "Name=vhttechkey" {
            // Remove the whole section header we just pushed
            out_lines.pop();
            skip = true;
        } else if skip && line.is_empty() {
            skip = false;
        } else if !skip {
            out_lines.push(line);
        }
    }
    std::fs::write(&path, out_lines.join("\n")).map_err(|e| e.to_string())
}

// ─── Setup state & actions ──────────────────────────────────────────────────

struct SetupState {
    daemon_pid: Option<u32>,
    ibus_available: bool,
    ibus_registered: bool,
    ibus_current_engine: Option<String>,
    gnome_has_source: bool,
    fcitx5_available: bool,
    fcitx5_registered: bool,
    fcitx5_current_im: Option<String>,
    fcitx5_in_profile: bool,
    autostart_enabled: bool,
    last_check: Instant,
    log: Vec<(bool, String)>,
}

impl SetupState {
    fn new() -> Self {
        let zero = Instant::now()
            .checked_sub(Duration::from_secs(60))
            .unwrap_or_else(Instant::now);
        Self {
            daemon_pid: None,
            ibus_available: false,
            ibus_registered: false,
            ibus_current_engine: None,
            gnome_has_source: false,
            fcitx5_available: false,
            fcitx5_registered: false,
            fcitx5_current_im: None,
            fcitx5_in_profile: false,
            autostart_enabled: false,
            last_check: zero,
            log: Vec::new(),
        }
    }

    fn force_refresh(&mut self) {
        self.last_check = Instant::now()
            .checked_sub(Duration::from_secs(60))
            .unwrap_or_else(Instant::now);
    }

    fn refresh(&mut self) {
        if self.last_check.elapsed() < Duration::from_secs(3) {
            return;
        }
        self.last_check = Instant::now();
        self.daemon_pid = find_daemon_pid();
        self.ibus_available = cmd_exists("ibus");
        self.ibus_registered = ibus_component_registered();
        if self.ibus_available {
            self.ibus_current_engine = ibus_get_current_engine();
            self.gnome_has_source = gnome_has_vhttechkey();
        }
        self.fcitx5_available = cmd_exists("fcitx5");
        self.fcitx5_registered = fcitx5_addon_installed();
        if self.fcitx5_available {
            self.fcitx5_current_im = fcitx5_current_im();
            self.fcitx5_in_profile = fcitx5_profile_has_vhttechkey();
        }
        self.autostart_enabled = autostart_path().exists();
    }

    fn log_push(&mut self, ok: bool, msg: impl Into<String>) {
        self.log.push((ok, msg.into()));
        if self.log.len() > 30 {
            self.log.remove(0);
        }
    }

    fn start_daemon(&mut self) {
        match daemon_bin_path() {
            None => self.log_push(false, "Không tìm thấy vi-daemon bên cạnh vi-ui"),
            Some(path) => match std::process::Command::new(&path).spawn() {
                Ok(child) => {
                    self.log_push(true, format!("Daemon khởi động (PID {})", child.id()));
                    self.force_refresh();
                }
                Err(e) => self.log_push(false, format!("Không thể khởi động daemon: {e}")),
            },
        }
    }

    fn stop_daemon(&mut self) {
        if let Some(pid) = self.daemon_pid {
            let out = std::process::Command::new("kill")
                .arg(pid.to_string())
                .output();
            match out {
                Ok(o) if o.status.success() => {
                    self.log_push(true, format!("Đã dừng daemon (PID {pid})"));
                    self.daemon_pid = None;
                }
                Ok(o) => self.log_push(
                    false,
                    format!(
                        "kill thất bại: {}",
                        String::from_utf8_lossy(&o.stderr).trim()
                    ),
                ),
                Err(e) => self.log_push(false, format!("kill lỗi: {e}")),
            }
        }
    }

    fn register_ibus(&mut self) {
        let daemon_path = match daemon_bin_path() {
            Some(p) => p,
            None => {
                self.log_push(
                    false,
                    "Không tìm thấy vi-daemon — chưa build hoặc chưa cài vào hệ thống",
                );
                return;
            }
        };
        let ui_path = ui_bin_path().unwrap_or_else(|| PathBuf::from(vi_config::SYSTEM_UI));
        let paths = vi_config::IbusInstallPaths::from_binaries(&daemon_path, &ui_path);
        let xml = vi_config::component_xml(&paths);

        let home = std::env::var("HOME").unwrap_or_default();
        let dir = PathBuf::from(&home).join(".local/share/ibus/component");
        if let Err(e) = std::fs::create_dir_all(&dir) {
            self.log_push(false, format!("Không tạo được thư mục: {e}"));
            return;
        }
        if let Err(e) = std::fs::write(dir.join("vhttechkey.xml"), &xml) {
            self.log_push(false, format!("Không ghi được component XML: {e}"));
            return;
        }

        match std::process::Command::new("ibus")
            .args(["write-cache"])
            .output()
        {
            Ok(o) if o.status.success() => {
                self.log_push(
                    true,
                    "Đã đăng ký IBus engine — chạy lại ibus-daemon để áp dụng",
                );
                self.ibus_registered = true;
            }
            Ok(o) => self.log_push(
                false,
                format!(
                    "ibus write-cache lỗi: {}",
                    String::from_utf8_lossy(&o.stderr).trim()
                ),
            ),
            Err(e) => self.log_push(false, format!("ibus write-cache: {e}")),
        }
    }

    fn register_fcitx5(&mut self) {
        let daemon_path = match daemon_bin_path() {
            Some(p) => p,
            None => {
                self.log_push(false, "Không tìm thấy vi-daemon");
                return;
            }
        };

        let home = std::env::var("HOME").unwrap_or_default();
        let addon_dir = PathBuf::from(&home).join(".local/share/fcitx5/addon");
        if let Err(e) = std::fs::create_dir_all(&addon_dir) {
            self.log_push(false, format!("Không tạo được thư mục addon: {e}"));
            return;
        }

        let conf = format!(
            "[Addon]\nName=vhttechkey\nVersion=1.0\nType=InputMethod\nLibrary={}\nOnDemand=False\nEnabled=True\n",
            daemon_path.display()
        );
        match std::fs::write(addon_dir.join("vhttechkey.conf"), &conf) {
            Ok(_) => {
                self.log_push(
                    true,
                    "Đã tạo Fcitx5 addon config — khởi động lại fcitx5 để áp dụng",
                );
                self.fcitx5_registered = true;
            }
            Err(e) => self.log_push(false, format!("Không ghi được addon conf: {e}")),
        }
    }

    fn ibus_switch_to_vhttechkey(&mut self) {
        match std::process::Command::new("ibus")
            .args(["engine", "vhttechkey"])
            .output()
        {
            Ok(o) if o.status.success() => {
                self.log_push(true, "IBus: đã chuyển sang engine vhttechkey");
                self.ibus_current_engine = Some("vhttechkey".to_string());
            }
            Ok(o) => self.log_push(
                false,
                format!("ibus engine: {}", String::from_utf8_lossy(&o.stderr).trim()),
            ),
            Err(e) => self.log_push(false, format!("ibus engine: {e}")),
        }
    }

    fn ibus_add_gnome_source(&mut self) {
        match gnome_add_vhttechkey() {
            Ok(msg) => {
                self.log_push(true, msg);
                self.gnome_has_source = true;
            }
            Err(e) => self.log_push(false, format!("gsettings: {e}")),
        }
    }

    fn ibus_remove_gnome_source(&mut self) {
        match gnome_remove_vhttechkey() {
            Ok(()) => {
                self.log_push(true, "Đã xóa vhttechkey khỏi GNOME input sources");
                self.gnome_has_source = false;
            }
            Err(e) => self.log_push(false, format!("gsettings remove: {e}")),
        }
    }

    fn fcitx5_switch(&mut self) {
        match std::process::Command::new("fcitx5-remote")
            .args(["-s", "vhttechkey"])
            .output()
        {
            Ok(o) if o.status.success() => {
                self.log_push(true, "Fcitx5: đã chuyển sang vhttechkey");
                self.fcitx5_current_im = Some("vhttechkey".to_string());
            }
            Ok(o) => self.log_push(
                false,
                format!(
                    "fcitx5-remote: {}",
                    String::from_utf8_lossy(&o.stderr).trim()
                ),
            ),
            Err(e) => self.log_push(false, format!("fcitx5-remote: {e}")),
        }
    }

    fn fcitx5_add_to_profile(&mut self) {
        match fcitx5_profile_add() {
            Ok(()) => {
                self.log_push(true, "Đã thêm vhttechkey vào ~/.config/fcitx5/profile");
                self.fcitx5_in_profile = true;
                // Reload fcitx5 config
                let _ = std::process::Command::new("fcitx5-remote")
                    .arg("-r")
                    .output();
                self.log_push(true, "Đã reload Fcitx5 config");
            }
            Err(e) => self.log_push(false, format!("fcitx5 profile: {e}")),
        }
    }

    fn fcitx5_remove_from_profile(&mut self) {
        match fcitx5_profile_remove() {
            Ok(()) => {
                self.log_push(true, "Đã xóa vhttechkey khỏi Fcitx5 profile");
                self.fcitx5_in_profile = false;
                let _ = std::process::Command::new("fcitx5-remote")
                    .arg("-r")
                    .output();
            }
            Err(e) => self.log_push(false, format!("fcitx5 profile remove: {e}")),
        }
    }

    fn set_autostart(&mut self, enable: bool) {
        let path = autostart_path();
        if !enable {
            match std::fs::remove_file(&path) {
                Ok(_) => {
                    self.log_push(true, "Đã tắt tự khởi động");
                    self.autostart_enabled = false;
                }
                Err(e) => self.log_push(false, format!("Không xóa được autostart: {e}")),
            }
            return;
        }

        let exe = match ui_bin_path() {
            Some(p) => p,
            None => {
                self.log_push(false, "Không tìm được đường dẫn vi-ui");
                return;
            }
        };
        let desktop = format!(
            "[Desktop Entry]\nName=VHTTechKey\nComment=Bộ gõ tiếng Việt\nExec={}\nTerminal=false\nType=Application\nCategories=Utility;\nX-GNOME-Autostart-enabled=true\n",
            exe.display()
        );
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        match std::fs::write(&path, desktop) {
            Ok(_) => {
                self.log_push(true, "Đã bật tự khởi động");
                self.autostart_enabled = true;
            }
            Err(e) => self.log_push(false, format!("Không tạo được autostart: {e}")),
        }
    }

    fn install_to_local_bin(&mut self) {
        let home = std::env::var("HOME").unwrap_or_default();
        let bin_dir = PathBuf::from(&home).join(".local/bin");
        if let Err(e) = std::fs::create_dir_all(&bin_dir) {
            self.log_push(false, format!("Không tạo được ~/.local/bin: {e}"));
            return;
        }

        let mut any_ok = false;
        if let Some(exe) = ui_bin_path() {
            let dest = bin_dir.join("vi-ui");
            match std::fs::copy(&exe, &dest) {
                Ok(_) => {
                    self.log_push(true, format!("vi-ui → {}", dest.display()));
                    any_ok = true;
                }
                Err(e) => self.log_push(false, format!("Sao chép vi-ui thất bại: {e}")),
            }
        }
        if let Some(daemon) = daemon_bin_path() {
            let dest = bin_dir.join("vi-daemon");
            match std::fs::copy(&daemon, &dest) {
                Ok(_) => {
                    self.log_push(true, format!("vi-daemon → {}", dest.display()));
                    any_ok = true;
                }
                Err(e) => self.log_push(false, format!("Sao chép vi-daemon thất bại: {e}")),
            }
        }
        if any_ok {
            self.log_push(true, "Thêm ~/.local/bin vào PATH nếu chưa có");
        }
    }
}

// ─── Rule table ────────────────────────────────────────────────────────────

fn rules_for_method(method: InputMethod) -> &'static [(&'static str, &'static str, &'static str)] {
    match method {
        InputMethod::Telex => &[
            ("aa", "â", "a mũ"),
            ("oo", "ô", "o mũ"),
            ("ee", "ê", "e mũ"),
            ("aw", "ă", "a móc trên"),
            ("ow", "ơ", "o móc"),
            ("uw", "ư", "u móc"),
            ("dd", "đ", "đ gạch"),
            ("s", "´ sắc", "dấu sắc"),
            ("f", "` huyền", "dấu huyền"),
            ("r", "? hỏi", "dấu hỏi"),
            ("x", "~ ngã", "dấu ngã"),
            ("j", ". nặng", "dấu nặng"),
            ("z", "∅", "xóa dấu thanh"),
        ],
        InputMethod::Vni => &[
            ("a6", "â", "a mũ"),
            ("o6", "ô", "o mũ"),
            ("e6", "ê", "e mũ"),
            ("a8", "ă", "a móc trên"),
            ("o7", "ơ", "o móc"),
            ("u7", "ư", "u móc"),
            ("d9", "đ", "đ gạch"),
            ("1", "´ sắc", "dấu sắc"),
            ("2", "` huyền", "dấu huyền"),
            ("3", "? hỏi", "dấu hỏi"),
            ("4", "~ ngã", "dấu ngã"),
            ("5", ". nặng", "dấu nặng"),
            ("0", "∅", "xóa dấu thanh"),
        ],
        InputMethod::Viqr => &[
            ("a^", "â", "a mũ"),
            ("o^", "ô", "o mũ"),
            ("e^", "ê", "e mũ"),
            ("a(", "ă", "a móc trên"),
            ("o+", "ơ", "o móc"),
            ("u+", "ư", "u móc"),
            ("dd", "đ", "đ gạch"),
            ("'", "´ sắc", "dấu sắc"),
            ("`", "` huyền", "dấu huyền"),
            ("?", "? hỏi", "dấu hỏi"),
            ("~", "~ ngã", "dấu ngã"),
            (".", ". nặng", "dấu nặng"),
        ],
    }
}

// ─── Typing test simulation (preview only) ─────────────────────────────────

fn preview_telex(input: &str) -> String {
    let chars: Vec<char> = input.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < chars.len() {
        let (c, next) = (chars[i], chars.get(i + 1).copied());
        let n = match (c, next) {
            ('a', Some('a')) => {
                out.push('â');
                2
            }
            ('o', Some('o')) => {
                out.push('ô');
                2
            }
            ('e', Some('e')) => {
                out.push('ê');
                2
            }
            ('a', Some('w')) => {
                out.push('ă');
                2
            }
            ('o', Some('w')) => {
                out.push('ơ');
                2
            }
            ('u', Some('w')) => {
                out.push('ư');
                2
            }
            ('d', Some('d')) => {
                out.push('đ');
                2
            }
            _ => {
                out.push(c);
                1
            }
        };
        i += n;
    }
    out
}

fn preview_vni(input: &str) -> String {
    let chars: Vec<char> = input.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < chars.len() {
        let (c, next) = (chars[i], chars.get(i + 1).copied());
        let n = match (c, next) {
            ('a', Some('6')) => {
                out.push('â');
                2
            }
            ('o', Some('6')) => {
                out.push('ô');
                2
            }
            ('e', Some('6')) => {
                out.push('ê');
                2
            }
            ('a', Some('8')) => {
                out.push('ă');
                2
            }
            ('o', Some('7')) => {
                out.push('ơ');
                2
            }
            ('u', Some('7')) => {
                out.push('ư');
                2
            }
            ('d', Some('9')) => {
                out.push('đ');
                2
            }
            _ => {
                out.push(c);
                1
            }
        };
        i += n;
    }
    out
}

fn preview_for(method: InputMethod, input: &str) -> String {
    match method {
        InputMethod::Telex => preview_telex(input),
        InputMethod::Vni => preview_vni(input),
        InputMethod::Viqr => input.to_string(),
    }
}

// ─── Constants ─────────────────────────────────────────────────────────────

const ACCENT: Color32 = Color32::from_rgb(0, 120, 212);
const BG_CARD: Color32 = Color32::from_rgb(30, 33, 40);
const BG_HINT: Color32 = Color32::from_rgb(18, 32, 58);
const TEXT_DIM: Color32 = Color32::from_gray(140);
const GREEN: Color32 = Color32::from_rgb(60, 200, 60);
const RED: Color32 = Color32::from_rgb(200, 60, 60);
const AMBER: Color32 = Color32::from_rgb(220, 140, 40);

fn card_frame() -> Frame {
    Frame::none()
        .fill(BG_CARD)
        .inner_margin(Margin::same(10.0))
        .rounding(6.0)
}

fn hint_frame() -> Frame {
    Frame::none()
        .fill(BG_HINT)
        .inner_margin(Margin::same(8.0))
        .rounding(4.0)
}

fn status_dot(ui: &mut Ui, color: Color32) {
    let (r, _) = ui.allocate_exact_size(egui::vec2(8.0, 8.0), egui::Sense::hover());
    ui.painter().circle_filled(r.center(), 4.0, color);
}

// ─── App state ─────────────────────────────────────────────────────────────

pub struct ViUiApp {
    ipc: IpcClient,
    last_poll: Instant,

    method: InputMethod,
    charset: OutputCharset,
    opts: Options,
    tab: Tab,
    setup: SetupState,

    typing_input: String,

    status_msg: String,
    status_ok: bool,
    backend: String,
}

impl ViUiApp {
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        Self {
            ipc: IpcClient::new(),
            last_poll: Instant::now()
                .checked_sub(Duration::from_secs(5))
                .unwrap_or_else(Instant::now),
            method: InputMethod::Telex,
            charset: OutputCharset::Unicode,
            opts: Options::default(),
            tab: Tab::Main,
            setup: SetupState::new(),
            typing_input: String::new(),
            status_msg: String::new(),
            status_ok: true,
            backend: String::new(),
        }
    }

    fn poll_daemon(&mut self) {
        if self.last_poll.elapsed() < Duration::from_secs(1) {
            return;
        }
        self.last_poll = Instant::now();
        if let Some(IpcResponse::Status {
            backend, method, ..
        }) = self.ipc.send(&IpcRequest::Status)
        {
            self.backend = backend;
            if let Some(m) = InputMethod::from_str(&method) {
                self.method = m;
            }
        }
    }

    fn apply_method(&mut self, m: InputMethod) {
        self.method = m;
        let req = IpcRequest::SetMethod {
            method: m.ipc_name().into(),
        };
        if let Some(IpcResponse::Error { message }) = self.ipc.send(&req) {
            self.set_status(false, format!("Lỗi: {message}"));
        }
    }

    fn apply_charset(&mut self, c: OutputCharset) {
        self.charset = c;
        let _ = self.ipc.send(&IpcRequest::SetCharset {
            charset: c.ipc_name().into(),
        });
    }

    fn set_status(&mut self, ok: bool, msg: impl Into<String>) {
        self.status_ok = ok;
        self.status_msg = msg.into();
    }

    // ── Header ─────────────────────────────────────────────────────────────

    fn show_header(&self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            let (rect, _) = ui.allocate_exact_size(egui::vec2(42.0, 42.0), egui::Sense::hover());
            ui.painter().rect_filled(rect, 10.0, ACCENT);
            ui.painter().text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                "V",
                egui::FontId::proportional(28.0),
                Color32::WHITE,
            );
            ui.add_space(10.0);
            ui.vertical(|ui| {
                ui.add_space(3.0);
                ui.label(
                    RichText::new("VHTTechKey")
                        .size(17.0)
                        .strong()
                        .color(ACCENT),
                );
                ui.label(RichText::new("Bộ gõ tiếng Việt").size(10.5).color(TEXT_DIM));
            });

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let (dot_col, tip) = if self.ipc.is_connected() {
                    (GREEN, "Đã kết nối daemon")
                } else {
                    (RED, "Chưa kết nối daemon")
                };
                let (r, _) = ui.allocate_exact_size(egui::vec2(10.0, 10.0), egui::Sense::hover());
                ui.painter().circle_filled(r.center(), 5.0, dot_col);
                ui.add_space(4.0);
                ui.label(RichText::new(tip).size(10.0).color(TEXT_DIM));
            });
        });
    }

    // ── Tab bar ────────────────────────────────────────────────────────────

    fn show_tabs(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            let tabs = [
                (Tab::Main, "Cài đặt"),
                (Tab::Setup, "Thiết lập"),
                (Tab::TypingTest, "Thử gõ"),
                (Tab::About, "Về..."),
            ];
            for (tab, lbl) in tabs {
                let selected = self.tab == tab;
                let col = if selected { ACCENT } else { TEXT_DIM };
                let btn = egui::Button::new(RichText::new(lbl).size(12.5).color(col))
                    .frame(false)
                    .fill(Color32::TRANSPARENT);
                let resp = ui.add(btn);
                if resp.clicked() {
                    self.tab = tab;
                }
                if selected {
                    let r = resp.rect;
                    ui.painter()
                        .hline(r.x_range(), r.max.y, egui::Stroke::new(2.0, ACCENT));
                }
                ui.add_space(4.0);
            }
        });
    }

    // ── Main settings tab ──────────────────────────────────────────────────

    fn show_main(&mut self, ui: &mut Ui) {
        let mut new_method = self.method;
        let mut new_charset = self.charset;

        let half_w = (ui.available_width() - 12.0) / 2.0;
        ui.horizontal_top(|ui| {
            card_frame().show(ui, |ui| {
                ui.set_min_width(half_w);
                ui.set_max_width(half_w);
                ui.vertical(|ui| {
                    ui.label(RichText::new("Kiểu gõ").strong().size(12.5).color(ACCENT));
                    ui.add_space(6.0);
                    for &m in InputMethod::ALL {
                        ui.radio_value(&mut new_method, m, RichText::new(m.label()).size(13.0));
                    }
                });
            });

            ui.add_space(8.0);

            card_frame().show(ui, |ui| {
                ui.set_min_width(half_w);
                ui.set_max_width(half_w);
                ui.vertical(|ui| {
                    ui.label(RichText::new("Bảng mã").strong().size(12.5).color(ACCENT));
                    ui.add_space(6.0);
                    for &c in OutputCharset::ALL {
                        ui.radio_value(&mut new_charset, c, RichText::new(c.label()).size(13.0));
                    }
                });
            });
        });

        if new_method != self.method {
            self.apply_method(new_method);
        }
        if new_charset != self.charset {
            self.apply_charset(new_charset);
        }

        ui.add_space(6.0);
        hint_frame().show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(
                    RichText::new("⌨")
                        .size(12.0)
                        .color(Color32::from_rgb(120, 160, 255)),
                );
                ui.label(
                    RichText::new(self.method.hint())
                        .size(11.0)
                        .color(Color32::from_rgb(140, 180, 255)),
                );
            });
            ui.add_space(2.0);
            ui.horizontal_wrapped(|ui| {
                ui.label(
                    RichText::new("✦")
                        .size(11.0)
                        .color(Color32::from_rgb(120, 160, 255)),
                );
                ui.label(
                    RichText::new(self.charset.note())
                        .size(11.0)
                        .color(Color32::from_rgb(140, 180, 255)),
                );
            });
        });

        ui.add_space(8.0);
        ui.label(RichText::new("Tùy chọn").strong().size(12.5).color(ACCENT));
        ui.add_space(4.0);
        card_frame().show(ui, |ui| {
            let prev_enabled = self.opts.enabled;
            egui::Grid::new("opts_grid")
                .num_columns(2)
                .spacing([24.0, 4.0])
                .show(ui, |ui| {
                    ui.checkbox(&mut self.opts.enabled, "Bật bộ gõ");
                    ui.checkbox(&mut self.opts.dd_freestyle, "Gõ tự do dấu đ (dd freestyle)");
                    ui.end_row();
                    ui.checkbox(&mut self.opts.spell_check, "Kiểm tra chính tả");
                    ui.checkbox(
                        &mut self.opts.restore_on_backspace,
                        "Phục hồi từ khi Backspace",
                    );
                    ui.end_row();
                });
            if prev_enabled != self.opts.enabled {
                self.set_status(
                    true,
                    if self.opts.enabled {
                        "Bộ gõ đã bật"
                    } else {
                        "Bộ gõ đã tắt"
                    },
                );
            }
        });

        if !self.status_msg.is_empty() {
            ui.add_space(6.0);
            let col = if self.status_ok {
                Color32::from_rgb(80, 200, 80)
            } else {
                Color32::from_rgb(220, 80, 80)
            };
            ui.label(RichText::new(&self.status_msg).size(11.0).color(col));
        }
    }

    // ── Setup tab ──────────────────────────────────────────────────────────

    fn show_setup(&mut self, ui: &mut Ui) {
        self.setup.refresh();

        ui.label(
            RichText::new("Thiết lập hệ thống")
                .strong()
                .size(12.5)
                .color(ACCENT),
        );
        ui.add_space(8.0);

        // ── Daemon ─────────────────────────────────────────────────────────
        card_frame().show(ui, |ui| {
            ui.vertical(|ui| {
                ui.label(RichText::new("Daemon vi-daemon").strong().size(11.5));
                ui.add_space(5.0);
                ui.horizontal(|ui| {
                    if let Some(pid) = self.setup.daemon_pid {
                        status_dot(ui, GREEN);
                        ui.label(RichText::new(format!("Đang chạy — PID {pid}")).size(11.0));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.button("Dừng").clicked() {
                                self.setup.stop_daemon();
                            }
                            if ui.button("Khởi động lại").clicked() {
                                self.setup.stop_daemon();
                                self.setup.start_daemon();
                            }
                        });
                    } else {
                        status_dot(ui, RED);
                        ui.label(RichText::new("Chưa chạy").size(11.0).color(TEXT_DIM));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.button("Khởi động").clicked() {
                                self.setup.start_daemon();
                            }
                        });
                    }
                });

                // show daemon binary path
                if let Some(p) = daemon_bin_path() {
                    ui.add_space(2.0);
                    ui.label(
                        RichText::new(p.display().to_string())
                            .size(10.0)
                            .color(TEXT_DIM),
                    );
                } else {
                    ui.add_space(2.0);
                    ui.label(
                        RichText::new("Không tìm thấy vi-daemon — cần build hoặc cài trước")
                            .size(10.0)
                            .color(AMBER),
                    );
                }
            });
        });

        ui.add_space(6.0);

        // ── IBus ───────────────────────────────────────────────────────────
        card_frame().show(ui, |ui| {
            ui.vertical(|ui| {
                ui.label(RichText::new("IBus").strong().size(11.5));
                ui.add_space(5.0);

                if !self.setup.ibus_available {
                    ui.horizontal(|ui| {
                        status_dot(ui, TEXT_DIM);
                        ui.label(
                            RichText::new("IBus không có trên hệ thống")
                                .size(11.0)
                                .color(TEXT_DIM),
                        );
                    });
                } else {
                    // Row 1: đăng ký component
                    ui.horizontal(|ui| {
                        let (dot, txt) = if self.setup.ibus_registered {
                            (GREEN, "Component đã đăng ký")
                        } else {
                            (AMBER, "Chưa đăng ký component")
                        };
                        status_dot(ui, dot);
                        ui.label(RichText::new(txt).size(11.0));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.small_button("Đăng ký lại").clicked() {
                                self.setup.register_ibus();
                            }
                            if !self.setup.ibus_registered && ui.button("Đăng ký").clicked() {
                                self.setup.register_ibus();
                            }
                            if ui.small_button("ibus-setup...").clicked() {
                                let _ = std::process::Command::new("ibus-setup").spawn();
                            }
                        });
                    });

                    // Row 2: GNOME input sources
                    ui.add_space(3.0);
                    ui.horizontal(|ui| {
                        let (dot, txt) = if self.setup.gnome_has_source {
                            (GREEN, "Trong GNOME input sources")
                        } else {
                            (AMBER, "Chưa thêm vào GNOME input sources")
                        };
                        status_dot(ui, dot);
                        ui.label(RichText::new(txt).size(11.0));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if self.setup.gnome_has_source {
                                if ui.small_button("Gỡ").clicked() {
                                    self.setup.ibus_remove_gnome_source();
                                }
                            } else if ui.button("Thêm vào GNOME").clicked() {
                                self.setup.ibus_add_gnome_source();
                            }
                        });
                    });

                    // Row 3: engine đang active
                    ui.add_space(3.0);
                    ui.horizontal(|ui| {
                        let active = self
                            .setup
                            .ibus_current_engine
                            .as_deref()
                            .unwrap_or("(không rõ)");
                        let is_ours = active == "vhttechkey";
                        let (dot, txt) = if is_ours {
                            (GREEN, format!("Engine hiện tại: {active}"))
                        } else {
                            (TEXT_DIM, format!("Engine hiện tại: {active}"))
                        };
                        status_dot(ui, dot);
                        ui.label(RichText::new(txt).size(11.0));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if !is_ours && self.setup.ibus_registered {
                                if ui.button("Kích hoạt").clicked() {
                                    self.setup.ibus_switch_to_vhttechkey();
                                }
                            }
                        });
                    });
                }
            });
        });

        ui.add_space(6.0);

        // ── Fcitx5 ─────────────────────────────────────────────────────────
        card_frame().show(ui, |ui| {
            ui.vertical(|ui| {
                ui.label(RichText::new("Fcitx5").strong().size(11.5));
                ui.add_space(5.0);

                if !self.setup.fcitx5_available {
                    ui.horizontal(|ui| {
                        status_dot(ui, TEXT_DIM);
                        ui.label(
                            RichText::new("Fcitx5 không có trên hệ thống")
                                .size(11.0)
                                .color(TEXT_DIM),
                        );
                    });
                } else {
                    // Row 1: addon
                    ui.horizontal(|ui| {
                        let (dot, txt) = if self.setup.fcitx5_registered {
                            (GREEN, "Addon đã cài")
                        } else {
                            (AMBER, "Chưa cài addon")
                        };
                        status_dot(ui, dot);
                        ui.label(RichText::new(txt).size(11.0));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.small_button("fcitx5-configtool...").clicked() {
                                let _ = std::process::Command::new("fcitx5-configtool").spawn();
                            }
                            if !self.setup.fcitx5_registered && ui.button("Cài addon").clicked() {
                                self.setup.register_fcitx5();
                            }
                        });
                    });

                    // Row 2: profile
                    ui.add_space(3.0);
                    ui.horizontal(|ui| {
                        let (dot, txt) = if self.setup.fcitx5_in_profile {
                            (GREEN, "Trong ~/.config/fcitx5/profile")
                        } else {
                            (AMBER, "Chưa có trong fcitx5 profile")
                        };
                        status_dot(ui, dot);
                        ui.label(RichText::new(txt).size(11.0));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if self.setup.fcitx5_in_profile {
                                if ui.small_button("Gỡ").clicked() {
                                    self.setup.fcitx5_remove_from_profile();
                                }
                            } else if ui.button("Thêm vào profile").clicked() {
                                self.setup.fcitx5_add_to_profile();
                            }
                        });
                    });

                    // Row 3: IM đang active
                    ui.add_space(3.0);
                    ui.horizontal(|ui| {
                        let active = self
                            .setup
                            .fcitx5_current_im
                            .as_deref()
                            .unwrap_or("(không rõ)");
                        let is_ours = active == "vhttechkey";
                        let (dot, txt) = if is_ours {
                            (GREEN, format!("IM hiện tại: {active}"))
                        } else {
                            (TEXT_DIM, format!("IM hiện tại: {active}"))
                        };
                        status_dot(ui, dot);
                        ui.label(RichText::new(txt).size(11.0));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if !is_ours && self.setup.fcitx5_in_profile {
                                if ui.button("Kích hoạt").clicked() {
                                    self.setup.fcitx5_switch();
                                }
                            }
                        });
                    });
                }
            });
        });

        ui.add_space(6.0);

        // ── Hệ thống ───────────────────────────────────────────────────────
        card_frame().show(ui, |ui| {
            ui.vertical(|ui| {
                ui.label(RichText::new("Hệ thống").strong().size(11.5));
                ui.add_space(5.0);

                // Autostart
                ui.horizontal(|ui| {
                    let (dot, txt) = if self.setup.autostart_enabled {
                        (GREEN, "Tự khởi động: đang bật")
                    } else {
                        (TEXT_DIM, "Tự khởi động: chưa bật")
                    };
                    status_dot(ui, dot);
                    ui.label(RichText::new(txt).size(11.0));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let btn_lbl = if self.setup.autostart_enabled {
                            "Tắt tự khởi động"
                        } else {
                            "Bật tự khởi động"
                        };
                        if ui.button(btn_lbl).clicked() {
                            let enable = !self.setup.autostart_enabled;
                            self.setup.set_autostart(enable);
                        }
                    });
                });

                ui.add_space(4.0);
                ui.separator();
                ui.add_space(4.0);

                // Install to ~/.local/bin
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Sao chép binary vào ~/.local/bin/").size(11.0));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("Cài vào hệ thống").clicked() {
                            self.setup.install_to_local_bin();
                        }
                    });
                });
            });
        });

        // ── Log hành động ──────────────────────────────────────────────────
        if !self.setup.log.is_empty() {
            ui.add_space(8.0);
            ui.label(RichText::new("Log").strong().size(11.0).color(ACCENT));
            ui.add_space(3.0);
            hint_frame().show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .max_height(120.0)
                    .stick_to_bottom(true)
                    .id_salt("setup_log_scroll")
                    .show(ui, |ui| {
                        for (ok, msg) in &self.setup.log {
                            let (icon, col) = if *ok {
                                ("✓", Color32::from_rgb(80, 200, 80))
                            } else {
                                ("✗", Color32::from_rgb(220, 80, 80))
                            };
                            ui.horizontal(|ui| {
                                ui.label(RichText::new(icon).size(11.0).color(col));
                                ui.label(RichText::new(msg.as_str()).size(11.0));
                            });
                        }
                    });
            });
        }
    }

    // ── Typing test tab ────────────────────────────────────────────────────

    fn show_typing_test(&mut self, ui: &mut Ui) {
        ui.label(RichText::new("Thử gõ").strong().size(12.5).color(ACCENT));
        ui.add_space(2.0);
        ui.label(
            RichText::new(format!(
                "Nhập chuỗi theo kiểu {} để xem kết quả (xử lý cục bộ, không qua IME).",
                self.method.label()
            ))
            .size(11.0)
            .color(TEXT_DIM),
        );
        ui.add_space(8.0);

        ui.horizontal(|ui| {
            ui.label("Nhập:");
            ui.add_sized(
                [ui.available_width() - 52.0, 24.0],
                egui::TextEdit::singleline(&mut self.typing_input).hint_text("vd: viet nam"),
            );
            if ui.button("Xóa").clicked() {
                self.typing_input.clear();
            }
        });

        if !self.typing_input.is_empty() {
            let preview = preview_for(self.method, &self.typing_input.clone());
            ui.add_space(8.0);
            card_frame().show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Kết quả:").size(11.5).color(TEXT_DIM));
                    ui.add_space(6.0);
                    ui.label(RichText::new(&preview).size(24.0).strong().color(ACCENT));
                });
            });
        }

        ui.add_space(12.0);
        ui.label(
            RichText::new(format!("Bảng phím — {}", self.method.label()))
                .strong()
                .size(12.0),
        );
        ui.add_space(4.0);
        egui::ScrollArea::vertical()
            .max_height(200.0)
            .id_salt("rules_scroll")
            .show(ui, |ui| {
                egui::Grid::new("rules_grid")
                    .num_columns(3)
                    .striped(true)
                    .min_col_width(80.0)
                    .show(ui, |ui| {
                        ui.label(RichText::new("Tổ hợp").strong().size(11.0));
                        ui.label(RichText::new("Ký tự").strong().size(11.0));
                        ui.label(RichText::new("Ghi chú").strong().size(11.0));
                        ui.end_row();
                        for &(trigger, output, note) in rules_for_method(self.method) {
                            ui.label(
                                RichText::new(trigger)
                                    .monospace()
                                    .size(12.0)
                                    .color(Color32::from_rgb(255, 200, 80)),
                            );
                            ui.label(RichText::new(output).size(14.0).strong());
                            ui.label(RichText::new(note).size(11.0).color(TEXT_DIM));
                            ui.end_row();
                        }
                    });
            });
    }

    // ── About tab ──────────────────────────────────────────────────────────

    fn show_about(&self, ui: &mut Ui) {
        ui.add_space(12.0);
        ui.vertical_centered(|ui| {
            let (rect, _) = ui.allocate_exact_size(egui::vec2(64.0, 64.0), egui::Sense::hover());
            ui.painter().rect_filled(rect, 14.0, ACCENT);
            ui.painter().text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                "V",
                egui::FontId::proportional(44.0),
                Color32::WHITE,
            );
            ui.add_space(10.0);
            ui.label(
                RichText::new("VHTTechKey")
                    .size(20.0)
                    .strong()
                    .color(ACCENT),
            );
            ui.label(
                RichText::new("Bộ gõ tiếng Việt cho Linux")
                    .size(12.0)
                    .color(TEXT_DIM),
            );
            ui.add_space(2.0);
            ui.label(
                RichText::new("Phiên bản 1.0")
                    .size(11.0)
                    .color(Color32::from_gray(110)),
            );

            ui.add_space(16.0);
            ui.separator();
            ui.add_space(10.0);

            egui::Grid::new("about_grid")
                .num_columns(2)
                .spacing([20.0, 6.0])
                .show(ui, |ui| {
                    let k = |s| RichText::new(s).size(11.5).color(TEXT_DIM);
                    let v = |s| RichText::new(s).size(11.5);

                    ui.label(k("Backend:"));
                    ui.label(v("IBus / Fcitx5"));
                    ui.end_row();
                    ui.label(k("Ngôn ngữ:"));
                    ui.label(v("Rust — egui"));
                    ui.end_row();
                    ui.label(k("Kiểu gõ:"));
                    ui.label(v("Telex · VNI · VIQR"));
                    ui.end_row();
                    ui.label(k("Bảng mã:"));
                    ui.label(v("Unicode NFC · VNI · VIQR · TCVN3"));
                    ui.end_row();
                    ui.label(k("Giấy phép:"));
                    ui.label(v("GPLv3+"));
                    ui.end_row();
                });

            ui.add_space(20.0);
            ui.label(
                RichText::new("© 2024 VHTTech")
                    .size(10.0)
                    .color(Color32::from_gray(90)),
            );
        });
    }

    // ── Status bar ─────────────────────────────────────────────────────────

    fn show_status_bar(&self, ui: &mut Ui) {
        ui.separator();
        ui.add_space(3.0);
        ui.horizontal(|ui| {
            Frame::none()
                .fill(ACCENT)
                .inner_margin(Margin::symmetric(7.0, 2.0))
                .rounding(4.0)
                .show(ui, |ui| {
                    ui.label(
                        RichText::new(self.method.label())
                            .size(11.0)
                            .strong()
                            .color(Color32::WHITE),
                    );
                });

            Frame::none()
                .fill(Color32::from_gray(50))
                .inner_margin(Margin::symmetric(7.0, 2.0))
                .rounding(4.0)
                .show(ui, |ui| {
                    let lbl = match self.charset {
                        OutputCharset::Unicode => "Unicode",
                        OutputCharset::Vni => "VNI",
                        OutputCharset::Viqr => "VIQR",
                        OutputCharset::Tcvn3 => "TCVN3",
                    };
                    ui.label(RichText::new(lbl).size(11.0).color(Color32::from_gray(200)));
                });

            if !self.backend.is_empty() {
                ui.label(
                    RichText::new(format!("│ {}", self.backend))
                        .size(10.0)
                        .color(TEXT_DIM),
                );
            }

            if let Some(ms) = self.ipc.last_latency_ms {
                ui.label(
                    RichText::new(format!("{ms:.1}ms"))
                        .size(10.0)
                        .color(TEXT_DIM),
                );
            }

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let (txt, col) = if self.opts.enabled {
                    ("● Đang bật", GREEN)
                } else {
                    ("○ Đang tắt", Color32::from_gray(110))
                };
                ui.label(RichText::new(txt).size(10.5).color(col));
            });
        });
        ui.add_space(2.0);
    }
}

// ─── eframe::App ───────────────────────────────────────────────────────────

impl eframe::App for ViUiApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_daemon();
        ctx.request_repaint_after(Duration::from_secs(1));

        egui::CentralPanel::default()
            .frame(Frame::central_panel(&ctx.style()).inner_margin(Margin::same(14.0)))
            .show(ctx, |ui| {
                self.show_header(ui);
                ui.add_space(8.0);
                self.show_tabs(ui);
                ui.add_space(8.0);
                ui.separator();
                ui.add_space(6.0);

                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        match self.tab {
                            Tab::Main => self.show_main(ui),
                            Tab::Setup => self.show_setup(ui),
                            Tab::TypingTest => self.show_typing_test(ui),
                            Tab::About => self.show_about(ui),
                        }
                        ui.add_space(48.0);
                    });

                ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
                    self.show_status_bar(ui);
                });
            });
    }
}
