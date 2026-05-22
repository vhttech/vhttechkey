#!/usr/bin/env bash
# Remove vhttechkey from the system.
# Run as root: sudo ./uninstall.sh
set -euo pipefail

echo "→ Stopping service..."
systemctl --user stop vhttechkey-daemon 2>/dev/null || true
systemctl --user disable vhttechkey-daemon 2>/dev/null || true

echo "→ Removing binaries..."
rm -rf /usr/lib/vhttechkey

echo "→ Removing shared assets..."
rm -rf /usr/share/vhttechkey

echo "→ Removing IBus component..."
rm -f /usr/share/ibus/component/vhttechkey-daemon.xml
ibus restart 2>/dev/null || true

echo "→ Removing Fcitx5 config..."
rm -f /usr/share/fcitx5/inputmethod/vhttechkey-daemon.conf

echo "→ Removing systemd service..."
rm -f /usr/lib/systemd/user/vhttechkey-daemon.service
systemctl --user daemon-reload 2>/dev/null || true

echo "→ Removing desktop entry..."
rm -f /usr/share/applications/vhttechkey-ui.desktop
update-desktop-database /usr/share/applications 2>/dev/null || true

echo "✓ vhttechkey removed."
