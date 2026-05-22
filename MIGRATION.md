# Migration guide: vi-* → vime-* (planned for 0.2.0)

> **Status**: The `vime-*` crates do not exist yet.  The current release (0.1.x)
> ships only `vi-*` binaries and crates.  This document describes the planned
> rename so that downstream scripts and packages can prepare in advance.

## Binary (planned)

| Current (0.1.x) | Planned (0.2.0) |
|---|---|
| `vi-daemon` | `vime-daemon` |
| `vi-tools` | `vime-tools` |
| `vi-ui` | `vime-ui` |

## Config directory

The config directory is already `~/.config/vime/` in the current 0.1.x release —
the daemon reads `~/.config/vime/config.toml` at startup.  No migration needed here.

## Why vime-*?

The `vime-*` series will add several improvements over the current `vi-*`:

- **xkbcommon integration** in the X11 backend — correct dead-key and AltGr
  handling instead of a hardcoded US QWERTY map
- **Key-repeat detection** — held keys no longer produce duplicate composition
  steps
- **Preedit callbacks** — applications receive granular preedit-change events
  rather than full-string replacements

## Crate mapping (planned)

| Current (`vi-*`) | Planned (`vime-*`) |
|---|---|
| `vi-core` | `vime-core` |
| `vi-daemon` | `vime-daemon` |
| `vi-ibus` | `vime-ibus` |
| `vi-fcitx5` | `vime-fcitx5` |
| `vi-wayland` | `vime-wayland` |
| `vi-x11` | `vime-x11` |
| `vi-config` | `vime-config` |
| `vi-testing` | `vime-tests` |
| `vi-platform` | _(will be merged into `vime-core`)_ |
| `vi-portal` | _(will be merged into `vime-daemon`)_ |
| `vi-tools` | `vime-tools` |
| `vi-ui` | `vime-ui` |
