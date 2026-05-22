#!/usr/bin/env bash
# Đóng gói vhttechkey thành file .rpm
# Cần: rpm-build, rpmdevtools. Binaries đã build release.
# Output: dist/vhttechkey-<version>-1.x86_64.rpm
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
BINARY_DIR="$REPO_ROOT/target/release"

VERSION=$(grep '^version' "$REPO_ROOT/Cargo.toml" | head -1 | sed 's/.*"\(.*\)".*/\1/')

if [[ ! -f "$BINARY_DIR/vi-daemon" ]]; then
    echo "ERROR: binaries not found. Run 'make build-release' first."
    exit 1
fi

RPM_BUILD_ROOT="$HOME/rpmbuild"
rpmdev-setuptree

# Copy sources vào rpmbuild/SOURCES
cp -f "$BINARY_DIR/vi-daemon"  "$RPM_BUILD_ROOT/SOURCES/"
[[ -f "$BINARY_DIR/vi-ui" ]]    && cp -f "$BINARY_DIR/vi-ui"    "$RPM_BUILD_ROOT/SOURCES/"
[[ -f "$BINARY_DIR/vi-tools" ]] && cp -f "$BINARY_DIR/vi-tools" "$RPM_BUILD_ROOT/SOURCES/"

# Copy packaging data
cp -rf "$SCRIPT_DIR/ibus"    "$RPM_BUILD_ROOT/SOURCES/"
cp -rf "$SCRIPT_DIR/fcitx5"  "$RPM_BUILD_ROOT/SOURCES/"
cp -rf "$SCRIPT_DIR/systemd" "$RPM_BUILD_ROOT/SOURCES/"
cp -rf "$SCRIPT_DIR/desktop" "$RPM_BUILD_ROOT/SOURCES/"

# Cập nhật version trong spec
sed "s/^Version:.*/Version: $VERSION/" "$SCRIPT_DIR/rpm/vhttechkey.spec" \
    > "$RPM_BUILD_ROOT/SPECS/vhttechkey.spec"

echo "→ Building .rpm..."
rpmbuild -bb "$RPM_BUILD_ROOT/SPECS/vhttechkey.spec"

mkdir -p "$REPO_ROOT/dist"
find "$RPM_BUILD_ROOT/RPMS/x86_64/" -name "vhttechkey-*.rpm" \
    -exec cp {} "$REPO_ROOT/dist/" \;

echo ""
echo "✓ dist/vhttechkey-${VERSION}-1.x86_64.rpm"
