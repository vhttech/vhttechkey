#!/usr/bin/env bash
# Đóng gói vhttechkey thành AppImage installer.
# Cần: appimagetool trong PATH, binaries đã build release.
# Output: dist/vhttechkey-installer-linux-x86_64.AppImage
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
BINARY_DIR="$REPO_ROOT/target/release"
APPDIR="$REPO_ROOT/dist/appimage-build/VHTTechKey-Installer.AppDir"

if [[ ! -f "$BINARY_DIR/vi-daemon" ]]; then
    echo "ERROR: binaries not found. Run 'make build-release' first."
    exit 1
fi

if ! command -v appimagetool &>/dev/null; then
    echo "ERROR: appimagetool not found in PATH."
    echo "  Download from: https://github.com/AppImage/appimagetool/releases"
    exit 1
fi

echo "→ Preparing AppDir structure..."
rm -rf "$APPDIR"
mkdir -p \
    "$APPDIR/binaries" \
    "$APPDIR/data/ibus" \
    "$APPDIR/data/fcitx5" \
    "$APPDIR/data/systemd" \
    "$APPDIR/data/desktop" \
    "$APPDIR/data/icons"

echo "→ Copying binaries..."
install -m 755 "$BINARY_DIR/vi-daemon" "$APPDIR/binaries/vi-daemon"
[[ -f "$BINARY_DIR/vi-ui" ]]    && install -m 755 "$BINARY_DIR/vi-ui"    "$APPDIR/binaries/vi-ui"
[[ -f "$BINARY_DIR/vi-tools" ]] && install -m 755 "$BINARY_DIR/vi-tools" "$APPDIR/binaries/vi-tools"

echo "→ Copying data files..."
install -m 644 "$SCRIPT_DIR/ibus/vhttechkey-daemon.xml"    "$APPDIR/data/ibus/"
install -m 644 "$SCRIPT_DIR/fcitx5/vhttechkey-daemon.conf" "$APPDIR/data/fcitx5/"
install -m 644 "$SCRIPT_DIR/systemd/vhttechkey-daemon.service" "$APPDIR/data/systemd/"
install -m 644 "$SCRIPT_DIR/desktop/vhttechkey-ui.desktop"  "$APPDIR/data/desktop/"

if [[ -d "$REPO_ROOT/assets/icons" ]]; then
    cp -r "$REPO_ROOT/assets/icons/." "$APPDIR/data/icons/"
fi

echo "→ Copying AppImage scripts..."
install -m 755 "$SCRIPT_DIR/appimage/AppRun"                "$APPDIR/AppRun"
install -m 755 "$SCRIPT_DIR/appimage/install-privileged.sh" "$APPDIR/install-privileged.sh"
install -m 755 "$SCRIPT_DIR/appimage/post-install-user.sh"  "$APPDIR/post-install-user.sh"
install -m 644 "$SCRIPT_DIR/appimage/vhttechkey.desktop"    "$APPDIR/vhttechkey.desktop"

# Icon (dùng placeholder nếu chưa có icon thật)
if [[ -f "$REPO_ROOT/assets/icons/vhttechkey.png" ]]; then
    install -m 644 "$REPO_ROOT/assets/icons/vhttechkey.png" "$APPDIR/vhttechkey.png"
elif [[ -f "$SCRIPT_DIR/appimage/vhttechkey.png" ]]; then
    install -m 644 "$SCRIPT_DIR/appimage/vhttechkey.png" "$APPDIR/vhttechkey.png"
else
    # Tạo placeholder icon bằng ImageMagick nếu có
    if command -v convert &>/dev/null; then
        convert -size 256x256 xc:'#1a73e8' \
            -fill white -font DejaVu-Sans-Bold -pointsize 80 \
            -gravity center -annotate 0 'VHT' \
            "$APPDIR/vhttechkey.png" 2>/dev/null || true
    fi
fi

echo "→ Building AppImage..."
mkdir -p "$REPO_ROOT/dist"
ARCH="${ARCH:-x86_64}" appimagetool "$APPDIR" \
    "$REPO_ROOT/dist/vhttechkey-installer-linux-x86_64.AppImage"

echo ""
echo "✓ dist/vhttechkey-installer-linux-x86_64.AppImage"
