# vime — Vietnamese Input Method Engine

A modern, compositor-aware Vietnamese IME for Linux, written in Rust.

## Features

- **Three built-in input methods**: Telex, VNI, VIQR — switch without restarting
- **Custom rule sets**: define your own key → character mappings in TOML
- **Named profiles**: create per-language or per-app profiles, export/import as TOML
- **Real-time configuration**: all changes apply instantly via IPC — no logout required
- **Wayland-native**: uses `zwp_text_input_v3` with per-compositor quirk workarounds
- **X11 support**: XIM for legacy apps alongside modern Wayland
- **IBus and Fcitx5**: plug-in engines for both major Linux IME frameworks
- **NFC output guaranteed**: all committed text passes through the unicode normalization pipeline
- **egui settings UI**: visual keyboard heatmap, typing test with NFC analysis, log viewer

## Requirements

```bash
# Debian / Ubuntu
sudo apt install \
    libdbus-1-dev libglib2.0-dev libibus-1.0-dev libfcitx5-qt-dev \
    libwayland-dev wayland-protocols libxcb1-dev libgl1-mesa-dev pkg-config

# Fedora
sudo dnf install \
    dbus-devel glib2-devel ibus-devel fcitx5-devel \
    wayland-devel wayland-protocols-devel libxcb-devel mesa-libGL-devel
```

Rust toolchain (stable) is required — see [docs/contributing.md](docs/contributing.md) for
the full dependency list, build steps, and optional tools (Miri, fuzz).

## Quick start

Five commands to get typing:

```bash
# 1. Start the daemon
vi-daemon &

# 2. Set Telex input method
vi-tools set-method telex

# 3. Configure your session (add to ~/.profile)
export GTK_IM_MODULE=ibus
export QT_IM_MODULE=ibus
export XMODIFIERS=@im=ibus

# 4. Start the settings UI
vi-ui &

# 5. Open any text editor and type:
#    viet nam  →  việt nam
#    khong     →  không
#    truong    →  trường
```

## Documentation

| Document | Description |
|---|---|
| [docs/architecture.md](docs/architecture.md) | Layered diagram, data flow, crate dependency graph |
| [docs/unicode-pipeline.md](docs/unicode-pipeline.md) | Why NFC, pipeline steps, common wrong encodings |
| [docs/wayland-compat.md](docs/wayland-compat.md) | Compositor compatibility table and known quirks |
| [docs/ibus-fcitx5.md](docs/ibus-fcitx5.md) | IBus/Fcitx5 sequence diagrams (typing, focus, restart) |
| [docs/troubleshooting.md](docs/troubleshooting.md) | Doubled characters, missing tones, stuck preedit, VS Code/Electron vs Chrome, CPU |
| [docs/contributing.md](docs/contributing.md) | Build from source, tests, adding input methods, compositor quirks |
| [docs/verification.md](docs/verification.md) | Release checklist (tests, Miri, fuzz, manual E2E, Valgrind) |
| [docs/sequence-diagrams.md](docs/sequence-diagrams.md) | Mermaid sequence diagrams: typing, focus-out, restart, method switch |
| [docs/state-machine.md](docs/state-machine.md) | CompositionEngine state machine diagram and transition table |

## Keyboard shortcuts (default)

| Action | Binding |
|---|---|
| Toggle IME on/off | `Ctrl+Space` |
| Next profile | `Ctrl+Shift+.` |
| Previous profile | `Ctrl+Shift+,` |
| Reset preedit | `Escape` |

All bindings are configurable in vi-ui → Key Map Editor.

## License

This project is licensed under the [GNU General Public License v3.0 or later](LICENSE)
(GPL-3.0-or-later). See the [LICENSE](LICENSE) file for the full text.

Bundled third-party assets may use other licenses (for example, the Hack font in
`data/fonts/Hack/` and the Vietnamese dictionary in `data/dictionaries/`).