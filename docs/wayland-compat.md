# Wayland Compatibility

vime uses the **zwp_text_input_v3** protocol for Wayland input method support
(`vi-wayland` crate).  Compositor behaviour varies; this table documents known
quirks and their workarounds.

## Compositor compatibility table

| Compositor | Protocol Version | Known Quirks | Workaround | Test Status |
|---|---|---|---|---|
| **GNOME Shell** (Mutter ≥ 44) | text-input-v3 | `commit_string` events are ignored if no `preedit_string` was sent in the same serial; sends spurious `leave` on window raise | Always send an empty `preedit_string("")` before `commit_string`; re-send `activate` on `enter` after focus restore | ✅ Passing |
| **KDE Plasma** (KWin ≥ 5.27) | text-input-v3 | Content-type hints not propagated to the IM; `surrounding_text` offset is byte-based, not char-based | Ignore content-type; treat all offsets as byte offsets and convert to char boundaries | ✅ Passing |
| **Sway** (wlroots ≥ 0.17) | text-input-v3 | Protocol-correct; no known quirks | None required | ✅ Passing |
| **Hyprland** (≥ 0.35) | text-input-v3 | `done` event arrives before `preedit_string` update in rapid typing; cursor rect occasionally reported at (0,0) | Buffer the `preedit_string` update; ignore cursor rect if both coordinates are zero | ✅ Passing |
| **river** (wlroots ≥ 0.17) | text-input-v3 | Same behaviour as Sway | None required | ✅ Passing |
| **Weston** (≥ 12.0) | text-input-v3 | Reference implementation; strict protocol ordering required | Follow protocol ordering exactly | ✅ Passing |
| **labwc** (≥ 0.7) | text-input-v3 | No protocol issues; IME popup positioning not supported | Suppress candidate window positioning | ⚠ Partial |
| **Mir** (Ubuntu 23.10+) | text-input-v3 | `surrounding_text` not sent | Operate without surrounding-text context | ⚠ Partial |
| **GNOME Shell** (Mutter 42–43) | text-input-v3 early | `text_change_cause` enum values differ | Map enum values to the v3-final equivalents | 🔶 Legacy |
| **Enlightenment** | text-input-v1 | v1 only; no v3 support | Falls back to X11 backend via XWayland | ❌ No native |
| **Gamescope** | none | No IME protocol support | No text input support in gaming mode | ❌ N/A |

### Status legend

| Symbol | Meaning |
|---|---|
| ✅ Passing | All manual and automated tests pass |
| ⚠ Partial | Core typing works; some features (candidate positioning, surrounding text) degraded |
| 🔶 Legacy | Workaround in place; only tested on distro LTS with older compositor |
| ❌ No native | Falls back to X11 backend or is unsupported |

## Testing a compositor

To verify vime works with a compositor, run the manual test suite:

```bash
# 1. Start vi-daemon in the Wayland session
vi-daemon &

# 2. Open a text editor (e.g. foot terminal + nano)
foot nano /tmp/test.txt

# 3. Type the Telex test string and verify output
#    Type: viet nam  → expect: việt nam
#    Type: khong  → expect: không

# 4. Check NFC
python3 -c "
import unicodedata, sys
text = open('/tmp/test.txt').read()
bad = [hex(ord(c)) for c in text if unicodedata.normalize('NFC', c) != c]
print('NFD chars:', bad if bad else 'none — all NFC')
"
```

## Compositor quirks

The table below documents the quirks detected by `crates/vi-wayland/src/quirks.rs` and
the mitigation applied at runtime.  Detection is performed by inspecting Wayland globals
advertised in the registry; an environment variable `VIME_COMPOSITOR_PROFILE` overrides
the heuristic for debugging.

| Compositor | Quirk (`CompositorQuirks` flag) | Detection signal | Mitigation |
|---|---|---|---|
| **GNOME Shell** (Mutter ≥ 44) / **Cinnamon** | `empty_preedit_before_commit` — `commit_string` is silently dropped when no `preedit_string` was sent in the same serial | `zwp_text_input_manager_v3` present, `kde_output_management_v2` absent (GNOME); `cinnamon_shell_v1` global (Cinnamon fast-path) | Always send `preedit_string("")` immediately before `commit_string` |
| **KDE Plasma** (KWin ≥ 5.27) | `snap_cursor_to_char_boundary` — `surrounding_text` byte offset may bisect a multi-byte codepoint | `kde_output_management_v2` global | Snap byte offset to nearest UTF-8 character boundary via `snap_to_char_boundary()` |
| **Hyprland** (≥ 0.35) / **labwc** | `buffer_preedit_updates` — socket flush deferred on rapid preedit updates | `hyprland_global_shortcuts_manager_v1` (Hyprland); `labwc_options_v1` (labwc fast-path, ≥ 0.7) | Batch preedit socket writes; defer flush until stable |
| **labwc** (≥ 0.7) | `suppress_candidate_position` — IME popup positioning not supported | `labwc_options_v1` global | Omit candidate-window positioning calls entirely |
| **Niri** | `niri_dual_protocol` — `zwp_input_method_v2` and `zwp_text_input_v3` lifecycles must be co-managed | `niri_ipc` global (fast-path) | Manage both protocol enable/disable sequences together |
| **Mir** (Ubuntu 23.10+) | `no_surrounding_text` — surrounding-text events are not delivered | `mir_shell` global | Operate without surrounding-text context |
| **XFCE** (xfwm4-wayland) | `delay_preedit_clear` — preedit clear must be delayed one event-loop roundtrip after `commit_string` | `wp_viewporter` present; `kde_output_management_v2`, `hyprland_global_shortcuts_manager_v1`, `wp_cursor_shape_manager_v1` absent | Delay preedit clear by one roundtrip |
| **LXQt** (Openbox-Wayland) | `virtual_keyboard_fallback` — no `zwp_input_method_manager_v2` present | Modern compositor globals without `zwp_text_input_manager_v3` or `zwp_input_method_manager_v2`; `wl_compositor` version ≥ 4 | Fall back to virtual-keyboard protocol |
| **Weston** (≥ 12.0) | None — reference implementation; strict protocol event ordering required | `weston_screenshooter` global | Follow protocol ordering exactly; no extra quirk flags needed |
| **Sway** / **River** (wlroots ≥ 0.17) | None — protocol-correct; serial counters must not overflow silently | `zwlr_output_manager_v1` without Hyprland globals (Sway); `river_control_v1` (River) | Use `wrapping_add` for serial counters |

## Adding a new compositor quirk

1. Reproduce the issue in `crates/vi-wayland/tests/integration_test.rs` with a
   mock compositor (see `tests/fixtures/mock_compositor.rs`).
2. Add the quirk detection in `crates/vi-wayland/src/lib.rs` behind a
   `CompositorQuirks` bitflag.
3. Document it in this table.
4. Add an entry to `docs/contributing.md` under "Adding a compositor quirk".
