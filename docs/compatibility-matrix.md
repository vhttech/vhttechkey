# Compatibility Matrix

Each cell shows the test status for a compositor × input-method-backend
combination.  Compositor quirks that affect the Wayland-direct backend are
handled in `crates/vi-wayland/src/lib.rs` via compile-time feature flags
(`gnome`, `kwin`, `hyprland`).

## Matrix

| Compositor | IBus | Fcitx5 | Wayland-direct | X11-XIM | Notes |
|---|---|---|---|---|---|
| **GNOME Shell** (Mutter ≥ 44) | ✅ | ✅ | ✅ | ⚠ | Wayland: empty preedit must precede commit; re-activate on focus restore (`gnome` flag). X11-XIM: XWayland apps only. |
| **KDE Plasma** (KWin ≥ 5.27) | ✅ | ✅ | ✅ | ⚠ | Wayland: `surrounding_text` offsets are byte-based, not char-based (`kwin` flag). X11-XIM: XWayland apps only. |
| **Hyprland** (≥ 0.35) | ✅ | ✅ | ✅ | ❌ | Wayland: falls back to `zwp_virtual_keyboard_v1` when `zwp_input_method_manager_v2` absent; buffers preedit on rapid typing (`hyprland` flag). |
| **Sway** (wlroots ≥ 0.17) | ✅ | ✅ | ✅ | ❌ | Protocol-correct; no quirks required. |
| **Niri** | ❓ | ❓ | ❓ | ❌ | Implements text-input-v3; untested — expected to work without quirks. |
| **Weston** (≥ 12.0) | ❓ | ❓ | ✅ | ❌ | Reference implementation; strict protocol event ordering required. Detection via `weston_screenshooter` global. |
| **XFCE** (xfwm4-wayland) | ✅ | ✅ | ⚠ | ✅ | Wayland: `delay_preedit_clear` quirk applied (detected via `wp_viewporter`). X11-XIM: native on classic xfwm4. |
| **Cinnamon** (Muffin ≥ 6.0) | ✅ | ✅ | ⚠ | ✅ | Wayland: `empty_preedit_before_commit` quirk applied (detected via `cinnamon_shell_v1`). X11-XIM: native on X11 Cinnamon. |

## Legend

| Symbol | Meaning |
|---|---|
| ✅ | Tested and passing |
| ⚠ | Partial — core typing works; some features degraded (see Notes) |
| ❓ | Untested / unknown |
| ❌ | Not supported |

## Backend notes

**IBus** — handled by the `vi-ibus` crate via the IBus D-Bus protocol.
Works on any compositor/desktop that runs an IBus daemon.

**Fcitx5** — handled by the `vi-fcitx5` crate via the Fcitx5 D-Bus protocol.
Works on any compositor/desktop that runs a Fcitx5 daemon.

**Wayland-direct** — handled by the `vi-wayland` crate using
`zwp_input_method_v2` + `zwp_text_input_v3`.  Compositor-specific quirks are
applied through compile-time feature flags; see
`crates/vi-wayland/src/lib.rs` for implementation details and
`docs/wayland-compat.md` for the full per-compositor quirk list.

**X11-XIM** — handled by the `vi-x11` crate using the X Input Method protocol.
Available natively on X11 compositors (Xfwm, Cinnamon, …) and for XWayland
applications running inside a Wayland session (GNOME, KDE Plasma).
