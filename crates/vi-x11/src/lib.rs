//! Deprecated: use the vime-* equivalent crate instead.
//! X11 XIM (X Input Method) backend.
//!
//! Implements `ImeBackend` by acting as an XIM server, following the
//! X11 Input Method Protocol (XIM).  Applications that set `XMODIFIERS=@im=vi`
//! will connect to this server for key event processing and text commit.
//!
//! # Protocol overview
//! XIM uses X11 `ClientMessage` events and window properties for IPC:
//! 1. The XIM server registers itself on the root window's `XIM_SERVERS`
//!    property.
//! 2. Clients discover the server and open a connection.
//! 3. Input contexts (IC) are created per text-entry widget.
//! 4. Key events are forwarded by the client via `XIM_FORWARD_EVENT`.
//! 5. The server sends committed text via `XIM_COMMIT` (XLookupChars).
//!
//! # Preedit styles
//! Supports `OverTheSpot` and `Root` preedit styles.  In `OverTheSpot` mode
//! the client renders the preedit string inside the application window at the
//! cursor position.  In `Root` mode the server owns a dedicated preedit window.
//!
//! # Focus
//! XIM focus (`XIM_SET_IC_FOCUS`) is separate from X keyboard focus.  Some
//! window managers use focus-follows-mouse, so the last `XIM_SET_IC_FOCUS`
//! call wins regardless of which window has the X keyboard focus.

pub mod fullscreen;
pub mod wine;

use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex},
};

use x11rb::protocol::randr;

use tokio::sync::mpsc;
use tracing::{debug, info, warn};
use vi_core::{InputEvent, Key, Modifiers, NfcString, PreeditText};
use vi_platform::{Capabilities, CharCursor, ImeBackend, PlatformError, Result, SurroundingText};
use x11rb::{
    connection::Connection as _,
    protocol::{
        xproto::{
            self, Atom, ClientMessageData, ClientMessageEvent, ConnectionExt as _, EventMask,
            KeyPressEvent, Window,
        },
        Event,
    },
    rust_connection::RustConnection,
    wrapper::ConnectionExt as _,
    COPY_DEPTH_FROM_PARENT,
};

// ── XIM opcode constants ──────────────────────────────────────────────────────

const XIM_CONNECT: u8 = 1;
const XIM_CONNECT_REPLY: u8 = 2;
const XIM_DISCONNECT: u8 = 5;
const XIM_OPEN: u8 = 30;
const XIM_OPEN_REPLY: u8 = 31;
const XIM_CLOSE: u8 = 32;
const XIM_CREATE_IC: u8 = 50;
const XIM_CREATE_IC_REPLY: u8 = 51;
const XIM_DESTROY_IC: u8 = 52;
const XIM_SET_IC_FOCUS: u8 = 55;
const XIM_UNSET_IC_FOCUS: u8 = 56;
const XIM_FORWARD_EVENT: u8 = 60;
const XIM_SYNC_REPLY: u8 = 62;
const XIM_COMMIT: u8 = 65;
const XIM_SET_IC_VALUES: u8 = 58;

/// IC attribute ID assigned by this server for XNSurroundingText.
///
/// Clients learn this ID from the IC attribute list in XIM_OPEN_REPLY.  When
/// a client sends XIM_SET_IC_VALUES with this ID, the value is a packed
/// surrounding-text record: `text_len(u32 LE) + text_bytes + pad_to_4 + cursor(u32 LE)`.
const XIM_ATTR_SURROUNDING_TEXT: u16 = 2;

/// XIM uses a magic atom name for server registration.
const XIM_SERVER_ATOM_PREFIX: &str = "@server=vi";

// ── Monitor geometry ──────────────────────────────────────────────────────────

/// Display metrics for a connected X11 monitor (from RandR).
#[derive(Debug, Clone)]
pub struct MonitorInfo {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    /// Scale factor; X11 does not expose per-monitor integer scale via RandR
    /// monitors directly, so this defaults to 1.0.
    pub scale_factor: f32,
}

// ── Input context ─────────────────────────────────────────────────────────────

#[derive(Debug, Default)]
#[allow(dead_code)]
struct InputContext {
    /// X11 window that owns this IC.
    client_win: Window,
    /// Whether this IC currently has XIM focus.
    focused: bool,
    /// Preedit style negotiated at IC creation.
    preedit_style: PreeditStyle,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
enum PreeditStyle {
    #[default]
    Root,
    OverTheSpot,
}

// ── Shared state ──────────────────────────────────────────────────────────────

#[derive(Debug, Default)]
struct SharedState {
    surrounding: Option<SurroundingText>,
    /// IC that currently has XIM focus.
    focused_ic: Option<u16>,
    preedit: String,
    preedit_cursor: usize,
    shadow_buf: String,
    /// Committed text pending delivery to the focused IC.
    pending_commit: Option<String>,
    modifier_state: Modifiers,
    /// Cursor position forwarded via XIM_SET_IC_VALUES XNSpotLocation.
    cursor_x: i32,
    cursor_y: i32,
    /// Focused window belongs to a Wine/Proton/Steam process.
    is_wine_window: bool,
    /// Focused window is in fullscreen mode; suppress preedit accumulation.
    fullscreen_mode: bool,
}

// ── XIM attribute parsing ─────────────────────────────────────────────────────

/// Parse the value bytes of an XNSurroundingText IC attribute.
///
/// Expected layout (little-endian):
/// ```text
/// [0..4]   text_len  : u32   – byte count of the UTF-8 text
/// [4..]    text      : [u8]  – UTF-8 encoded surrounding text (text_len bytes)
/// [pad]               align to next 4-byte boundary
/// [+4]     cursor    : u32   – byte offset of the insertion cursor in `text`
/// ```
///
/// Returns `None` if the bytes are too short, the text is not valid UTF-8, or
/// `cursor` is out of bounds / not on a codepoint boundary.
fn parse_surrounding_text_attr(value: &[u8]) -> Option<(String, usize)> {
    if value.len() < 8 {
        return None;
    }
    let text_len = u32::from_le_bytes([value[0], value[1], value[2], value[3]]) as usize;
    let text_end = 4usize.checked_add(text_len)?;
    if text_end > value.len() {
        return None;
    }
    let text = String::from_utf8(value[4..text_end].to_vec()).ok()?;
    // Align cursor field to the next 4-byte boundary after the text.
    let cursor_start = (text_end + 3) & !3;
    let cursor_end = cursor_start.checked_add(4)?;
    if cursor_end > value.len() {
        return None;
    }
    let cursor = u32::from_le_bytes([
        value[cursor_start],
        value[cursor_start + 1],
        value[cursor_start + 2],
        value[cursor_start + 3],
    ]) as usize;
    if cursor > text.len() || !text.is_char_boundary(cursor) {
        return None;
    }
    Some((text, cursor))
}

// ── XIM server state ──────────────────────────────────────────────────────────

#[allow(dead_code)]
struct XimServer {
    conn: Arc<RustConnection>,
    screen_num: usize,
    server_win: Window,
    /// Atoms resolved at startup.
    xim_servers_atom: Atom,
    xim_proto_atom: Atom,
    xim_server_atom: Atom,
    /// `im_id` → `ic_id` → InputContext
    contexts: HashMap<u16, HashMap<u16, InputContext>>,
    next_im_id: u16,
    next_ic_id: u16,
    shared: Arc<Mutex<SharedState>>,
    tx: mpsc::Sender<InputEvent>,
    /// `(ic_id, keycode)` pairs that have a recorded key-press but no
    /// matching key-release yet.  Used to detect ghost releases (XKeyRelease
    /// without a prior XKeyPress) and to suppress them.
    pressed_keys: HashSet<(u16, u8)>,
    /// Connected monitors; rebuilt on RandR ScreenChangeNotify.
    monitor_map: Arc<Mutex<Vec<MonitorInfo>>>,
    /// XKB state for layout-aware key symbol resolution.
    xkb_state: xkbcommon::xkb::State,
    /// Wine/Proton commit bridge.
    wine_bridge: wine::WineXimBridge,
    /// Fullscreen state tracker.
    fullscreen_monitor: fullscreen::FullscreenMonitor,
}

impl XimServer {
    fn new(
        conn: Arc<RustConnection>,
        screen_num: usize,
        shared: Arc<Mutex<SharedState>>,
        monitor_map: Arc<Mutex<Vec<MonitorInfo>>>,
        tx: mpsc::Sender<InputEvent>,
    ) -> Result<Self> {
        let root = conn.setup().roots[screen_num].root;

        // Create the server selection window.
        let server_win = conn
            .generate_id()
            .map_err(|e| PlatformError::X11(e.to_string()))?;
        conn.create_window(
            COPY_DEPTH_FROM_PARENT,
            server_win,
            root,
            0,
            0,
            1,
            1,
            0,
            xproto::WindowClass::INPUT_ONLY,
            0,
            &xproto::CreateWindowAux::new(),
        )
        .map_err(|e| PlatformError::X11(e.to_string()))?
        .check()
        .map_err(|e| PlatformError::X11(e.to_string()))?;

        // Intern XIM atoms.
        let xim_servers_atom = intern_atom(&conn, "XIM_SERVERS")?;
        let xim_proto_atom = intern_atom(&conn, "_XIM_PROTOCOL")?;
        let xim_server_atom = intern_atom(&conn, XIM_SERVER_ATOM_PREFIX)?;

        // Announce ourselves on the root window's XIM_SERVERS property.
        conn.change_property32(
            xproto::PropMode::PREPEND,
            root,
            xim_servers_atom,
            xproto::AtomEnum::ATOM,
            &[xim_server_atom],
        )
        .map_err(|e| PlatformError::X11(e.to_string()))?
        .check()
        .map_err(|e| PlatformError::X11(e.to_string()))?;

        // Select ClientMessage events on our server window.
        conn.change_window_attributes(
            server_win,
            &xproto::ChangeWindowAttributesAux::new().event_mask(EventMask::NO_EVENT),
        )
        .map_err(|e| PlatformError::X11(e.to_string()))?;

        conn.flush()
            .map_err(|e| PlatformError::X11(e.to_string()))?;
        info!("X11 XIM: server window={server_win:#x}");

        // Initialise RandR: query version, build initial monitor map, and
        // subscribe to ScreenChangeNotify so hot-plug events refresh the map.
        init_randr(&conn, root, screen_num, &monitor_map);

        let xkb_ctx = xkbcommon::xkb::Context::new(xkbcommon::xkb::CONTEXT_NO_FLAGS);
        let xkb_keymap = xkbcommon::xkb::Keymap::new_from_names(
            &xkb_ctx,
            "",
            "",
            "",
            "",
            None,
            xkbcommon::xkb::KEYMAP_COMPILE_NO_FLAGS,
        )
        .ok_or_else(|| PlatformError::X11("xkb keymap init failed".into()))?;
        let xkb_state = xkbcommon::xkb::State::new(&xkb_keymap);

        let net_wm_pid = intern_atom(&conn, "_NET_WM_PID")?;
        let net_wm_state = intern_atom(&conn, "_NET_WM_STATE")?;
        let net_wm_state_fullscreen = intern_atom(&conn, "_NET_WM_STATE_FULLSCREEN")?;

        let wine_bridge = wine::WineXimBridge::new(Arc::clone(&conn), net_wm_pid);
        let fullscreen_monitor = fullscreen::FullscreenMonitor::new(
            Arc::clone(&conn),
            net_wm_state,
            net_wm_state_fullscreen,
        );
        // Observe _NET_WM_STATE changes on the root window.
        let _ = fullscreen_monitor.subscribe_root(root);

        Ok(XimServer {
            conn,
            screen_num,
            server_win,
            xim_servers_atom,
            xim_proto_atom,
            xim_server_atom,
            contexts: HashMap::new(),
            next_im_id: 1,
            next_ic_id: 1,
            shared,
            tx,
            pressed_keys: HashSet::new(),
            monitor_map,
            xkb_state,
            wine_bridge,
            fullscreen_monitor,
        })
    }

    fn focused_client_window(&self) -> Option<Window> {
        let focused_ic = self
            .shared
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .focused_ic?;
        self.contexts
            .values()
            .find_map(|ics| ics.get(&focused_ic))
            .map(|ic| ic.client_win)
    }

    /// Process one X11 event.  Returns `false` if the server should stop.
    fn handle_event(&mut self, event: &Event) -> bool {
        match event {
            Event::ClientMessage(cm) => {
                self.handle_client_message(cm);
            }
            Event::DestroyNotify(dn) => {
                // A client window was destroyed; clean up its ICs.
                self.remove_client_window(dn.window);
            }
            Event::RandrScreenChangeNotify(_) => {
                // Monitor configuration changed; rebuild the map.
                let root = self.conn.setup().roots[self.screen_num].root;
                let new_map = build_monitor_map(&self.conn, root);
                *self.monitor_map.lock().unwrap_or_else(|e| e.into_inner()) = new_map;
                debug!("X11 RandR: monitor map refreshed");
            }
            Event::PropertyNotify(pn) if pn.atom == self.fullscreen_monitor.net_wm_state => {
                // Refresh fullscreen state when _NET_WM_STATE changes on the
                // focused window or on the root (some WMs notify via root).
                if let Some(win) = self.focused_client_window() {
                    let is_fs = self.fullscreen_monitor.is_fullscreen(win);
                    self.shared
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .fullscreen_mode = is_fs;
                }
            }
            _ => {}
        }
        true
    }

    fn handle_client_message(&mut self, cm: &ClientMessageEvent) {
        if cm.type_ != self.xim_proto_atom {
            return;
        }
        if cm.format != 8 {
            return;
        }
        let data = cm.data.as_data8();
        if data.len() < 4 {
            return;
        }
        let major_opcode = data[0];
        let _minor_opcode = data[1];

        debug!("XIM: opcode={major_opcode}");
        match major_opcode {
            XIM_CONNECT => self.send_connect_reply(cm.window),
            XIM_DISCONNECT => {
                self.remove_client_window(cm.window);
            }
            XIM_OPEN => {
                let im_id = self.next_im_id;
                self.next_im_id = self.next_im_id.wrapping_add(1);
                self.contexts.insert(im_id, HashMap::new());
                self.send_open_reply(cm.window, im_id);
            }
            XIM_CLOSE => {
                let im_id = u16::from_le_bytes([data[4], data[5]]);
                self.contexts.remove(&im_id);
            }
            XIM_CREATE_IC => {
                let im_id = u16::from_le_bytes([data[4], data[5]]);
                let ic_id = self.next_ic_id;
                self.next_ic_id = self.next_ic_id.wrapping_add(1);

                // Parse IC attributes from the CREATE_IC request to detect the
                // client's requested input style.  Attributes start at byte 8
                // (after major_opcode, minor_opcode, length, im_id, padding).
                // XNInputStyle is a u32 bitmask; bit 0x0008 = XIMPreeditPosition
                // (OverTheSpot).  Without proper attribute-ID negotiation in
                // XIM_OPEN_REPLY we cannot map attribute IDs reliably, so we
                // inspect every 4-byte attribute and check its style bits.
                let mut preedit_style = PreeditStyle::OverTheSpot;
                {
                    let mut off = 8usize;
                    while off + 4 <= data.len() {
                        let attr_len = u16::from_le_bytes([data[off + 2], data[off + 3]]) as usize;
                        off += 4;
                        if off + attr_len > data.len() {
                            break;
                        }
                        if attr_len == 4 {
                            let v = u32::from_le_bytes([
                                data[off],
                                data[off + 1],
                                data[off + 2],
                                data[off + 3],
                            ]);
                            // XIMPreeditRoot = 0x0004; if client explicitly requests Root
                            // and does NOT request OverTheSpot (0x0008), honour it.
                            if v & 0x0004 != 0 && v & 0x0008 == 0 {
                                preedit_style = PreeditStyle::Root;
                            }
                        }
                        off += (attr_len + 3) & !3;
                    }
                }

                debug!("XIM: CREATE_IC im={im_id} ic={ic_id} style={preedit_style:?}");
                if let Some(ics) = self.contexts.get_mut(&im_id) {
                    ics.insert(
                        ic_id,
                        InputContext {
                            client_win: cm.window,
                            focused: false,
                            preedit_style,
                        },
                    );
                }
                self.send_create_ic_reply(cm.window, im_id, ic_id);
            }
            XIM_DESTROY_IC => {
                let im_id = u16::from_le_bytes([data[4], data[5]]);
                let ic_id = u16::from_le_bytes([data[6], data[7]]);
                if let Some(ics) = self.contexts.get_mut(&im_id) {
                    ics.remove(&ic_id);
                }
            }
            XIM_SET_IC_FOCUS => {
                let im_id = u16::from_le_bytes([data[4], data[5]]);
                let ic_id = u16::from_le_bytes([data[6], data[7]]);
                // Detect Wine/Proton and fullscreen for the newly focused window.
                let client_win = self
                    .contexts
                    .get(&im_id)
                    .and_then(|ics| ics.get(&ic_id))
                    .map(|ic| ic.client_win);
                let (is_wine, is_fs) = if let Some(win) = client_win {
                    let w = self.wine_bridge.is_wine_window(win);
                    let fs = self.fullscreen_monitor.is_fullscreen(win);
                    if w {
                        debug!("XIM: IC {ic_id} is a Wine/Proton window");
                    }
                    if fs {
                        debug!("XIM: IC {ic_id} is fullscreen");
                    }
                    // Subscribe to PropertyNotify on this window so future
                    // fullscreen transitions are picked up.
                    let _ = self.fullscreen_monitor.subscribe_window(win);
                    (w, fs)
                } else {
                    (false, false)
                };
                let mut s = self.shared.lock().unwrap_or_else(|e| e.into_inner());
                s.focused_ic = Some(ic_id);
                s.is_wine_window = is_wine;
                s.fullscreen_mode = is_fs;
                s.shadow_buf.clear();
            }
            XIM_UNSET_IC_FOCUS => {
                let _im_id = u16::from_le_bytes([data[4], data[5]]);
                let ic_id = u16::from_le_bytes([data[6], data[7]]);
                let mut s = self.shared.lock().unwrap_or_else(|e| e.into_inner());
                if s.focused_ic == Some(ic_id) {
                    s.focused_ic = None;
                    s.is_wine_window = false;
                    s.fullscreen_mode = false;
                }
            }
            XIM_FORWARD_EVENT => {
                debug!("XIM: ForwardEvent from client");
                if data.len() < 16 {
                    return;
                }
                let im_id = u16::from_le_bytes([data[4], data[5]]);
                let ic_id = u16::from_le_bytes([data[6], data[7]]);
                let flag = u16::from_le_bytes([data[8], data[9]]);
                let sync = (flag & 1) != 0;
                // XIM protocol: bit 1 of flag is set when the client has marked
                // this event as a key-auto-repeat event.
                let is_key_repeat = (flag & 2) != 0;
                // Embedded XKeyEvent: type at offset 12, keycode (detail) at 13.
                let event_type = data[12];
                let keycode = data[13];
                let pressed = event_type == 2; // KeyPress = 2
                let released = event_type == 3; // KeyRelease = 3

                // Ghost-key guard: track (ic_id, keycode) pairs.
                // * For presses, record the pair.
                // * For releases, verify a matching press exists; discard ghost
                //   releases that have no recorded press (these arise from driver
                //   bugs or race conditions on focus transfer).
                let ic_key = (ic_id, keycode);
                if released {
                    if !self.pressed_keys.remove(&ic_key) {
                        // Ghost release: no prior press on this IC.  Forward to
                        // the application (it may need it for state consistency)
                        // but do not feed it to the engine.
                        if sync {
                            send_sync_reply(
                                &self.conn,
                                self.xim_proto_atom,
                                cm.window,
                                im_id,
                                ic_id,
                            );
                        }
                        forward_event_passthrough(
                            &self.conn,
                            self.xim_proto_atom,
                            cm.window,
                            im_id,
                            ic_id,
                            &data,
                        );
                        // Skip the pending-commit flush that follows.
                        return self.flush_pending_commit(cm.window);
                    }
                } else if pressed {
                    self.pressed_keys.insert(ic_key);
                }

                // Update modifier state for modifier keys before mapping.
                update_modifier_state(keycode, pressed, &self.shared, &mut self.xkb_state);

                // Key-repeat filter: XIM_FORWARD_EVENT with the XKeyRepeat flag
                // set must not be fed into the engine a second time (the engine
                // has already processed the initial press).  Forward the event to
                // the application so it can handle its own repeat logic.
                if is_key_repeat && pressed {
                    if sync {
                        send_sync_reply(&self.conn, self.xim_proto_atom, cm.window, im_id, ic_id);
                    }
                    forward_event_passthrough(
                        &self.conn,
                        self.xim_proto_atom,
                        cm.window,
                        im_id,
                        ic_id,
                        &data,
                    );
                    return self.flush_pending_commit(cm.window);
                }

                let consumed = {
                    let xkb_keycode = xkbcommon::xkb::Keycode::new(keycode as u32);
                    let keysym = self.xkb_state.key_get_one_sym(xkb_keycode);
                    let s = self.shared.lock().unwrap_or_else(|e| e.into_inner());
                    let mods = s.modifier_state;
                    if let Some(ev) = keysym_to_key(keysym.raw()).map(|k| {
                        if pressed {
                            InputEvent::KeyDown(k, mods)
                        } else {
                            InputEvent::KeyUp(k)
                        }
                    }) {
                        if !mods.ctrl {
                            let _ = self.tx.try_send(ev);
                            true
                        } else {
                            false
                        }
                    } else {
                        false
                    }
                };
                if sync || consumed {
                    send_sync_reply(&self.conn, self.xim_proto_atom, cm.window, im_id, ic_id);
                }
                if !consumed {
                    forward_event_passthrough(
                        &self.conn,
                        self.xim_proto_atom,
                        cm.window,
                        im_id,
                        ic_id,
                        &data,
                    );
                }
            }
            XIM_SET_IC_VALUES => {
                // Scan the full IC attribute list.
                // The attribute list starts at byte 8 after the XIM header.
                // Each entry: attr_id(u16) + attr_len(u16) + value([u8; attr_len]).
                let mut offset = 8usize;
                while offset + 4 <= data.len() {
                    let attr_id = u16::from_le_bytes([data[offset], data[offset + 1]]);
                    let attr_len =
                        u16::from_le_bytes([data[offset + 2], data[offset + 3]]) as usize;
                    offset += 4;
                    if offset + attr_len > data.len() {
                        break;
                    }
                    let value = &data[offset..offset + attr_len];
                    if attr_len == 4 {
                        // XNSpotLocation: XPoint { x: i16, y: i16 }
                        let cx = i16::from_le_bytes([value[0], value[1]]) as i32;
                        let cy = i16::from_le_bytes([value[2], value[3]]) as i32;
                        debug!("XIM: SET_IC_VALUES attr_id={attr_id} spot=({cx},{cy})");
                        let mut s = self.shared.lock().unwrap_or_else(|e| e.into_inner());
                        s.cursor_x = cx;
                        s.cursor_y = cy;
                    } else if attr_id == XIM_ATTR_SURROUNDING_TEXT {
                        if let Some((text, cursor)) = parse_surrounding_text_attr(value) {
                            debug!(
                                "XIM: SET_IC_VALUES surrounding text len={} cursor={cursor}",
                                text.len()
                            );
                            let surr = SurroundingText {
                                text: text.clone(),
                                cursor_pos: cursor as u32,
                                anchor_pos: cursor as u32,
                            };
                            self.shared
                                .lock()
                                .unwrap_or_else(|e| e.into_inner())
                                .surrounding = Some(surr);
                            let _ = self
                                .tx
                                .try_send(InputEvent::SurroundingText { text, cursor });
                        }
                    }
                    // Advance past this attribute value (4-byte aligned).
                    offset += (attr_len + 3) & !3;
                }
            }
            _ => {
                debug!("XIM: unknown opcode {major_opcode}");
            }
        }

        // Flush any pending commit text.
        self.flush_pending_commit(cm.window);
    }

    /// Deliver any pending commit text to the client window.
    fn flush_pending_commit(&mut self, win: Window) {
        let (pending, is_wine) = {
            let mut s = self.shared.lock().unwrap_or_else(|e| e.into_inner());
            (s.pending_commit.take(), s.is_wine_window)
        };
        if let Some(text) = pending {
            if is_wine {
                let root = self.conn.setup().roots[self.screen_num].root;
                let _ = self.wine_bridge.commit(win, root, &text);
            } else {
                self.send_commit(win, &text);
            }
        }
    }

    fn remove_client_window(&mut self, win: Window) {
        self.contexts.retain(|_, ics| {
            ics.retain(|_, ic| ic.client_win != win);
            !ics.is_empty()
        });
    }

    // ── XIM reply helpers ─────────────────────────────────────────────────────

    fn send_client_message(&self, dest: Window, data: [u8; 20]) {
        let ev = ClientMessageEvent {
            response_type: xproto::CLIENT_MESSAGE_EVENT,
            format: 8,
            sequence: 0,
            window: dest,
            type_: self.xim_proto_atom,
            data: ClientMessageData::from(data),
        };
        let _ = self.conn.send_event(false, dest, EventMask::NO_EVENT, ev);
        let _ = self.conn.flush();
    }

    fn send_connect_reply(&self, dest: Window) {
        let mut buf = [0u8; 20];
        buf[0] = XIM_CONNECT_REPLY;
        // major/minor version: 1.0
        buf[4] = 1;
        buf[6] = 0;
        self.send_client_message(dest, buf);
    }

    fn send_open_reply(&self, dest: Window, im_id: u16) {
        let mut buf = [0u8; 20];
        buf[0] = XIM_OPEN_REPLY;
        let id_bytes = im_id.to_le_bytes();
        buf[4] = id_bytes[0];
        buf[5] = id_bytes[1];
        self.send_client_message(dest, buf);
    }

    fn send_create_ic_reply(&self, dest: Window, im_id: u16, ic_id: u16) {
        let mut buf = [0u8; 20];
        buf[0] = XIM_CREATE_IC_REPLY;
        let im_bytes = im_id.to_le_bytes();
        buf[4] = im_bytes[0];
        buf[5] = im_bytes[1];
        let ic_bytes = ic_id.to_le_bytes();
        buf[6] = ic_bytes[0];
        buf[7] = ic_bytes[1];
        self.send_client_message(dest, buf);
    }

    fn send_commit(&self, dest: Window, text: &str) {
        if text.len() > 14 {
            self.send_commit_long(dest, text);
            return;
        }
        // XIM_COMMIT with XLookupChars flag (0x0002).
        let bytes = text.as_bytes();
        let len = bytes.len() as u8;
        let mut buf = [0u8; 20];
        buf[0] = XIM_COMMIT;
        buf[2] = 0x02; // XLookupChars
        buf[4] = len;
        buf[5..5 + len as usize].copy_from_slice(bytes);
        self.send_client_message(dest, buf);
    }

    /// Property-based XIM_COMMIT for strings that exceed the 14-byte Data8 limit.
    /// Writes the XIM message to a property on `dest` and sends a format-32
    /// ClientMessage pointing to it, per the XIM property-transport convention.
    fn send_commit_long(&self, dest: Window, text: &str) {
        let bytes = text.as_bytes();
        let n = bytes.len() as u16;
        // Build the XIM_COMMIT payload: 4-byte header + XLookupChars(u16) +
        // string-length(u16) + string bytes, padded to 4-byte boundary.
        let body_len = 4u16 + n; // flag(2) + strlen(2) + str
        let units = body_len.div_ceil(4);
        let mut payload = Vec::with_capacity(4 + body_len as usize);
        payload.push(XIM_COMMIT);
        payload.push(0u8); // minor opcode
        payload.extend_from_slice(&units.to_le_bytes());
        payload.extend_from_slice(&0x0002u16.to_le_bytes()); // XLookupChars
        payload.extend_from_slice(&n.to_le_bytes());
        payload.extend_from_slice(bytes);
        while payload.len() % 4 != 0 {
            payload.push(0);
        }
        let _ = self.conn.change_property8(
            xproto::PropMode::REPLACE,
            dest,
            self.xim_proto_atom,
            xproto::AtomEnum::STRING,
            &payload,
        );
        let _ = self.conn.flush();
        let total = payload.len() as u32;
        let ev = ClientMessageEvent {
            response_type: xproto::CLIENT_MESSAGE_EVENT,
            format: 32,
            sequence: 0,
            window: dest,
            type_: self.xim_proto_atom,
            data: ClientMessageData::from([total, self.xim_proto_atom, 0, 0, 0]),
        };
        let _ = self.conn.send_event(false, dest, EventMask::NO_EVENT, ev);
        let _ = self.conn.flush();
    }
}

// ── XIM protocol helpers ──────────────────────────────────────────────────────

fn send_sync_reply(
    conn: &RustConnection,
    xim_proto_atom: Atom,
    dest: Window,
    im_id: u16,
    ic_id: u16,
) {
    let mut buf = [0u8; 20];
    buf[0] = XIM_SYNC_REPLY;
    let im_bytes = im_id.to_le_bytes();
    buf[4] = im_bytes[0];
    buf[5] = im_bytes[1];
    let ic_bytes = ic_id.to_le_bytes();
    buf[6] = ic_bytes[0];
    buf[7] = ic_bytes[1];
    let ev = ClientMessageEvent {
        response_type: xproto::CLIENT_MESSAGE_EVENT,
        format: 8,
        sequence: 0,
        window: dest,
        type_: xim_proto_atom,
        data: ClientMessageData::from(buf),
    };
    let _ = conn.send_event(false, dest, EventMask::NO_EVENT, ev);
    let _ = conn.flush();
}

/// Send the unmodified XIM_FORWARD_EVENT payload back to the client so the
/// application can process the key event itself (passthrough / not consumed).
fn forward_event_passthrough(
    conn: &RustConnection,
    xim_proto_atom: Atom,
    dest: Window,
    im_id: u16,
    ic_id: u16,
    data: &[u8],
) {
    // Re-send the original Data8 payload.  Truncate or zero-pad to 20 bytes.
    let mut buf = [0u8; 20];
    let copy_len = data.len().min(20);
    buf[..copy_len].copy_from_slice(&data[..copy_len]);
    // Rewrite IM/IC ids in case they differ (they shouldn't).
    let im_bytes = im_id.to_le_bytes();
    buf[4] = im_bytes[0];
    buf[5] = im_bytes[1];
    let ic_bytes = ic_id.to_le_bytes();
    buf[6] = ic_bytes[0];
    buf[7] = ic_bytes[1];
    let ev = ClientMessageEvent {
        response_type: xproto::CLIENT_MESSAGE_EVENT,
        format: 8,
        sequence: 0,
        window: dest,
        type_: xim_proto_atom,
        data: ClientMessageData::from(buf),
    };
    let _ = conn.send_event(false, dest, EventMask::NO_EVENT, ev);
    let _ = conn.flush();
}

// ── Key mapping ───────────────────────────────────────────────────────────────

fn keysym_to_key(keysym: u32) -> Option<Key> {
    use xkbcommon::xkb::keysyms;
    match keysym {
        keysyms::KEY_Escape => return Some(Key::Escape),
        keysyms::KEY_BackSpace => return Some(Key::Backspace),
        keysyms::KEY_Tab => return Some(Key::Tab),
        keysyms::KEY_Return => return Some(Key::Return),
        keysyms::KEY_Home => return Some(Key::Home),
        keysyms::KEY_End => return Some(Key::End),
        keysyms::KEY_Up => return Some(Key::Up),
        keysyms::KEY_Down => return Some(Key::Down),
        keysyms::KEY_Left => return Some(Key::Left),
        keysyms::KEY_Right => return Some(Key::Right),
        keysyms::KEY_Delete => return Some(Key::Delete),
        _ => {}
    }
    let codepoint = xkbcommon::xkb::keysym_to_utf32(xkbcommon::xkb::Keysym::from(keysym));
    if codepoint > 0x1f && codepoint != 0x7f {
        char::from_u32(codepoint).map(Key::Char)
    } else {
        None
    }
}

/// Map a logical `Key` to an X11 keycode (US QWERTY layout).
fn map_key_to_x11_keycode(key: &Key) -> u8 {
    match key {
        Key::Char(c) => char_to_x11_keycode(*c),
        Key::Escape => 9,
        Key::Backspace => 22,
        Key::Tab => 23,
        Key::Return => 36,
        Key::Home => 110,
        Key::Up => 111,
        Key::Left => 113,
        Key::Right => 114,
        Key::End => 115,
        Key::Down => 116,
        Key::Delete => 119,
        Key::Keysym(_) | Key::DeadKey(_) | Key::ComposeKey => 0,
    }
}

pub(crate) fn char_to_x11_keycode(c: char) -> u8 {
    match c.to_ascii_lowercase() {
        '1' => 10,
        '2' => 11,
        '3' => 12,
        '4' => 13,
        '5' => 14,
        '6' => 15,
        '7' => 16,
        '8' => 17,
        '9' => 18,
        '0' => 19,
        'q' => 24,
        'w' => 25,
        'e' => 26,
        'r' => 27,
        't' => 28,
        'y' => 29,
        'u' => 30,
        'i' => 31,
        'o' => 32,
        'p' => 33,
        'a' => 38,
        's' => 39,
        'd' => 40,
        'f' => 41,
        'g' => 42,
        'h' => 43,
        'j' => 44,
        'k' => 45,
        'l' => 46,
        'z' => 52,
        'x' => 53,
        'c' => 54,
        'v' => 55,
        'b' => 56,
        'n' => 57,
        'm' => 58,
        ' ' => 65,
        _ => 0,
    }
}

fn mods_to_x11_state(mods: Modifiers) -> u16 {
    let mut s = 0u16;
    if mods.shift {
        s |= 0x0001;
    } // ShiftMask
    if mods.caps_lock {
        s |= 0x0002;
    } // LockMask
    if mods.ctrl {
        s |= 0x0004;
    } // ControlMask
    if mods.alt {
        s |= 0x0008;
    } // Mod1Mask
    if mods.super_key {
        s |= 0x0040;
    } // Mod4Mask
    s
}

/// Update the shared modifier state when a modifier keycode is pressed/released.
fn update_modifier_state(
    keycode: u8,
    pressed: bool,
    shared: &Arc<Mutex<SharedState>>,
    xkb_state: &mut xkbcommon::xkb::State,
) {
    let direction = if pressed {
        xkbcommon::xkb::KeyDirection::Down
    } else {
        xkbcommon::xkb::KeyDirection::Up
    };
    xkb_state.update_key(xkbcommon::xkb::Keycode::new(keycode as u32), direction);
    let mut s = shared.lock().unwrap_or_else(|e| e.into_inner());
    match keycode {
        50 | 62 => s.modifier_state.shift = pressed, // Shift L/R
        37 | 105 => s.modifier_state.ctrl = pressed, // Ctrl L/R
        64 | 108 => s.modifier_state.alt = pressed,  // Alt L/R
        66 => s.modifier_state.caps_lock = pressed,  // CapsLock
        133 | 134 => s.modifier_state.super_key = pressed, // Super L/R
        _ => {}
    }
}

/// Build a press+release pair of synthetic XKeyEvents (for testing and forward_key).
pub(crate) fn build_key_events(
    keycode: u8,
    state: u16,
    client_win: Window,
    root_win: Window,
) -> [KeyPressEvent; 2] {
    let press = KeyPressEvent {
        response_type: xproto::KEY_PRESS_EVENT,
        detail: keycode,
        sequence: 0,
        time: 0,
        root: root_win,
        event: client_win,
        child: 0,
        root_x: 0,
        root_y: 0,
        event_x: 0,
        event_y: 0,
        state: state.into(),
        same_screen: true,
    };
    let mut release = press;
    release.response_type = xproto::KEY_RELEASE_EVENT;
    [press, release]
}

// ── RandR helpers ─────────────────────────────────────────────────────────────

/// Query RandR, subscribe to ScreenChangeNotify, and seed `monitor_map`.
/// Any failure is logged and silently ignored; the caller keeps a fallback.
fn init_randr(
    conn: &RustConnection,
    root: Window,
    screen_num: usize,
    monitor_map: &Arc<Mutex<Vec<MonitorInfo>>>,
) {
    use x11rb::protocol::randr::ConnectionExt as _;

    // Negotiate RandR version (1.5 added GetMonitors).  Convert both
    // ConnectionError and ReplyError to Option to avoid type-mismatch.
    let version_ok = conn
        .randr_query_version(1, 5)
        .ok()
        .and_then(|c| c.reply().ok())
        .map(|r| r.major_version > 1 || (r.major_version == 1 && r.minor_version >= 5))
        .unwrap_or(false);

    if !version_ok {
        warn!("X11 RandR 1.5 not available; multi-monitor placement disabled");
        return;
    }

    // Subscribe to screen change events so hot-plug is handled.
    if conn
        .randr_select_input(root, randr::NotifyMask::SCREEN_CHANGE)
        .ok()
        .and_then(|c| c.check().ok())
        .is_none()
    {
        warn!("X11 RandR: randr_select_input failed");
    }

    *monitor_map.lock().unwrap_or_else(|e| e.into_inner()) = build_monitor_map(conn, root);

    let count = monitor_map.lock().unwrap_or_else(|e| e.into_inner()).len();
    info!("X11 RandR: found {count} monitor(s) on screen {screen_num}");
}

/// Build a `Vec<MonitorInfo>` from RandR 1.5 GetMonitors.
/// Falls back to a single monitor covering the full screen on error.
fn build_monitor_map(conn: &RustConnection, root: Window) -> Vec<MonitorInfo> {
    use x11rb::protocol::randr::ConnectionExt as _;

    // Convert both ConnectionError and ReplyError to Option so we can chain
    // without a type-mismatch between the two distinct error types.
    let monitors_opt = conn
        .randr_get_monitors(root, true)
        .ok()
        .and_then(|c| c.reply().ok());

    if let Some(reply) = monitors_opt {
        if !reply.monitors.is_empty() {
            return reply
                .monitors
                .iter()
                .map(|m: &randr::MonitorInfo| MonitorInfo {
                    x: m.x as i32,
                    y: m.y as i32,
                    width: m.width as i32,
                    height: m.height as i32,
                    scale_factor: 1.0,
                })
                .collect();
        }
    }

    // Fallback: single monitor covering the full screen geometry.
    let screen = &conn.setup().roots[0];
    vec![MonitorInfo {
        x: 0,
        y: 0,
        width: screen.width_in_pixels as i32,
        height: screen.height_in_pixels as i32,
        scale_factor: 1.0,
    }]
}

// ── Connection helpers ────────────────────────────────────────────────────────

fn intern_atom(conn: &RustConnection, name: &str) -> Result<Atom> {
    conn.intern_atom(false, name.as_bytes())
        .map_err(|e| PlatformError::X11(e.to_string()))?
        .reply()
        .map(|r| r.atom)
        .map_err(|e| PlatformError::X11(e.to_string()))
}

// ── Public backend ────────────────────────────────────────────────────────────

/// An XIM server backend implementing `ImeBackend`.
pub struct X11Backend {
    #[allow(dead_code)]
    rt: tokio::runtime::Handle,
    shared: Arc<Mutex<SharedState>>,
    /// Connection clone for outgoing requests from ImeBackend methods.
    conn: Arc<RustConnection>,
    /// The destination window for commit messages (last focused client window).
    dest_win: Arc<Mutex<Option<Window>>>,
    xim_proto_atom: Atom,
    /// Connected monitors, updated on RandR ScreenChangeNotify.
    monitor_map: Arc<Mutex<Vec<MonitorInfo>>>,
    /// Wine/Proton commit bridge (used by ImeBackend::commit when is_wine_window).
    wine_bridge: Arc<wine::WineXimBridge>,
}

impl X11Backend {
    /// Start the XIM server and begin listening for client connections.
    ///
    /// The event loop runs in a background `tokio::task::spawn_blocking` task.
    pub fn connect(rt: tokio::runtime::Handle, tx: mpsc::Sender<InputEvent>) -> Result<Self> {
        let display =
            std::env::var("DISPLAY").map_err(|_| PlatformError::X11("DISPLAY not set".into()))?;

        let (conn, screen_num) = RustConnection::connect(Some(&display))
            .map_err(|e| PlatformError::X11(e.to_string()))?;
        let conn = Arc::new(conn);
        let shared = Arc::new(Mutex::new(SharedState::default()));
        let dest_win: Arc<Mutex<Option<Window>>> = Arc::new(Mutex::new(None));
        let monitor_map: Arc<Mutex<Vec<MonitorInfo>>> = Arc::new(Mutex::new(Vec::new()));

        let xim_proto_atom = intern_atom(&conn, "_XIM_PROTOCOL")?;
        let net_wm_pid = intern_atom(&conn, "_NET_WM_PID")?;
        let backend_wine_bridge = Arc::new(wine::WineXimBridge::new(Arc::clone(&conn), net_wm_pid));

        // Spawn the event loop in a blocking thread.
        let conn2 = Arc::clone(&conn);
        let shared2 = Arc::clone(&shared);
        let dest_win2 = Arc::clone(&dest_win);
        let monitor_map2 = Arc::clone(&monitor_map);
        let screen_num2 = screen_num;
        rt.spawn_blocking(move || {
            match XimServer::new(conn2, screen_num2, shared2, monitor_map2, tx) {
                Ok(mut server) => {
                    info!("X11 XIM: event loop started");
                    loop {
                        match server.conn.wait_for_event() {
                            Ok(ev) => {
                                if !server.handle_event(&ev) {
                                    break;
                                }
                                // Track the last focused client window for commits.
                                let focused_ic = server
                                    .shared
                                    .lock()
                                    .unwrap_or_else(|e| e.into_inner())
                                    .focused_ic;
                                if let Some(ic_id) = focused_ic {
                                    for ics in server.contexts.values() {
                                        if let Some(ic) = ics.get(&ic_id) {
                                            *dest_win2.lock().unwrap_or_else(|e| e.into_inner()) =
                                                Some(ic.client_win);
                                            break;
                                        }
                                    }
                                }
                            }
                            Err(e) => {
                                warn!("X11 XIM: connection error: {e}");
                                break;
                            }
                        }
                    }
                }
                Err(e) => {
                    warn!("X11 XIM: server init failed: {e}");
                }
            }
        });

        Ok(X11Backend {
            rt,
            shared,
            conn,
            dest_win,
            xim_proto_atom,
            monitor_map,
            wine_bridge: backend_wine_bridge,
        })
    }

    /// Return the [`MonitorInfo`] for the monitor containing `(cx, cy)` in
    /// global screen coordinates, falling back to the first known monitor.
    pub fn cursor_monitor(&self, cx: i32, cy: i32) -> Option<MonitorInfo> {
        let map = self.monitor_map.lock().unwrap_or_else(|e| e.into_inner());
        map.iter()
            .find(|m| cx >= m.x && cx < m.x + m.width && cy >= m.y && cy < m.y + m.height)
            .or_else(|| map.first())
            .cloned()
    }

    fn send_commit_to_client(&self, text: &str) -> Result<()> {
        let dest = *self.dest_win.lock().unwrap_or_else(|e| e.into_inner());
        let win = match dest {
            Some(w) => w,
            None => return Ok(()),
        };
        if text.len() > 14 {
            return self.send_commit_long(win, text);
        }
        let bytes = text.as_bytes();
        let len = bytes.len() as u8;
        let mut buf = [0u8; 20];
        buf[0] = XIM_COMMIT;
        buf[2] = 0x02; // XLookupChars
        buf[4] = len;
        buf[5..5 + len as usize].copy_from_slice(bytes);
        let ev = ClientMessageEvent {
            response_type: xproto::CLIENT_MESSAGE_EVENT,
            format: 8,
            sequence: 0,
            window: win,
            type_: self.xim_proto_atom,
            data: ClientMessageData::from(buf),
        };
        self.conn
            .send_event(false, win, EventMask::NO_EVENT, ev)
            .map_err(|e| PlatformError::X11(e.to_string()))?
            .check()
            .map_err(|e| PlatformError::X11(e.to_string()))?;
        self.conn
            .flush()
            .map_err(|e| PlatformError::X11(e.to_string()))
    }

    /// Send `count` BackSpace press+release pairs to `win`.
    fn send_backspace_events(&self, win: Window, root: Window, count: usize) -> Result<()> {
        for _ in 0..count {
            for ev in build_key_events(22, 0, win, root) {
                self.conn
                    .send_event(false, win, EventMask::NO_EVENT, ev)
                    .map_err(|e| PlatformError::X11(e.to_string()))?
                    .check()
                    .map_err(|e| PlatformError::X11(e.to_string()))?;
            }
        }
        if count > 0 {
            self.conn
                .flush()
                .map_err(|e| PlatformError::X11(e.to_string()))?;
        }
        Ok(())
    }

    // ── Test helpers ─────────────────────────────────────────────────────────

    /// Constructs a backend wired to `conn` without spawning the XIM event loop.
    ///
    /// Returns the backend together with an idle single-thread runtime whose
    /// handle is stored inside the backend.  Callers must keep the runtime
    /// alive for the backend's lifetime.  No blocking tasks are ever spawned,
    /// so dropping the runtime at test-function exit is instantaneous.
    ///
    /// For regression tests only.
    pub fn for_test(conn: Arc<RustConnection>) -> Result<(Self, tokio::runtime::Runtime)> {
        let shared = Arc::new(Mutex::new(SharedState::default()));
        let dest_win: Arc<Mutex<Option<Window>>> = Arc::new(Mutex::new(None));
        let monitor_map: Arc<Mutex<Vec<MonitorInfo>>> = Arc::new(Mutex::new(Vec::new()));
        let xim_proto_atom = intern_atom(&conn, "_XIM_PROTOCOL")?;
        let net_wm_pid = intern_atom(&conn, "_NET_WM_PID")?;
        let wine_bridge = Arc::new(wine::WineXimBridge::new(Arc::clone(&conn), net_wm_pid));
        let rt = tokio::runtime::Builder::new_current_thread()
            .build()
            .map_err(|e| PlatformError::X11(e.to_string()))?;
        let handle = rt.handle().clone();
        let backend = X11Backend {
            rt: handle,
            shared,
            conn,
            dest_win,
            xim_proto_atom,
            monitor_map,
            wine_bridge,
        };
        Ok((backend, rt))
    }

    /// Returns a snapshot of the current shadow buffer. For regression tests.
    pub fn shadow_buf_snapshot(&self) -> String {
        self.shared
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .shadow_buf
            .clone()
    }

    /// Overrides the destination window for commit/backspace delivery. For regression tests.
    pub fn set_dest_win_for_test(&self, win: Option<Window>) {
        *self.dest_win.lock().unwrap_or_else(|e| e.into_inner()) = win;
    }

    /// Overrides the fullscreen_mode flag. For regression tests.
    pub fn set_fullscreen_for_test(&self, v: bool) {
        self.shared
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .fullscreen_mode = v;
    }

    /// Property-based XIM_COMMIT for strings that exceed the 14-byte Data8 limit.
    fn send_commit_long(&self, dest: Window, text: &str) -> Result<()> {
        let bytes = text.as_bytes();
        let n = bytes.len() as u16;
        let body_len = 4u16 + n;
        let units = body_len.div_ceil(4);
        let mut payload = Vec::with_capacity(4 + body_len as usize);
        payload.push(XIM_COMMIT);
        payload.push(0u8);
        payload.extend_from_slice(&units.to_le_bytes());
        payload.extend_from_slice(&0x0002u16.to_le_bytes()); // XLookupChars
        payload.extend_from_slice(&n.to_le_bytes());
        payload.extend_from_slice(bytes);
        while payload.len() % 4 != 0 {
            payload.push(0);
        }
        self.conn
            .change_property8(
                xproto::PropMode::REPLACE,
                dest,
                self.xim_proto_atom,
                xproto::AtomEnum::STRING,
                &payload,
            )
            .map_err(|e| PlatformError::X11(e.to_string()))?
            .check()
            .map_err(|e| PlatformError::X11(e.to_string()))?;
        let total = payload.len() as u32;
        let ev = ClientMessageEvent {
            response_type: xproto::CLIENT_MESSAGE_EVENT,
            format: 32,
            sequence: 0,
            window: dest,
            type_: self.xim_proto_atom,
            data: ClientMessageData::from([total, self.xim_proto_atom, 0, 0, 0]),
        };
        self.conn
            .send_event(false, dest, EventMask::NO_EVENT, ev)
            .map_err(|e| PlatformError::X11(e.to_string()))?
            .check()
            .map_err(|e| PlatformError::X11(e.to_string()))?;
        self.conn
            .flush()
            .map_err(|e| PlatformError::X11(e.to_string()))
    }
}

impl ImeBackend for X11Backend {
    fn commit(&self, text: &NfcString) -> Result<()> {
        let is_wine = self
            .shared
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .is_wine_window;
        let dest = *self.dest_win.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(win) = dest {
            if is_wine {
                let root = self.conn.setup().roots[0].root;
                self.wine_bridge.commit(win, root, text.as_str())?;
            } else {
                self.send_commit_to_client(text.as_str())?;
            }
        }
        let mut s = self.shared.lock().unwrap_or_else(|e| e.into_inner());
        s.preedit.clear();
        s.preedit_cursor = 0;
        s.shadow_buf.clear();
        Ok(())
    }

    fn update_preedit(&self, text: &PreeditText, cursor: CharCursor) -> Result<()> {
        let new_preedit = text.as_str();
        let (backspaces, new_tail, is_wine) = {
            let mut s = self.shared.lock().unwrap_or_else(|e| e.into_inner());
            // In fullscreen mode suppress preedit accumulation: the candidate
            // window is invisible in fullscreen and many games mishandle preedit.
            if s.fullscreen_mode {
                return Ok(());
            }
            let common_len = s
                .shadow_buf
                .chars()
                .zip(new_preedit.chars())
                .take_while(|(a, b)| a == b)
                .count();
            let backspaces = s.shadow_buf.chars().count() - common_len;
            let new_tail: String = new_preedit.chars().skip(common_len).collect();
            s.preedit = new_preedit.to_owned();
            s.preedit_cursor = cursor.0;
            (backspaces, new_tail, s.is_wine_window)
        };
        if !is_wine {
            let dest = *self.dest_win.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(win) = dest {
                let root = self.conn.setup().roots[0].root;
                self.send_backspace_events(win, root, backspaces)?;
                if !new_tail.is_empty() {
                    self.send_commit_to_client(&new_tail)?;
                }
            }
        }
        self.shared
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .shadow_buf = new_preedit.to_owned();
        Ok(())
    }

    fn clear_preedit(&self) -> Result<()> {
        let (n_backspaces, is_wine) = {
            let s = self.shared.lock().unwrap_or_else(|e| e.into_inner());
            (s.shadow_buf.chars().count(), s.is_wine_window)
        };
        if !is_wine {
            let dest = *self.dest_win.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(win) = dest {
                let root = self.conn.setup().roots[0].root;
                self.send_backspace_events(win, root, n_backspaces)?;
            }
        }
        let mut s = self.shared.lock().unwrap_or_else(|e| e.into_inner());
        s.preedit.clear();
        s.preedit_cursor = 0;
        s.shadow_buf.clear();
        Ok(())
    }

    fn forward_key(&self, key: &InputEvent) -> Result<()> {
        let dest = *self.dest_win.lock().unwrap_or_else(|e| e.into_inner());
        let win = match dest {
            Some(w) => w,
            None => return Ok(()),
        };
        let root = self.conn.setup().roots[0].root;
        let (keycode, mods) = match key {
            InputEvent::KeyDown(k, m) | InputEvent::KeyRepeat(k, m) => {
                (map_key_to_x11_keycode(k), *m)
            }
            InputEvent::KeyUp(k) => (map_key_to_x11_keycode(k), Modifiers::default()),
            _ => return Ok(()),
        };
        let state = mods_to_x11_state(mods);
        for ev in build_key_events(keycode, state, win, root) {
            self.conn
                .send_event(false, win, EventMask::NO_EVENT, ev)
                .map_err(|e| PlatformError::X11(e.to_string()))?
                .check()
                .map_err(|e| PlatformError::X11(e.to_string()))?;
        }
        self.conn
            .flush()
            .map_err(|e| PlatformError::X11(e.to_string()))
    }

    fn surrounding_text(&self) -> Result<Option<SurroundingText>> {
        Ok(self
            .shared
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .surrounding
            .clone())
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            surrounding_text: false,
            preedit: true,
            lookup_table: true,
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keysym_to_key_named_keys() {
        use xkbcommon::xkb::keysyms;
        assert_eq!(keysym_to_key(keysyms::KEY_BackSpace), Some(Key::Backspace));
        assert_eq!(keysym_to_key(keysyms::KEY_Return), Some(Key::Return));
        assert_eq!(keysym_to_key(keysyms::KEY_Escape), Some(Key::Escape));
        assert_eq!(keysym_to_key(keysyms::KEY_Tab), Some(Key::Tab));
    }

    #[test]
    fn keysym_to_key_printable() {
        use xkbcommon::xkb::keysyms;
        assert_eq!(keysym_to_key(keysyms::KEY_a), Some(Key::Char('a')));
        assert_eq!(keysym_to_key(keysyms::KEY_space), Some(Key::Char(' ')));
    }

    #[test]
    fn forward_key_char_a_produces_press_and_release() {
        let keycode = map_key_to_x11_keycode(&Key::Char('a'));
        assert_eq!(keycode, 38, "X11 keycode for 'a' must be 38");
        let events = build_key_events(keycode, 0, 0, 0);
        assert_eq!(
            events[0].response_type,
            xproto::KEY_PRESS_EVENT,
            "first event must be KEY_PRESS"
        );
        assert_eq!(
            events[1].response_type,
            xproto::KEY_RELEASE_EVENT,
            "second event must be KEY_RELEASE"
        );
        assert_eq!(events[0].detail, 38);
        assert_eq!(events[1].detail, 38);
    }

    #[test]
    fn map_key_roundtrip() {
        for (key, expected_kc) in [
            (Key::Char('a'), 38u8),
            (Key::Char('z'), 52),
            (Key::Return, 36),
            (Key::Escape, 9),
            (Key::Backspace, 22),
            (Key::Tab, 23),
        ] {
            assert_eq!(map_key_to_x11_keycode(&key), expected_kc, "{key:?}");
        }
    }

    #[test]
    fn mods_to_x11_state_shift() {
        let m = Modifiers {
            shift: true,
            ..Default::default()
        };
        assert_eq!(mods_to_x11_state(m) & 0x0001, 0x0001);
    }

    #[test]
    fn mods_to_x11_state_ctrl() {
        let m = Modifiers {
            ctrl: true,
            ..Default::default()
        };
        assert_eq!(mods_to_x11_state(m) & 0x0004, 0x0004);
    }

    // ── XIM surrounding text attribute parsing ────────────────────────────────

    fn build_surrounding_attr(text: &str, cursor: usize) -> Vec<u8> {
        let text_bytes = text.as_bytes();
        let text_len = text_bytes.len() as u32;
        let mut value = Vec::new();
        value.extend_from_slice(&text_len.to_le_bytes());
        value.extend_from_slice(text_bytes);
        // Pad to 4-byte boundary.
        let padded = (4 + text_bytes.len() + 3) & !3;
        value.resize(padded, 0u8);
        value.extend_from_slice(&(cursor as u32).to_le_bytes());
        value
    }

    #[test]
    fn parse_surrounding_text_attr_empty() {
        let value = build_surrounding_attr("", 0);
        assert_eq!(
            parse_surrounding_text_attr(&value),
            Some(("".to_owned(), 0))
        );
    }

    #[test]
    fn parse_surrounding_text_attr_ascii() {
        let value = build_surrounding_attr("hello", 5);
        assert_eq!(
            parse_surrounding_text_attr(&value),
            Some(("hello".to_owned(), 5))
        );
    }

    #[test]
    fn parse_surrounding_text_attr_vietnamese() {
        // "ắ" = U+1EAF, UTF-8: [0xE1, 0xBA, 0xAF] = 3 bytes; cursor after it = 3.
        let value = build_surrounding_attr("ắ", 3);
        assert_eq!(
            parse_surrounding_text_attr(&value),
            Some(("ắ".to_owned(), 3))
        );
    }

    #[test]
    fn parse_surrounding_text_attr_cursor_mid() {
        // Cursor in the middle of a multi-byte sequence is invalid.
        let value = build_surrounding_attr("ắ", 1); // 1 is not a codepoint boundary
        assert_eq!(parse_surrounding_text_attr(&value), None);
    }

    #[test]
    fn parse_surrounding_text_attr_too_short() {
        assert_eq!(parse_surrounding_text_attr(&[0u8; 7]), None);
    }

    #[test]
    fn parse_surrounding_text_attr_cursor_oob() {
        let value = build_surrounding_attr("hi", 99);
        assert_eq!(parse_surrounding_text_attr(&value), None);
    }
}
