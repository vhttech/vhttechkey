#!/usr/bin/env bash
# Chạy với quyền root (qua pkexec hoặc sudo).
# Cài file vào các thư mục hệ thống.
# Tham số: <APPDIR> <ime_framework> [--uninstall]
set -euo pipefail

APPDIR="${1:?APPDIR required}"
IME_FRAMEWORK="${2:-none}"
ACTION="${3:-install}"

INSTALL_DIR=/usr/lib/vhttechkey
SHARE_DIR=/usr/share/vhttechkey
IBUS_COMPONENT_DIR=/usr/share/ibus/component
FCITX5_IM_DIR=/usr/share/fcitx5/inputmethod
SYSTEMD_USER_DIR=/usr/lib/systemd/user
DESKTOP_DIR=/usr/share/applications

do_install() {
    echo "  [1/5] Cài đặt binaries..."
    install -d "$INSTALL_DIR"
    install -m 755 "$APPDIR/binaries/vi-daemon" "$INSTALL_DIR/vi-daemon"
    [[ -f "$APPDIR/binaries/vi-ui" ]]    && install -m 755 "$APPDIR/binaries/vi-ui"    "$INSTALL_DIR/vi-ui"
    [[ -f "$APPDIR/binaries/vi-tools" ]] && install -m 755 "$APPDIR/binaries/vi-tools" "$INSTALL_DIR/vi-tools"

    echo "  [2/5] Đăng ký IBus component..."
    install -d "$IBUS_COMPONENT_DIR"
    install -m 644 "$APPDIR/data/ibus/vhttechkey-daemon.xml" "$IBUS_COMPONENT_DIR/vhttechkey-daemon.xml"
    if [[ "$IME_FRAMEWORK" == "ibus" ]]; then
        ibus restart 2>/dev/null || true
    fi

    echo "  [3/5] Cài đặt Fcitx5 config..."
    if [[ "$IME_FRAMEWORK" == "fcitx5" ]]; then
        install -d "$FCITX5_IM_DIR"
        install -m 644 "$APPDIR/data/fcitx5/vhttechkey-daemon.conf" "$FCITX5_IM_DIR/vhttechkey-daemon.conf"
    fi

    echo "  [4/5] Cài đặt systemd service..."
    install -d "$SYSTEMD_USER_DIR"
    install -m 644 "$APPDIR/data/systemd/vhttechkey-daemon.service" "$SYSTEMD_USER_DIR/vhttechkey-daemon.service"
    systemctl daemon-reload 2>/dev/null || true

    echo "  [5/5] Cài desktop entry..."
    install -d "$DESKTOP_DIR"
    install -m 644 "$APPDIR/data/desktop/vhttechkey-ui.desktop" "$DESKTOP_DIR/vhttechkey-ui.desktop"
    if [[ -d "$APPDIR/data/icons" ]]; then
        install -d "$SHARE_DIR/icons"
        cp -r "$APPDIR/data/icons/." "$SHARE_DIR/icons/"
    fi
    update-desktop-database "$DESKTOP_DIR" 2>/dev/null || true
}

do_uninstall() {
    systemctl --user stop vhttechkey-daemon 2>/dev/null || true
    systemctl --user disable vhttechkey-daemon 2>/dev/null || true
    rm -rf "$INSTALL_DIR" "$SHARE_DIR"
    rm -f "$IBUS_COMPONENT_DIR/vhttechkey-daemon.xml"
    rm -f "$FCITX5_IM_DIR/vhttechkey-daemon.conf"
    rm -f "$SYSTEMD_USER_DIR/vhttechkey-daemon.service"
    rm -f "$DESKTOP_DIR/vhttechkey-ui.desktop"
    systemctl daemon-reload 2>/dev/null || true
    update-desktop-database "$DESKTOP_DIR" 2>/dev/null || true
    ibus restart 2>/dev/null || true
}

if [[ "$ACTION" == "--uninstall" ]]; then
    do_uninstall
else
    do_install
fi
