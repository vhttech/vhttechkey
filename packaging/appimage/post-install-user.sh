#!/usr/bin/env bash
# Chạy với quyền user thường (không cần root).
# Enable systemd service và đăng ký IBus engine cho user hiện tại.
set -euo pipefail

IME_FRAMEWORK="${1:-none}"

enable_systemd_service() {
    if systemctl --user enable --now vhttechkey-daemon 2>/dev/null; then
        echo "  ✓ Daemon đã được kích hoạt tự động khởi động"
    else
        echo "  (Daemon sẽ tự khởi động sau khi đăng nhập lại)"
    fi
}

register_ibus_engine() {
    [[ "$IME_FRAMEWORK" != "ibus" ]] && return
    command -v gsettings &>/dev/null || return

    local current
    current=$(gsettings get org.freedesktop.ibus.general preload-engines 2>/dev/null || echo "@as []")

    if [[ "$current" == *"vhttechkey"* ]]; then
        echo "  ✓ IBus engine đã được đăng ký"
        return
    fi

    local new_list
    if [[ "$current" == "@as []" || "$current" == "[]" ]]; then
        new_list="['xkb:us::eng', 'vhttechkey']"
    else
        new_list="${current%]}, 'vhttechkey']"
    fi

    gsettings set org.freedesktop.ibus.general preload-engines "$new_list" 2>/dev/null || true
    gsettings set org.freedesktop.ibus.general engines-order "$new_list" 2>/dev/null || true
    echo "  ✓ Đã đăng ký VHTTechKey vào danh sách IME của IBus"
}

set_env_hint() {
    local profile_file="$HOME/.profile"
    local marker="# vhttechkey-ime-env"

    if grep -q "$marker" "$profile_file" 2>/dev/null; then
        return
    fi

    if [[ "$IME_FRAMEWORK" == "ibus" ]]; then
        cat >> "$profile_file" << 'EOF'

# vhttechkey-ime-env
export GTK_IM_MODULE=ibus
export QT_IM_MODULE=ibus
export XMODIFIERS=@im=ibus
EOF
        echo "  ✓ Đã thêm biến môi trường IBus vào ~/.profile"
    fi
}

enable_systemd_service
register_ibus_engine
set_env_hint
