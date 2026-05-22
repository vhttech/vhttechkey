# Hướng dẫn migration: vi-* → vime-* (dự kiến 0.2.0)

> **Trạng thái**: Crate `vime-*` chưa tồn tại. Bản phát hành hiện tại (0.1.x)
> chỉ có binary và crate `vi-*`. Tài liệu này mô tả kế hoạch đổi tên để
> script và gói downstream chuẩn bị trước.

## Binary (dự kiến)

| Hiện tại (0.1.x) | Dự kiến (0.2.0) |
|---|---|
| `vi-daemon` | `vime-daemon` |
| `vi-tools` | `vime-tools` |
| `vi-ui` | `vime-ui` |

## Thư mục cấu hình

Thư mục cấu hình đã là `~/.config/vime/` từ bản 0.1.x —
daemon đọc `~/.config/vime/config.toml` lúc khởi động. Không cần migration ở đây.

## Vì sao vime-*?

Chuỗi `vime-*` sẽ bổ sung nhiều cải tiến so với `vi-*` hiện tại:

- **Tích hợp xkbcommon** trong backend X11 — xử lý dead-key và AltGr đúng
  thay vì bản đồ US QWERTY cứng
- **Phát hiện lặp phím** — giữ phím không còn tạo bước composition trùng
- **Callback preedit** — ứng dụng nhận sự kiện thay đổi preedit chi tiết
  thay vì thay thế toàn chuỗi

## Ánh xạ crate (dự kiến)

| Hiện tại (`vi-*`) | Dự kiến (`vime-*`) |
|---|---|
| `vi-core` | `vime-core` |
| `vi-daemon` | `vime-daemon` |
| `vi-ibus` | `vime-ibus` |
| `vi-fcitx5` | `vime-fcitx5` |
| `vi-wayland` | `vime-wayland` |
| `vi-x11` | `vime-x11` |
| `vi-config` | `vime-config` |
| `vi-testing` | `vime-tests` |
| `vi-platform` | _(sẽ gộp vào `vime-core`)_ |
| `vi-portal` | _(sẽ gộp vào `vime-daemon`)_ |
| `vi-tools` | `vime-tools` |
| `vi-ui` | `vime-ui` |
