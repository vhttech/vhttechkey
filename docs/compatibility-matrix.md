# Ma trận tương thích

Mỗi ô cho biết trạng thái test của tổ hợp compositor × backend kiểu gõ.
Quirk compositor ảnh hưởng backend Wayland trực tiếp được xử lý trong
`crates/vi-wayland/src/lib.rs` qua cờ feature lúc biên dịch
(`gnome`, `kwin`, `hyprland`).

## Ma trận

| Compositor | IBus | Fcitx5 | Wayland trực tiếp | X11-XIM | Ghi chú |
|---|---|---|---|---|---|
| **GNOME Shell** (Mutter ≥ 44) | ✅ | ✅ | ✅ | ⚠ | Wayland: preedit rỗng phải gửi trước commit; kích hoạt lại khi restore focus (`gnome`). X11-XIM: chỉ app XWayland. |
| **KDE Plasma** (KWin ≥ 5.27) | ✅ | ✅ | ✅ | ⚠ | Wayland: offset `surrounding_text` theo byte, không theo ký tự (`kwin`). X11-XIM: chỉ app XWayland. |
| **Hyprland** (≥ 0.35) | ✅ | ✅ | ✅ | ❌ | Wayland: fallback `zwp_virtual_keyboard_v1` khi thiếu `zwp_input_method_manager_v2`; buffer preedit khi gõ nhanh (`hyprland`). |
| **Sway** (wlroots ≥ 0.17) | ✅ | ✅ | ✅ | ❌ | Đúng giao thức; không cần quirk. |
| **Niri** | ❓ | ❓ | ❓ | ❌ | Có text-input-v3; chưa test — dự kiến chạy không cần quirk. |
| **Weston** (≥ 12.0) | ❓ | ❓ | ✅ | ❌ | Triển khai tham chiếu; cần thứ tự sự kiện giao thức chặt. Phát hiện qua global `weston_screenshooter`. |
| **XFCE** (xfwm4-wayland) | ✅ | ✅ | ⚠ | ✅ | Wayland: quirk `delay_preedit_clear` (phát hiện qua `wp_viewporter`). X11-XIM: native trên xfwm4 cổ điển. |
| **Cinnamon** (Muffin ≥ 6.0) | ✅ | ✅ | ⚠ | ✅ | Wayland: quirk `empty_preedit_before_commit` (phát hiện qua `cinnamon_shell_v1`). X11-XIM: native trên Cinnamon X11. |

## Chú giải

| Ký hiệu | Ý nghĩa |
|---|---|
| ✅ | Đã test và pass |
| ⚠ | Một phần — gõ cơ bản ổn; một số tính năng suy giảm (xem Ghi chú) |
| ❓ | Chưa test / chưa rõ |
| ❌ | Không hỗ trợ |

## Ghi chú backend

**IBus** — xử lý bởi crate `vi-ibus` qua giao thức D-Bus IBus.
Chạy trên mọi compositor/desktop có daemon IBus.

**Fcitx5** — xử lý bởi crate `vi-fcitx5` qua giao thức D-Bus Fcitx5.
Chạy trên mọi compositor/desktop có daemon Fcitx5.

**Wayland trực tiếp** — xử lý bởi crate `vi-wayland` dùng
`zwp_input_method_v2` + `zwp_text_input_v3`. Quirk theo compositor áp dụng
qua cờ feature lúc biên dịch; xem `crates/vi-wayland/src/lib.rs` để biết chi tiết
triển khai và `docs/wayland-compat.md` để xem danh sách quirk đầy đủ.

**X11-XIM** — xử lý bởi crate `vi-x11` qua giao thức X Input Method.
Có native trên compositor X11 (Xfwm, Cinnamon, …) và cho ứng dụng XWayland
chạy trong phiên Wayland (GNOME, KDE Plasma).
