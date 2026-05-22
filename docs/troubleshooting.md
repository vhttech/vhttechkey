# Xử lý sự cố

## Chữ bị lặp đôi

**Triệu chứng**: gõ `aa` ra `aâ` thay vì `â`, hoặc `dd` ra `dđ`.

**Nguyên nhân và cách khắc phục**:

1. **Hai IME chạy cùng lúc** (hay gặp nhất).
   Cả IBus và Fcitx5 đều đang bật, mỗi bên xử lý phím một lần.
   ```bash
   # Kiểm tra framework IME nào đang chạy
   pgrep -a ibus-daemon
   pgrep -a fcitx5
   # Dừng cái bạn không dùng
   pkill ibus-daemon  # hoặc pkill fcitx5
   ```

2. **`GTK_IM_MODULE` / `QT_IM_MODULE` không khớp**.
   Ứng dụng gửi phím tới IME khác với IME mà vi-daemon đăng ký.
   ```bash
   echo $GTK_IM_MODULE $QT_IM_MODULE
   # Phải là "ibus" hoặc "fcitx5", không trộn lẫn
   ```

3. **XWayland replay phím**.
   Trên Wayland với app XWayland, compositor có thể gửi lại phím sau khi IME commit.
   Cách xử lý: đặt `XMODIFIERS=@im=ibus` (hoặc fcitx5) trong phiên làm việc,
   không dùng `XMODIFIERS=@im=none`.

4. **vi-daemon chạy hai instance**.
   ```bash
   pgrep -c vi-daemon   # phải in ra 1
   pkill -o vi-daemon   # dừng instance cũ hơn
   ```

---

## Telex: ba phím `o` / `a` / `e` liên tiếp (giống UniKey)

**Đúng với Telex phổ biến** (UniKey, v.v.):

- `oo` → **ô** (hai phím `o` ghép thành ô)
- `ooo` → **oo** (phím thứ ba hủy ghép, cho ra hai chữ `o` thường — cần khi gõ từ mượn hoặc chuỗi cần hai `o` liền nhau)

Tương tự: `aa` → **â**, còn `aaa` → **aa**; `ee` → **ê**, còn `eee` → **ee**.

Nếu sau ba lần `o` bạn vẫn thấy **ôo** thay vì **oo**, hãy dùng bản vi-core đã có quy tắc “triple escape” này.

---

## Telex: vần **ươ** — một hay hai chữ `w`?

**Một chữ `w` (đúng với quy tắc “phụ âm + uo + w → ươ”)**  
Gõ **nguyên âm `u` rồi `o`**, sau đó **một lần `w`** sẽ gộp thành **ươ** (khi `u` đứng sau phụ âm, không phải cụm `qu`…). Ví dụ:

- `thuowng` → **thương**; `truowngf` → **trường** (một `w` sau `uo`).

**Hai chữ `w` (thói quen `uw` rồi `ow`)**  
Nếu bạn gõ **`u` rồi `w` trước** (`uw` → **ư**), sau đó **`o`** rồi **`w`** (`ow` → **ơ**), bạn đã dùng **hai quy tắc Telex khác nhau**, nên cần **hai lần `w`** — ví dụ `thuwowng` cũng ra **thương**, giống một số IME khác.

Tóm lại: để **chỉ một `w` cho cả vần ươ** sau phụ âm, hãy gõ **`…uo` rồi `w`**, đừng gõ `uw` trước khi có `o`.

**Gõ coda trước, `w` sau:** nếu bạn quen gõ **`thuong` rồi mới `w`** (`thuongw`), engine hiện tại cũng gộp thành **thương** (không còn lỗi `thuơng` do chỉ áp `ow`→ơ lên chữ `o`).

---

## Cổng composition (ASCII / spam — không delay)

Khi preedit **chưa có** ký tự tiếng Việt thật (đ, dấu thanh, khối U+1EA0…), engine có thể **không gọi** Telex/VNI nếu token hiện tại trông như **spam ASCII** hoặc **identifier**:

- Chuỗi xen kẽ **2 chữ cái** nhưng **ít nhất một chữ là phím thanh Telex** (`s` `f` `r` `x` `j` `z`), ví dụ `xox`, `xoxoxoxo` — tránh thanh `x`/`s`… bị áp nhầm và **retro** kiểu `oo`→`ô` giữa chuỗi spam. Chuỗi kiểu `toto` / `totos` (không chứa các chữ đó trong cặp xen kẽ) **không** bị chặn.
- Lặp mẫu ngắn ở cuối, chạy nguyên âm dài (`ooooo`), hoặc `_` + số / `::` / `->` / …

Sau khi đã có ký tự Việt trong preedit, cổng **mở** lại — gõ tiếng Việt bình thường.

### Kiểm tra từ điển khi commit

vi-daemon có thể nạp `vietnamese.cm.dict` (một từ một dòng). Trong profile đang active, mục **`spell_check`** (xem `vi-config`): khi commit, nếu preedit đã có chữ Việt nhưng **chuỗi NFC chữ thường không có trong từ điển**, IME **commit dãy phím Telex/VNI gốc** thay vì Unicode đã ghép.

- **`VIME_VIETNAMESE_DICT`**: đường dẫn tùy chỉnh tới file `.dict`.
- Đường dẫn mặc định được thử: `data/dictionaries/vietnamese.cm.dict` (cwd), `/usr/share/vhttechkey/data/dictionaries/vietnamese.cm.dict`, v.v.

**Chống bounce phím:** hai `KeyDown` cùng ký tự **liền nhau** (không có `KeyUp` giữa) luôn bị bỏ qua; gõ `x` rồi `o` rồi `x` nhanh **không** bị nuốt chữ (khác debounce theo ms cũ).

---

## Mất dấu thanh

**Triệu chứng**: gõ `viet` ra `viet` thay vì `việt`.

**Các bước chẩn đoán**:

1. Kiểm tra vi-daemon đang chạy và kết nối:
   ```bash
   # Gửi yêu cầu trạng thái trực tiếp
   echo '{"cmd":"status"}' | nc -U "$XDG_RUNTIME_DIR/vi-daemon.sock"
   ```
   Kết quả mong đợi: `{"type":"status","backend":"ibus","method":"telex","preedit":""}`

2. Xác nhận kiểu gõ đang active là Telex (không phải pass-through):
   ```bash
   echo '{"cmd":"set_method","method":"telex"}' | nc -U "$XDG_RUNTIME_DIR/vi-daemon.sock"
   ```

3. Kiểm tra IME đã bật cho ứng dụng:
   - IBus: bấm biểu tượng IBus và chọn engine **VHTTechKey**, không phải "English".
   - Fcitx5: khay hệ thống phải hiện "VI", không phải "EN".

4. Thử panel Typing Test trong vi-ui. Nếu panel hiện "việt" mà gedit không,
   lỗi nằm ở module GTK IM, không phải vi-daemon.

5. Đảm bảo `XMODIFIERS`, `GTK_IM_MODULE`, `QT_IM_MODULE` và `SDL_IM_MODULE`
   được đặt trong `~/.profile` hoặc `/etc/environment`:
   ```bash
   export GTK_IM_MODULE=ibus
   export QT_IM_MODULE=ibus
   export XMODIFIERS=@im=ibus
   ```
   Đăng xuất và đăng nhập lại sau khi thay đổi.

---

## Preedit bị kẹt

**Triệu chứng**: chuỗi preedit gạch chân vẫn còn trong app và không commit,
kể cả sau khi bấm Space hoặc Enter.

**Nguyên nhân và cách khắc phục**:

1. **vi-daemon crash giữa chừng khi đang soạn**.
   Preedit đã gửi tới app nhưng commit chưa tới.
   ```bash
   # Xóa preedit bằng cách khởi động lại vi-daemon
   pkill vi-daemon && vi-daemon &
   # Sau đó bấm Escape trong app bị ảnh hưởng để xóa preedit cũ
   ```

2. **App không hỗ trợ preedit** (terminal không có IME).
   Dùng `foot` hoặc `kitty` thay vì `xterm` trên Wayland; hoặc cấu hình
   chế độ XIM `overTheSpot` cho app X11.

3. **Fcitx5 input context chưa kích hoạt**.
   Một số app Qt cần cài `fcitx5-qt` riêng:
   ```bash
   sudo apt install fcitx5-frontend-qt6   # Ubuntu/Debian
   sudo dnf install fcitx5-qt             # Fedora
   ```

4. **Serial compositor không khớp** (chỉ Wayland).
   Preedit được gửi với serial cũ. Nâng vi-wayland lên ≥ 0.2.0 và
   compositor lên bản chấp nhận serial cũ.

---

## IBus: gạch chân khi gõ, Chrome khác Telegram / VS Code

**Triệu chứng**: ở Chromium thấy gạch chân (hoặc kiểu composition) rõ hơn; ở Telegram (Qt),
VS Code (Electron) hoặc gedit (GTK) có thể **không** gạch chân dù vẫn gõ Telex bình thường.

**Nguyên nhân**: VHTTechKey gửi `UpdatePreeditText` với `IBusAttrList` chứa thuộc tính
`IBUS_ATTR_TYPE_NONE` (mặc định *IBnoUnderline*): không
cố ý vẽ gạch chân. Toolkit và từng ứng dụng vẫn quyết định có hiển thị “đang soạn” hay
không — Chromium thường vẽ khác Gtk/Qt.

**Không cần cấu hình** nếu gõ đúng; đây là hành vi kỹ thuật, không phải lỗi IME.

---

## Chrome / Chromium: không ra chữ, lệch chữ, hoặc cần chỉnh đường IME

**Bối cảnh**: VHTTechKey trên IBus **không** tự bật `surrounding_commit` (`DeleteSurroundingText`)
hay `ForwardKeyEvent` theo bit `SetCapabilities` — mọi client mặc định dùng preedit
(`UpdatePreeditText` với `IBUS_ENGINE_PREEDIT_COMMIT` = 1 theo `ibustypes.h`). Nhánh
cũ dùng `DeleteSurroundingText` đã gây hỏng trên Chrome / Electron nên bị loại khỏi chọn
tự động.

**Nếu vẫn lỗi trên Chromium / XWayland**, thử lần lượt trong `~/.config/vime/config.toml`:

```toml
[ibus]
# Bật khi preedit chuẩn vẫn hỏng (từng bản Chromium / Ozone).
force_chrome_direct = true
```

Khi đã bật `force_chrome_direct` nhưng **một app** lại xấu hơn, dùng:

```toml
[ibus]
force_chrome_direct = true
force_preedit_mode = true   # tắt chrome_direct, về preedit chuẩn
```

Khởi động lại `vi-daemon` sau khi sửa cấu hình.

---

## Electron (VS Code, Slack, …): không có chữ tiếng Việt hoặc chữ biến mất khi commit

**Triệu chứng**: không thấy preedit, hoặc vừa “commit” thì ký tự biến mất. Hay gặp:
**gõ xong một từ** (Space, ngắt âm tiết, hoặc phím kết thúc từ) thì **cả cụm đang
soạn biến mất**, không còn chữ trong editor.

**Nguyên nhân**: Electron dùng stack IME của Chromium qua D-Bus; không dùng
`GTK_IM_MODULE`. Trên Wayland, đường `text-input-v3` ổn định hơn XWayland.

### Chrome / Telegram ổn nhưng VS Code (hay app Electron khác) vẫn mất chữ

Điều này **không** có nghĩa VHTTechKey “chỉ hỏng một app”: cùng một `vi-daemon`, nhưng
mỗi tiến trình dùng **đường IME khác**:

- **Chromium / Chrome**: tiến trình riêng; nhiều bản cài đã chạy **Ozone Wayland**
  native nên `text-input-v3` ổn.
- **Telegram Desktop**: **Qt** (`QT_IM_MODULE`), không đi qua stack Electron.
- **VS Code, Cursor, Slack, …**: **Electron**; nếu cửa sổ đang **XWayland** hoặc
  thiếu cờ Ozone, lớp IME có thể xử lý commit/preedit sai — biểu hiện điển hình là
  **mất cả từ ngay khi kết thúc gõ**.

**Việc nên làm** (ưu tiên từ trên xuống; tập trung sửa VS Code trước khi đổi cấu
hình IBus toàn cục):

1. Kiểm tra Electron: `vi-tools sandbox-status` khi VS Code đang mở. Nếu dòng
   `code` báo **`needs-flags`**, hãy luôn khởi động editor với Ozone Wayland:
   ```bash
   code --enable-features=UseOzonePlatform --ozone-platform=wayland
   ```
   Có thể gắn cố định bằng cách sửa file `.desktop` của VS Code (mục `Exec=`) hoặc
   shell alias để không quên cờ.

2. Môi trường chỉ X11: thử `ELECTRON_OZONE_PLATFORM_HINT=auto code` rồi gõ lại.

3. IBus: khi focus vào VS Code, đảm bảo chế độ **Input Method** (bật VHTTechKey), **không**
   để **Direct Input** cho cửa sổ đó.

4. Nếu sau các bước trên **vẫn** mất chữ, thử trong `~/.config/vime/config.toml`:

   ```toml
   [ibus]
   force_chrome_direct = true
   ```

   Sau đó **khởi động lại `vi-daemon`**. Đây là tùy chọn **toàn cục**; Chrome và
   Telegram thường vẫn ổn. Nếu một ứng dụng khác lại xấu đi, xem mục **Chrome /
   Chromium** phía trên (dùng `force_preedit_mode = true` cùng khối `[ibus]`).

5. Cuối cùng: mở VS Code từ terminal với `XMODIFIERS=@im=ibus code` (tránh snap /
   môi trường thiếu biến XMODIFIERS).

**Ghi chú**: VHTTechKey trên IBus gửi **`CommitText` rồi mới `HidePreeditText`** khi kết thúc
từ (thứ tự commit của VHTTechKey), vì một số bản Electron (VS Code) xóa cả
cụm preedit nếu nhận `HidePreeditText` trước. Nếu vẫn lỗi sau khi cập nhật bản build,
phần còn lại thường là **Electron phải nhận IME đúng phiên** (Ozone Wayland, v.v.).

---

## CPU cao

**Triệu chứng**: `vi-daemon` hoặc `ibus-daemon` tiêu thụ >5% CPU khi rảnh.

**Các bước chẩn đoán**:

1. Kiểm tra VHTTechKey có poll quá dày không:
   ```bash
   strace -p $(pgrep vi-daemon) -e trace=epoll_wait,read,write 2>&1 | head -40
   ```
   Bạn nên thấy `epoll_wait` block với timeout ~1000 ms, không quay vòng liên tục.

2. Kiểm tra preedit bị kẹt:
   ```bash
   echo '{"cmd":"status"}' | nc -U "$XDG_RUNTIME_DIR/vi-daemon.sock"
   # Nếu "preedit" khác rỗng, engine đang giữ trạng thái không cần thiết
   ```
   Reset bằng:
   ```bash
   echo '{"cmd":"set_method","method":"telex"}' | nc -U "$XDG_RUNTIME_DIR/vi-daemon.sock"
   ```

3. Kiểm tra vòng lặp log spam:
   ```bash
   journalctl -u vi-daemon -f --since "1 minute ago"
   ```
   Nếu thấy lỗi lặp >10 lần/giây, có vòng reconnect. Kiểm tra `$XDG_RUNTIME_DIR`
   ghi được và đường socket không còn stale.

4. Profile:
   ```bash
   perf record -g -p $(pgrep vi-daemon) -- sleep 10
   perf report
   ```
   Hot path thường gặp khi rảnh: chuẩn hóa Unicode mỗi phím (~0.1 ms, bình thường),
   poll D-Bus zbus (~0.5% CPU, bình thường).

5. Tắt file watcher `notify` nếu không cần hot-reload cấu hình:
   ```bash
   # Trong ~/.config/vime/config.toml
   [watcher]
   enabled = false
   ```

---

## Tự phát hiện sandbox (Electron, Flatpak, Snap)

vi-daemon tự quét `/proc` lúc khởi động để phát hiện ứng dụng trong sandbox
và ghi cảnh báo kèm hướng xử lý.

### Đọc log daemon

```bash
journalctl -u vi-daemon --since today | grep -E 'WARN|electron|flatpak|snap'
```

**Electron trên Wayland** in dòng `WARN` kiểu:

```
WARN vi_daemon: Electron process detected on Wayland pid=12345
     relaunch with `--ozone-platform=wayland --enable-features=UseOzonePlatform`
     to enable IME input
```

Áp dụng gợi ý bằng cách sửa launcher `.desktop` của ứng dụng hoặc
truyền cờ trên dòng lệnh:
```bash
code --ozone-platform=wayland --enable-features=UseOzonePlatform
```

**App Flatpak** in dòng `INFO` kèm app-id. Nếu portal
`org.freedesktop.portal.InputMethod` chưa cài, daemon cũng ghi `WARN`; cài
`xdg-desktop-portal` và backend phù hợp (ví dụ `xdg-desktop-portal-gnome`).

### Kiểm tra trạng thái sandbox bằng vi-tools

```bash
vi-tools sandbox-status
```

Ví dụ kết quả:

```
PID       TYPE          IME STATUS
-------------------------------------------------------
12345     electron      needs-flags (--ozone-platform=wayland)
67890     flatpak       OK (portal) [org.gnome.Gedit]
11223     snap          unsupported [firefox]
```

Giá trị trạng thái:
- **needs-flags** — tiến trình Electron; khởi động lại với cờ hiển thị.
- **OK (portal)** — app Flatpak; đường IME qua portal khả dụng.
- **unsupported** — app Snap; tích hợp IME trực tiếp không được hỗ trợ; thử
  `XMODIFIERS=@im=ibus` nếu snap cho phép.

### Giao diện chẩn đoán D-Bus

Khi phát hiện tiến trình Electron, vi-daemon đăng ký object D-Bus để
thành phần desktop truy vấn:

```bash
dbus-send --session --print-reply \
  --dest=org.freedesktop.vime \
  /org/freedesktop/vime/Diagnostics \
  org.freedesktop.vime.Diagnostics1.SuggestElectronFlags
```

Phản hồi là mảng chuỗi cờ khuyến nghị, ví dụ
`["--ozone-platform=wayland", "--enable-features=UseOzonePlatform"]`.
