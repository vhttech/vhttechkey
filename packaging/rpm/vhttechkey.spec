Name:           vhttechkey
Version:        0.1.0
Release:        1%{?dist}
Summary:        Bộ gõ tiếng Việt hiện đại cho Linux
License:        GPL-3.0-or-later
URL:            https://github.com/vhttech/vhttechkey

# Không dùng BuildRequires vì binaries đã được build sẵn
# Source binaries phải có trong %{_builddir} trước khi chạy rpmbuild

Requires:       dbus
Requires:       (ibus or fcitx5)
Recommends:     ibus

%description
VHTTechKey là bộ gõ tiếng Việt nhận biết compositor, hỗ trợ Wayland và X11.
Tích hợp với IBus và Fcitx5. Hỗ trợ ba kiểu gõ: Telex, VNI, VIQR.

Sau khi cài, đăng xuất và đăng nhập lại để bắt đầu gõ tiếng Việt.

%install
rm -rf %{buildroot}

# Binaries
install -Dm 755 %{_sourcedir}/vi-daemon  %{buildroot}/usr/lib/vhttechkey/vi-daemon
install -Dm 755 %{_sourcedir}/vi-ui      %{buildroot}/usr/lib/vhttechkey/vi-ui
install -Dm 755 %{_sourcedir}/vi-tools   %{buildroot}/usr/lib/vhttechkey/vi-tools

# IBus
install -Dm 644 %{_sourcedir}/ibus/vhttechkey-daemon.xml \
    %{buildroot}/usr/share/ibus/component/vhttechkey-daemon.xml

# Fcitx5
install -Dm 644 %{_sourcedir}/fcitx5/vhttechkey-daemon.conf \
    %{buildroot}/usr/share/fcitx5/inputmethod/vhttechkey-daemon.conf

# Systemd
install -Dm 644 %{_sourcedir}/systemd/vhttechkey-daemon.service \
    %{buildroot}/usr/lib/systemd/user/vhttechkey-daemon.service

# Desktop entry
install -Dm 644 %{_sourcedir}/desktop/vhttechkey-ui.desktop \
    %{buildroot}/usr/share/applications/vhttechkey-ui.desktop

%files
/usr/lib/vhttechkey/vi-daemon
/usr/lib/vhttechkey/vi-ui
/usr/lib/vhttechkey/vi-tools
/usr/share/ibus/component/vhttechkey-daemon.xml
/usr/share/fcitx5/inputmethod/vhttechkey-daemon.conf
/usr/lib/systemd/user/vhttechkey-daemon.service
/usr/share/applications/vhttechkey-ui.desktop

%post
# Đăng ký IBus engine và enable service
if command -v ibus &>/dev/null; then
    ibus restart 2>/dev/null || true
fi
systemctl daemon-reload 2>/dev/null || true
update-desktop-database /usr/share/applications 2>/dev/null || true

# Enable service cho user đang đăng nhập
if command -v loginctl &>/dev/null; then
    while IFS= read -r uid; do
        username=$(id -un "$uid" 2>/dev/null) || continue
        sudo -u "$username" \
            XDG_RUNTIME_DIR="/run/user/$uid" \
            DBUS_SESSION_BUS_ADDRESS="unix:path=/run/user/$uid/bus" \
            systemctl --user enable --now vhttechkey-daemon 2>/dev/null || true
    done < <(loginctl list-users --no-legend 2>/dev/null | awk '{print $1}')
fi

echo ""
echo "╔══════════════════════════════════════════════════════╗"
echo "║       ✓ Cài đặt VHTTechKey thành công!              ║"
echo "╠══════════════════════════════════════════════════════╣"
echo "║  Vui lòng ĐĂNG XUẤT và ĐĂNG NHẬP LẠI để bắt đầu   ║"
echo "║  gõ tiếng Việt.                                      ║"
echo "║  Phím tắt: Ctrl+Space để bật/tắt bộ gõ              ║"
echo "╚══════════════════════════════════════════════════════╝"

%preun
if [ $1 -eq 0 ]; then
    # Gỡ hoàn toàn (không phải upgrade)
    if command -v loginctl &>/dev/null; then
        while IFS= read -r uid; do
            username=$(id -un "$uid" 2>/dev/null) || continue
            sudo -u "$username" \
                XDG_RUNTIME_DIR="/run/user/$uid" \
                DBUS_SESSION_BUS_ADDRESS="unix:path=/run/user/$uid/bus" \
                systemctl --user disable --now vhttechkey-daemon 2>/dev/null || true
        done < <(loginctl list-users --no-legend 2>/dev/null | awk '{print $1}')
    fi
    if command -v ibus &>/dev/null; then
        ibus restart 2>/dev/null || true
    fi
fi

%changelog
* Thu May 22 2025 VHT Tech <vinhhp@vhttech.com> - 0.1.0-1
- Phiên bản đầu tiên
