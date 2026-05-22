# Đóng góp

## Build từ source

### Yêu cầu trước

```bash
# Rust toolchain (stable + nightly cho Miri/fuzz)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
rustup toolchain install nightly
rustup component add miri --toolchain nightly

# Thư viện hệ thống (Ubuntu/Debian)
sudo apt install \
    libdbus-1-dev \
    libglib2.0-dev \
    libibus-1.0-dev \
    libfcitx5-qt-dev \
    libwayland-dev \
    wayland-protocols \
    libxcb1-dev \
    libgl1-mesa-dev \
    pkg-config

# Thư viện hệ thống (Fedora)
sudo dnf install \
    dbus-devel \
    glib2-devel \
    ibus-devel \
    fcitx5-devel \
    wayland-devel \
    wayland-protocols-devel \
    libxcb-devel \
    mesa-libGL-devel
```

### Build

```bash
git clone ssh://git@git.hocitvn.com:22222/vhttech/miliondolar/vhttechkey.git
cd vhttechkey

# Build debug (toàn bộ crate)
cargo build --workspace

# Build release
cargo build --workspace --release

# Chỉ build giao diện cài đặt
cargo build -p vi-ui --release

# Chỉ build daemon
cargo build -p vi-daemon --release
```

Binary daemon ở `target/release/vi-daemon`, UI ở
`target/release/vi-ui`. Công cụ CLI ở `target/release/vi-tools`.

## Chạy test

```bash
# Toàn bộ unit test và integration test
cargo test --workspace

# Một crate
cargo test -p vi-core

# Có output (hữu ích khi debug)
cargo test --workspace -- --nocapture

# Lint (phải sạch — CI bắt buộc -D warnings)
cargo clippy --workspace -- -D warnings

# Kiểm tra format
cargo fmt --check

# Miri (an toàn bộ nhớ, chỉ nightly)
cargo +nightly miri test -p vi-core

# Fuzz (cần cargo-fuzz)
cargo install cargo-fuzz
cargo fuzz run fuzz_key_sequence -- -max_total_time=60
cargo fuzz run fuzz_config -- -max_total_time=60
cargo fuzz run fuzz_unicode_pipeline -- -max_total_time=60
```

## Thêm bộ quy tắc kiểu gõ mới

Kiểu gõ nằm trong `crates/vi-core/src/methods/`.

1. **Tạo file quy tắc** — sao chép `telex.rs` làm mẫu:
   ```bash
   cp crates/vi-core/src/methods/telex.rs crates/vi-core/src/methods/mymethod.rs
   ```

2. **Triển khai trait**:
   ```rust
   // crates/vi-core/src/methods/mymethod.rs
   use crate::{InputEvent, StateTransition};
   use super::MethodEngine;

   pub struct MyMethod { /* trường bảng quy tắc */ }

   impl MethodEngine for MyMethod {
       fn name(&self) -> &'static str { "mymethod" }
       fn process(&mut self, event: &InputEvent) -> StateTransition { /* … */ }
       fn reset(&mut self) { /* xóa trạng thái */ }
   }
   ```

3. **Đăng ký** trong `crates/vi-core/src/methods/mod.rs`:
   ```rust
   mod mymethod;
   pub use mymethod::MyMethod;

   impl InputMethod {
       pub fn engine(&self) -> Box<dyn MethodEngine> {
           match self {
               // … nhánh hiện có …
               InputMethod::MyMethod => Box::new(MyMethod::new()),
           }
       }
   }
   ```

4. **Thêm biến thể** vào enum `InputMethod` trong `types.rs`.

5. **Viết golden test** trong `crates/vi-core/tests/` — xem `syllables.rs` làm
   mẫu. Thêm ít nhất:
   - Mọi dấu thanh trên ít nhất 3 lớp nguyên âm
   - Xử lý Backspace
   - Round-trip: văn bản commit là NFC

6. **Thêm fuzz target** trong `fuzz/fuzz_targets/` nạp byte ngẫu nhiên vào
   kiểu gõ mới và khẳng định đầu ra là NFC.

7. **Ghi tài liệu** bảng quy tắc trong `docs/unicode-pipeline.md`.

## Thêm quirk compositor

Workaround theo compositor nằm trong `crates/vi-wayland/src/lib.rs` sau
struct bitflag `CompositorQuirks`.

1. **Tái hiện** lỗi trong `crates/vi-wayland/tests/integration_test.rs` bằng
   mock compositor:
   ```rust
   #[test]
   fn gnome_commit_without_preedit_quirk() {
       let mut session = MockCompositorSession::new(CompositorKind::Gnome);
       session.quirks |= CompositorQuirks::GNOME_PREEDIT_REQUIRED_BEFORE_COMMIT;
       // … khẳng định hành vi đúng có/không có workaround
   }
   ```

2. **Phát hiện compositor** trong `detect_compositor()` (đã đọc
   `$XDG_CURRENT_DESKTOP` và tên `wl_compositor` server quảng bá).

3. **Áp dụng workaround** trong handler giao thức tương ứng:
   ```rust
   if self.quirks.contains(CompositorQuirks::GNOME_PREEDIT_REQUIRED) {
       self.send_empty_preedit_before_commit();
   }
   ```

4. **Cập nhật** `docs/wayland-compat.md` với hàng mới trong bảng tương thích.

5. **Cập nhật** `docs/troubleshooting.md` nếu quirk gây triệu chứng người dùng
   thấy được, đáng thêm mục xử lý sự cố.

## Quy ước commit

- Dòng subject: thì mệnh lệnh, ≤72 ký tự, không dấu chấm cuối.
- Body: giải thích *vì sao*, không phải *làm gì* (diff đã cho thấy làm gì).
- Tham chiếu issue: `Fixes #123` hoặc `Part of #456`.
- Không merge commit trên `main`; rebase nhánh trước khi mở PR.

## CI

GitHub Actions chạy trên mỗi PR:

1. `cargo test --workspace`
2. `cargo clippy --workspace -- -D warnings`
3. `cargo fmt --check`
4. `cargo +nightly miri test -p vi-core`
5. Fuzz 60 giây trên mỗi target

Mọi kiểm tra phải pass trước khi merge.
