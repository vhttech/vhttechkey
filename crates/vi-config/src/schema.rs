use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use vi_core::methods::InputMethod;

pub const CURRENT_VERSION: u32 = 1;

/// How the daemon selects the IME protocol backend.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum SandboxMode {
    /// Detect automatically: use the XDG portal when a sandbox is detected,
    /// otherwise connect directly to the compositor.
    #[default]
    Auto,
    /// Always use the XDG portal backend regardless of sandbox detection.
    XdgPortal,
    /// Always connect directly to Wayland / X11, bypassing portal detection.
    Direct,
}

/// How the IBus engine delivers composed text to applications.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum IbusCommitMode {
    /// Use `UpdatePreeditText` inline (default — works for all apps).
    #[default]
    Preedit,
    /// `ForwardKeyEvent(BackSpace)` × N + `CommitText` — reliable direct-commit
    /// for apps that ignore or mishandle preedit text.
    BackspaceCommit,
    /// Legacy: `DeleteSurroundingText` + `CommitText` (kept for backward compat).
    SurroundingCommit,
}

/// IBus-specific configuration options.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct IbusConfig {
    /// When `true`, clears `chrome_direct_mode` and keeps the standard IBus
    /// preedit path (`UpdatePreeditText` + `HidePreeditText` + `CommitText`).
    ///
    /// Use this if you enabled `[ibus] force_chrome_direct` globally but a
    /// specific app misbehaves with that override.  It does **not** re-enable
    /// `DeleteSurroundingText` / surrounding-commit — vime never auto-selects
    /// that path anymore.
    #[serde(default)]
    pub force_preedit_mode: bool,
    /// Force `chrome_direct_mode`: every preedit step is driven through
    /// `UpdatePreeditText` with a local `shadow_buf` (see `vi-ibus` sources).
    /// Opt-in workaround for rare Chromium/XWayland glitches; not derived from
    /// `SetCapabilities` bits.
    #[serde(default)]
    pub force_chrome_direct: bool,
    /// Global default commit mode for the IBus backend.
    /// `backspace_commit` is the reliable direct-commit mode for apps that
    /// ignore preedit; `preedit` is the default inline composition mode.
    #[serde(default)]
    pub commit_mode: IbusCommitMode,
}

/// Top-level configuration file structure.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Schema version; incremented on each breaking change.
    #[serde(default = "default_version")]
    pub version: u32,

    /// Name of the active profile.
    #[serde(default = "default_profile_name")]
    pub active_profile: String,

    /// Named composition profiles.
    #[serde(default)]
    pub profiles: HashMap<String, Profile>,

    /// Per-application configuration overrides.
    #[serde(default)]
    pub app_overrides: Vec<AppOverride>,

    /// How to select the IME protocol backend in sandboxed environments.
    #[serde(default)]
    pub sandbox_mode: SandboxMode,

    /// When `true`, preedit is suppressed and keys are forwarded unmodified
    /// while a fullscreen window matching [`game_list`] has focus.
    #[serde(default = "default_true")]
    pub fullscreen_passthrough: bool,

    /// When `true`, switch to passthrough mode (no preedit, immediate commit)
    /// when a remote-desktop session is detected.
    #[serde(default = "default_true")]
    pub remote_desktop_passthrough: bool,

    /// WM_CLASS glob patterns for games that should receive raw key events
    /// when in fullscreen.  Example: `["steam*", "*game*"]`.
    #[serde(default)]
    pub game_list: Vec<String>,

    /// Global key binding overrides sent by the UI (action → key string).
    #[serde(default)]
    pub key_bindings: HashMap<String, String>,

    /// IBus backend options.
    #[serde(default)]
    pub ibus: IbusConfig,
}

impl Default for Config {
    fn default() -> Self {
        let mut profiles = HashMap::new();
        profiles.insert("default".to_string(), Profile::default());
        Self {
            version: CURRENT_VERSION,
            active_profile: "default".to_string(),
            profiles,
            app_overrides: Vec::new(),
            sandbox_mode: SandboxMode::default(),
            fullscreen_passthrough: true,
            remote_desktop_passthrough: true,
            game_list: Vec::new(),
            key_bindings: HashMap::new(),
            ibus: IbusConfig::default(),
        }
    }
}

impl Config {
    pub fn set_key_bindings(&mut self, bindings: HashMap<String, String>) {
        self.key_bindings = bindings;
    }
}

fn default_version() -> u32 {
    CURRENT_VERSION
}

fn default_profile_name() -> String {
    "default".to_string()
}

/// vhttechkey dictionary verification at commit time (`vietnamese.cm.dict`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SpellCheckProfile {
    /// Master switch: when false, composition behaves without dictionary checks.
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// When true (and `enabled`), commit raw Telex/VNI keys if the NFC lowercase word is absent from the dictionary.
    #[serde(default = "default_true")]
    pub commit_with_dictionary: bool,
    /// `dd_freestyle`: never substitute raw keys when the buffer contains `đ`.
    #[serde(default = "default_true")]
    pub dd_freestyle: bool,
}

impl Default for SpellCheckProfile {
    fn default() -> Self {
        Self {
            enabled: true,
            commit_with_dictionary: true,
            dd_freestyle: true,
        }
    }
}

/// A named composition profile.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Profile {
    /// Active input method for this profile.
    #[serde(default)]
    pub input_method: InputMethodKind,

    /// Where to place tone marks relative to vowels.
    #[serde(default)]
    pub tone_placement: TonePlacement,

    /// Key bindings.
    #[serde(default)]
    pub key_bindings: KeyBindings,

    /// UI display preferences.
    #[serde(default)]
    pub ui: UiPreferences,

    /// Commit-time spell check using the system `vietnamese.cm.dict`.
    #[serde(default)]
    pub spell_check: SpellCheckProfile,
}

/// Which input method to use.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum InputMethodKind {
    #[default]
    Telex,
    Vni,
    Viqr,
    Custom(CustomMethod),
}

impl InputMethodKind {
    /// Convert to a `vi_core::InputMethod` when possible.
    pub fn to_core_method(&self) -> Option<InputMethod> {
        match self {
            Self::Telex => Some(InputMethod::Telex),
            Self::Vni => Some(InputMethod::Vni),
            Self::Viqr => Some(InputMethod::Viqr),
            Self::Custom(_) => None,
        }
    }
}

/// A user-defined composition method with explicit rules.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CustomMethod {
    /// Human-readable name shown in the UI.
    pub name: String,
    /// Ordered list of composition rules.
    pub rules: Vec<CompositionRule>,
}

/// A single substitution rule: when `trigger` characters are seen in sequence,
/// replace them with `replacement`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CompositionRule {
    /// Character sequence that activates this rule (e.g. `"aw"`).
    pub trigger: String,
    /// NFC text to substitute (e.g. `"ă"`).
    pub replacement: String,
}

/// Tone-mark placement policy (new-style vs. traditional).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum TonePlacement {
    /// Modern placement — tone on the main vowel of the syllable nucleus.
    #[default]
    New,
    /// Traditional placement — tone on the first vowel typed.
    Old,
}

/// Global and profile-level key binding overrides.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct KeyBindings {
    /// Keystroke to toggle the IME on/off (e.g. `"ctrl+space"`).
    pub toggle_input_method: Option<String>,
    /// Keystroke to advance to the next profile.
    pub next_profile: Option<String>,
    /// Keystroke to go back to the previous profile.
    pub prev_profile: Option<String>,
    /// Keystroke to reset composition state.
    pub reset: Option<String>,
}

/// UI display preferences.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct UiPreferences {
    /// Whether to show a candidate / preedit preview window.
    #[serde(default = "default_true")]
    pub show_candidate_window: bool,

    /// Layout direction of the candidate window.
    #[serde(default)]
    pub candidate_orientation: CandidateOrientation,

    /// Override font family for the candidate window.
    pub font_name: Option<String>,

    /// Override font size (pt) for the candidate window.
    pub font_size: Option<f32>,

    /// Named UI theme.
    pub theme: Option<String>,
}

impl Default for UiPreferences {
    fn default() -> Self {
        Self {
            show_candidate_window: true,
            candidate_orientation: CandidateOrientation::default(),
            font_name: None,
            font_size: None,
            theme: None,
        }
    }
}

fn default_true() -> bool {
    true
}

/// Orientation of the candidate window.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum CandidateOrientation {
    #[default]
    Horizontal,
    Vertical,
}

/// Override configuration for a specific application.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppOverride {
    /// Glob or substring pattern matched against the application name / WM class.
    pub app_name_pattern: String,
    /// Input method override for this application.
    pub input_method: Option<InputMethodKind>,
    /// When `true`, the IME is fully disabled for this application.
    #[serde(default)]
    pub disabled: bool,
    /// IBus commit mode override for this application.
    /// When `None`, the global `[ibus] commit_mode` applies.
    #[serde(default)]
    pub ibus_commit_mode: Option<IbusCommitMode>,
}
