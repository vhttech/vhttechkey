#!/usr/bin/env bash
# Đóng gói vhttechkey thành file .deb
# Cần chạy sau khi đã build release: make build-release
# Output: dist/vhttechkey_<version>_amd64.deb
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
BINARY_DIR="$REPO_ROOT/target/release"

VERSION=$(grep '^version' "$REPO_ROOT/Cargo.toml" | head -1 | sed 's/.*"\(.*\)".*/\1/')
PKG_DIR="$REPO_ROOT/dist/deb-build/vhttechkey_${VERSION}_amd64"

if [[ ! -f "$BINARY_DIR/vi-daemon" ]]; then
    echo "ERROR: binaries not found. Run 'make build-release' first."
    exit 1
fi

echo "→ Preparing package structure..."
rm -rf "$PKG_DIR"
mkdir -p \
    "$PKG_DIR/DEBIAN" \
    "$PKG_DIR/usr/lib/vhttechkey" \
    "$PKG_DIR/usr/share/ibus/component" \
    "$PKG_DIR/usr/share/fcitx5/inputmethod" \
    "$PKG_DIR/usr/lib/systemd/user" \
    "$PKG_DIR/usr/share/applications" \
    "$PKG_DIR/usr/share/vhttechkey/icons"

echo "→ Copying binaries..."
install -m 755 "$BINARY_DIR/vi-daemon" "$PKG_DIR/usr/lib/vhttechkey/vi-daemon"
[[ -f "$BINARY_DIR/vi-ui" ]]    && install -m 755 "$BINARY_DIR/vi-ui"    "$PKG_DIR/usr/lib/vhttechkey/vi-ui"
[[ -f "$BINARY_DIR/vi-tools" ]] && install -m 755 "$BINARY_DIR/vi-tools" "$PKG_DIR/usr/lib/vhttechkey/vi-tools"

echo "→ Copying packaging files..."
install -m 644 "$SCRIPT_DIR/ibus/vhttechkey-daemon.xml"    "$PKG_DIR/usr/share/ibus/component/"
install -m 644 "$SCRIPT_DIR/fcitx5/vhttechkey-daemon.conf" "$PKG_DIR/usr/share/fcitx5/inputmethod/"
install -m 644 "$SCRIPT_DIR/systemd/vhttechkey-daemon.service" "$PKG_DIR/usr/lib/systemd/user/"
install -m 644 "$SCRIPT_DIR/desktop/vhttechkey-ui.desktop"  "$PKG_DIR/usr/share/applications/"

if [[ -d "$REPO_ROOT/assets/icons" ]]; then
    cp -r "$REPO_ROOT/assets/icons/." "$PKG_DIR/usr/share/vhttechkey/icons/"
fi

echo "→ Copying DEBIAN control files..."
install -m 644 "$SCRIPT_DIR/deb/DEBIAN/control" "$PKG_DIR/DEBIAN/control"
install -m 755 "$SCRIPT_DIR/deb/DEBIAN/postinst" "$PKG_DIR/DEBIAN/postinst"
install -m 755 "$SCRIPT_DIR/deb/DEBIAN/prerm"    "$PKG_DIR/DEBIAN/prerm"

# Cập nhật version trong control file
sed -i "s/^Version:.*/Version: $VERSION/" "$PKG_DIR/DEBIAN/control"

echo "→ Building .deb package..."
mkdir -p "$REPO_ROOT/dist"
dpkg-deb --build --root-owner-group "$PKG_DIR" \
    "$REPO_ROOT/dist/vhttechkey_${VERSION}_amd64.deb"

echo ""
echo "✓ dist/vhttechkey_${VERSION}_amd64.deb"
