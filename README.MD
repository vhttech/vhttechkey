# VHTTechKey — Bộ gõ tiếng Việt

Bộ gõ tiếng Việt hiện đại, nhận biết compositor, dành cho Linux — viết bằng Rust.

---

## Cài đặt nhanh

> **Dành cho người dùng.** Không cần biết lập trình, không cần cài Rust.

### Cách 1: AppImage — Khuyên dùng (mọi distro)

1. Tải file từ trang [Releases](../../releases/latest):
   **`vhttechkey-installer-linux-x86_64.AppImage`**
2. Mở terminal, chạy:
   ```bash
   chmod +x vhttechkey-installer-linux-x86_64.AppImage
   ./vhttechkey-installer-linux-x86_64.AppImage
   ```
3. Nhập mật khẩu khi được hỏi.
4. **Đăng xuất và đăng nhập lại** → gõ tiếng Việt ngay!

> Hoặc double-click file AppImage trong trình quản lý file (cần FUSE được cài).

---

### Cách 2: Ubuntu / Debian / Linux Mint

1. Tải file **`vhttechkey_0.1.0_amd64.deb`** từ trang [Releases](../../releases/latest).
2. Double-click vào file `.deb` → Ubuntu Software mở → bấm **Install**.
3. **Đăng xuất và đăng nhập lại.**

Hoặc qua terminal:
```bash
sudo dpkg -i vhttechkey_0.1.0_amd64.deb
```

---

### Cách 3: Fedora / RHEL / openSUSE

```bash
sudo rpm -i vhttechkey-0.1.0-1.x86_64.rpm
```

---

### Sau khi cài

| Thao tác | Cách làm |
|---|---|
| Bật/tắt bộ gõ | `Ctrl+Space` |
| Mở cài đặt | Tìm **VHTTechKey Settings** trong menu ứng dụng |
| Kiểm tra hoạt động | Mở gedit, gõ `viet nam` → nhận được `việt nam` |

---

## Tính năng

- **Ba kiểu gõ tích hợp**: Telex, VNI, VIQR — chuyển đổi không cần khởi động lại
- **Bộ quy tắc tùy chỉnh**: định nghĩa ánh xạ phím → ký tự trong TOML
- **Profile đặt tên**: tạo profile theo ngôn ngữ hoặc ứng dụng, xuất/nhập dạng TOML
- **Cấu hình thời gian thực**: mọi thay đổi áp dụng ngay qua IPC — không cần đăng xuất
- **Wayland-native**: dùng `zwp_text_input_v3` kèm workaround theo từng compositor
- **Hỗ trợ X11**: XIM cho ứng dụng cũ, song song với Wayland hiện đại
- **IBus và Fcitx5**: engine cắm vào hai framework IME phổ biến trên Linux
- **Đầu ra NFC đảm bảo**: mọi văn bản commit đều qua pipeline chuẩn hóa Unicode
- **Giao diện cài đặt egui**: bản đồ nhiệt bàn phím, bài test gõ với phân tích NFC, xem log

---

## Dành cho Developer

### Yêu cầu build

```bash
# Debian / Ubuntu
sudo apt install \
    libdbus-1-dev libglib2.0-dev libibus-1.0-dev libfcitx5-qt-dev \
    libwayland-dev wayland-protocols libxcb1-dev libgl1-mesa-dev pkg-config

# Fedora
sudo dnf install \
    dbus-devel glib2-devel ibus-devel fcitx5-devel \
    wayland-devel wayland-protocols-devel libxcb-devel mesa-libGL-devel
```

Cần Rust toolchain (stable) — xem [docs/contributing.md](docs/contributing.md) để biết
đầy đủ phụ thuộc, các bước build và công cụ tùy chọn (Miri, fuzz).

### Bắt đầu nhanh (build từ source)

Năm lệnh để bắt đầu gõ:

```bash
# 1. Khởi động daemon
vi-daemon &

# 2. Chọn kiểu gõ Telex
vi-tools set-method telex

# 3. Cấu hình phiên làm việc (thêm vào ~/.profile)
export GTK_IM_MODULE=ibus
export QT_IM_MODULE=ibus
export XMODIFIERS=@im=ibus

# 4. Mở giao diện cài đặt
vi-ui &

# 5. Mở trình soạn thảo bất kỳ và gõ thử:
#    viet nam  →  việt nam
#    khong     →  không
#    truong    →  trường
```

## Tài liệu

| Tài liệu | Mô tả |
|---|---|
| [docs/architecture.md](docs/architecture.md) | Sơ đồ lớp, luồng dữ liệu, đồ thị phụ thuộc crate |
| [docs/unicode-pipeline.md](docs/unicode-pipeline.md) | Vì sao dùng NFC, các bước pipeline, lỗi mã hóa thường gặp |
| [docs/wayland-compat.md](docs/wayland-compat.md) | Bảng tương thích compositor và quirk đã biết |
| [docs/ibus-fcitx5.md](docs/ibus-fcitx5.md) | Sơ đồ trình tự IBus/Fcitx5 (gõ, focus, khởi động lại) |
| [docs/troubleshooting.md](docs/troubleshooting.md) | Chữ đôi, mất dấu, preedit kẹt, VS Code/Electron vs Chrome, CPU cao |
| [docs/contributing.md](docs/contributing.md) | Build từ source, test, thêm kiểu gõ, quirk compositor |
| [docs/verification.md](docs/verification.md) | Checklist phát hành (test, Miri, fuzz, E2E thủ công, Valgrind) |
| [docs/sequence-diagrams.md](docs/sequence-diagrams.md) | Sơ đồ trình tự Mermaid: gõ, focus-out, restart, đổi kiểu gõ |
| [docs/state-machine.md](docs/state-machine.md) | Sơ đồ và bảng chuyển trạng thái CompositionEngine |

## Phím tắt (mặc định)

| Thao tác | Phím |
|---|---|
| Bật/tắt IME | `Ctrl+Space` |
| Profile kế tiếp | `Ctrl+Shift+.` |
| Profile trước | `Ctrl+Shift+,` |
| Xóa preedit | `Escape` |

Mọi phím tắt đều cấu hình được trong vi-ui → Key Map Editor.

## Giấy phép

Dự án được cấp phép theo [GNU General Public License v3.0 hoặc mới hơn](LICENSE)
(GPL-3.0-or-later). Xem file [LICENSE](LICENSE) để đọc toàn văn.

Tài nguyên bên thứ ba đi kèm có thể dùng giấy phép khác (ví dụ font Hack trong
`data/fonts/Hack/` và từ điển tiếng Việt trong `data/dictionaries/`).
