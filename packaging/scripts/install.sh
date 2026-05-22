#!/usr/bin/env bash
# Install vhttechkey binaries, component files, and service.
# Run as root: sudo ./install.sh
set -euo pipefail

INSTALL_DIR=/usr/lib/vhttechkey
SHARE_DIR=/usr/share/vhttechkey
IBUS_COMPONENT_DIR=/usr/share/ibus/component
FCITX5_IM_DIR=/usr/share/fcitx5/inputmethod
SYSTEMD_USER_DIR=/usr/lib/systemd/user
DESKTOP_DIR=/usr/share/applications
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
BINARY_DIR="$REPO_ROOT/target/release"

check_built() {
    if [[ ! -f "$BINARY_DIR/vi-daemon" ]]; then
        echo "ERROR: release binaries not found. Run 'make build-release' first."
        exit 1
    fi
}

install_binaries() {
    echo "→ Installing binaries to $INSTALL_DIR"
    install -d "$INSTALL_DIR"
    install -m 755 "$BINARY_DIR/vi-daemon" "$INSTALL_DIR/vi-daemon"
    if [[ -f "$BINARY_DIR/vi-ui" ]]; then
        install -m 755 "$BINARY_DIR/vi-ui" "$INSTALL_DIR/vi-ui"
    fi
    if [[ -f "$BINARY_DIR/vi-tools" ]]; then
        install -m 755 "$BINARY_DIR/vi-tools" "$INSTALL_DIR/vi-tools"
    fi
}

install_icons() {
    echo "→ Installing icons to $SHARE_DIR/icons"
    install -d "$SHARE_DIR/icons"
    if [[ -d "$REPO_ROOT/assets/icons" ]]; then
        cp -r "$REPO_ROOT/assets/icons/." "$SHARE_DIR/icons/"
    else
        echo "  (no assets/icons directory found — skipping icons)"
    fi
}

install_ibus() {
    echo "→ Installing IBus component to $IBUS_COMPONENT_DIR"
    install -d "$IBUS_COMPONENT_DIR"
    install -m 644 "$SCRIPT_DIR/../ibus/vhttechkey-daemon.xml" "$IBUS_COMPONENT_DIR/vhttechkey-daemon.xml"
    echo "  Restarting ibus-daemon..."
    ibus restart 2>/dev/null || true
    register_ibus_engine
}

register_ibus_engine() {
    local real_user="${SUDO_USER:-}"
    if [[ -z "$real_user" ]]; then
        echo "  (skipping IBus engine registration — not running via sudo)"
        return
    fi

    # Find the user's D-Bus session bus address from their running processes
    local dbus_addr=""
    while IFS= read -r pid; do
        local addr
        addr=$(grep -z DBUS_SESSION_BUS_ADDRESS /proc/"$pid"/environ 2>/dev/null \
               | tr -d '\0' | sed 's/DBUS_SESSION_BUS_ADDRESS=//' || true)
        if [[ -n "$addr" ]]; then
            dbus_addr="$addr"
            break
        fi
    done < <(pgrep -u "$real_user" 2>/dev/null | head -30)

    if [[ -z "$dbus_addr" ]]; then
        echo "  (no D-Bus session found — IBus engine not auto-registered)"
        echo "  Run manually: ibus-setup → Input Method → Add → VHTTechKey"
        return
    fi

    local engine="vhttechkey"

    local current
    current=$(sudo -u "$real_user" DBUS_SESSION_BUS_ADDRESS="$dbus_addr" \
              gsettings get org.freedesktop.ibus.general preload-engines 2>/dev/null \
              || echo "@as []")

    if [[ "$current" == *"$engine"* ]]; then
        echo "  IBus engine already registered."
        return
    fi

    local new_list
    if [[ "$current" == "@as []" || "$current" == "[]" ]]; then
        new_list="['xkb:us::eng', '$engine']"
    else
        new_list="${current%]}, '$engine']"
    fi

    sudo -u "$real_user" DBUS_SESSION_BUS_ADDRESS="$dbus_addr" \
        gsettings set org.freedesktop.ibus.general preload-engines "$new_list" 2>/dev/null || true
    sudo -u "$real_user" DBUS_SESSION_BUS_ADDRESS="$dbus_addr" \
        gsettings set org.freedesktop.ibus.general engines-order "$new_list" 2>/dev/null || true
    echo "  Registered $engine in IBus input methods."
}

install_fcitx5() {
    if command -v fcitx5 &>/dev/null; then
        echo "→ Installing Fcitx5 input method config to $FCITX5_IM_DIR"
        install -d "$FCITX5_IM_DIR"
        install -m 644 "$SCRIPT_DIR/../fcitx5/vhttechkey-daemon.conf" "$FCITX5_IM_DIR/vhttechkey-daemon.conf"
    else
        echo "  (fcitx5 not found — skipping Fcitx5 config)"
    fi
}

install_systemd() {
    echo "→ Installing systemd user service to $SYSTEMD_USER_DIR"
    install -d "$SYSTEMD_USER_DIR"
    install -m 644 "$SCRIPT_DIR/../systemd/vhttechkey-daemon.service" "$SYSTEMD_USER_DIR/vhttechkey-daemon.service"
    systemctl --user daemon-reload 2>/dev/null || true
    echo "  To enable autostart: systemctl --user enable --now vhttechkey-daemon"
}

install_desktop() {
    echo "→ Installing desktop entry to $DESKTOP_DIR"
    install -d "$DESKTOP_DIR"
    install -m 644 "$SCRIPT_DIR/../desktop/vhttechkey-ui.desktop" "$DESKTOP_DIR/vhttechkey-ui.desktop"
    update-desktop-database "$DESKTOP_DIR" 2>/dev/null || true
}

main() {
    if [[ $EUID -ne 0 ]]; then
        echo "ERROR: run as root (sudo $0)"
        exit 1
    fi

    check_built
    install_binaries
    install_icons
    install_ibus
    install_fcitx5
    install_systemd
    install_desktop

    echo ""
    echo "✓ vhttechkey installed successfully."
    echo ""
    echo "Next steps:"
    echo "  1. Enable autostart:  systemctl --user enable --now vhttechkey-daemon"
    echo "  2. Open settings:     vi-ui"
}

main "$@"
