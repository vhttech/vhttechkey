//! Fcitx5 input method engine backend.
//!
//! # Architecture
//! vi-daemon acts as a Fcitx5 **IM engine** rather than a client application.
//! The companion C++ addon `vi-fcitx5-shim` (compiled to `libvi-fcitx5.so`)
//! loads inside the Fcitx5 process, receives key/focus events from the Fcitx5
//! C++ API, and forwards them to vi-daemon over a Unix domain socket.
//! vi-daemon processes each key with vi-core and replies with preedit and
//! commit operations; the shim executes those on the focused application's
//! `InputContext`.
//!
//! # Socket protocol (newline-terminated text lines)
//!
//! **Shim → daemon requests:**
//! ```text
//! ACTIVATE <ctx_id>
//! DEACTIVATE <ctx_id>
//! KEY <keyval> <keycode> <state> <serial>
//! KEYUP <keyval>
//! ```
//!
//! **Daemon → shim responses (one per request):**
//! ```text
//! RESULT <consumed:0|1> <num_ops>
//! OP COMMIT <hex_utf8_text>
//! OP PREEDIT <cursor_char_count> <hex_utf8_text>
//! OP CLEAR
//! OP FWDKEY <keyval> <keycode> <state>
//! ```
//!
//! `hex_utf8_text` is every UTF-8 byte printed as two lowercase hex digits
//! (no separator), safe for a line protocol without further escaping.

use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};

use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::{UnixListener, UnixStream},
};
use tracing::{info, trace, warn};
use vi_core::{
    CompositionEngine, InputEvent, Key, Modifiers, NfcString, PreeditText, StandardEngine,
    StateTransition,
};
use vi_platform::{Capabilities, CharCursor, ImeBackend, Result, SurroundingText};

// ── Config file writing ───────────────────────────────────────────────────────

fn xdg_data_home() -> PathBuf {
    std::env::var("XDG_DATA_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_owned()))
                .join(".local/share")
        })
}

/// Default Unix socket path for the shim ↔ daemon connection.
pub fn engine_socket_path() -> PathBuf {
    xdg_data_home().join("vi-ime/engine.sock")
}

/// Write Fcitx5 addon and input-method configuration files.
///
/// Creates `~/.local/share/fcitx5/addon/vi-ime.conf` and
/// `~/.local/share/fcitx5/inputmethod/vi-ime.conf` so that Fcitx5 loads
/// `libvi-fcitx5.so` at startup and exposes the "Vi IME" input method.
/// Overwrites existing files; call once at daemon start-up.
pub fn write_fcitx5_config() -> std::io::Result<()> {
    let base = xdg_data_home();
    let addon_dir = base.join("fcitx5/addon");
    let im_dir = base.join("fcitx5/inputmethod");
    std::fs::create_dir_all(&addon_dir)?;
    std::fs::create_dir_all(&im_dir)?;
    std::fs::write(
        addon_dir.join("vi-ime.conf"),
        "[Addon]\nName=Vi IME\nCategory=InputMethod\nVersion=0.1.0\
         \nLibrary=libvi-fcitx5.so\nType=SharedLibrary\n",
    )?;
    std::fs::write(
        im_dir.join("vi-ime.conf"),
        "[InputMethod]\nName=Vi IME\nNativeName=Bộ gõ tiếng Việt\
         \nIcon=vi-ime\nLangCode=vi\nAddon=vi-ime\n",
    )?;
    Ok(())
}

// ── Preedit delta helpers ─────────────────────────────────────────────────────
//
// These are kept for reference and used by the vi-fcitx5-shim C++ addon when
// it needs to compute minimal key-event sequences to update a displayed preedit.

#[cfg(test)]
#[derive(Debug, PartialEq)]
enum DeltaOp {
    Insert(char),
    Backspace,
}

#[cfg(test)]
fn compute_preedit_delta(old: &str, new: &str) -> Vec<DeltaOp> {
    let (backspaces, to_insert): (usize, Vec<char>) = if let Some(suffix) = new.strip_prefix(old) {
        (0, suffix.chars().collect())
    } else if old.starts_with(new) {
        (old.chars().count() - new.chars().count(), vec![])
    } else {
        (old.chars().count(), new.chars().collect())
    };
    let mut ops = Vec::with_capacity(backspaces + to_insert.len());
    for _ in 0..backspaces {
        ops.push(DeltaOp::Backspace);
    }
    for ch in to_insert {
        ops.push(DeltaOp::Insert(ch));
    }
    ops
}

#[cfg(test)]
fn soft_preedit_diff(old: &str, new: &str) -> (usize, String) {
    let prefix_chars = old
        .chars()
        .zip(new.chars())
        .take_while(|(a, b)| a == b)
        .count();
    let old_tail = old
        .char_indices()
        .nth(prefix_chars)
        .map(|(i, _)| i)
        .unwrap_or(old.len());
    let new_tail = new
        .char_indices()
        .nth(prefix_chars)
        .map(|(i, _)| i)
        .unwrap_or(new.len());
    (old[old_tail..].chars().count(), new[new_tail..].to_string())
}

// ── Protocol helpers ──────────────────────────────────────────────────────────

/// Hex-encode every UTF-8 byte of `s`; safe for use in a line protocol.
pub(crate) fn hex_encode(s: &str) -> String {
    s.bytes().map(|b| format!("{b:02x}")).collect()
}

// ── Backend ───────────────────────────────────────────────────────────────────

/// Fcitx5 IM engine backend driven by the `vi-fcitx5-shim` C++ addon.
///
/// Call [`FcitxShimBackend::run`] to start the Unix socket server.  The C++
/// shim connects, sends key/focus events, and receives preedit/commit
/// operations to execute on the focused application's `InputContext`.
pub struct FcitxShimBackend {
    socket_path: PathBuf,
    engine: Arc<Mutex<StandardEngine>>,
}

impl FcitxShimBackend {
    pub fn new(socket_path: PathBuf, engine: Arc<Mutex<StandardEngine>>) -> Self {
        Self {
            socket_path,
            engine,
        }
    }

    /// Start the Unix socket server; runs until cancelled.
    ///
    /// Removes any stale socket file, binds, and loops accepting connections
    /// from the C++ shim.  Each connection runs in its own tokio task.
    pub async fn run(&self) -> std::io::Result<()> {
        let _ = std::fs::remove_file(&self.socket_path);
        if let Some(p) = self.socket_path.parent() {
            std::fs::create_dir_all(p)?;
        }
        let listener = UnixListener::bind(&self.socket_path)?;
        info!("Fcitx5 engine: listening on {}", self.socket_path.display());
        loop {
            match listener.accept().await {
                Ok((stream, _)) => {
                    let engine = Arc::clone(&self.engine);
                    tokio::spawn(handle_shim_connection(stream, engine));
                }
                Err(e) => warn!("Fcitx5 engine: accept error: {e}"),
            }
        }
    }
}

/// No-op `ImeBackend` implementation for API compatibility.
///
/// The real processing happens inside [`FcitxShimBackend::run`], which drives
/// the composition engine directly over the Unix socket.  These methods are
/// never called in normal operation; they exist so the type can be stored as
/// `Arc<dyn ImeBackend>`.
impl ImeBackend for FcitxShimBackend {
    fn commit(&self, _text: &NfcString) -> Result<()> {
        Ok(())
    }
    fn update_preedit(&self, _text: &PreeditText, _cursor: CharCursor) -> Result<()> {
        Ok(())
    }
    fn clear_preedit(&self) -> Result<()> {
        Ok(())
    }
    fn forward_key(&self, _key: &InputEvent) -> Result<()> {
        Ok(())
    }
    fn surrounding_text(&self) -> Result<Option<SurroundingText>> {
        Ok(None)
    }
    fn capabilities(&self) -> Capabilities {
        Capabilities {
            preedit: true,
            surrounding_text: false,
            lookup_table: true,
        }
    }
}

// ── Connection handler ────────────────────────────────────────────────────────

pub async fn handle_shim_connection(stream: UnixStream, engine: Arc<Mutex<StandardEngine>>) {
    let (reader, mut writer) = stream.into_split();
    let mut lines = BufReader::new(reader).lines();
    info!("Fcitx5 shim: connection opened");

    while let Ok(Some(line)) = lines.next_line().await {
        let mut response = dispatch_shim_line(&line, &engine);
        response.push('\n');
        trace!("Fcitx5 shim: sending: {:?}", response);
        if let Err(e) = writer.write_all(response.as_bytes()).await {
            warn!("Fcitx5 shim: write error: {e}");
            break;
        }
        if let Err(e) = writer.flush().await {
            warn!("Fcitx5 shim write flush failed: {e}");
            break;
        }
    }
    info!("Fcitx5 shim: connection closed");
}

/// Dispatch one line from the shim, return the response line(s) as a string.
///
/// The returned string does **not** end with `\n`; the caller appends it.
/// Multi-line responses (RESULT with ops) use embedded `\n` between lines.
pub(crate) fn dispatch_shim_line(line: &str, engine: &Arc<Mutex<StandardEngine>>) -> String {
    let mut parts = line.split_ascii_whitespace();
    match parts.next() {
        Some("ACTIVATE") => {
            engine
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .process(&InputEvent::FocusIn)
                .ok();
            "RESULT 1 0".to_owned()
        }
        Some("DEACTIVATE") => {
            // FocusOut commits any pending preedit before yielding focus.
            let t = engine
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .process(&InputEvent::FocusOut);
            let ops = t
                .ok()
                .map(|tr| state_transition_to_ops(tr, None))
                .unwrap_or_default();
            format_response(true, ops)
        }
        Some("KEYUP") => {
            // Reset the repeat-guard so a genuine re-press is never suppressed.
            let keyval: u32 = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
            engine
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .process(&InputEvent::KeyUp(map_keyval_to_key(keyval)))
                .ok();
            "RESULT 0 0".to_owned()
        }
        Some("KEY") => {
            let keyval: u32 = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
            let _keycode: u32 = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
            let state: u32 = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
            process_key_event_impl(engine, keyval, state)
        }
        _ => {
            warn!("Fcitx5 shim: unknown request: {line:?}");
            "RESULT 0 0".to_owned()
        }
    }
}

pub(crate) fn process_key_event_impl(
    engine: &Arc<Mutex<StandardEngine>>,
    keyval: u32,
    state: u32,
) -> String {
    let Some(event) = map_fcitx5_key(keyval, state, false) else {
        return "RESULT 0 0".to_owned();
    };
    let mut eng = engine.lock().unwrap_or_else(|e| e.into_inner());
    match eng.process(&event) {
        Ok(StateTransition::PassThrough) => "RESULT 0 0".to_owned(),
        Ok(t) => {
            let ops = state_transition_to_ops(t, Some(&event));
            format_response(true, ops)
        }
        Err(e) => {
            warn!("composition error: {e:?}");
            eng.reset();
            "RESULT 0 0".to_owned()
        }
    }
}

fn format_response(consumed: bool, ops: Vec<String>) -> String {
    let mut out = format!("RESULT {} {}", consumed as u8, ops.len());
    for op in ops {
        out.push('\n');
        out.push_str(&op);
    }
    out
}

fn state_transition_to_ops(t: StateTransition, original: Option<&InputEvent>) -> Vec<String> {
    match t {
        StateTransition::PreeditUpdated(p) => {
            let n = p.as_str().chars().count();
            vec![format!("OP PREEDIT {n} {}", hex_encode(p.as_str()))]
        }
        StateTransition::Commit(ct) | StateTransition::CommitAndClear(ct) => {
            vec![format!("OP COMMIT {}", hex_encode(ct.as_str()))]
        }
        StateTransition::CommitThenPreedit(ct, p) => {
            let n = p.as_str().chars().count();
            vec![
                format!("OP COMMIT {}", hex_encode(ct.as_str())),
                format!("OP PREEDIT {n} {}", hex_encode(p.as_str())),
            ]
        }
        StateTransition::CommitThenPassThrough(ct) => {
            let mut ops = vec![format!("OP COMMIT {}", hex_encode(ct.as_str()))];
            if let Some(ev) = original {
                let (kv, kc, st) = event_to_xkb(ev);
                ops.push(format!("OP FWDKEY {kv} {kc} {st}"));
            }
            ops
        }
        StateTransition::Cleared => vec!["OP CLEAR".to_owned()],
        // PassThrough is handled before calling this function; Consumed has no ops.
        StateTransition::PassThrough | StateTransition::Consumed => vec![],
    }
}

fn event_to_xkb(event: &InputEvent) -> (u32, u32, u32) {
    match event {
        InputEvent::KeyDown(key, mods) | InputEvent::KeyRepeat(key, mods) => {
            (key_to_xkb(key), 0, mods_to_xkb(mods))
        }
        InputEvent::KeyUp(key) => (key_to_xkb(key), 0, 0),
        _ => (0, 0, 0),
    }
}

// ── Key mapping (XKB ↔ vi-core) ──────────────────────────────────────────────

/// Convert an XKB keyval/state pair to an engine `InputEvent`.
///
/// Returns `None` for events the engine should not see: key releases,
/// bare modifier keys, and Ctrl+letter combinations.  The shim passes
/// those through to the focused application directly.
pub fn map_fcitx5_key(keyval: u32, state: u32, is_release: bool) -> Option<InputEvent> {
    if is_release {
        return None;
    }
    if is_modifier_keysym(keyval) {
        return None;
    }
    let mods = map_xkb_state(state);
    if mods.ctrl {
        return None;
    }
    Some(InputEvent::KeyDown(map_keyval_to_key(keyval), mods))
}

fn map_keyval_to_key(keyval: u32) -> Key {
    match keyval {
        0xff08 => Key::Backspace,
        0xffff => Key::Delete,
        0xff0d => Key::Return,
        0xff1b => Key::Escape,
        0xff09 => Key::Tab,
        0xff51 => Key::Left,
        0xff52 => Key::Up,
        0xff53 => Key::Right,
        0xff54 => Key::Down,
        0xff50 => Key::Home,
        0xff57 => Key::End,
        // Printable ASCII (space through tilde).
        0x0020..=0x007e => Key::Char(char::from_u32(keyval).unwrap()),
        // X11 Unicode extension: keysym = 0x01000000 + Unicode codepoint.
        v @ 0x01000000..=0x0110ffff => Key::Char(char::from_u32(v - 0x01000000).unwrap_or('\0')),
        other => Key::Keysym(other),
    }
}

fn map_xkb_state(state: u32) -> Modifiers {
    Modifiers {
        shift: state & 1 != 0,
        caps_lock: state & 2 != 0,
        ctrl: state & 4 != 0,
        alt: state & 8 != 0,
        altgr: state & (1 << 7) != 0,
        super_key: state & (1 << 26) != 0,
    }
}

fn is_modifier_keysym(keyval: u32) -> bool {
    matches!(
        keyval,
        0xffe1 | 0xffe2   // Shift_L / Shift_R
        | 0xffe3 | 0xffe4 // Control_L / Control_R
        | 0xffe5 | 0xffe6 // Caps_Lock / Shift_Lock
        | 0xffe7 | 0xffe8 // Meta_L / Meta_R
        | 0xffe9 | 0xffea // Alt_L / Alt_R
        | 0xffeb | 0xffec // Super_L / Super_R
        | 0xff7f          // Num_Lock
        | 0xfe03 // ISO_Level3_Shift (AltGr)
    )
}

fn key_to_xkb(key: &Key) -> u32 {
    match key {
        Key::Char(c) => *c as u32,
        Key::Backspace => 0xff08,
        Key::Delete => 0xffff,
        Key::Return => 0xff0d,
        Key::Escape => 0xff1b,
        Key::Tab => 0xff09,
        Key::Left => 0xff51,
        Key::Up => 0xff52,
        Key::Right => 0xff53,
        Key::Down => 0xff54,
        Key::Home => 0xff50,
        Key::End => 0xff57,
        Key::Keysym(k) => *k,
        Key::DeadKey(_) | Key::ComposeKey => 0,
    }
}

fn mods_to_xkb(mods: &Modifiers) -> u32 {
    let mut m = 0u32;
    if mods.shift {
        m |= 1;
    }
    if mods.caps_lock {
        m |= 2;
    }
    if mods.ctrl {
        m |= 4;
    }
    if mods.alt {
        m |= 8;
    }
    if mods.super_key {
        m |= 1 << 26;
    }
    m
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use vi_core::{InputMethod, StandardEngine};

    // ── Key filtering ────────────────────────────────────────────────────────

    #[test]
    fn ctrl_c_is_not_consumed() {
        // keyval = 'c' (0x63), state = ControlMask (bit 2 = 4)
        assert!(map_fcitx5_key(0x63, 4, false).is_none());
    }

    #[test]
    fn key_release_is_not_consumed() {
        assert!(map_fcitx5_key(b'a' as u32, 0, true).is_none());
    }

    #[test]
    fn modifier_keysym_is_not_consumed() {
        assert!(map_fcitx5_key(0xffe3, 0, false).is_none()); // Control_L
    }

    #[test]
    fn plain_char_produces_key_down() {
        let event = map_fcitx5_key(b'a' as u32, 0, false).unwrap();
        assert_eq!(
            event,
            InputEvent::KeyDown(Key::Char('a'), Modifiers::none())
        );
    }

    #[test]
    fn special_keys_map_correctly() {
        assert_eq!(map_keyval_to_key(0xff08), Key::Backspace);
        assert_eq!(map_keyval_to_key(0xff0d), Key::Return);
        assert_eq!(map_keyval_to_key(0xff1b), Key::Escape);
        assert_eq!(map_keyval_to_key(0xff09), Key::Tab);
    }

    // ── compute_preedit_delta ────────────────────────────────────────────────

    #[test]
    fn test_update_preedit_delta_append() {
        let ops = compute_preedit_delta("nh", "nha");
        assert_eq!(ops, vec![DeltaOp::Insert('a')]);
    }

    #[test]
    fn test_update_preedit_delta_rule_fired() {
        // Telex "oo" → "ô": 2 backspaces + insert 'ô'.
        let ops = compute_preedit_delta("oo", "ô");
        assert_eq!(
            ops,
            vec![DeltaOp::Backspace, DeltaOp::Backspace, DeltaOp::Insert('ô')]
        );
    }

    #[test]
    fn test_update_preedit_delta_backspace() {
        let ops = compute_preedit_delta("nha", "nh");
        assert_eq!(ops, vec![DeltaOp::Backspace]);
    }

    // ── soft_preedit_diff ────────────────────────────────────────────────────

    #[test]
    fn soft_preedit_diff_same() {
        assert_eq!(soft_preedit_diff("abc", "abc"), (0, String::new()));
    }

    #[test]
    fn soft_preedit_diff_insert_only() {
        assert_eq!(soft_preedit_diff("nh", "nha"), (0, "a".to_string()));
    }

    #[test]
    fn soft_preedit_diff_backspace_only() {
        assert_eq!(soft_preedit_diff("nha", "nh"), (1, String::new()));
    }

    #[test]
    fn soft_preedit_diff_middle_changed() {
        assert_eq!(soft_preedit_diff("abc", "abd"), (1, "d".to_string()));
    }

    // ── Full Telex composition: "tôi" (t-o-o-i) + space commit ──────────────
    //
    // The engine receives: t, o, [KeyUp o], o (oo→ô rule fires), i, space.
    // KEYUP is sent between the two 'o' presses to reset the repeat-guard so
    // the second press is not suppressed as a duplicate within the 20 ms window.
    //
    // Keyvals: t=116, o=111, i=105, space=32.

    #[test]
    fn toi_preedit_and_space_commit() {
        let engine = Arc::new(Mutex::new(StandardEngine::new(InputMethod::Telex)));

        // 't' → PreeditUpdated("t"), cursor 1
        assert_eq!(
            dispatch_shim_line("KEY 116 0 0 0", &engine),
            format!("RESULT 1 1\nOP PREEDIT 1 {}", hex_encode("t"))
        );
        dispatch_shim_line("KEYUP 116", &engine);

        // 'o' → PreeditUpdated("to"), cursor 2
        assert_eq!(
            dispatch_shim_line("KEY 111 0 0 1", &engine),
            format!("RESULT 1 1\nOP PREEDIT 2 {}", hex_encode("to"))
        );
        // KeyUp resets the repeat-guard so the second 'o' fires the oo→ô rule.
        dispatch_shim_line("KEYUP 111", &engine);

        // 'o' (second) → Telex oo→ô → PreeditUpdated("tô"), cursor 2
        assert_eq!(
            dispatch_shim_line("KEY 111 0 0 2", &engine),
            format!("RESULT 1 1\nOP PREEDIT 2 {}", hex_encode("tô"))
        );

        // 'i' → PreeditUpdated("tôi"), cursor 3
        assert_eq!(
            dispatch_shim_line("KEY 105 0 0 3", &engine),
            format!("RESULT 1 1\nOP PREEDIT 3 {}", hex_encode("tôi"))
        );

        // space → CommitThenPassThrough("tôi") + forward space (keyval 32)
        assert_eq!(
            dispatch_shim_line("KEY 32 0 0 4", &engine),
            format!(
                "RESULT 1 2\nOP COMMIT {}\nOP FWDKEY 32 0 0",
                hex_encode("tôi")
            )
        );
    }
}
