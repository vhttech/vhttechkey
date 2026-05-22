//! Deprecated: use the vime-* equivalent crate instead.
//! Wayland input method backend.
//!
//! Implements `ImeBackend` using `zwp_input_method_v2`.
//! Key forwarding uses `zwp_virtual_keyboard_v1` as a fallback when the
//! compositor does not advertise input-method-v2.
//!
//! Compositor quirks are detected at runtime via [`quirks::CompositorQuirks`]
//! rather than compile-time feature flags, so a single binary handles all
//! compositor families.
//!
//! Event ordering: the implementation reconciles state on every event rather
//! than assuming a fixed ordering from the compositor.

pub mod adapter;
pub mod compat;
pub mod quirks;

use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};
use parking_lot::Mutex;

use adapter::{clamp_preedit_cursor, TextInputAdapter};
use compat::{guard_surrounding_text, needs_text_input_lifecycle};
use quirks::{CompositorProfile, CompositorQuirks};
use tokio::sync::mpsc;
use tracing::{debug, info, warn};
use vi_core::{InputEvent, Key, Modifiers, NfcString, PreeditText};
use vi_platform::{Capabilities, CharCursor, ImeBackend, PlatformError, Result, SurroundingText};
use wayland_client::{
    backend::ObjectId,
    delegate_noop,
    globals::{registry_queue_init, GlobalListContents},
    protocol::{wl_keyboard, wl_output, wl_registry, wl_seat},
    Connection, Dispatch, EventQueue, Proxy, QueueHandle, WEnum,
};
use wayland_protocols::wp::text_input::zv3::client::{
    zwp_text_input_manager_v3::ZwpTextInputManagerV3,
    zwp_text_input_v3::{self, ZwpTextInputV3},
};
use wayland_protocols_misc::{
    zwp_input_method_v2::client::{
        zwp_input_method_keyboard_grab_v2::{self, ZwpInputMethodKeyboardGrabV2},
        zwp_input_method_manager_v2::ZwpInputMethodManagerV2,
        zwp_input_method_v2::{self, ZwpInputMethodV2},
    },
    zwp_virtual_keyboard_v1::client::{
        zwp_virtual_keyboard_manager_v1::ZwpVirtualKeyboardManagerV1,
        zwp_virtual_keyboard_v1::ZwpVirtualKeyboardV1,
    },
};

const SERIAL_WARN_THRESHOLD: u32 = u32::MAX - 10_000;

// ── Monitor geometry ──────────────────────────────────────────────────────────

/// Display metrics for a connected Wayland output.
#[derive(Debug, Clone)]
pub struct MonitorInfo {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    /// Integer scale factor reported by the compositor via `wl_output.scale`.
    pub scale_factor: f32,
}

impl Default for MonitorInfo {
    fn default() -> Self {
        Self { x: 0, y: 0, width: 1920, height: 1080, scale_factor: 1.0 }
    }
}

// ── Shared state (thread-safe) ────────────────────────────────────────────────

#[derive(Debug, Default)]
struct WaylandShared {
    surrounding: Option<SurroundingText>,
    active: bool,
    /// Preedit cached for the GNOME focus-change flush.
    pending_preedit: Option<(String, i32)>,
    /// Number of `zwp_input_method_v2::done` events received since last Activate.
    /// This is the value the protocol requires in every `commit(serial)` call.
    serial: u32,
    /// Number of `commit()` calls sent since last Activate; used only for
    /// Niri dual-protocol desync detection (compared against `ti_serial`).
    sent_commits: u32,
    /// Number of text-input-v3 Done events received; used for Niri desync detection.
    ti_serial: u32,
    /// Value of `serial` at the time surrounding text was last stored.
    surrounding_serial: u32,
}

// ── Wayland dispatch state ────────────────────────────────────────────────────

// ObjectId keys contain Arc<AtomicBool> (alive flag); Hash/Eq are based on the
// numeric ID so interior mutability does not affect map correctness.
#[allow(clippy::mutable_key_type)]
struct WlState {
    shared: Arc<Mutex<WaylandShared>>,
    quirks: CompositorQuirks,
    input_method: Option<ZwpInputMethodV2>,
    _keyboard_grab: Option<ZwpInputMethodKeyboardGrabV2>,
    virtual_keyboard: Option<ZwpVirtualKeyboardV1>,
    _text_input: Option<ZwpTextInputV3>,
    /// Kept alive for protocol re-init after serial wraparound.
    im_mgr: Option<ZwpInputMethodManagerV2>,
    /// Kept alive for protocol re-init after serial wraparound.
    seat: Option<wl_seat::WlSeat>,
    /// State machine for the secondary text-input-v3 binding (GNOME, Niri).
    ti_adapter: TextInputAdapter,
    modifier_state: Modifiers,
    tx: mpsc::Sender<InputEvent>,
    /// Evdev keycodes currently in the pressed state.  A second press event
    /// for a key that is already pressed is a compositor-generated key-repeat
    /// event; it is converted to `KeyRepeat` rather than `KeyDown`.
    pressed_keys: HashSet<u32>,
    /// Key-repeat rate in repeats per second as advertised by the compositor
    /// via `RepeatInfo`.  Zero means no repeats are expected.
    repeat_rate: i32,
    /// `wl_output` proxies kept alive so the server keeps sending events.
    /// Each entry is `(global_name, proxy)` for removal on `GlobalRemove`.
    outputs: Vec<(u32, wl_output::WlOutput)>,
    /// Monitor geometry and scale, keyed by output object ID.
    monitor_map: HashMap<ObjectId, MonitorInfo>,
}

impl WlState {
    fn new(
        shared: Arc<Mutex<WaylandShared>>,
        quirks: CompositorQuirks,
        tx: mpsc::Sender<InputEvent>,
    ) -> Self {
        Self {
            shared,
            quirks,
            input_method: None,
            _keyboard_grab: None,
            virtual_keyboard: None,
            _text_input: None,
            im_mgr: None,
            seat: None,
            ti_adapter: TextInputAdapter::default(),
            modifier_state: Modifiers::default(),
            tx,
            pressed_keys: HashSet::new(),
            repeat_rate: 25, // sensible default; overwritten by RepeatInfo
            outputs: Vec::new(),
            monitor_map: HashMap::new(),
        }
    }
}

// ── Dispatch impls ────────────────────────────────────────────────────────────

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for WlState {
    fn event(
        state: &mut Self,
        registry: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        _data: &GlobalListContents,
        _conn: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        match event {
            wl_registry::Event::Global { name, interface, version }
                if interface == "wl_output" =>
            {
                // Bind up to version 4 (which adds wl_output.name /
                // wl_output.description; version 2 added scale).
                let output: wl_output::WlOutput =
                    registry.bind(name, version.min(4), qh, ());
                state.monitor_map.insert(output.id(), MonitorInfo::default());
                state.outputs.push((name, output));
            }
            wl_registry::Event::GlobalRemove { name } => {
                // Find and remove the matching output proxy.
                let removed_id = state
                    .outputs
                    .iter()
                    .find(|(n, _)| *n == name)
                    .map(|(_, o)| o.id());
                state.outputs.retain(|(n, _)| *n != name);
                if let Some(id) = removed_id {
                    state.monitor_map.remove(&id);
                    debug!("wl_output: removed monitor {id:?}");
                }
            }
            _ => {}
        }
    }
}

impl Dispatch<ZwpInputMethodManagerV2, ()> for WlState {
    fn event(
        _state: &mut Self,
        _proxy: &ZwpInputMethodManagerV2,
        _event: <ZwpInputMethodManagerV2 as wayland_client::Proxy>::Event,
        _: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ZwpInputMethodV2, ()> for WlState {
    fn event(
        state: &mut Self,
        im: &ZwpInputMethodV2,
        event: zwp_input_method_v2::Event,
        _: &(),
        _conn: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        match event {
            zwp_input_method_v2::Event::Activate => {
                debug!("IM: Activate");
                let mut s = state.shared.lock();
                s.active = true;
                // Reset the serial counter on each activation cycle. The protocol
                // requires commit(serial) where serial equals the number of done
                // events received since the last activate event.
                s.serial = 0;
                s.sent_commits = 0;
                s.ti_serial = 0;
                // Discard any preedit left from a previous focus session so it
                // does not leak into the new one.
                s.pending_preedit = None;
            }
            zwp_input_method_v2::Event::Deactivate => {
                debug!("IM: Deactivate");
                let mut s = state.shared.lock();
                s.active = false;
                if state.quirks.empty_preedit_before_commit {
                    if let Some((preedit, _)) = s.pending_preedit.take() {
                        let serial = s.serial;
                        drop(s);
                        if !preedit.is_empty() {
                            // Clear preedit and commit text in one atomic protocol
                            // commit so the serial is used exactly once. Two separate
                            // commit() calls with the same serial cause the compositor
                            // to silently drop the second one.
                            im.set_preedit_string(String::new(), -1, -1);
                            im.commit_string(preedit);
                            im.commit(serial);
                            let mut sh = state.shared.lock();
                            sh.sent_commits = sh.sent_commits.wrapping_add(1);
                        }
                    }
                }
            }
            zwp_input_method_v2::Event::SurroundingText { text, cursor, anchor } => {
                // Hyprland/wlroots sends empty surrounding_text for widgets
                // with no text; discard it to avoid clobbering valid context.
                if matches!(state.quirks.profile, CompositorProfile::Hyprland | CompositorProfile::Labwc) && !guard_surrounding_text(&text) {
                    return;
                }
                let mut s = state.shared.lock();
                s.surrounding_serial = s.serial;
                s.surrounding = Some(SurroundingText {
                    text,
                    cursor_pos: cursor,
                    anchor_pos: anchor,
                });
            }
            zwp_input_method_v2::Event::Done => {
                let mut s = state.shared.lock();
                s.serial = s.serial.wrapping_add(1);
                if s.serial > SERIAL_WARN_THRESHOLD {
                    drop(s);
                    warn!("Wayland input-method serial near u32 wraparound — reinitialising");
                    if let (Some(mgr), Some(seat)) = (&state.im_mgr, &state.seat) {
                        let new_im = mgr.get_input_method(seat, qh, ());
                        let new_grab = new_im.grab_keyboard(qh, ());
                        state._keyboard_grab = Some(new_grab);
                        state.input_method = Some(new_im);
                        // Serial resets to 0 on the compositor's next Activate.
                    }
                }
            }
            zwp_input_method_v2::Event::Unavailable => {
                warn!("zwp_input_method_v2 unavailable on this compositor");
            }
            event => {
                debug!("Unhandled wayland event: {:?}", event);
            }
        }
    }
}

impl Dispatch<ZwpTextInputManagerV3, ()> for WlState {
    fn event(
        _state: &mut Self,
        _proxy: &ZwpTextInputManagerV3,
        _event: <ZwpTextInputManagerV3 as wayland_client::Proxy>::Event,
        _: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ZwpTextInputV3, ()> for WlState {
    fn event(
        state: &mut Self,
        ti: &ZwpTextInputV3,
        event: zwp_text_input_v3::Event,
        _: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        match event {
            zwp_text_input_v3::Event::Enter { surface: _ } => {
                debug!("TextInput: Enter");
                // GNOME and Niri require explicit enable()+commit() on enter so
                // the compositor transitions the text-input session to active.
                if needs_text_input_lifecycle(state.quirks.profile) {
                    state.ti_adapter.on_enter(ti);
                }
            }
            zwp_text_input_v3::Event::Leave { surface: _ } => {
                debug!("TextInput: Leave");
                // GNOME fires text-input-v3 Leave before input-method-v2
                // Deactivate.  Flush the pending preedit first so it is not
                // swallowed, then formally disable the text-input session.
                if state.quirks.empty_preedit_before_commit {
                    let (preedit_opt, serial) = {
                        let mut sh = state.shared.lock();
                        (sh.pending_preedit.take(), sh.serial)
                    };
                    if let (Some((p, _)), Some(im)) = (preedit_opt, &state.input_method) {
                        if !p.is_empty() {
                            // Single atomic commit to avoid stale-serial rejection.
                            im.set_preedit_string(String::new(), -1, -1);
                            im.commit_string(p);
                            im.commit(serial);
                        }
                    }
                }
                if needs_text_input_lifecycle(state.quirks.profile) {
                    state.ti_adapter.on_leave(ti);
                }
            }
            zwp_text_input_v3::Event::Done { serial } => {
                state.ti_adapter.on_done(serial);
                let mut s = state.shared.lock();
                s.ti_serial = s.ti_serial.wrapping_add(1);
            }
            event => {
                debug!("Unhandled wayland event: {:?}", event);
            }
        }
    }
}

impl Dispatch<wl_output::WlOutput, ()> for WlState {
    fn event(
        state: &mut Self,
        output: &wl_output::WlOutput,
        event: wl_output::Event,
        _: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        let id = output.id();
        let entry = state.monitor_map.entry(id.clone()).or_default();
        match event {
            wl_output::Event::Geometry { x, y, .. } => {
                entry.x = x;
                entry.y = y;
            }
            // Mode events carry the resolution; the last one before Done
            // (with or without the Current flag) is what we display.
            wl_output::Event::Mode { width, height, .. } => {
                entry.width = width;
                entry.height = height;
            }
            // Integer scale factor (1 = 1×, 2 = HiDPI 2×, …).
            wl_output::Event::Scale { factor } => {
                entry.scale_factor = factor as f32;
                debug!("wl_output: scale={factor} for output {id:?}");
            }
            wl_output::Event::Done => {
                debug!("wl_output: done — {:?}", state.monitor_map.get(&id));
            }
            _ => {}
        }
    }
}

impl Dispatch<ZwpVirtualKeyboardManagerV1, ()> for WlState {
    fn event(
        _: &mut Self,
        _: &ZwpVirtualKeyboardManagerV1,
        _event: <ZwpVirtualKeyboardManagerV1 as wayland_client::Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ZwpVirtualKeyboardV1, ()> for WlState {
    fn event(
        _: &mut Self,
        _: &ZwpVirtualKeyboardV1,
        _event: <ZwpVirtualKeyboardV1 as wayland_client::Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ZwpInputMethodKeyboardGrabV2, ()> for WlState {
    fn event(
        state: &mut Self,
        _grab: &ZwpInputMethodKeyboardGrabV2,
        event: zwp_input_method_keyboard_grab_v2::Event,
        _: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        match event {
            zwp_input_method_keyboard_grab_v2::Event::Key {
                key,
                state: key_state,
                ..
            } => {
                let pressed =
                    matches!(key_state, WEnum::Value(wl_keyboard::KeyState::Pressed));

                // Track pressed keys to distinguish genuine presses from
                // compositor-generated key-repeat events.
                // `HashSet::insert` returns false if the key was already present.
                let is_repeat = if pressed {
                    !state.pressed_keys.insert(key)
                } else {
                    state.pressed_keys.remove(&key);
                    false
                };

                // If the compositor advertises no repeat, discard extra presses.
                if is_repeat && state.repeat_rate == 0 {
                    return;
                }

                if let Some(ev) =
                    map_evdev_to_input_event(key, pressed, &state.modifier_state)
                {
                    if !state.modifier_state.ctrl {
                        // Convert compositor repeat presses to KeyRepeat so the
                        // engine does not re-fire composition rules.
                        let ev = if is_repeat { to_key_repeat(ev) } else { ev };
                        let _ = state.tx.try_send(ev);
                    }
                }
            }
            zwp_input_method_keyboard_grab_v2::Event::RepeatInfo { rate, delay: _ } => {
                // Store the advertised rate (repeats per second).  Zero means
                // the compositor will not generate repeat events.
                state.repeat_rate = rate;
            }
            zwp_input_method_keyboard_grab_v2::Event::Modifiers {
                mods_depressed,
                mods_latched,
                mods_locked,
                ..
            } => {
                // XKB modifier bits: Shift=1, Lock=2, Control=4, Mod1(Alt)=8, Mod4(Super)=64
                let effective = mods_depressed | mods_latched | mods_locked;
                state.modifier_state = Modifiers {
                    shift: (effective & 1) != 0,
                    caps_lock: (effective & 2) != 0,
                    ctrl: (effective & 4) != 0,
                    alt: (effective & 8) != 0,
                    super_key: (effective & 64) != 0,
                    altgr: (effective & 128) != 0,
                };
            }
            _ => {}
        }
    }
}

delegate_noop!(WlState: ignore wl_seat::WlSeat);
delegate_noop!(WlState: ignore wl_keyboard::WlKeyboard);

// ── Public backend ────────────────────────────────────────────────────────────

/// A live Wayland input-method connection implementing `ImeBackend`.
pub struct WaylandBackend {
    _rt: tokio::runtime::Handle,
    eq: Arc<Mutex<EventQueue<WlState>>>,
    wl: Arc<Mutex<WlState>>,
    shared: Arc<Mutex<WaylandShared>>,
    quirks: CompositorQuirks,
}

impl WaylandBackend {
    /// Connect to the Wayland compositor and bind IME protocols.
    pub fn connect(rt: tokio::runtime::Handle, tx: mpsc::Sender<InputEvent>) -> Result<Self> {
        let conn = Connection::connect_to_env()
            .map_err(|e| PlatformError::Wayland(e.to_string()))?;

        let (globals, mut eq) = registry_queue_init::<WlState>(&conn)
            .map_err(|e| PlatformError::Wayland(e.to_string()))?;

        let qh = eq.handle();
        let shared = Arc::new(Mutex::new(WaylandShared::default()));
        let quirks = CompositorQuirks::detect(&globals);
        let mut wl = WlState::new(Arc::clone(&shared), quirks, tx);

        // Bind seat (required for input-method and virtual-keyboard).
        let seat: Option<wl_seat::WlSeat> =
            globals.bind(&qh, 1..=8, ()).ok();

        // Try to bind input-method-v2.
        let im_mgr: Option<ZwpInputMethodManagerV2> =
            globals.bind(&qh, 1..=1, ()).ok();

        if let (Some(mgr), Some(s)) = (&im_mgr, &seat) {
            let im = mgr.get_input_method(s, &qh, ());
            let grab = im.grab_keyboard(&qh, ());
            wl._keyboard_grab = Some(grab);
            wl.input_method = Some(im);
            info!("Wayland: bound zwp_input_method_v2");
        } else {
            // No input-method-v2 — try virtual-keyboard-v1 fallback.
            let vk_mgr: Option<ZwpVirtualKeyboardManagerV1> =
                globals.bind(&qh, 1..=1, ()).ok();
            if let (Some(mgr), Some(s)) = (vk_mgr, &seat) {
                let vk = mgr.create_virtual_keyboard(s, &qh, ());
                wl.virtual_keyboard = Some(vk);
                if quirks.virtual_keyboard_fallback {
                    info!("Wayland: LXQt/Openbox compositor; using zwp_virtual_keyboard_v1 fallback");
                } else {
                    info!("Wayland: using zwp_virtual_keyboard_v1 fallback");
                }
            } else {
                return Err(PlatformError::Wayland(
                    "neither zwp_input_method_manager_v2 nor \
                     zwp_virtual_keyboard_manager_v1 advertised"
                        .into(),
                ));
            }
        }

        // Bind text-input-v3 for focus sync (optional).
        let ti_mgr: Option<ZwpTextInputManagerV3> =
            globals.bind(&qh, 1..=1, ()).ok();
        if let (Some(mgr), Some(s)) = (ti_mgr, &seat) {
            let ti = mgr.get_text_input(s, &qh, ());
            wl._text_input = Some(ti);
        }

        // Store manager and seat for potential serial-wraparound re-init.
        wl.im_mgr = im_mgr;
        wl.seat = seat;

        // Process initial wl_output events (Geometry, Mode, Scale, Done) so
        // the monitor map is populated before the backend is handed to the
        // caller.  A roundtrip flushes our pending bind requests and waits
        // for the server to acknowledge them.
        eq.roundtrip(&mut wl)
            .map_err(|e| PlatformError::Wayland(e.to_string()))?;

        let eq_arc = Arc::new(Mutex::new(eq));
        let wl_arc = Arc::new(Mutex::new(wl));

        let conn_bg = conn.clone();
        let eq_bg = Arc::clone(&eq_arc);
        let wl_bg = Arc::clone(&wl_arc);
        rt.spawn_blocking(move || {
            loop {
                // Read from the Wayland socket WITHOUT holding eq/wl locks so
                // that with_im() (preedit sends) can proceed concurrently.
                if let Some(guard) = conn_bg.prepare_read() {
                    if guard.read().is_err() {
                        break;
                    }
                }
                // Brief lock acquisition to dispatch buffered events.
                let mut eq = eq_bg.lock();
                let mut wl = wl_bg.lock();
                if eq.dispatch_pending(&mut *wl).is_err() {
                    break;
                }
            }
            debug!("Wayland background reader exiting");
        });

        Ok(WaylandBackend {
            _rt: rt,
            eq: eq_arc,
            wl: wl_arc,
            shared,
            quirks,
        })
    }

    /// Return the [`MonitorInfo`] for the monitor that contains `(cx, cy)` in
    /// global screen coordinates, falling back to the first known monitor.
    ///
    /// Returns `None` if no monitors have been reported by the compositor yet.
    #[allow(clippy::mutable_key_type)]
    pub fn cursor_monitor(&self, cx: i32, cy: i32) -> Option<MonitorInfo> {
        let wl = self.wl.lock();
        let map = &wl.monitor_map;
        map.values()
            .find(|m| {
                cx >= m.x
                    && cx < m.x + m.width
                    && cy >= m.y
                    && cy < m.y + m.height
            })
            .or_else(|| map.values().next())
            .cloned()
    }

    fn flush(&self) -> Result<()> {
        let mut eq = self.eq.lock();
        let mut wl = self.wl.lock();
        eq.flush().map_err(|e| PlatformError::Wayland(e.to_string()))?;
        eq.dispatch_pending(&mut *wl)
            .map_err(|e| PlatformError::Wayland(e.to_string()))?;
        Ok(())
    }

    fn roundtrip(&self) -> Result<()> {
        let mut eq = self.eq.lock();
        let mut wl = self.wl.lock();
        eq.roundtrip(&mut *wl)
            .map_err(|e| PlatformError::Wayland(e.to_string()))?;
        Ok(())
    }

    fn with_im<F>(&self, f: F) -> Result<()>
    where
        F: FnOnce(&ZwpInputMethodV2, u32),
    {
        let wl = self.wl.lock();
        let shared = self.shared.lock();
        // The protocol requires commit(serial) where serial is the count of
        // zwp_input_method_v2::done events received since the last Activate.
        let serial = shared.serial;
        if self.quirks.niri_dual_protocol
            && shared.sent_commits.wrapping_sub(shared.ti_serial) > 2
        {
            warn!(
                "Niri dual-protocol serial desync: im={} ti={} — proceeding",
                shared.sent_commits, shared.ti_serial
            );
        }
        let sent_commits = shared.sent_commits;
        drop(shared);
        match &wl.input_method {
            Some(im) => {
                f(im, serial);
                self.shared.lock().sent_commits = sent_commits.wrapping_add(1);
                drop(wl);
                self.flush()
            }
            None => {
                warn!("with_im: input method not available (no active IM session)");
                Err(PlatformError::NotSupported)
            }
        }
    }
}

impl ImeBackend for WaylandBackend {
    fn commit(&self, text: &NfcString) -> Result<()> {
        {
            self.shared.lock().pending_preedit = None;
        }
        let s = text.as_str().to_owned();
        self.with_im(|im, serial| {
            im.commit_string(s.clone());
            im.commit(serial);
        })
    }

    fn commit_replacing_preedit(&self, text: &NfcString) -> Result<()> {
        {
            self.shared.lock().pending_preedit = None;
        }
        let s = text.as_str().to_owned();
        self.with_im(|im, serial| {
            // Clear preedit and commit text in one atomic protocol frame so the
            // serial is consumed exactly once.  Sending im.commit(serial) twice
            // (once from clear_preedit and once from commit) with the same serial
            // causes compositors such as GNOME/Mutter to silently drop the second
            // request, meaning the text is never inserted into the application.
            im.set_preedit_string(String::new(), -1, -1);
            im.commit_string(s.clone());
            im.commit(serial);
        })
    }

    fn commit_then_update_preedit(
        &self,
        text: &NfcString,
        preedit: &PreeditText,
        _cursor: CharCursor,
    ) -> Result<()> {
        let commit_s = text.as_str().to_owned();
        let preedit_s = preedit.as_str().to_owned();
        let cursor_byte = clamp_preedit_cursor(&preedit_s, preedit.cursor_byte_offset as i32);
        {
            let mut sh = self.shared.lock();
            sh.pending_preedit = Some((preedit_s.clone(), cursor_byte));
        }
        self.with_im(|im, serial| {
            im.commit_string(commit_s.clone());
            im.set_preedit_string(preedit_s.clone(), cursor_byte, cursor_byte);
            im.commit(serial);
        })
    }

    fn update_preedit(&self, text: &PreeditText, _cursor: CharCursor) -> Result<()> {
        let s = text.as_str().to_owned();
        // Use the pre-computed byte offset from the preedit struct; clamp to a
        // valid UTF-8 boundary in case the compositor has stricter requirements.
        let cursor_byte = clamp_preedit_cursor(&s, text.cursor_byte_offset as i32);
        {
            let mut sh = self.shared.lock();
            sh.pending_preedit = Some((s.clone(), cursor_byte));
        }
        self.with_im(|im, serial| {
            im.set_preedit_string(s.clone(), cursor_byte, cursor_byte);
            im.commit(serial);
        })
    }

    fn clear_preedit(&self) -> Result<()> {
        {
            self.shared.lock().pending_preedit = None;
        }
        // xfwm4 drops a preedit-clear that arrives in the same frame as a commit;
        // a roundtrip ensures the compositor has processed any preceding commit
        // before the clear is sent.
        if self.quirks.delay_preedit_clear {
            self.roundtrip()?;
        }
        self.with_im(|im, serial| {
            im.set_preedit_string(String::new(), -1, -1);
            im.commit(serial);
        })
    }

    fn forward_key(&self, key: &InputEvent) -> Result<()> {
        let wl = self.wl.lock();
        if let Some(vk) = &wl.virtual_keyboard {
            let (time, code, state) = input_event_to_vk(key);
            vk.key(time, code, state);
            drop(wl);
            return self.flush();
        }
        warn!("forward_key called but zwp_virtual_keyboard_v1 is not bound");
        // With input-method-v2 there is no direct forward-key method; the
        // compositor handles it via preedit commit → PassThrough flow.
        Err(PlatformError::NotSupported)
    }

    fn surrounding_text(&self) -> Result<Option<SurroundingText>> {
        let s = self.shared.lock();
        if s.serial.wrapping_sub(s.surrounding_serial) > 2 {
            debug!(
                "surrounding text stale (surrounding_serial={} serial={})",
                s.surrounding_serial, s.serial
            );
            return Ok(None);
        }
        Ok(s.surrounding.clone())
    }

    fn capabilities(&self) -> Capabilities {
        let active = self.shared.lock().active;
        Capabilities {
            surrounding_text: true,
            preedit: active,
            lookup_table: active,
        }
    }
}

// ── Key helpers ───────────────────────────────────────────────────────────────

/// Convert a `KeyDown` event to `KeyRepeat` so the engine handles it as a
/// held-key repeat rather than a new press.  Other event types are unchanged.
fn to_key_repeat(event: InputEvent) -> InputEvent {
    match event {
        InputEvent::KeyDown(key, mods) => InputEvent::KeyRepeat(key, mods),
        other => other,
    }
}

/// Map an evdev keycode + press state to an `InputEvent`, or `None` for
/// unrecognised codes (modifier-only keys, multimedia keys, etc.).
fn map_evdev_to_input_event(key: u32, pressed: bool, mods: &Modifiers) -> Option<InputEvent> {
    let k = match key {
        1 => Key::Escape,
        14 => Key::Backspace,
        15 => Key::Tab,
        28 => Key::Return,
        57 => Key::Char(' '),
        102 => Key::Home,
        103 => Key::Up,
        104 => Key::Delete, // PgUp maps to Delete for now
        105 => Key::Left,
        106 => Key::Right,
        107 => Key::End,
        108 => Key::Down,
        111 => Key::Delete,
        _ => {
            let shifted = mods.shift ^ mods.caps_lock;
            Key::Char(evdev_to_char(key, shifted)?)
        }
    };
    if pressed {
        Some(InputEvent::KeyDown(k, *mods))
    } else {
        Some(InputEvent::KeyUp(k))
    }
}

fn evdev_to_char(key: u32, shifted: bool) -> Option<char> {
    // Evdev keycodes follow the physical US QWERTY layout.
    let lower = match key {
        2 => '1', 3 => '2', 4 => '3', 5 => '4', 6 => '5',
        7 => '6', 8 => '7', 9 => '8', 10 => '9', 11 => '0',
        16 => 'q', 17 => 'w', 18 => 'e', 19 => 'r', 20 => 't',
        21 => 'y', 22 => 'u', 23 => 'i', 24 => 'o', 25 => 'p',
        30 => 'a', 31 => 's', 32 => 'd', 33 => 'f', 34 => 'g',
        35 => 'h', 36 => 'j', 37 => 'k', 38 => 'l',
        44 => 'z', 45 => 'x', 46 => 'c', 47 => 'v', 48 => 'b',
        49 => 'n', 50 => 'm',
        _ => return None,
    };
    if shifted {
        lower.to_uppercase().next()
    } else {
        Some(lower)
    }
}

fn input_event_to_vk(event: &InputEvent) -> (u32, u32, u32) {
    let code = match event {
        InputEvent::KeyDown(k, _)
        | InputEvent::KeyUp(k)
        | InputEvent::KeyRepeat(k, _) => key_to_evdev(k),
        _ => return (0, 0, 0),
    };
    let state = match event {
        InputEvent::KeyDown(_, _) | InputEvent::KeyRepeat(_, _) => 1,
        _ => 0,
    };
    (0, code, state)
}

fn key_to_evdev(key: &Key) -> u32 {
    match key {
        Key::Backspace => 14,
        Key::Delete => 111,
        Key::Return => 28,
        Key::Escape => 1,
        Key::Tab => 15,
        Key::Left => 105,
        Key::Right => 106,
        Key::Up => 103,
        Key::Down => 108,
        Key::Home => 102,
        Key::End => 107,
        Key::Char(c) => char_to_evdev(*c),
        Key::Keysym(_) | Key::DeadKey(_) | Key::ComposeKey => 0,
    }
}

fn char_to_evdev(c: char) -> u32 {
    match c {
        'a'..='z' => (c as u32) - ('a' as u32) + 30,
        '1'..='9' => (c as u32) - ('1' as u32) + 2,
        '0' => 11,
        ' ' => 57,
        _ => 0,
    }
}

#[allow(dead_code)]
fn mods_to_xkb(mods: &Modifiers) -> u32 {
    let mut m = 0u32;
    if mods.shift { m |= 1; }
    if mods.caps_lock { m |= 2; }
    if mods.ctrl { m |= 4; }
    if mods.alt { m |= 8; }
    if mods.super_key { m |= 1 << 26; }
    m
}
