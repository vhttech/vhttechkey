# Tương thích Wayland

VHTTechKey dùng giao thức **zwp_text_input_v3** cho hỗ trợ bộ gõ trên Wayland
(crate `vi-wayland`). Hành vi compositor khác nhau; bảng này ghi quirk đã biết
và cách xử lý.

## Bảng tương thích compositor

| Compositor | Phiên bản giao thức | Quirk đã biết | Cách xử lý | Trạng thái test |
|---|---|---|---|---|
| **GNOME Shell** (Mutter ≥ 44) | text-input-v3 | `commit_string` bị bỏ qua nếu không có `preedit_string` cùng serial; gửi `leave` thừa khi raise cửa sổ | Luôn gửi `preedit_string("")` trước `commit_string`; gửi lại `activate` trên `enter` sau khi restore focus | ✅ Pass |
| **KDE Plasma** (KWin ≥ 5.27) | text-input-v3 | Gợi ý content-type không truyền tới IM; offset `surrounding_text` theo byte, không theo ký tự | Bỏ qua content-type; coi mọi offset là byte và chuyển sang ranh giới ký tự | ✅ Pass |
| **Sway** (wlroots ≥ 0.17) | text-input-v3 | Đúng giao thức; không quirk | Không cần | ✅ Pass |
| **Hyprland** (≥ 0.35) | text-input-v3 | Sự kiện `done` tới trước cập nhật `preedit_string` khi gõ nhanh; cursor rect đôi khi (0,0) | Buffer cập nhật `preedit_string`; bỏ qua cursor rect nếu cả hai tọa độ bằng 0 | ✅ Pass |
| **river** (wlroots ≥ 0.17) | text-input-v3 | Giống Sway | Không cần | ✅ Pass |
| **Weston** (≥ 12.0) | text-input-v3 | Triển khai tham chiếu; cần thứ tự sự kiện giao thức chặt | Tuân thủ thứ tự giao thức chính xác | ✅ Pass |
| **labwc** (≥ 0.7) | text-input-v3 | Không lỗi giao thức; không hỗ trợ định vị popup IME | Tắt định vị cửa sổ candidate | ⚠ Một phần |
| **Mir** (Ubuntu 23.10+) | text-input-v3 | Không gửi `surrounding_text` | Hoạt động không có ngữ cảnh surrounding-text | ⚠ Một phần |
| **GNOME Shell** (Mutter 42–43) | text-input-v3 sớm | Giá trị enum `text_change_cause` khác | Ánh xạ enum sang tương đương v3-final | 🔶 Legacy |
| **Enlightenment** | text-input-v1 | Chỉ v1; không hỗ trợ v3 | Fallback backend X11 qua XWayland | ❌ Không native |
| **Gamescope** | none | Không hỗ trợ giao thức IME | Không hỗ trợ nhập văn bản ở chế độ game | ❌ N/A |

### Chú giải trạng thái

| Ký hiệu | Ý nghĩa |
|---|---|
| ✅ Pass | Mọi test thủ công và tự động pass |
| ⚠ Một phần | Gõ cơ bản ổn; một số tính năng (định vị candidate, surrounding text) suy giảm |
| 🔶 Legacy | Có workaround; chỉ test trên distro LTS với compositor cũ |
| ❌ Không native | Fallback backend X11 hoặc không hỗ trợ |

## Test compositor

Để xác minh VHTTechKey chạy với compositor, chạy bộ test thủ công:

```bash
# 1. Khởi động vi-daemon trong phiên Wayland
vi-daemon &

# 2. Mở trình soạn thảo (ví dụ foot terminal + nano)
foot nano /tmp/test.txt

# 3. Gõ chuỗi test Telex và xác minh kết quả
#    Gõ: viet nam  → mong đợi: việt nam
#    Gõ: khong  → mong đợi: không

# 4. Kiểm tra NFC
python3 -c "
import unicodedata, sys
text = open('/tmp/test.txt').read()
bad = [hex(ord(c)) for c in text if unicodedata.normalize('NFC', c) != c]
print('NFD chars:', bad if bad else 'none — all NFC')
"
```

## Quirk compositor

Bảng dưới ghi quirk được phát hiện bởi `crates/vi-wayland/src/quirks.rs` và
biện pháp giảm thiểu áp dụng lúc runtime. Phát hiện bằng cách kiểm tra global
Wayland quảng bá trong registry; biến môi trường `VIME_COMPOSITOR_PROFILE` ghi đè
heuristic để debug.

| Compositor | Quirk (cờ `CompositorQuirks`) | Tín hiệu phát hiện | Biện pháp |
|---|---|---|---|
| **GNOME Shell** (Mutter ≥ 44) / **Cinnamon** | `empty_preedit_before_commit` — `commit_string` bị bỏ im lặng khi không có `preedit_string` cùng serial | Có `zwp_text_input_manager_v3`, không có `kde_output_management_v2` (GNOME); global `cinnamon_shell_v1` (Cinnamon fast-path) | Luôn gửi `preedit_string("")` ngay trước `commit_string` |
| **KDE Plasma** (KWin ≥ 5.27) | `snap_cursor_to_char_boundary` — offset byte `surrounding_text` có thể cắt giữa codepoint nhiều byte | Global `kde_output_management_v2` | Snap offset byte về ranh giới ký tự UTF-8 gần nhất qua `snap_to_char_boundary()` |
| **Hyprland** (≥ 0.35) / **labwc** | `buffer_preedit_updates` — flush socket bị trì hoãn khi cập nhật preedit nhanh | `hyprland_global_shortcuts_manager_v1` (Hyprland); `labwc_options_v1` (labwc fast-path, ≥ 0.7) | Gom ghi socket preedit; trì hoãn flush đến khi ổn định |
| **labwc** (≥ 0.7) | `suppress_candidate_position` — không hỗ trợ định vị popup IME | Global `labwc_options_v1` | Bỏ hoàn toàn lệnh định vị cửa sổ candidate |
| **Niri** | `niri_dual_protocol` — vòng đời `zwp_input_method_v2` và `zwp_text_input_v3` phải đồng quản lý | Global `niri_ipc` (fast-path) | Quản lý chuỗi enable/disable cả hai giao thức cùng lúc |
| **Mir** (Ubuntu 23.10+) | `no_surrounding_text` — không gửi sự kiện surrounding-text | Global `mir_shell` | Hoạt động không có ngữ cảnh surrounding-text |
| **XFCE** (xfwm4-wayland) | `delay_preedit_clear` — xóa preedit phải trì hoãn một vòng event-loop sau `commit_string` | Có `wp_viewporter`; không có `kde_output_management_v2`, `hyprland_global_shortcuts_manager_v1`, `wp_cursor_shape_manager_v1` | Trì hoãn xóa preedit một roundtrip |
| **LXQt** (Openbox-Wayland) | `virtual_keyboard_fallback` — không có `zwp_input_method_manager_v2` | Global compositor hiện đại nhưng thiếu `zwp_text_input_manager_v3` hoặc `zwp_input_method_manager_v2`; `wl_compositor` version ≥ 4 | Fallback giao thức virtual-keyboard |
| **Weston** (≥ 12.0) | Không — triển khai tham chiếu; cần thứ tự sự kiện giao thức chặt | Global `weston_screenshooter` | Tuân thủ thứ tự giao thức chính xác; không cần cờ quirk |
| **Sway** / **River** (wlroots ≥ 0.17) | Không — đúng giao thức; bộ đếm serial không được tràn im lặng | `zwlr_output_manager_v1` không có global Hyprland (Sway); `river_control_v1` (River) | Dùng `wrapping_add` cho bộ đếm serial |

## Thêm quirk compositor mới

1. Tái hiện lỗi trong `crates/vi-wayland/tests/integration_test.rs` với
   mock compositor (xem `tests/fixtures/mock_compositor.rs`).
2. Thêm phát hiện quirk trong `crates/vi-wayland/src/lib.rs` sau
   bitflag `CompositorQuirks`.
3. Ghi vào bảng này.
4. Thêm mục vào `docs/contributing.md` dưới "Thêm quirk compositor".
