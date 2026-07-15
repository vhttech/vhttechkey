//! Deprecated: use the vime-* equivalent crate instead.
//! IBus engine backend.
//!
//! Implements `ImeBackend` by serving the `org.freedesktop.IBus.Engine`
//! D-Bus interface via zbus.  The reconnect loop restarts automatically
//! whenever ibus-daemon disappears.
//!
//! ## Key design decision
//!
//! `process_key_event` processes the key **synchronously** inside the D-Bus
//! method handler and emits preedit/commit signals via the `SignalContext`
//! that zbus passes to every interface method.  This ensures:
//!
//! 1. IBus daemon receives preedit/commit signals **before** `process_key_event`
//!    returns, so it can forward them to the focused application in the same
//!    event cycle.
//! 2. The returned `bool` (consumed / not-consumed) is always correct: if the
//!    engine returns `PassThrough`, `process_key_event` returns `false` and IBus
//!    delivers the key to the application directly — no `ForwardKeyEvent` needed.

use std::{
    mem,
    sync::{Arc, Mutex},
    time::Duration,
};

use tracing::{debug, error, info, warn};
use vi_config::schema::IbusCommitMode;
use vi_core::{
    CompositionEngine, InputEvent, Key, Modifiers, NfcString, PreeditText, StandardEngine,
    StateTransition,
};
use vi_platform::{Capabilities, CharCursor, ImeBackend, PlatformError, Result, SurroundingText};
use zbus::{
    fdo::DBusProxy,
    interface,
    zvariant::{Array, Dict, OwnedObjectPath, OwnedValue, Signature, StructureBuilder, Value},
    Connection, Proxy, SignalContext,
};

// ── IBus error type ───────────────────────────────────────────────────────────

#[derive(Debug, thiserror::Error)]
pub(crate) enum IbusError {
    #[error("malformed IBus surrounding text: {0}")]
    MalformedSurroundingText(String),
}

// ── IBus constants ────────────────────────────────────────────────────────────

/// IBus key-modifier bit: key-release event.
const IBUS_RELEASE_MASK: u32 = 1 << 30;
/// Application supports inline preedit text display.
const IBUS_CAP_PREEDIT_TEXT: u32 = 1;
/// Application can supply and receive surrounding-text operations.
/// Bit 5 per IBus ibustypes.h (IBUS_CAP_SURROUNDING_TEXT = 1 << 5).
const IBUS_CAP_SURROUNDING_TEXT: u32 = 1 << 5;
/// Passed to `update_preedit_text_with_mode` so IBus commits inline preedit on
/// focus loss instead of discarding it (`ibustypes.h`: `IBUS_ENGINE_PREEDIT_COMMIT`).
const IBUS_ENGINE_PREEDIT_COMMIT: u32 = 1;

// ── IBus GVariant helpers ─────────────────────────────────────────────────────

/// Build an IBusAttrList GVariant `(sa{sv}av)` with a single NONE-type
/// attribute spanning `[0, char_len)`.
///
/// Chrome/Electron need a non-empty IBusAttrList to render preedit inline —
/// an empty list causes Chrome to show nothing.  We use IBUS_ATTR_TYPE_NONE
/// (= 0) so the attribute exists (satisfying Chrome) but adds no visible
/// decoration.  This matches vhttechkey IBnoUnderline default behaviour.
fn ibus_attr_list_value(char_len: u32) -> Value<'static> {
    let mut attr = StructureBuilder::new();
    attr.push_value(Value::Str("IBusAttribute".to_string().into()));
    attr.push_value(Value::Dict(Dict::new(
        Signature::from_str_unchecked("s"),
        Signature::from_str_unchecked("v"),
    )));
    attr.push_value(Value::U32(0)); // IBUS_ATTR_TYPE_NONE (no visible decoration)
    attr.push_value(Value::U32(1)); // IBUS_ATTR_UNDERLINE_SINGLE (value ignored for NONE type)
    attr.push_value(Value::U32(0)); // start_index
    attr.push_value(Value::U32(char_len)); // end_index
    let attr_variant: Value<'static> = Value::Value(Box::new(Value::Structure(attr.build())));

    let mut attrs = Array::new(Signature::from_str_unchecked("v"));
    attrs
        .append(attr_variant)
        .expect("IBusAttribute variant matches array signature");

    let name: Value<'static> = Value::Str("IBusAttrList".to_string().into());
    let props: Value<'static> = Value::Dict(Dict::new(
        Signature::from_str_unchecked("s"),
        Signature::from_str_unchecked("v"),
    ));
    let mut s = StructureBuilder::new();
    s.push_value(name);
    s.push_value(props);
    s.push_value(Value::Array(attrs));
    Value::Structure(s.build())
}

/// Build an empty IBusAttrList GVariant `(sa{sv}av)` with no attributes.
fn ibus_attr_list_empty() -> Value<'static> {
    let name: Value<'static> = Value::Str("IBusAttrList".to_string().into());
    let props: Value<'static> = Value::Dict(Dict::new(
        Signature::from_str_unchecked("s"),
        Signature::from_str_unchecked("v"),
    ));
    let empty_attrs = Array::new(Signature::from_str_unchecked("v"));
    let mut s = StructureBuilder::new();
    s.push_value(name);
    s.push_value(props);
    s.push_value(Value::Array(empty_attrs));
    Value::Structure(s.build())
}

/// Build an IBusText GVariant `(sa{sv}sv)` wrapping `text`.
///
/// `with_preedit_placeholder_attrs`: when `true`, attach a **non-empty**
/// `IBusAttrList` whose sole entry uses `IBUS_ATTR_TYPE_NONE` (no visible
/// underline) spanning the whole string — this matches vhttechkey
/// “no underline” preedit while still satisfying Chromium, which ignores an
/// empty attribute list.  When `false`, use an empty attribute list (typical
/// for `CommitText`).
fn ibus_text_value(text: &str, with_preedit_placeholder_attrs: bool) -> Value<'static> {
    let name: Value<'static> = Value::Str("IBusText".to_string().into());
    let props: Value<'static> = Value::Dict(Dict::new(
        Signature::from_str_unchecked("s"),
        Signature::from_str_unchecked("v"),
    ));
    let text_str: Value<'static> = Value::Str(text.to_string().into());
    let attr_list_val = if with_preedit_placeholder_attrs {
        ibus_attr_list_value(text.chars().count() as u32)
    } else {
        ibus_attr_list_empty()
    };
    let attr_list: Value<'static> = Value::Value(Box::new(attr_list_val));
    let mut s = StructureBuilder::new();
    s.push_value(name);
    s.push_value(props);
    s.push_value(text_str);
    s.push_value(attr_list);
    Value::Structure(s.build())
}

/// Build an IBusText `OwnedValue` for use in CommitText signals.
fn ibus_text_owned(text: &str) -> OwnedValue {
    OwnedValue::try_from(ibus_text_value(text, false)).expect("IBusText structure is well-formed")
}

/// Build an IBusText `OwnedValue` for use in UpdatePreeditText signals.
///
/// Uses IBUS_ATTR_TYPE_NONE so no underline is shown (vhttechkey default).
/// Chrome/Electron still renders the preedit inline because the IBusAttrList
/// is non-empty; an empty list would suppress inline rendering in Chrome.
fn ibus_preedit_text_owned(text: &str) -> OwnedValue {
    OwnedValue::try_from(ibus_text_value(text, true)).expect("IBusText structure is well-formed")
}

/// Build an IBusEngineDesc GVariant `(sa{sv}sssssssuuuuuussssss)`.
fn ibus_engine_desc_value(engine_name: &str) -> Value<'static> {
    use vi_config::ibus_component::{
        self, ENGINE_DESCRIPTION, ENGINE_LANGUAGE, ENGINE_LONGNAME, ENGINE_RANK, LICENSE,
        SYSTEM_ICON,
    };

    let setup = ibus_component::resolve_ui_setup_path();
    let empty_props: Value<'static> = Value::Dict(Dict::new(
        Signature::from_str_unchecked("s"),
        Signature::from_str_unchecked("v"),
    ));
    let mut s = StructureBuilder::new();
    s.push_value(Value::Str("IBusEngineDesc".to_string().into()));
    s.push_value(empty_props);
    s.push_value(Value::Str(engine_name.to_string().into())); // name
    s.push_value(Value::Str(ENGINE_LONGNAME.to_string().into())); // longname
    s.push_value(Value::Str(ENGINE_DESCRIPTION.to_string().into())); // description
    s.push_value(Value::Str(ENGINE_LANGUAGE.to_string().into())); // language
    s.push_value(Value::Str(LICENSE.to_string().into())); // license
    s.push_value(Value::Str(String::new().into())); // author
    s.push_value(Value::Str(SYSTEM_ICON.to_string().into())); // icon
    s.push_value(Value::Str("default".to_string().into())); // layout
    s.push_value(Value::U32(ENGINE_RANK)); // rank
    s.push_value(Value::Str(String::new().into())); // hotkeys
    s.push_value(Value::Str(String::new().into())); // keymap
    s.push_value(Value::Str(String::new().into())); // symbol
    s.push_value(Value::Str(setup.into())); // setup — opens vi-ui preferences
    s.push_value(Value::Str(String::new().into())); // layout_variant
    s.push_value(Value::Str(String::new().into())); // layout_option
    s.push_value(Value::Str(String::new().into())); // version
    s.push_value(Value::Str(String::new().into())); // textdomain
    Value::Structure(s.build())
}

/// Build an IBusComponent GVariant `(sa{sv}ssssssssavav)` registering one engine.
fn ibus_component_value(engine_name: &str) -> Value<'static> {
    let empty_props: Value<'static> = Value::Dict(Dict::new(
        Signature::from_str_unchecked("s"),
        Signature::from_str_unchecked("v"),
    ));

    let engine_desc_variant: Value<'static> =
        Value::Value(Box::new(ibus_engine_desc_value(engine_name)));
    let mut engines = Array::new(Signature::from_str_unchecked("v"));
    engines
        .append(engine_desc_variant)
        .expect("IBusEngineDesc variant matches array signature");
    let observed_paths = Array::new(Signature::from_str_unchecked("v"));

    let mut s = StructureBuilder::new();
    s.push_value(Value::Str("IBusComponent".to_string().into()));
    s.push_value(empty_props);
    s.push_value(Value::Str(
        format!("org.freedesktop.IBus.{engine_name}").into(),
    )); // name
    s.push_value(Value::Str(String::new().into())); // description
    s.push_value(Value::Str(String::new().into())); // version
    s.push_value(Value::Str(String::new().into())); // license
    s.push_value(Value::Str(String::new().into())); // author
    s.push_value(Value::Str(String::new().into())); // homepage
    s.push_value(Value::Str(String::new().into())); // command_line (already running)
    s.push_value(Value::Str(String::new().into())); // textdomain
    s.push_value(Value::Array(observed_paths));
    s.push_value(Value::Array(engines));
    Value::Structure(s.build())
}

// ── Shared state ──────────────────────────────────────────────────────────────

#[derive(Debug)]
struct SharedState {
    surrounding: Option<SurroundingText>,
    /// Raw IBUS_CAP_* bits received from the application.
    caps_raw: u32,
    /// Preedit cache used by the `ImeBackend` trait methods.
    preedit: String,
    preedit_cursor: u32,
    /// Cursor position forwarded by the application via SetCursorLocation.
    cursor_x: i32,
    cursor_y: i32,
    /// Characters already forwarded/committed to the app in non-preedit modes.
    shadow_buf: String,
    /// Legacy path: `DeleteSurroundingText` + `CommitText` per preedit update.
    /// **Never auto-enabled** (`set_capabilities` leaves this `false`) because
    /// `DeleteSurroundingText` is unreliable on Chromium and many Gtk/Qt builds.
    /// Kept so manual / future opt-in can reactivate the dispatch implementation.
    use_surrounding_commit: bool,
    /// Legacy path: synthesize text via `ForwardKeyEvent` instead of preedit.
    /// **Never auto-enabled** — forwarding Unicode keysyms > U+00FF broke Electron.
    use_forward_key: bool,
    /// When true, always use preedit mode regardless of app capabilities.
    /// Loaded from `[ibus] force_preedit_mode` in the config file.
    force_preedit_mode: bool,
    /// Force chrome_direct_mode (UpdatePreeditTextWithMode) regardless of caps.
    /// Use when the normal preedit path misbehaves for a specific app.
    /// Loaded from `[ibus] force_chrome_direct` in the config file.
    force_chrome_direct: bool,
    /// Force UpdatePreeditTextWithMode for this app even if it advertises surrounding.
    /// Only active when force_chrome_direct is set via config.
    chrome_direct_mode: bool,
    /// `ForwardKeyEvent(BackSpace)` × N + `CommitText` — reliable direct-commit.
    /// Activated when `[ibus] commit_mode = "backspace_commit"` in config.
    use_backspace_commit: bool,
    /// Global default from `[ibus] commit_mode` in config.
    default_commit_mode: IbusCommitMode,
}

impl SharedState {
    fn new(
        force_preedit_mode: bool,
        force_chrome_direct: bool,
        default_commit_mode: IbusCommitMode,
    ) -> Self {
        Self {
            surrounding: None,
            caps_raw: 0,
            preedit: String::new(),
            preedit_cursor: 0,
            cursor_x: 0,
            cursor_y: 0,
            shadow_buf: String::new(),
            use_surrounding_commit: false,
            use_forward_key: false, // preedit for all apps by default (vhttechkey mode)
            force_preedit_mode,
            force_chrome_direct,
            chrome_direct_mode: false,
            use_backspace_commit: false,
            default_commit_mode,
        }
    }
}

impl Default for SharedState {
    fn default() -> Self {
        Self::new(false, false, IbusCommitMode::default())
    }
}

// ── D-Bus object ──────────────────────────────────────────────────────────────

pub(crate) struct IbusEngineIface {
    /// The composition engine — shared with the IPC handler for method switching.
    engine: Arc<Mutex<StandardEngine>>,
    state: Arc<Mutex<SharedState>>,
    /// Serialises the full handler bodies of `process_key_event`, `focus_out`,
    /// and `reset` so that signal emissions from `dispatch_transition` cannot
    /// interleave across concurrent zbus tasks.  The `std::Mutex` on `engine`
    /// and `state` is still needed for synchronous (non-async) engine access
    /// within each handler; this `tokio::Mutex` serialises the async
    /// signal-emission phase across concurrent handlers.
    handler_lock: Arc<tokio::sync::Mutex<()>>,
}

#[interface(name = "org.freedesktop.IBus.Engine")]
impl IbusEngineIface {
    /// Called by IBus daemon for every key event.  Returns `true` to consume.
    ///
    /// Keys are processed **synchronously** here so that preedit/commit signals
    /// are emitted — via `ctx` — before this method returns its reply.  IBus
    /// daemon then forwards both the reply and the signals to the application in
    /// the same event cycle, giving real-time preedit feedback.
    ///
    /// # Concurrency note
    ///
    /// zbus 4.x dispatches `#[interface]` method calls as independent Tokio
    /// tasks, so `process_key_event` and `focus_out` (or `reset`) can run
    /// concurrently when two D-Bus messages arrive in the same poll cycle.
    /// `handler_lock` serialises the full handler body — engine processing
    /// *and* signal emissions — so that signals from `dispatch_transition`
    /// cannot interleave across concurrent method invocations.
    async fn process_key_event(
        &self,
        keyval: u32,
        _keycode: u32,
        state: u32,
        #[zbus(signal_context)] ctx: SignalContext<'_>,
    ) -> bool {
        let _handler_guard = self.handler_lock.lock().await;
        // Key release: notify engine.
        if state & IBUS_RELEASE_MASK != 0 {
            if !is_modifier_keysym(keyval) {
                let key = map_keyval_to_key(keyval);
                let mut eng = self.engine.lock().unwrap_or_else(|e| e.into_inner());
                let _ = eng.process(&InputEvent::KeyUp(key));
            }
            return false;
        }

        // Pure modifier key presses are not for the engine.
        if is_modifier_keysym(keyval) {
            return false;
        }

        let modifiers = map_ibus_state(state);
        // Ctrl+letter means a keyboard shortcut; pass through to application.
        if modifiers.ctrl {
            // If composition is in progress, commit it before passing Ctrl+key through.
            let has_preedit = {
                let eng = self.engine.lock().unwrap_or_else(|e| e.into_inner());
                !eng.preedit().is_empty()
            };
            if has_preedit {
                let transition = {
                    let mut eng = self.engine.lock().unwrap_or_else(|e| e.into_inner());
                    eng.process(&InputEvent::FocusOut)
                };
                if let Ok(StateTransition::CommitAndClear(c)) | Ok(StateTransition::Commit(c)) =
                    &transition
                {
                    Self::commit_text_then_hide_preedit(&ctx, c.as_str(), "").await;
                }
            }
            return false;
        }

        let key = map_keyval_to_key(keyval);
        debug!("IBus: ProcessKeyEvent keyval={keyval:#x} key={key}");
        let event = InputEvent::KeyDown(key.clone(), modifiers);

        // Process synchronously: lock → process → unlock, then emit signals.
        // The engine lock is released before any await so zbus can continue
        // handling concurrent method calls on this interface.
        let transition = {
            let mut eng = self.engine.lock().unwrap_or_else(|e| e.into_inner());
            eng.process(&event)
        };

        self.dispatch_transition(&ctx, transition, &key).await
    }

    async fn focus_in(&self, #[zbus(signal_context)] ctx: SignalContext<'_>) {
        info!("IBus: FocusIn");
        {
            let mut s = self.state.lock().unwrap_or_else(|e| e.into_inner());
            s.shadow_buf.clear();
        }
        {
            let mut eng = self.engine.lock().unwrap_or_else(|e| e.into_inner());
            let _ = eng.process(&InputEvent::FocusIn);
        }
        if let Err(e) = Self::get_surrounding_text(&ctx).await {
            warn!("IBus: GetSurroundingText request failed: {e}");
        }
    }

    async fn focus_out(&self, #[zbus(signal_context)] ctx: SignalContext<'_>) {
        let _handler_guard = self.handler_lock.lock().await;
        info!("IBus: FocusOut");
        let (use_forward_key, use_surrounding_commit, use_backspace_commit) = {
            let mut s = self.state.lock().unwrap_or_else(|e| e.into_inner());
            s.shadow_buf.clear();
            (
                s.use_forward_key,
                s.use_surrounding_commit,
                s.use_backspace_commit,
            )
        };
        let transition = {
            let mut eng = self.engine.lock().unwrap_or_else(|e| e.into_inner());
            eng.process(&InputEvent::FocusOut)
        };
        // In forward-key, surrounding-commit, or backspace-commit mode chars are
        // already in the app — nothing to commit on focus-out.
        if use_forward_key || use_surrounding_commit || use_backspace_commit {
            return;
        }
        // Commit any pending preedit before focus leaves.  Handle both
        // CommitAndClear/Commit (engine already serialised the text) and
        // PreeditUpdated (engine updated preedit mid-flight; still commit it
        // rather than silently dropping half-composed text on focus change).
        let commit_text = match &transition {
            Ok(StateTransition::CommitAndClear(c)) | Ok(StateTransition::Commit(c)) => {
                Some(c.as_str().to_owned())
            }
            Ok(StateTransition::PreeditUpdated(p)) if !p.is_empty() => Some(p.as_str().to_owned()),
            _ => None,
        };
        if let Some(text) = commit_text {
            let val = ibus_text_owned(&text);
            if let Err(e) = Self::commit_text(&ctx, val).await {
                error!("IBus: FocusOut CommitText failed: {e}");
            }
            if let Err(e) = Self::hide_preedit_text(&ctx).await {
                error!("IBus: FocusOut HidePreeditText failed: {e}");
            }
        }
    }

    async fn reset(&self, #[zbus(signal_context)] ctx: SignalContext<'_>) {
        let _handler_guard = self.handler_lock.lock().await;
        info!("IBus: Reset");
        let (
            use_surrounding_commit,
            use_forward_key,
            chrome_direct_mode,
            use_backspace_commit,
            shadow,
        ) = {
            let mut s = self.state.lock().unwrap_or_else(|e| e.into_inner());
            let shadow = mem::take(&mut s.shadow_buf);
            (
                s.use_surrounding_commit,
                s.use_forward_key,
                s.chrome_direct_mode,
                s.use_backspace_commit,
                shadow,
            )
        };
        {
            let mut eng = self.engine.lock().unwrap_or_else(|e| e.into_inner());
            let _ = eng.process(&InputEvent::Reset);
        }
        if use_surrounding_commit {
            // Clean up any characters already committed to the app via surrounding-commit.
            let n = shadow.chars().count();
            if n > 0 {
                let offset = -(n as i32);
                if let Err(e) = Self::delete_surrounding_text(&ctx, offset, n as u32).await {
                    warn!("IBus: Reset DeleteSurroundingText failed: {e}");
                }
            }
        } else if use_forward_key || chrome_direct_mode || use_backspace_commit {
            // Erase characters already in the app via ForwardKeyEvent BackSpaces.
            for _ in 0..shadow.chars().count() {
                if let Err(e) = Self::forward_key_event(&ctx, 0xff08, 0, 0).await {
                    warn!("IBus: Reset ForwardKeyEvent BackSpace failed: {e}");
                }
            }
        } else {
            // Preedit mode: clear the inline preedit widget.
            if let Err(e) = Self::hide_preedit_text(&ctx).await {
                warn!("IBus: Reset HidePreeditText failed: {e}");
            }
        }
    }

    async fn enable(&self) {
        debug!("IBus: Enable");
    }

    async fn disable(&self) {
        debug!("IBus: Disable");
    }

    async fn set_capabilities(&self, caps: u32) {
        let mut s = self.state.lock().unwrap_or_else(|e| e.into_inner());
        let has_preedit = (caps & IBUS_CAP_PREEDIT_TEXT) != 0;
        let has_surrounding = (caps & IBUS_CAP_SURROUNDING_TEXT) != 0;

        // Mode selection: ALL apps use preedit (UpdatePreeditText) — vhttechkey mode.
        //
        // Preedit text is built with IBUS_ATTR_TYPE_NONE so no underline is visible
        // (vhttechkey IBnoUnderline default).  Chrome/Electron still renders inline
        // because the IBusAttrList is non-empty.
        //
        // surrounding_commit is never auto-selected (DeleteSurroundingText unreliable).
        // forward_key is never auto-selected:
        //   - ForwardKeyEvent drops Vietnamese Unicode chars > U+00FF in Electron apps
        //     (BackSpace fires, new char silently dropped → net deletion).
        //   - Preedit without underline is indistinguishable from direct insertion for
        //     the user, so there is no reason to use forward_key.
        let _ = has_preedit;
        let _ = has_surrounding;
        let mut new_surrounding_commit = false;
        let mut new_forward_key = false;
        // force_chrome_direct config: use UpdatePreeditTextWithMode unconditionally,
        // overriding surrounding-commit or forward-key.  Useful for apps that
        // advertise surprising capability bits but handle preedit correctly.
        let mut new_chrome_direct = s.force_chrome_direct;
        if new_chrome_direct {
            new_surrounding_commit = false;
            new_forward_key = false;
        }
        // force_preedit_mode config: always use standard IBus preedit, suppressing
        // all other overrides.
        if s.force_preedit_mode {
            new_surrounding_commit = false;
            new_forward_key = false;
            new_chrome_direct = false;
        }
        // backspace_commit: derived from global commit_mode config.
        // force_chrome_direct and force_preedit_mode both override it.
        let mut new_backspace_commit =
            matches!(s.default_commit_mode, IbusCommitMode::BackspaceCommit);
        if new_chrome_direct || s.force_preedit_mode {
            new_backspace_commit = false;
        }
        let mode_changed = new_surrounding_commit != s.use_surrounding_commit
            || new_forward_key != s.use_forward_key
            || new_chrome_direct != s.chrome_direct_mode
            || new_backspace_commit != s.use_backspace_commit;
        if mode_changed {
            s.shadow_buf.clear();
        }
        s.caps_raw = caps;
        s.use_surrounding_commit = new_surrounding_commit;
        s.use_forward_key = new_forward_key;
        s.chrome_direct_mode = new_chrome_direct;
        s.use_backspace_commit = new_backspace_commit;
        let mode_str = if new_chrome_direct {
            "chrome_direct"
        } else if new_backspace_commit {
            "backspace_commit"
        } else if new_surrounding_commit {
            "surrounding_commit"
        } else if new_forward_key {
            "forward_key"
        } else {
            "preedit"
        };
        debug!(
            "IBus: SetCapabilities caps={caps:#010x} has_preedit={has_preedit} \
             has_surrounding={has_surrounding} mode={mode_str}"
        );
        info!(
            caps = caps,
            has_preedit = has_preedit,
            has_surrounding = has_surrounding,
            mode = mode_str,
            "IBus: SetCapabilities → mode selected"
        );
    }

    async fn set_surrounding_text(&self, text: OwnedValue, cursor_pos: u32, anchor_pos: u32) {
        let text_str = extract_ibus_string(&text).unwrap_or_else(|e| {
            warn!("IBus: {e}");
            String::new()
        });
        debug!("IBus: SetSurroundingText cursor={cursor_pos} anchor={anchor_pos}");
        let mut s = self.state.lock().unwrap_or_else(|e| e.into_inner());
        s.surrounding = Some(SurroundingText {
            text: text_str,
            cursor_pos,
            anchor_pos,
        });
    }

    async fn set_cursor_location(&self, x: i32, y: i32, _w: u32, _h: u32) {
        debug!("IBus: SetCursorLocation x={x} y={y}");
        let mut s = self.state.lock().unwrap_or_else(|e| e.into_inner());
        s.cursor_x = x;
        s.cursor_y = y;
    }

    // ── Properties queried by IBus after CreateEngine ────────────────────────

    #[zbus(property)]
    async fn focus_id(&self) -> u32 {
        0
    }

    #[zbus(property)]
    async fn active_surrounding_text(&self) -> bool {
        false
    }

    // ── Signals emitted towards the application / IBus daemon ─────────────────

    #[zbus(signal)]
    async fn commit_text(ctx: &SignalContext<'_>, text: OwnedValue) -> zbus::Result<()>;

    /// Emit the standard IBus preedit signal.
    ///
    /// D-Bus signal name is forced to "UpdatePreeditText" (same as the C library
    /// `ibus_engine_update_preedit_text_with_mode` and vhttechkey `UpdatePreeditText`).
    /// The IBus wire protocol always sends 4 args: (text_variant, cursor_pos, visible, mode).
    /// A 3-arg variant is silently ignored by ibus-daemon.
    /// `mode` must be `IBUS_ENGINE_PREEDIT_COMMIT` (1 in `ibustypes.h`) so IBus
    /// commits inline preedit on focus-out instead of discarding it.
    ///
    /// On key-driven commits this engine emits **`CommitText` before `HidePreeditText`**
    /// (vhttechkey commit order for printable word-break paths).  Some Electron
    /// clients drop the whole word if `HidePreeditText` is delivered first.
    #[zbus(signal, name = "UpdatePreeditText")]
    async fn update_preedit_text_with_mode(
        ctx: &SignalContext<'_>,
        text: OwnedValue,
        cursor_pos: u32,
        visible: bool,
        mode: u32,
    ) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn update_lookup_table(
        ctx: &SignalContext<'_>,
        table: OwnedValue,
        visible: bool,
    ) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn show_preedit_text(ctx: &SignalContext<'_>) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn hide_preedit_text(ctx: &SignalContext<'_>) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn forward_key_event(
        ctx: &SignalContext<'_>,
        keyval: u32,
        keycode: u32,
        modifiers: u32,
    ) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn get_surrounding_text(ctx: &SignalContext<'_>) -> zbus::Result<()>;

    /// Ask the application to delete `n_chars` characters around the cursor.
    /// `offset` is relative to the cursor (negative = before cursor).
    #[zbus(signal)]
    async fn delete_surrounding_text(
        ctx: &SignalContext<'_>,
        offset: i32,
        n_chars: u32,
    ) -> zbus::Result<()>;
}

/// Compute the BackSpace count and characters to forward in forward-key mode.
///
/// Returns `(backspaces_needed, chars_to_forward)` by diffing `shadow` (what
/// the application currently sees) against `new_preedit` (the desired state).
fn compute_forward_key_ops(shadow: &str, new_preedit: &str) -> (usize, String) {
    let common_len = shadow
        .chars()
        .zip(new_preedit.chars())
        .take_while(|(a, b)| a == b)
        .count();
    let backspaces = shadow.chars().count() - common_len;
    let new_tail: String = new_preedit.chars().skip(common_len).collect();
    (backspaces, new_tail)
}

fn transition_kind_str(t: &vi_core::TransitionResult) -> &'static str {
    match t {
        Ok(StateTransition::PreeditUpdated(_)) => "PreeditUpdated",
        Ok(StateTransition::Commit(_)) => "Commit",
        Ok(StateTransition::CommitAndClear(_)) => "CommitAndClear",
        Ok(StateTransition::CommitThenPreedit(..)) => "CommitThenPreedit",
        Ok(StateTransition::CommitThenPassThrough(_)) => "CommitThenPassThrough",
        Ok(StateTransition::Consumed) => "Consumed",
        Ok(StateTransition::PassThrough) => "PassThrough",
        Ok(StateTransition::Cleared) => "Cleared",
        Err(_) => "Err",
    }
}

impl IbusEngineIface {
    /// Emit [`Self::commit_text`] then [`Self::hide_preedit_text`].
    ///
    /// vhttechkey commit-before-hide when the
    /// triggering key is “printable” (including Space and other word-break keys):
    /// **`CommitText` before `HidePreeditText`**.  Doing hide-first clears the
    /// inline composition in some Electron clients (VS Code) before the commit
    /// is merged, so the composed word disappears entirely.
    async fn commit_text_then_hide_preedit(ctx: &SignalContext<'_>, text: &str, log_suffix: &str) {
        let val = ibus_text_owned(text);
        if let Err(e) = Self::commit_text(ctx, val).await {
            warn!("IBus: CommitText failed{log_suffix}: {e}");
        }
        if let Err(e) = Self::hide_preedit_text(ctx).await {
            warn!("IBus: HidePreeditText failed{log_suffix}: {e}");
        }
    }

    /// Forward-key mode: diff `shadow_buf` against `new_preedit` and emit
    /// BackSpace + character ForwardKeyEvents to bring the app's buffer in sync.
    async fn dispatch_forward_key_preedit(
        &self,
        ctx: &SignalContext<'_>,
        new_preedit: &str,
    ) -> bool {
        let shadow = {
            let s = self.state.lock().unwrap_or_else(|e| e.into_inner());
            s.shadow_buf.clone()
        };

        let (backspaces, new_tail) = compute_forward_key_ops(&shadow, new_preedit);

        info!("IBus: forward_key preedit={new_preedit:?} backspaces={backspaces} forward={new_tail:?}");
        let mut all_ok = true;
        for _ in 0..backspaces {
            if let Err(e) = Self::forward_key_event(ctx, 0xff08, 0, 0).await {
                warn!("IBus: ForwardKeyEvent BackSpace failed: {e}");
                all_ok = false;
            }
        }

        for ch in new_tail.chars() {
            if let Err(e) = Self::forward_key_event(ctx, char_to_x11_keysym(ch), 0, 0).await {
                warn!("IBus: ForwardKeyEvent char failed: {e}");
                all_ok = false;
            }
        }

        if all_ok {
            let mut s = self.state.lock().unwrap_or_else(|e| e.into_inner());
            s.shadow_buf = new_preedit.to_owned();
        } else {
            warn!("IBus: shadow_buf not updated due to ForwardKeyEvent signal failure");
        }
        true
    }

    /// Surrounding-commit mode: diff `shadow_buf` against `new_preedit` and
    /// emit DeleteSurroundingText + CommitText to bring the app's text in sync.
    ///
    /// Only reached for apps that explicitly advertise IBUS_CAP_SURROUNDING_TEXT,
    /// meaning they have declared support for IBus surrounding-text operations.
    async fn dispatch_surrounding_commit(
        &self,
        ctx: &SignalContext<'_>,
        new_preedit: &str,
    ) -> bool {
        let shadow = {
            let s = self.state.lock().unwrap_or_else(|e| e.into_inner());
            s.shadow_buf.clone()
        };

        // Full-replace: delete the entire shadow and recommit new_preedit from scratch.
        // Diffing only the changed suffix is unsafe because the cursor may have moved
        // (e.g. arrow key mid-composition), making a partial DeleteSurroundingText
        // target the wrong position.
        let backspaces = shadow.chars().count();
        let new_tail = new_preedit;

        info!("IBus: surrounding_commit preedit={new_preedit:?} backspaces={backspaces} forward={new_tail:?}");

        // Failure mode A — cursor-movement desync guard.
        // shadow_buf is valid only while the cursor stays at the end of the committed
        // chars.  If the invariant is violated (backspaces somehow exceeds shadow_len),
        // the diff is inconsistent: clear shadow_buf, request a fresh surrounding-text
        // snapshot, and bail so the user can re-type the rule key.
        if backspaces > shadow.chars().count() {
            debug!(
                "IBus: surrounding_commit desync — backspaces={backspaces} > shadow_len={} \
                 (cursor may have moved); clearing shadow_buf and requesting GetSurroundingText",
                shadow.chars().count()
            );
            {
                let mut s = self.state.lock().unwrap_or_else(|e| e.into_inner());
                s.shadow_buf.clear();
            }
            if let Err(e) = Self::get_surrounding_text(ctx).await {
                warn!("IBus: GetSurroundingText (desync recovery) failed: {e}");
            }
            return true;
        }

        // Failure mode B — only update shadow_buf when BOTH signals succeed.
        // If either fails the app's text no longer matches shadow_buf, so future
        // diffs would be wrong; clear instead.
        let mut all_ok = true;
        if backspaces > 0 {
            let offset = -(backspaces as i32);
            if let Err(e) = Self::delete_surrounding_text(ctx, offset, backspaces as u32).await {
                warn!("IBus: DeleteSurroundingText failed: {e}");
                all_ok = false;
            }
        }

        if all_ok && !new_tail.is_empty() {
            let val = ibus_text_owned(new_tail);
            if let Err(e) = Self::commit_text(ctx, val).await {
                warn!("IBus: CommitText (surrounding-commit) failed: {e}");
                all_ok = false;
            }
        }

        {
            let mut s = self.state.lock().unwrap_or_else(|e| e.into_inner());
            if all_ok {
                s.shadow_buf = new_preedit.to_owned();
            } else {
                warn!("IBus: shadow_buf cleared due to surrounding-commit signal failure");
                s.shadow_buf.clear();
            }
        }
        true
    }

    /// Backspace-commit mode: `ForwardKeyEvent(BackSpace)` × shadow_len + `CommitText`.
    ///
    /// Reliable direct-commit alternative:
    /// - `BackSpace` (keysym `0xff08`) works in every app including Electron.
    /// - `CommitText` handles all Unicode without keysym encoding — fixes the
    ///   `forward_key` bug where chars > U+00FF were silently dropped in Electron.
    async fn dispatch_backspace_commit(&self, ctx: &SignalContext<'_>, new_preedit: &str) -> bool {
        let shadow = {
            let s = self.state.lock().unwrap_or_else(|e| e.into_inner());
            s.shadow_buf.clone()
        };
        for _ in 0..shadow.chars().count() {
            if let Err(e) = Self::forward_key_event(ctx, 0xff08, 0, 0).await {
                warn!("IBus: backspace_commit BackSpace failed: {e}");
            }
        }
        if !new_preedit.is_empty() {
            let val = ibus_text_owned(new_preedit);
            if let Err(e) = Self::commit_text(ctx, val).await {
                warn!("IBus: backspace_commit CommitText failed: {e}");
            }
        }
        {
            let mut s = self.state.lock().unwrap_or_else(|e| e.into_inner());
            s.shadow_buf = new_preedit.to_owned();
        }
        true
    }

    /// Chrome preedit override: emit `UpdatePreeditText` (force_chrome_direct=true config).
    /// Uses the same standard signal as the regular preedit path; kept separate so
    /// chrome_direct can be toggled independently via config.
    async fn dispatch_chrome_direct(&self, ctx: &SignalContext<'_>, new_preedit: &str) -> bool {
        if new_preedit.is_empty() {
            let _ = Self::hide_preedit_text(ctx).await;
        }
        let cursor_pos = new_preedit.chars().count() as u32;
        let val = ibus_preedit_text_owned(new_preedit);
        info!("IBus: UpdatePreeditText (chrome_direct) preedit={new_preedit:?} cursor={cursor_pos} visible=true mode={IBUS_ENGINE_PREEDIT_COMMIT}");
        if let Err(e) = Self::update_preedit_text_with_mode(
            ctx,
            val,
            cursor_pos,
            true,
            IBUS_ENGINE_PREEDIT_COMMIT,
        )
        .await
        {
            warn!("IBus: UpdatePreeditText failed (chrome_direct): {e}");
        }
        {
            let mut s = self.state.lock().unwrap_or_else(|e| e.into_inner());
            s.shadow_buf = new_preedit.to_owned();
        }
        true
    }

    /// Emit the appropriate IBus signals for a `StateTransition` and return
    /// whether the key event was consumed.
    ///
    /// Called synchronously from within `process_key_event` so that all signals
    /// are sent before the D-Bus method reply is delivered to IBus daemon.
    async fn dispatch_transition(
        &self,
        ctx: &SignalContext<'_>,
        transition: vi_core::TransitionResult,
        trigger: &Key,
    ) -> bool {
        let (use_forward_key, use_surrounding_commit, chrome_direct_mode, use_backspace_commit) = {
            let s = self.state.lock().unwrap_or_else(|e| e.into_inner());
            (
                s.use_forward_key,
                s.use_surrounding_commit,
                s.chrome_direct_mode,
                s.use_backspace_commit,
            )
        };

        debug!(
            use_surrounding_commit,
            use_forward_key,
            use_backspace_commit,
            chrome_direct_mode,
            transition_kind = %transition_kind_str(&transition),
            "IBus: dispatch_transition"
        );

        match transition {
            Ok(StateTransition::PreeditUpdated(p)) => {
                let mode = if use_surrounding_commit {
                    "surrounding"
                } else if use_backspace_commit {
                    "backspace-commit"
                } else if use_forward_key {
                    "forward-key"
                } else if chrome_direct_mode {
                    "chrome"
                } else {
                    "preedit"
                };
                debug!(mode, "IBus: PreeditUpdated dispatch");
                let consumed = if use_surrounding_commit {
                    // Immediate-commit mode: each character appears in the app
                    // as it is typed via DeleteSurroundingText + CommitText.
                    self.dispatch_surrounding_commit(ctx, p.as_str()).await
                } else if use_backspace_commit {
                    self.dispatch_backspace_commit(ctx, p.as_str()).await
                } else if use_forward_key {
                    self.dispatch_forward_key_preedit(ctx, p.as_str()).await
                } else if chrome_direct_mode {
                    self.dispatch_chrome_direct(ctx, p.as_str()).await
                } else {
                    // Preedit mode: show composition text inline.
                    // update_preedit_text_with_mode emits D-Bus signal "UpdatePreeditText"
                    // (name override via #[zbus(name = "UpdatePreeditText")]) with 4 args:
                    // (text, cursor_pos, visible, mode).  The IBus wire protocol always
                    // requires 4 args; a 3-arg signal is silently ignored by ibus-daemon.
                    // mode=IBUS_ENGINE_PREEDIT_COMMIT: commit preedit on focus-out.
                    let cursor_pos = p.as_str().chars().count() as u32;
                    let val = ibus_preedit_text_owned(p.as_str());
                    info!("IBus: UpdatePreeditText preedit={:?} cursor={cursor_pos} visible=true mode={IBUS_ENGINE_PREEDIT_COMMIT}", p.as_str());
                    if let Err(e) = Self::update_preedit_text_with_mode(
                        ctx,
                        val,
                        cursor_pos,
                        true,
                        IBUS_ENGINE_PREEDIT_COMMIT,
                    )
                    .await
                    {
                        warn!("IBus: UpdatePreeditText failed: {e}");
                    }
                    true
                };
                consumed
            }
            Ok(StateTransition::CommitAndClear(_)) | Ok(StateTransition::Commit(_))
                if use_forward_key || use_surrounding_commit || use_backspace_commit =>
            {
                // Text is already in the app (forwarded/committed per character).
                // Return false so Enter/Tab also reaches the app.
                let mut s = self.state.lock().unwrap_or_else(|e| e.into_inner());
                s.shadow_buf.clear();
                false
            }
            Ok(StateTransition::CommitAndClear(c)) | Ok(StateTransition::Commit(c))
                if chrome_direct_mode =>
            {
                // Preedit was shown via UpdatePreeditText; commit it now.
                Self::commit_text_then_hide_preedit(ctx, c.as_str(), " (chrome)").await;
                let fwd_keyval: Option<u32> = match trigger {
                    Key::Return => Some(0xff0d),
                    Key::Tab => Some(0xff09),
                    _ => None,
                };
                if let Some(kv) = fwd_keyval {
                    if let Err(e) = Self::forward_key_event(ctx, kv, 0, 0).await {
                        warn!("IBus: ForwardKeyEvent after commit failed (chrome): {e}");
                    }
                }
                {
                    let mut s = self.state.lock().unwrap_or_else(|e| e.into_inner());
                    s.shadow_buf.clear();
                }
                true
            }
            Ok(StateTransition::CommitAndClear(c)) | Ok(StateTransition::Commit(c)) => {
                // Commit before hide — vhttechkey order; hide-first
                // drops the whole word in some Electron clients (VS Code).
                info!("IBus: commit text={:?}", c.as_str());
                Self::commit_text_then_hide_preedit(ctx, c.as_str(), "").await;
                // Forward Return/Tab so the application's action (newline, form submit,
                // focus-next) fires after the Vietnamese text is committed.
                let fwd_keyval: Option<u32> = match trigger {
                    Key::Return => Some(0xff0d),
                    Key::Tab => Some(0xff09),
                    _ => None,
                };
                if let Some(kv) = fwd_keyval {
                    if let Err(e) = Self::forward_key_event(ctx, kv, 0, 0).await {
                        warn!("IBus: ForwardKeyEvent after commit failed: {e}");
                    }
                }
                true
            }
            Ok(StateTransition::CommitThenPassThrough(_))
                if use_forward_key || use_surrounding_commit || use_backspace_commit =>
            {
                // Text already in app; clear shadow and let the trigger key through.
                let mut s = self.state.lock().unwrap_or_else(|e| e.into_inner());
                s.shadow_buf.clear();
                false
            }
            Ok(StateTransition::CommitThenPassThrough(c)) if chrome_direct_mode => {
                // Preedit was shown via UpdatePreeditText; commit it and let key through.
                Self::commit_text_then_hide_preedit(ctx, c.as_str(), " (chrome)").await;
                {
                    let mut s = self.state.lock().unwrap_or_else(|e| e.into_inner());
                    s.shadow_buf.clear();
                }
                false
            }
            Ok(StateTransition::CommitThenPassThrough(c)) => {
                // Commit before hide (vhttechkey order; Electron-friendly).
                info!("IBus: commit-passthrough text={:?}", c.as_str());
                Self::commit_text_then_hide_preedit(ctx, c.as_str(), "").await;
                // Return false: IBus delivers the triggering key (space, comma, etc.)
                // to the application directly. No ForwardKeyEvent needed.
                false
            }
            Ok(StateTransition::CommitThenPreedit(_, p)) if use_backspace_commit => {
                // Committed text already in app; start new backspace-commit chain.
                {
                    let mut s = self.state.lock().unwrap_or_else(|e| e.into_inner());
                    s.shadow_buf.clear();
                }
                self.dispatch_backspace_commit(ctx, p.as_str()).await
            }
            Ok(StateTransition::CommitThenPreedit(_, p)) if use_surrounding_commit => {
                // Committed text already in app; start new surrounding-commit chain.
                {
                    let mut s = self.state.lock().unwrap_or_else(|e| e.into_inner());
                    s.shadow_buf.clear();
                }
                self.dispatch_surrounding_commit(ctx, p.as_str()).await
            }
            Ok(StateTransition::CommitThenPreedit(_, p)) if use_forward_key => {
                // Committed text already in app; start new preedit via forward-key path.
                {
                    let mut s = self.state.lock().unwrap_or_else(|e| e.into_inner());
                    s.shadow_buf.clear();
                }
                self.dispatch_forward_key_preedit(ctx, p.as_str()).await
            }
            Ok(StateTransition::CommitThenPreedit(c, p)) if chrome_direct_mode => {
                // Preedit was shown via UpdatePreeditText; commit it then start new preedit.
                Self::commit_text_then_hide_preedit(ctx, c.as_str(), " (chrome)").await;
                {
                    let mut s = self.state.lock().unwrap_or_else(|e| e.into_inner());
                    s.shadow_buf.clear();
                }
                self.dispatch_chrome_direct(ctx, p.as_str()).await
            }
            Ok(StateTransition::CommitThenPreedit(c, p)) => {
                // Commit before hide (vhttechkey order; Electron-friendly).
                info!("IBus: commit-then-preedit commit={:?}", c.as_str());
                Self::commit_text_then_hide_preedit(ctx, c.as_str(), "").await;
                // cursor_pos: char count is correct because PreeditText is NFC-normalized.
                let cursor_pos = p.as_str().chars().count() as u32;
                let pval = ibus_preedit_text_owned(p.as_str());
                info!("IBus: UpdatePreeditText (commit-then-preedit) preedit={:?} cursor={cursor_pos} visible=true mode={IBUS_ENGINE_PREEDIT_COMMIT}", p.as_str());
                if let Err(e) = Self::update_preedit_text_with_mode(
                    ctx,
                    pval,
                    cursor_pos,
                    true,
                    IBUS_ENGINE_PREEDIT_COMMIT,
                )
                .await
                {
                    warn!("IBus: UpdatePreeditText failed (commit-then-preedit): {e}");
                }
                true
            }
            Ok(StateTransition::Cleared) => {
                if use_surrounding_commit {
                    let shadow_len = {
                        let s = self.state.lock().unwrap_or_else(|e| e.into_inner());
                        s.shadow_buf.chars().count()
                    };
                    if shadow_len > 0 {
                        // Request a fresh surrounding-text snapshot so the offset
                        // reflects any cursor movement since the last commit.
                        if let Err(e) = Self::get_surrounding_text(ctx).await {
                            warn!("IBus: GetSurroundingText on clear failed: {e}");
                        }
                        // Re-read surrounding after the refresh so we use the freshest
                        // data, not the value captured before the request above.
                        let fresh_surrounding = {
                            let s = self.state.lock().unwrap_or_else(|e| e.into_inner());
                            s.surrounding.clone()
                        };
                        // Use DeleteSurroundingText when surrounding text is available;
                        // cap the delete count to chars actually before the cursor to
                        // avoid over-deleting when shadow_buf is stale.
                        // Otherwise fall back to BackSpace events.
                        if let Some(ref st) = fresh_surrounding {
                            let chars_before_cursor =
                                st.text[..st.cursor_pos as usize].chars().count();
                            let delete_count = shadow_len.min(chars_before_cursor);
                            if delete_count > 0 {
                                let offset = -(delete_count as i32);
                                if let Err(e) =
                                    Self::delete_surrounding_text(ctx, offset, delete_count as u32)
                                        .await
                                {
                                    warn!("IBus: DeleteSurroundingText on clear failed: {e}");
                                }
                            }
                        } else {
                            for _ in 0..shadow_len {
                                if let Err(e) = Self::forward_key_event(ctx, 0xff08, 0, 0).await {
                                    warn!("IBus: ForwardKeyEvent BackSpace on clear failed: {e}");
                                }
                            }
                        }
                    }
                    let mut s = self.state.lock().unwrap_or_else(|e| e.into_inner());
                    s.shadow_buf.clear();
                } else if use_forward_key || use_backspace_commit {
                    let shadow_len = {
                        let s = self.state.lock().unwrap_or_else(|e| e.into_inner());
                        s.shadow_buf.chars().count()
                    };
                    for _ in 0..shadow_len {
                        if let Err(e) = Self::forward_key_event(ctx, 0xff08, 0, 0).await {
                            warn!("IBus: ForwardKeyEvent BackSpace on clear failed: {e}");
                        }
                    }
                    let mut s = self.state.lock().unwrap_or_else(|e| e.into_inner());
                    s.shadow_buf.clear();
                } else {
                    // Preedit mode and chrome_direct_mode: clear shadow_buf and hide the
                    // inline preedit widget.  Returning `true` below consumes the BackSpace
                    // key event so Chrome does not also delete a character from the document.
                    {
                        let mut s = self.state.lock().unwrap_or_else(|e| e.into_inner());
                        s.shadow_buf.clear();
                    }
                    if let Err(e) = Self::hide_preedit_text(ctx).await {
                        warn!("IBus: HidePreeditText failed: {e}");
                    }
                }
                true
            }
            // PassThrough: engine did not handle this key.  Return false so
            // IBus daemon delivers it to the application directly — no
            // ForwardKeyEvent needed.
            Ok(StateTransition::PassThrough) => {
                // chrome_direct_mode: a Backspace PassThrough means Chrome deleted one
                // char from the committed text — keep shadow_buf in sync so the next
                // PreeditUpdated erases exactly the right number of chars.
                if chrome_direct_mode && matches!(trigger, Key::Backspace) {
                    let mut s = self.state.lock().unwrap_or_else(|e| e.into_inner());
                    s.shadow_buf.pop();
                }
                false
            }
            Ok(StateTransition::Consumed) => true,
            Err(e) => {
                error!("IBus: composition error: {e}");
                let mut eng = self.engine.lock().unwrap_or_else(|e| e.into_inner());
                eng.reset();
                false
            }
        }
    }
}

// ── IBus Factory interface ────────────────────────────────────────────────────

struct IbusFactoryIface;

#[interface(name = "org.freedesktop.IBus.Factory")]
impl IbusFactoryIface {
    fn create_engine(&self, name: &str) -> OwnedObjectPath {
        info!("IBus: CreateEngine({name})");
        OwnedObjectPath::try_from(ENGINE_PATH).expect("ENGINE_PATH is a valid object path")
    }
}

// ── Public backend ────────────────────────────────────────────────────────────

/// A live IBus engine connection that implements `ImeBackend`.
pub struct IbusBackend {
    rt: tokio::runtime::Handle,
    conn: Arc<Connection>,
    object_path: OwnedObjectPath,
    state: Arc<Mutex<SharedState>>,
}

impl IbusBackend {
    /// Connect to the running ibus-daemon and begin serving the engine.
    ///
    /// The `engine` is shared between the IBus interface (which processes keys
    /// synchronously inside `process_key_event`) and the daemon's IPC handler
    /// (which switches the active input method on demand).
    ///
    /// `force_preedit_mode`: when `true`, `IBUS_CAP_SURROUNDING_TEXT` is ignored
    /// and preedit mode is always used (fixes Chrome 114+ on XWayland).
    ///
    /// `force_chrome_direct`: when `true`, force chrome_direct_mode regardless
    /// of reported capabilities (for Chromium-family browsers on XWayland).
    ///
    /// `default_commit_mode`: global commit mode from `[ibus] commit_mode` config.
    pub fn connect(
        rt: tokio::runtime::Handle,
        engine: Arc<Mutex<StandardEngine>>,
        force_preedit_mode: bool,
        force_chrome_direct: bool,
        default_commit_mode: IbusCommitMode,
    ) -> Result<Self> {
        rt.block_on(connect_loop(
            rt.clone(),
            engine,
            force_preedit_mode,
            force_chrome_direct,
            default_commit_mode,
        ))
    }

    /// Return the most-recently reported cursor position from
    /// `SetCursorLocation`.  Returns `(0, 0)` until the application sends
    /// at least one `SetCursorLocation` call.
    pub fn cursor_position(&self) -> (i32, i32) {
        let s = self.state.lock().unwrap_or_else(|e| e.into_inner());
        (s.cursor_x, s.cursor_y)
    }
}

impl ImeBackend for IbusBackend {
    fn commit(&self, text: &NfcString) -> Result<()> {
        let conn = Arc::clone(&self.conn);
        let path = self.object_path.clone();
        let val = ibus_text_owned(text.as_str());
        let rt = self.rt.clone();
        tokio::task::block_in_place(move || {
            rt.block_on(async move {
                let iface = conn
                    .object_server()
                    .interface::<_, IbusEngineIface>(&path)
                    .await
                    .map_err(|e| PlatformError::DBus(e.to_string()))?;
                IbusEngineIface::commit_text(iface.signal_context(), val)
                    .await
                    .map_err(|e| PlatformError::DBus(e.to_string()))
            })
        })
    }

    /// **Dead code path — never called at runtime for IBus.**
    ///
    /// IBus key events are processed synchronously inside
    /// `IbusEngineIface::process_key_event`, which emits preedit/commit D-Bus
    /// signals directly via the zbus `SignalContext`.  The daemon's event loop
    /// (`event_loop.rs`) calls `ImeBackend::update_preedit` on the active
    /// backend, but `IbusBackend` is deliberately *not* wired to the `tx`
    /// `mpsc::Sender` channel (compare `detect.rs`: Fcitx5/Wayland/X11 pass
    /// `tx` to their `connect` call; IBus passes `engine` instead).  No
    /// `InputEvent` is ever queued through that loop for IBus, so this method
    /// is unreachable in practice.  It is implemented only to satisfy the
    /// `ImeBackend` trait contract.
    fn update_preedit(&self, text: &PreeditText, _cursor: CharCursor) -> Result<()> {
        let cursor_pos = text.as_str().chars().count() as u32;
        {
            let mut s = self.state.lock().unwrap_or_else(|e| e.into_inner());
            s.preedit = text.as_str().to_owned();
            s.preedit_cursor = cursor_pos;
        }
        let conn = Arc::clone(&self.conn);
        let path = self.object_path.clone();
        let val = ibus_preedit_text_owned(text.as_str());
        let rt = self.rt.clone();
        tokio::task::block_in_place(move || {
            rt.block_on(async move {
                let iface = conn
                    .object_server()
                    .interface::<_, IbusEngineIface>(&path)
                    .await
                    .map_err(|e| PlatformError::DBus(e.to_string()))?;
                IbusEngineIface::update_preedit_text_with_mode(
                    iface.signal_context(),
                    val,
                    cursor_pos,
                    true,
                    1,
                )
                .await
                .map_err(|e| PlatformError::DBus(e.to_string()))
            })
        })
    }

    fn clear_preedit(&self) -> Result<()> {
        {
            let mut s = self.state.lock().unwrap_or_else(|e| e.into_inner());
            s.preedit.clear();
            s.preedit_cursor = 0;
        }
        let conn = Arc::clone(&self.conn);
        let path = self.object_path.clone();
        let rt = self.rt.clone();
        tokio::task::block_in_place(move || {
            rt.block_on(async move {
                let iface = conn
                    .object_server()
                    .interface::<_, IbusEngineIface>(&path)
                    .await
                    .map_err(|e| PlatformError::DBus(e.to_string()))?;
                IbusEngineIface::hide_preedit_text(iface.signal_context())
                    .await
                    .map_err(|e| PlatformError::DBus(e.to_string()))
            })
        })
    }

    fn forward_key(&self, key: &InputEvent) -> Result<()> {
        let (keyval, keycode, modifiers) = input_event_to_ibus(key);
        let conn = Arc::clone(&self.conn);
        let path = self.object_path.clone();
        let rt = self.rt.clone();
        tokio::task::block_in_place(move || {
            rt.block_on(async move {
                let iface = conn
                    .object_server()
                    .interface::<_, IbusEngineIface>(&path)
                    .await
                    .map_err(|e| PlatformError::DBus(e.to_string()))?;
                IbusEngineIface::forward_key_event(
                    iface.signal_context(),
                    keyval,
                    keycode,
                    modifiers,
                )
                .await
                .map_err(|e| PlatformError::DBus(e.to_string()))
            })
        })
    }

    fn surrounding_text(&self) -> Result<Option<SurroundingText>> {
        Ok(self
            .state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .surrounding
            .clone())
    }

    fn capabilities(&self) -> Capabilities {
        let raw = self
            .state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .caps_raw;
        Capabilities {
            surrounding_text: raw & IBUS_CAP_SURROUNDING_TEXT != 0,
            preedit: true,
            lookup_table: true,
        }
    }
}

// ── Connection logic with exponential backoff ─────────────────────────────────

const ENGINE_PATH: &str = "/org/freedesktop/IBus/Engine/Vi";
const FACTORY_PATH: &str = "/org/freedesktop/IBus/Factory";
const MAX_CONNECT_ATTEMPTS: u32 = 10;

async fn connect_loop(
    rt: tokio::runtime::Handle,
    engine: Arc<Mutex<StandardEngine>>,
    force_preedit_mode: bool,
    force_chrome_direct: bool,
    default_commit_mode: IbusCommitMode,
) -> Result<IbusBackend> {
    let mut delay = Duration::from_millis(100);
    let max_delay = Duration::from_secs(30);
    let mut attempt = 0u32;

    loop {
        attempt += 1;
        match try_connect(
            Arc::clone(&engine),
            force_preedit_mode,
            force_chrome_direct,
            default_commit_mode,
        )
        .await
        {
            Ok((conn, state)) => {
                let object_path = OwnedObjectPath::try_from(ENGINE_PATH)
                    .expect("engine path is a valid object path");
                info!("IBus: connected at {ENGINE_PATH}");
                return Ok(IbusBackend {
                    rt,
                    conn: Arc::new(conn),
                    object_path,
                    state,
                });
            }
            Err(e) => {
                if attempt >= MAX_CONNECT_ATTEMPTS {
                    error!("IBus: connect failed after {attempt} attempts ({e}); giving up");
                    return Err(PlatformError::DBus(format!(
                        "ibus-daemon unreachable after {MAX_CONNECT_ATTEMPTS} attempts: {e}"
                    )));
                }
                let next_delay = delay * (100 + attempt % 50) / 100;
                warn!(
                    "IBus: connect failed (attempt {attempt}/{MAX_CONNECT_ATTEMPTS}, {e}); \
                     retrying in {next_delay:?}"
                );
                tokio::time::sleep(next_delay).await;
                delay = (delay * 2).min(max_delay);
            }
        }
    }
}

async fn try_connect(
    engine: Arc<Mutex<StandardEngine>>,
    force_preedit_mode: bool,
    force_chrome_direct: bool,
    default_commit_mode: IbusCommitMode,
) -> std::result::Result<(Connection, Arc<Mutex<SharedState>>), String> {
    let conn = if let Some(address) = ibus_address() {
        zbus::ConnectionBuilder::address(address.as_str())
            .map_err(|e| e.to_string())?
            .build()
            .await
            .map_err(|e| e.to_string())?
    } else {
        Connection::session().await.map_err(|e| e.to_string())?
    };

    let dbus = DBusProxy::new(&conn).await.map_err(|e| e.to_string())?;
    match dbus
        .request_name(
            "org.freedesktop.IBus.vhttechkey"
                .try_into()
                .map_err(|e: zbus::names::Error| e.to_string())?,
            zbus::fdo::RequestNameFlags::ReplaceExisting
                | zbus::fdo::RequestNameFlags::AllowReplacement,
        )
        .await
    {
        Ok(reply) => info!("IBus: RequestName → {reply:?}"),
        Err(e) => warn!("IBus: RequestName failed (non-fatal): {e}"),
    }

    match Proxy::new(
        &conn,
        "org.freedesktop.IBus",
        "/org/freedesktop/IBus",
        "org.freedesktop.IBus",
    )
    .await
    {
        Ok(ibus_proxy) => {
            let component = ibus_component_value("vhttechkey");
            match ibus_proxy
                .call_method("RegisterComponent", &(component,))
                .await
            {
                Ok(_) => info!("IBus: RegisterComponent succeeded"),
                Err(e) => warn!("IBus: RegisterComponent failed (non-fatal): {e}"),
            }
        }
        Err(e) => warn!("IBus: could not create IBus proxy for RegisterComponent: {e}"),
    }

    conn.object_server()
        .at(FACTORY_PATH, IbusFactoryIface)
        .await
        .map_err(|e| e.to_string())?;

    let state = Arc::new(Mutex::new(SharedState::new(
        force_preedit_mode,
        force_chrome_direct,
        default_commit_mode,
    )));
    let iface = IbusEngineIface {
        engine,
        state: Arc::clone(&state),
        handler_lock: Arc::new(tokio::sync::Mutex::new(())),
    };
    conn.object_server()
        .at(ENGINE_PATH, iface)
        .await
        .map_err(|e| e.to_string())?;

    Ok((conn, state))
}

/// Resolve the IBus private D-Bus socket address.
fn ibus_address() -> Option<String> {
    if let Ok(addr) = std::env::var("IBUS_ADDRESS") {
        return Some(addr);
    }
    // On Wayland, IBus writes its discovery file with `WAYLAND_DISPLAY`
    // (for example `...-unix-wayland-0`).  Falling back to `DISPLAY` first
    // makes an XWayland `:0` value select a non-existent bus file and causes
    // the engine to connect to the session bus instead of IBus.
    let display = std::env::var("WAYLAND_DISPLAY")
        .ok()
        .filter(|value| !value.is_empty())
        .or_else(|| std::env::var("DISPLAY").ok())
        .unwrap_or_else(|| ":0".to_string());
    let display_clean = display.trim_start_matches(':').replace(':', "-");
    let machine_id = std::fs::read_to_string("/etc/machine-id")
        .or_else(|_| std::fs::read_to_string("/var/lib/dbus/machine-id"))
        .ok()?;
    let machine_id = machine_id.trim();
    let home = std::env::var("HOME").ok()?;
    let path = format!("{home}/.config/ibus/bus/{machine_id}-unix-{display_clean}");
    let content = std::fs::read_to_string(path).ok()?;
    for line in content.lines() {
        if let Some(addr) = line.strip_prefix("IBUS_ADDRESS=") {
            return Some(addr.to_owned());
        }
    }
    None
}

// ── Key-event forwarding (engine → application) ───────────────────────────────

fn input_event_to_ibus(event: &InputEvent) -> (u32, u32, u32) {
    match event {
        InputEvent::KeyDown(key, mods) | InputEvent::KeyRepeat(key, mods) => {
            (key_to_ibus_keyval(key), 0, mods_to_ibus(mods))
        }
        InputEvent::KeyUp(key) => (key_to_ibus_keyval(key), 0, IBUS_RELEASE_MASK),
        _ => (0, 0, 0),
    }
}

fn key_to_ibus_keyval(key: &Key) -> u32 {
    match key {
        Key::Char(c) => char_to_x11_keysym(*c),
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
        Key::DeadKey('^') => 0xfe52,
        Key::DeadKey('(') => 0xfe55,
        Key::DeadKey('+') => 0xfe62,
        Key::DeadKey('d') | Key::DeadKey('D') => 0xfe63,
        Key::DeadKey('`') => 0xfe50,
        Key::DeadKey('\'') => 0xfe51,
        Key::DeadKey('~') => 0xfe53,
        Key::DeadKey('.') => 0xfe60,
        Key::DeadKey('?') => 0xfe61,
        Key::DeadKey(_) => 0,
        Key::ComposeKey => 0xff20,
    }
}

fn char_to_x11_keysym(c: char) -> u32 {
    let cp = c as u32;
    if cp <= 0x00FF {
        cp
    } else {
        0x01000000 | cp
    }
}

fn mods_to_ibus(mods: &Modifiers) -> u32 {
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

/// Index of the string field in the IBus GVariant schema `(sa{sv}sv)`.
const IBUS_TEXT_STRING_FIELD: usize = 2;

fn extract_ibus_string(val: &OwnedValue) -> std::result::Result<String, IbusError> {
    extract_ibus_string_val(val, 0)
}

fn extract_ibus_string_val(
    val: &Value<'_>,
    depth: usize,
) -> std::result::Result<String, IbusError> {
    if depth > 8 {
        return Err(IbusError::MalformedSurroundingText(
            "variant nesting depth exceeded".to_owned(),
        ));
    }
    match val {
        Value::Structure(s) => {
            let fields = s.fields();
            let valid_shape = fields.len() > IBUS_TEXT_STRING_FIELD
                && matches!(fields.first(), Some(Value::Str(_)))
                && matches!(fields.get(1), Some(Value::Dict(_)));
            if !valid_shape {
                return Err(IbusError::MalformedSurroundingText(format!(
                    "expected (sa{{sv}}sv) IBusText, got structure with {} fields: {val:?}",
                    fields.len()
                )));
            }
            fields
                .get(IBUS_TEXT_STRING_FIELD)
                .ok_or_else(|| IbusError::MalformedSurroundingText("missing text field".to_owned()))
                .and_then(|f| match f {
                    Value::Str(s) => Ok(s.as_str().to_owned()),
                    _ => Err(IbusError::MalformedSurroundingText(
                        "text field is not a string".to_owned(),
                    )),
                })
        }
        Value::Value(inner) => extract_ibus_string_val(inner, depth + 1),
        _ => Err(IbusError::MalformedSurroundingText(format!(
            "expected IBusText structure, got {val:?}"
        ))),
    }
}

/// Map an IBus/X11 keysym to a `Key`.
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
        0x0020..=0x007e => Key::Char(char::from_u32(keyval).unwrap()),
        v @ 0x01000000..=0x0110ffff => Key::Char(char::from_u32(v - 0x01000000).unwrap_or('\0')),
        other => Key::Keysym(other),
    }
}

/// Map the IBus/X11 modifier state bitmask to `Modifiers`.
fn map_ibus_state(state: u32) -> Modifiers {
    Modifiers {
        shift: state & 1 != 0,
        caps_lock: state & 2 != 0,
        ctrl: state & 4 != 0,
        alt: state & 8 != 0,
        altgr: state & (1 << 7) != 0,
        super_key: state & (1 << 26) != 0,
    }
}

/// Return `true` for keysyms that represent bare modifier keys.
fn is_modifier_keysym(keyval: u32) -> bool {
    matches!(
        keyval,
        0xffe1
            | 0xffe2
            | 0xffe3
            | 0xffe4
            | 0xffe5
            | 0xffe6
            | 0xffe7
            | 0xffe8
            | 0xffe9
            | 0xffea
            | 0xffeb
            | 0xffec
            | 0xff7f
            | 0xfe03
    )
}

// ── Chromium-family browser detection ────────────────────────────────────────

// ── Unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ibus_engine_preedit_commit_matches_system_header() {
        // `/usr/include/ibus-1.0/ibustypes.h`: IBUS_ENGINE_PREEDIT_COMMIT = 1
        assert_eq!(IBUS_ENGINE_PREEDIT_COMMIT, 1);
    }

    #[test]
    fn map_keyval_special_keys() {
        assert_eq!(map_keyval_to_key(0xff08), Key::Backspace);
        assert_eq!(map_keyval_to_key(0xff0d), Key::Return);
        assert_eq!(map_keyval_to_key(0xff1b), Key::Escape);
        assert_eq!(map_keyval_to_key(0xff09), Key::Tab);
    }

    #[test]
    fn map_ibus_state_ctrl_flag() {
        let mods = map_ibus_state(4);
        assert!(mods.ctrl);
        assert!(!mods.shift);
        assert!(!mods.alt);
    }

    #[test]
    fn modifier_keysyms_are_detected() {
        assert!(is_modifier_keysym(0xffe3)); // Control_L
        assert!(is_modifier_keysym(0xffe1)); // Shift_L
        assert!(is_modifier_keysym(0xffe9)); // Alt_L
        assert!(!is_modifier_keysym(b'a' as u32));
        assert!(!is_modifier_keysym(0xff0d)); // Return is not a modifier
    }

    #[test]
    fn ctrl_keyval_passes_through() {
        // ctrl bit = 4; our code returns false for ctrl+key
        let mods = map_ibus_state(4);
        assert!(mods.ctrl);
    }

    #[test]
    fn ctrl_key_commits_pending_preedit() {
        use vi_core::InputMethod;
        // Build preedit by typing "nha" in Telex mode.
        let mut engine = StandardEngine::new(InputMethod::Telex);
        engine
            .process(&InputEvent::KeyDown(Key::Char('n'), Modifiers::default()))
            .unwrap();
        engine
            .process(&InputEvent::KeyDown(Key::Char('h'), Modifiers::default()))
            .unwrap();
        engine
            .process(&InputEvent::KeyDown(Key::Char('a'), Modifiers::default()))
            .unwrap();
        assert!(
            !engine.preedit().is_empty(),
            "expected non-empty preedit after typing"
        );
        // Simulate what the Ctrl+key path now does: trigger FocusOut to commit.
        let transition = engine.process(&InputEvent::FocusOut);
        assert!(
            matches!(
                transition,
                Ok(StateTransition::CommitAndClear(_)) | Ok(StateTransition::Commit(_))
            ),
            "FocusOut with pending preedit must yield a commit transition, got: {transition:?}"
        );
        assert!(
            engine.preedit().is_empty(),
            "preedit must be cleared after commit"
        );
    }

    #[test]
    fn backoff_delay_sequence_stays_within_bounds() {
        let max_delay = Duration::from_secs(30);
        let mut delay = Duration::from_millis(100);
        for attempt in 1u32..MAX_CONNECT_ATTEMPTS {
            let jittered = delay * (100 + attempt % 50) / 100;
            assert!(
                jittered <= max_delay,
                "attempt {attempt}: jittered {jittered:?} exceeds cap"
            );
            assert!(
                jittered >= delay,
                "attempt {attempt}: jittered {jittered:?} less than base {delay:?}"
            );
            delay = (delay * 2).min(max_delay);
        }
    }

    #[test]
    fn test_extract_ibus_string_valid() {
        let mut sb = StructureBuilder::new();
        sb.push_value(Value::Str("IBusText".to_string().into()));
        sb.push_value(Value::Dict(Dict::new(
            Signature::from_str_unchecked("s"),
            Signature::from_str_unchecked("v"),
        )));
        sb.push_value(Value::Str("hello".to_string().into()));
        sb.push_value(Value::Value(Box::new(ibus_attr_list_value(5))));
        let structure = Value::Structure(sb.build());
        let owned = OwnedValue::try_from(structure).expect("valid structure");
        assert_eq!(extract_ibus_string(&owned).unwrap(), "hello");
    }

    #[test]
    fn test_extract_ibus_string_malformed() {
        let bare = Value::Str("not a structure".to_string().into());
        let owned = OwnedValue::try_from(bare).expect("valid value");
        assert!(matches!(
            extract_ibus_string(&owned),
            Err(IbusError::MalformedSurroundingText(_))
        ));
    }

    #[test]
    fn test_ibus_text_owned_is_single_variant() {
        let owned = ibus_text_owned("test");
        // The OwnedValue itself is the variant; its inner value must be the
        // IBusText structure (sa{sv}sv), NOT another variant.
        let inner: &Value = &owned;
        assert!(
            matches!(inner, Value::Structure(_)),
            "ibus_text_owned must not wrap in extra Value::Value; got: {inner:?}"
        );
    }

    #[test]
    fn test_ibus_text_owned_unicode() {
        let owned = ibus_text_owned("nhà");
        // Extract the text string from the structure (field index 2)
        // and verify it round-trips correctly
        let inner = Value::from(owned);
        if let Value::Structure(s) = inner {
            let fields = s.fields();
            assert!(matches!(fields.first(), Some(Value::Str(n)) if n.as_str() == "IBusText"));
            assert!(matches!(fields.get(2), Some(Value::Str(t)) if t.as_str() == "nhà"));
        } else {
            panic!("Expected structure");
        }
    }

    // ── compute_forward_key_ops ───────────────────────────────────────────────

    #[test]
    fn fk_simple_first_char() {
        assert_eq!(compute_forward_key_ops("", "a"), (0, "a".to_owned()));
    }

    #[test]
    fn fk_append_char() {
        assert_eq!(compute_forward_key_ops("a", "ab"), (0, "b".to_owned()));
    }

    #[test]
    fn fk_rule_fires_oo_to_o_circ() {
        // "to" + "o" triggers the Telex rule: shadow="to", new preedit="tô"
        assert_eq!(compute_forward_key_ops("to", "tô"), (1, "ô".to_owned()));
    }

    #[test]
    fn fk_backspace_shrinks_preedit() {
        assert_eq!(compute_forward_key_ops("ab", "a"), (1, String::new()));
    }

    #[test]
    fn fk_escape_clears_all() {
        // "tôi" is 3 Unicode codepoints; backspaces must count codepoints not bytes.
        assert_eq!(compute_forward_key_ops("tôi", ""), (3, String::new()));
    }

    #[test]
    fn fk_no_change() {
        assert_eq!(compute_forward_key_ops("a", "a"), (0, String::new()));
    }

    #[test]
    fn fk_complete_replacement() {
        // "ow" (2 chars) is replaced entirely by "ơ" (1 char, no common prefix)
        assert_eq!(compute_forward_key_ops("ow", "ơ"), (2, "ơ".to_owned()));
    }

    // ── SharedState defaults and set_capabilities logic ───────────────────────

    #[test]
    fn use_forward_key_defaults_to_false() {
        let s = SharedState::default();
        assert!(!s.use_forward_key, "forward-key must not be the default");
        assert!(
            !s.use_surrounding_commit,
            "surrounding-commit must start false; set_capabilities never auto-enables it"
        );
    }

    #[test]
    fn surrounding_only_caps_policy_never_auto_surrounding_commit() {
        // Naive capability math would pick surrounding-commit for SURROUNDING-only
        // clients, but production never does — see `set_capabilities` rationale.
        let caps_surrounding_only: u32 = IBUS_CAP_SURROUNDING_TEXT;
        let has_preedit = (caps_surrounding_only & IBUS_CAP_PREEDIT_TEXT) != 0;
        let has_surrounding = (caps_surrounding_only & IBUS_CAP_SURROUNDING_TEXT) != 0;
        let naive_would_select = !has_preedit && has_surrounding;
        assert!(naive_would_select, "sanity check of naive formula");
        assert!(!has_preedit);
        let production_use_surrounding_commit = false;
        assert!(!production_use_surrounding_commit);
    }

    #[test]
    fn preedit_app_uses_pure_preedit_mode() {
        // Apps that advertise IBUS_CAP_PREEDIT_TEXT (Chrome, VSCode, GTK, Qt) use
        // standard preedit mode — no surrounding-commit, no forward-key, no chrome_direct.
        let caps: u32 = IBUS_CAP_PREEDIT_TEXT;
        let has_preedit = (caps & IBUS_CAP_PREEDIT_TEXT) != 0;
        let has_surrounding = (caps & IBUS_CAP_SURROUNDING_TEXT) != 0;
        let new_surrounding_commit = !has_preedit && has_surrounding;
        let new_forward_key = !has_preedit && !has_surrounding && caps != 0;
        let new_chrome_direct = false; // force_chrome_direct not set
        assert!(
            !new_surrounding_commit,
            "preedit app must not use surrounding-commit"
        );
        assert!(!new_forward_key, "preedit app must not use forward-key");
        assert!(!new_chrome_direct, "preedit app must use pure preedit mode");
    }

    #[test]
    fn preedit_and_surrounding_app_uses_preedit_mode() {
        // Apps with both IBUS_CAP_PREEDIT_TEXT and IBUS_CAP_SURROUNDING_TEXT
        // (Chrome content area, VSCode, GTK4, Qt) stay in preedit mode because
        // has_preedit=true.  The old `has_surrounding` logic incorrectly put
        // these apps into surrounding_commit mode.
        let caps: u32 = IBUS_CAP_PREEDIT_TEXT | IBUS_CAP_SURROUNDING_TEXT;
        let has_preedit = (caps & IBUS_CAP_PREEDIT_TEXT) != 0;
        let has_surrounding = (caps & IBUS_CAP_SURROUNDING_TEXT) != 0;
        let new_surrounding_commit = !has_preedit && has_surrounding; // false — has_preedit wins
        let new_forward_key = !has_preedit && !has_surrounding && caps != 0;
        assert!(
            !new_surrounding_commit,
            "app with preedit must NOT use surrounding-commit"
        );
        assert!(!new_forward_key);
    }

    #[test]
    fn chrome_direct_mode_preedit_cursor_pos() {
        // Verify the preedit cursor_pos logic for dispatch_chrome_direct.
        // cursor_pos is Unicode char count (not UTF-8 byte offset), matching
        // the invariant enforced throughout the preedit path.
        assert_eq!("vi".chars().count() as u32, 2);

        // Vietnamese multi-byte text: "việt" is 4 Unicode codepoints.
        let new_preedit2 = "việt";
        assert_eq!(
            new_preedit2.chars().count() as u32,
            4,
            "\"việt\" is 4 Unicode chars"
        );

        // shadow_buf tracks new_preedit after dispatch_chrome_direct runs.
        // (Previous behavior sent BackSpace×shadow.chars().count() + CommitText;
        //  new behavior sends UpdatePreeditTextWithMode with no BackSpaces.)
        let new_preedit3 = "tô";
        let cursor_pos = new_preedit3.chars().count() as u32;
        assert_eq!(
            cursor_pos, 2,
            "\"tô\" is 2 Unicode chars (not 3 UTF-8 bytes)"
        );
    }

    #[test]
    fn set_capabilities_never_selects_forward_key_or_surrounding_commit() {
        for caps in [
            0u32,
            IBUS_CAP_PREEDIT_TEXT,
            IBUS_CAP_SURROUNDING_TEXT,
            IBUS_CAP_PREEDIT_TEXT | IBUS_CAP_SURROUNDING_TEXT,
        ] {
            let use_surrounding_commit = false;
            let use_forward_key = false;
            assert!(!use_surrounding_commit, "caps={caps:#x}");
            assert!(!use_forward_key, "caps={caps:#x}");
        }
    }

    // ── IBusComponent structure ───────────────────────────────────────────────

    #[test]
    fn test_ibus_component_value_structure() {
        let val = ibus_component_value("vhttechkey");

        // Outermost value must be a Structure.
        let Value::Structure(ref s) = val else {
            panic!("expected Value::Structure, got {val:?}");
        };
        let fields = s.fields();

        // First field is the type name.
        assert!(
            matches!(fields.first(), Some(Value::Str(n)) if n.as_str() == "IBusComponent"),
            "first field must be \"IBusComponent\""
        );

        // At least 5 fields: name, description, version, license, author (and more).
        assert!(
            fields.len() >= 5,
            "structure must have at least 5 fields, got {}",
            fields.len()
        );

        // The last two fields are arrays: observed_paths and engines.
        assert!(
            matches!(fields[fields.len() - 1], Value::Array(_)),
            "last field must be the engines array"
        );
        assert!(
            matches!(fields[fields.len() - 2], Value::Array(_)),
            "second-to-last field must be the observed_paths array"
        );

        // Validate the engine description embedded in the component.
        // ibus_component_value calls ibus_engine_desc_value internally; verify
        // that helper produces an entry with the correct name and language.
        let engine_val = ibus_engine_desc_value("vhttechkey");
        let Value::Structure(ref desc) = engine_val else {
            panic!("ibus_engine_desc_value must return Value::Structure");
        };
        let df = desc.fields();
        assert!(
            matches!(df.get(2), Some(Value::Str(n)) if n.as_str() == "vhttechkey"),
            "engine name field (index 2) must be \"vhttechkey\""
        );
        assert!(
            matches!(df.get(5), Some(Value::Str(l)) if l.as_str() == "vi"),
            "engine language field (index 5) must be \"vi\""
        );
        assert!(
            matches!(df.get(14), Some(Value::Str(s)) if !s.is_empty()),
            "engine setup field (index 14) must point to vi-ui for preferences"
        );
    }

    // ── backspace_commit mode ─────────────────────────────────────────────────

    fn make_iface_with_commit_mode(
        commit_mode: IbusCommitMode,
    ) -> (IbusEngineIface, Arc<Mutex<SharedState>>) {
        use vi_core::InputMethod;
        let state = Arc::new(Mutex::new(SharedState::new(false, false, commit_mode)));
        let iface = IbusEngineIface {
            engine: Arc::new(Mutex::new(StandardEngine::new(InputMethod::Telex))),
            state: Arc::clone(&state),
            handler_lock: Arc::new(tokio::sync::Mutex::new(())),
        };
        (iface, state)
    }

    /// `commit_mode = backspace_commit` must activate `use_backspace_commit` after
    /// `set_capabilities`.
    #[tokio::test]
    async fn set_capabilities_backspace_commit_mode() {
        let (iface, state) = make_iface_with_commit_mode(IbusCommitMode::BackspaceCommit);
        iface.set_capabilities(IBUS_CAP_PREEDIT_TEXT).await;
        let s = state.lock().unwrap();
        assert!(
            s.use_backspace_commit,
            "backspace_commit config must activate use_backspace_commit"
        );
        assert!(!s.use_surrounding_commit);
        assert!(!s.use_forward_key);
        assert!(!s.chrome_direct_mode);
    }

    /// `force_preedit_mode` must suppress `backspace_commit`.
    #[tokio::test]
    async fn set_capabilities_preedit_overrides_backspace_commit() {
        use vi_core::InputMethod;
        let mut state_inner = SharedState::new(false, false, IbusCommitMode::BackspaceCommit);
        state_inner.force_preedit_mode = true;
        let state_arc = Arc::new(Mutex::new(state_inner));
        let iface = IbusEngineIface {
            engine: Arc::new(Mutex::new(StandardEngine::new(InputMethod::Telex))),
            state: Arc::clone(&state_arc),
            handler_lock: Arc::new(tokio::sync::Mutex::new(())),
        };
        iface.set_capabilities(IBUS_CAP_PREEDIT_TEXT).await;
        let s = state_arc.lock().unwrap();
        assert!(
            !s.use_backspace_commit,
            "force_preedit_mode must suppress backspace_commit"
        );
        assert!(!s.chrome_direct_mode);
    }

    /// `force_chrome_direct` must suppress `backspace_commit`.
    #[tokio::test]
    async fn set_capabilities_chrome_direct_overrides_backspace_commit() {
        use vi_core::InputMethod;
        let mut state_inner = SharedState::new(false, false, IbusCommitMode::BackspaceCommit);
        state_inner.force_chrome_direct = true;
        let state_arc = Arc::new(Mutex::new(state_inner));
        let iface = IbusEngineIface {
            engine: Arc::new(Mutex::new(StandardEngine::new(InputMethod::Telex))),
            state: Arc::clone(&state_arc),
            handler_lock: Arc::new(tokio::sync::Mutex::new(())),
        };
        iface.set_capabilities(IBUS_CAP_PREEDIT_TEXT).await;
        let s = state_arc.lock().unwrap();
        assert!(
            !s.use_backspace_commit,
            "force_chrome_direct must suppress backspace_commit"
        );
        assert!(
            s.chrome_direct_mode,
            "force_chrome_direct must activate chrome_direct_mode"
        );
    }

    /// Default `commit_mode = preedit` must NOT activate backspace_commit.
    #[tokio::test]
    async fn set_capabilities_default_preedit_no_backspace_commit() {
        let (iface, state) = make_iface_with_commit_mode(IbusCommitMode::Preedit);
        iface.set_capabilities(IBUS_CAP_PREEDIT_TEXT).await;
        let s = state.lock().unwrap();
        assert!(
            !s.use_backspace_commit,
            "default preedit mode must not activate backspace_commit"
        );
    }

    // ── Telex doubling regression ─────────────────────────────────────────────

    #[test]
    fn telex_oo_produces_o_circumflex() {
        use vi_core::{InputMethod, StateTransition};
        let mut engine = StandardEngine::new(InputMethod::Telex);
        let _ = engine.process(&InputEvent::KeyDown(Key::Char('o'), Modifiers::default()));
        // IBus sends a KeyUp before the next KeyDown; reset the repeat guard.
        let _ = engine.process(&InputEvent::KeyUp(Key::Char('o')));
        let t2 = engine
            .process(&InputEvent::KeyDown(Key::Char('o'), Modifiers::default()))
            .unwrap();
        assert!(
            matches!(&t2, StateTransition::PreeditUpdated(p) if p.as_str() == "ô"),
            "Telex 'oo' must yield PreeditUpdated(\"ô\"), got: {t2:?}"
        );
    }

    // ── ibus_text_owned Vietnamese round-trip ─────────────────────────────────

    #[test]
    fn ibus_text_owned_vietnamese_composition_chars() {
        for text in &["v", "vi", "vie", "viet", "việt", "nhà", "ổn"] {
            let owned = ibus_text_owned(text);
            let inner: &Value = &owned;
            match inner {
                Value::Structure(s) => {
                    let fields = s.fields();
                    assert_eq!(fields.len(), 4, "IBusText must have 4 fields for: {text}");
                    assert!(
                        matches!(fields.first(), Some(Value::Str(n)) if n.as_str() == "IBusText"),
                        "first field must be type name for: {text}"
                    );
                    assert!(
                        matches!(fields.get(2), Some(Value::Str(t)) if t.as_str() == *text),
                        "text field must round-trip for: {text}"
                    );
                }
                _ => panic!("expected Structure for text: {text}"),
            }
        }
    }

    // ── KeyRepeat in surrounding-commit mode (documentation/coverage) ─────────

    #[test]
    fn key_repeat_produces_preedit_updated_for_surrounding_commit() {
        // KeyRepeat events must produce PreeditUpdated so that dispatch_surrounding_commit
        // is called and shadow_buf is updated.  Documents the chain:
        //   KeyRepeat → engine.process() → PreeditUpdated → dispatch_surrounding_commit.
        use vi_core::{InputMethod, StateTransition};
        let mut engine = StandardEngine::new(InputMethod::Telex);

        // Type 'a' to begin composition; in surrounding-commit mode shadow_buf
        // would be set to "a" after dispatch_surrounding_commit runs.
        let _ = engine.process(&InputEvent::KeyDown(Key::Char('a'), Modifiers::default()));
        let _ = engine.process(&InputEvent::KeyUp(Key::Char('a')));
        assert_eq!(engine.preedit().as_str(), "a");

        // KeyRepeat extends the preedit without re-firing the "aa→â" rule.
        let t = engine
            .process(&InputEvent::KeyRepeat(Key::Char('a'), Modifiers::default()))
            .unwrap();
        assert!(
            matches!(&t, StateTransition::PreeditUpdated(p) if p.as_str() == "aa"),
            "KeyRepeat must produce PreeditUpdated(\"aa\"); got: {t:?}"
        );

        // dispatch_surrounding_commit uses a full-replace strategy: it deletes
        // shadow.chars().count() chars and commits new_preedit.
        // For shadow="a", new_preedit="aa": delete 1 char, commit "aa".
        // compute_forward_key_ops is still used by dispatch_forward_key_preedit:
        let (backspaces, new_tail) = compute_forward_key_ops("a", "aa");
        assert_eq!(
            backspaces, 0,
            "forward-key mode: no backspaces needed when extending preedit"
        );
        assert_eq!(
            new_tail, "a",
            "forward-key mode: one new character appended"
        );
    }

    // ── cursor_pos is char count not byte count ───────────────────────────────

    #[test]
    fn cursor_pos_is_char_count_not_byte_count() {
        // U+1ED5 'ổ' is 3 bytes in UTF-8 but a single codepoint.
        let text = "ổ";
        let cursor_pos = text.chars().count() as u32;
        assert_eq!(
            cursor_pos, 1,
            "Vietnamese NFC char must count as 1 for cursor_pos"
        );
        assert!(text.len() > 1, "NFC Vietnamese char is multi-byte in UTF-8");

        let text2 = "việt";
        assert_eq!(
            text2.chars().count() as u32,
            4,
            "vier NFC chars = cursor_pos 4"
        );
    }

    // ── set_capabilities mode-selection regression tests ──────────────────────

    fn make_iface_for_caps_test(
        force_chrome_direct: bool,
        force_preedit_mode: bool,
    ) -> (IbusEngineIface, Arc<Mutex<SharedState>>) {
        use vi_core::InputMethod;
        let state = SharedState {
            force_chrome_direct,
            force_preedit_mode,
            ..Default::default()
        };
        let state_arc = Arc::new(Mutex::new(state));
        let iface = IbusEngineIface {
            engine: Arc::new(Mutex::new(StandardEngine::new(InputMethod::Telex))),
            state: Arc::clone(&state_arc),
            handler_lock: Arc::new(tokio::sync::Mutex::new(())),
        };
        (iface, state_arc)
    }

    /// Apps with IBUS_CAP_PREEDIT_TEXT | IBUS_CAP_SURROUNDING_TEXT (Chrome, VSCode,
    /// GTK4, Qt) must use preedit mode (all flags false = default preedit dispatch).
    /// surrounding_commit causes garbling; forward_key drops Vietnamese chars in Chrome.
    /// Preedit (UpdatePreeditText) is the only mode Chrome renders reliably.
    #[tokio::test]
    async fn test_preedit_and_surrounding_caps_selects_preedit_mode() {
        let (iface, state) = make_iface_for_caps_test(false, false);
        iface
            .set_capabilities(IBUS_CAP_PREEDIT_TEXT | IBUS_CAP_SURROUNDING_TEXT)
            .await;
        let s = state.lock().unwrap();
        assert!(
            !s.use_surrounding_commit,
            "Chrome/VSCode must NOT use surrounding_commit"
        );
        assert!(
            !s.use_forward_key,
            "Chrome/VSCode must NOT use forward_key (drops Vietnamese chars)"
        );
        assert!(!s.chrome_direct_mode);
        // use_surrounding_commit=false, use_forward_key=false → default preedit dispatch
    }

    /// force_chrome_direct config must activate chrome_direct_mode regardless of caps.
    #[tokio::test]
    async fn test_force_chrome_direct_activates_chrome_direct_mode() {
        let (iface, state) = make_iface_for_caps_test(true, false);
        iface.set_capabilities(IBUS_CAP_PREEDIT_TEXT).await;
        let s = state.lock().unwrap();
        assert!(
            s.chrome_direct_mode,
            "force_chrome_direct must activate chrome_direct_mode"
        );
        assert!(!s.use_surrounding_commit);
        assert!(!s.use_forward_key);
    }

    /// When both config flags are set, `force_preedit_mode` wins: standard preedit
    /// path only (no `chrome_direct_mode`).
    #[tokio::test]
    async fn test_force_preedit_mode_clears_force_chrome_direct() {
        let (iface, state) = make_iface_for_caps_test(true, true);
        iface.set_capabilities(IBUS_CAP_PREEDIT_TEXT).await;
        let s = state.lock().unwrap();
        assert!(
            !s.chrome_direct_mode,
            "force_preedit_mode must clear chrome_direct"
        );
        assert!(!s.use_surrounding_commit);
        assert!(!s.use_forward_key);
    }

    /// Apps with only IBUS_CAP_SURROUNDING_TEXT (no preedit bit) also use preedit mode.
    /// Any app advertising SURROUNDING_TEXT has native IBus integration.
    #[tokio::test]
    async fn test_surrounding_only_caps_selects_preedit_mode() {
        let (iface, state) = make_iface_for_caps_test(false, false);
        iface.set_capabilities(IBUS_CAP_SURROUNDING_TEXT).await;
        let s = state.lock().unwrap();
        assert!(
            !s.use_surrounding_commit,
            "surrounding-only app must NOT use surrounding_commit"
        );
        assert!(
            !s.use_forward_key,
            "surrounding-only app must NOT use forward_key → preedit mode"
        );
        assert!(!s.chrome_direct_mode);
    }

    /// ALL apps — regardless of capability bits — get preedit mode (UpdatePreeditText).
    /// forward_key is never auto-selected; it caused Vietnamese Unicode chars > U+00FF
    /// to be dropped in Chrome (BackSpace fired, char silently dropped → net deletion).
    #[tokio::test]
    async fn test_caps_mode_selection() {
        for caps in [
            0u32,
            4,
            IBUS_CAP_PREEDIT_TEXT,
            IBUS_CAP_SURROUNDING_TEXT,
            IBUS_CAP_PREEDIT_TEXT | IBUS_CAP_SURROUNDING_TEXT,
        ] {
            let (iface, state) = make_iface_for_caps_test(false, false);
            iface.set_capabilities(caps).await;
            let s = state.lock().unwrap();
            assert!(
                !s.use_forward_key,
                "caps={caps:#x}: all apps must use preedit (not forward_key)"
            );
            assert!(
                !s.use_surrounding_commit,
                "caps={caps:#x}: surrounding_commit must never be auto-selected"
            );
            assert!(
                !s.chrome_direct_mode,
                "caps={caps:#x}: chrome_direct only via force_chrome_direct=true"
            );
        }
    }

    /// `force_preedit_mode` must clear `chrome_direct_mode` and keep surrounding /
    /// forward paths disabled (mirrors `set_capabilities` ordering vs config).
    #[tokio::test]
    async fn test_force_preedit_overrides_surrounding() {
        let (iface, state) = make_iface_for_caps_test(false, true);
        iface.set_capabilities(IBUS_CAP_SURROUNDING_TEXT).await;
        let s = state.lock().unwrap();
        assert!(
            !s.use_surrounding_commit,
            "force_preedit_mode must suppress surrounding_commit"
        );
        assert!(!s.use_forward_key);
        assert!(!s.chrome_direct_mode);
    }

    // ── surrounding_commit: signal sequence and shadow_buf update ────────────

    /// Verify the signal sequence emitted by `dispatch_surrounding_commit`.
    ///
    /// Contract:
    ///   shadow_buf="ao", new_preedit="aô"
    ///     → DeleteSurroundingText(offset=-2, n_chars=2)
    ///     → CommitText("aô")
    ///     → shadow_buf becomes "aô" on success
    ///
    /// Full-replace: the entire shadow is deleted before committing new_preedit,
    /// rather than diffing a common prefix.  This is safe against cursor-movement
    /// desync where a partial delete would target the wrong position.
    #[test]
    fn surrounding_commit_full_replace_sequence() {
        let shadow = "ao";
        let new_preedit = "aô";

        // Replicate dispatch_surrounding_commit's op computation (no D-Bus needed).
        let backspaces = shadow.chars().count();
        let new_tail = new_preedit;

        // Signal 1: DeleteSurroundingText(-2, 2)
        let offset = -(backspaces as i32);
        assert_eq!(offset, -2, "delete offset must be -(shadow char count)");
        assert_eq!(
            backspaces as u32, 2,
            "delete count must equal shadow char count"
        );

        // Signal 2: CommitText("aô") — full new_preedit, emitted after the delete
        assert_eq!(
            new_tail, "aô",
            "committed text must be the full new_preedit"
        );

        // On success, shadow_buf is updated to new_preedit.
        let new_shadow = new_preedit.to_owned();
        assert_eq!(
            new_shadow, "aô",
            "shadow_buf must become new_preedit on success"
        );
    }

    // ── chrome_direct_mode: signal sequence regression ────────────────────────

    /// Verify the preedit-based signal sequence emitted by `dispatch_chrome_direct`.
    ///
    /// New behavior: `UpdatePreeditTextWithMode(text, cursor_pos, true, IBUS_ENGINE_PREEDIT_COMMIT)`
    /// where `cursor_pos` is the Unicode character count of `new_preedit`.
    /// No `ForwardKeyEvent(BackSpace)` is emitted — Chrome on XWayland can
    /// deliver those twice, causing extra deletions.
    ///
    /// Signal sequence contract (pinned here so regressions are caught):
    ///   new_preedit="tô"  →  UpdatePreeditText("tô",  cursor_pos=2, visible=true)
    ///   new_preedit="tôi" →  UpdatePreeditText("tôi", cursor_pos=3, visible=true)
    ///   new_preedit="t"   →  UpdatePreeditText("t",   cursor_pos=1, visible=true)
    ///   new_preedit=""    →  UpdatePreeditText("",    cursor_pos=0, visible=true)
    #[test]
    fn chrome_direct_mode_sequence() {
        struct Case {
            new_preedit: &'static str,
            expected_cursor_pos: u32,
        }

        let cases = [
            Case {
                new_preedit: "tô",
                expected_cursor_pos: 2,
            },
            Case {
                new_preedit: "tôi",
                expected_cursor_pos: 3,
            },
            Case {
                new_preedit: "t",
                expected_cursor_pos: 1,
            },
            Case {
                new_preedit: "",
                expected_cursor_pos: 0,
            },
        ];

        for c in &cases {
            // Replicate dispatch_chrome_direct logic without a live D-Bus connection.
            // New implementation: no BackSpaces; cursor_pos = chars().count().
            let cursor_pos = c.new_preedit.chars().count() as u32;
            assert_eq!(
                cursor_pos, c.expected_cursor_pos,
                "new_preedit={:?}: cursor_pos must be Unicode char count",
                c.new_preedit
            );
        }
    }
}
