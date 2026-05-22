# Troubleshooting

## Characters appear doubled

**Symptom**: typing `aa` produces `aâ` instead of `â`, or `dd` produces `dđ`.

**Causes and fixes**:

1. **Two IMEs running simultaneously** (most common).
   Both IBus and Fcitx5 are active and each processes the key once.
   ```bash
   # Check which IME frameworks are running
   pgrep -a ibus-daemon
   pgrep -a fcitx5
   # Stop the one you don't use
   pkill ibus-daemon  # or pkill fcitx5
   ```

2. **`GTK_IM_MODULE` / `QT_IM_MODULE` mismatch**.
   The app is sending keys to a different IME than vi-daemon is registered with.
   ```bash
   echo $GTK_IM_MODULE $QT_IM_MODULE
   # Should be either "ibus" or "fcitx5", not both or mixed
   ```

3. **XWayland key replay**.
   On Wayland with XWayland apps, the compositor may replay the key after the
   IME commits.  Workaround: ensure `XMODIFIERS=@im=ibus` (or fcitx5) is set in
   your session, not `XMODIFIERS=@im=none`.

4. **vi-daemon running twice**.
   ```bash
   pgrep -c vi-daemon   # should print 1
   pkill -o vi-daemon   # kill the older instance
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

## Tone marks missing

**Symptom**: typing `viet` produces `viet` instead of `việt`.

**Diagnosis steps**:

1. Check vi-daemon is running and connected:
   ```bash
   # Send a status request directly
   echo '{"cmd":"status"}' | nc -U "$XDG_RUNTIME_DIR/vi-daemon.sock"
   ```
   Expected: `{"type":"status","backend":"ibus","method":"telex","preedit":""}`

2. Verify the active method is Telex (not pass-through):
   ```bash
   echo '{"cmd":"set_method","method":"telex"}' | nc -U "$XDG_RUNTIME_DIR/vi-daemon.sock"
   ```

3. Check the IME is enabled for the application:
   - IBus: click the IBus indicator and ensure it shows the vime engine, not "English".
   - Fcitx5: the system tray should show "VI" not "EN".

4. Test with the vi-ui Typing Test panel.  If it shows "việt" but gedit doesn't,
   the issue is the GTK IM module, not vi-daemon.

5. Ensure `XMODIFIERS`, `GTK_IM_MODULE`, `QT_IM_MODULE`, and `SDL_IM_MODULE` are
   set in your `~/.profile` or `/etc/environment`:
   ```bash
   export GTK_IM_MODULE=ibus
   export QT_IM_MODULE=ibus
   export XMODIFIERS=@im=ibus
   ```
   Log out and back in after changing.

---

## Preedit stuck

**Symptom**: the underlined preedit text remains in the app and nothing commits,
even after pressing Space or Enter.

**Causes and fixes**:

1. **vi-daemon crashed mid-composition**.
   The preedit string was sent to the app but the commit never arrived.
   ```bash
   # Force-clear preedit by restarting vi-daemon
   pkill vi-daemon && vi-daemon &
   # Then press Escape in the affected app to clear the stale preedit
   ```

2. **App does not support preedit** (terminal emulators without IME support).
   Use `foot` or `kitty` instead of `xterm` for Wayland; or configure
   `overTheSpot` XIM mode for X11 apps.

3. **Fcitx5 input context not activated**.
   Some Qt apps need `fcitx5-qt` installed separately:
   ```bash
   sudo apt install fcitx5-frontend-qt6   # Ubuntu/Debian
   sudo dnf install fcitx5-qt             # Fedora
   ```

4. **Compositor serial number mismatch** (Wayland only).
   The preedit was sent with an old serial.  Upgrade vi-wayland to ≥ 0.2.0 and
   the compositor to a version that tolerates stale serials.

---

## IBus: gạch chân khi gõ, Chrome khác Telegram / VS Code

**Triệu chứng**: ở Chromium thấy gạch chân (hoặc kiểu composition) rõ hơn; ở Telegram (Qt),
VS Code (Electron) hoặc gedit (GTK) có thể **không** gạch chân dù vẫn gõ Telex bình thường.

**Nguyên nhân**: vime gửi `UpdatePreeditText` với `IBusAttrList` chứa một thuộc tính
`IBUS_ATTR_TYPE_NONE` (mặc định *IBnoUnderline* của vhttechkey): không
cố ý vẽ underline.  Toolkit và từng ứng dụng vẫn quyết định có hiển thị “đang soạn” hay
không — Chromium thường vẽ khác Gtk/Qt.

**Không cần cấu hình** nếu gõ đúng; đây là hành vi kỹ thuật, không phải lỗi IME.

---

## Chrome / Chromium: không ra chữ, lệch chữ, hoặc cần chỉnh đường IME

**Bối cảnh**: vime trên IBus **không** tự bật `surrounding_commit` (`DeleteSurroundingText`)
hay `ForwardKeyEvent` theo bit `SetCapabilities` — mọi client mặc định dùng preedit
(`UpdatePreeditText` với `IBUS_ENGINE_PREEDIT_COMMIT` = 1 theo `ibustypes.h`).  Nhánh
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

**Triệu chứng**: không thấy preedit, hoặc vừa “commit” thì ký tự biến mất.  Hay gặp:
**gõ xong một từ** (Space, ngắt âm tiết, hoặc phím kết thúc từ) thì **cả cụm đang
soạn biến mất**, không còn chữ trong editor.

**Nguyên nhân**: Electron dùng stack IME của Chromium qua D-Bus; không dùng
`GTK_IM_MODULE`.  Trên Wayland, đường `text-input-v3` ổn định hơn XWayland.

### Chrome / Telegram ổn nhưng VS Code (hay app Electron khác) vẫn mất chữ

Điều này **không** có nghĩa vime “chỉ hỏng một app”: cùng một `vi-daemon`, nhưng
mỗi tiến trình dùng **đường IME khác**:

- **Chromium / Chrome**: tiến trình riêng; nhiều bản cài đã chạy **Ozone Wayland**
  native nên `text-input-v3` ổn.
- **Telegram Desktop**: **Qt** (`QT_IM_MODULE`), không đi qua stack Electron.
- **VS Code, Cursor, Slack, …**: **Electron**; nếu cửa sổ đang **XWayland** hoặc
  thiếu cờ Ozone, lớp IME có thể xử lý commit/preedit sai — biểu hiện điển hình là
  **mất cả từ ngay khi kết thúc gõ**.

**Việc nên làm** (ưu tiên từ trên xuống; tập trung sửa VS Code trước khi đổi cấu
hình IBus toàn cục):

1. Kiểm tra Electron: `vi-tools sandbox-status` khi VS Code đang mở.  Nếu dòng
   `code` báo **`needs-flags`**, hãy luôn khởi động editor với Ozone Wayland:
   ```bash
   code --enable-features=UseOzonePlatform --ozone-platform=wayland
   ```
   Có thể gắn cố định bằng cách sửa file `.desktop` của VS Code (mục `Exec=`) hoặc
   shell alias để không quên cờ.

2. Môi trường chỉ X11: thử `ELECTRON_OZONE_PLATFORM_HINT=auto code` rồi gõ lại.

3. IBus: khi focus vào VS Code, đảm bảo chế độ **Input Method** (bật vime), **không**
   để **Direct Input** cho cửa sổ đó.

4. Nếu sau các bước trên **vẫn** mất chữ, thử trong `~/.config/vime/config.toml`:

   ```toml
   [ibus]
   force_chrome_direct = true
   ```

   Sau đó **khởi động lại `vi-daemon`**.  Đây là tùy chọn **toàn cục**; Chrome và
   Telegram thường vẫn ổn.  Nếu một ứng dụng khác lại xấu đi, xem mục **Chrome /
   Chromium** phía trên (dùng `force_preedit_mode = true` cùng khối `[ibus]`).

5. Cuối cùng: mở VS Code từ terminal với `XMODIFIERS=@im=ibus code` (tránh snap /
   môi trường thiếu biến XMODIFIERS).

**Ghi chú**: vime trên IBus gửi **`CommitText` rồi mới `HidePreeditText`** khi kết thúc
từ (vhttechkey commit order), vì một số bản Electron (VS Code) xóa cả
cụm preedit nếu nhận `HidePreeditText` trước.  Nếu vẫn lỗi sau khi cập nhật bản build,
phần còn lại thường là **Electron phải nhận IME đúng phiên** (Ozone Wayland, v.v.).

---

## High CPU usage

**Symptom**: `vi-daemon` or `ibus-daemon` consumes >5% CPU at idle.

**Diagnosis steps**:

1. Check if vime is polling too aggressively:
   ```bash
   strace -p $(pgrep vi-daemon) -e trace=epoll_wait,read,write 2>&1 | head -40
   ```
   You should see `epoll_wait` blocking with a timeout of ~1000 ms, not spinning.

2. Check for a stuck preedit:
   ```bash
   echo '{"cmd":"status"}' | nc -U "$XDG_RUNTIME_DIR/vi-daemon.sock"
   # If "preedit" is non-empty, the engine is holding state unnecessarily
   ```
   Reset with:
   ```bash
   echo '{"cmd":"set_method","method":"telex"}' | nc -U "$XDG_RUNTIME_DIR/vi-daemon.sock"
   ```

3. Check for a log-spam loop:
   ```bash
   journalctl -u vi-daemon -f --since "1 minute ago"
   ```
   If you see repeated error messages at >10/second, there is a reconnect loop.
   Check that `$XDG_RUNTIME_DIR` is writable and the socket path is not stale.

4. Profile:
   ```bash
   perf record -g -p $(pgrep vi-daemon) -- sleep 10
   perf report
   ```
   Common hot paths at idle: unicode normalization on every keypress (expected
   ~0.1 ms), zbus D-Bus polling (expected ~0.5% CPU).

5. Disable the `notify` file watcher if config hot-reload is not needed:
   ```bash
   # In ~/.config/vime/config.toml
   [watcher]
   enabled = false
   ```

---

## Sandbox auto-detection (Electron, Flatpak, Snap)

vi-daemon automatically scans `/proc` at startup to detect sandboxed
applications and logs warnings with suggested remediation.

### Reading daemon logs

```bash
journalctl -u vi-daemon --since today | grep -E 'WARN|electron|flatpak|snap'
```

**Electron on Wayland** produces a `WARN` line like:

```
WARN vi_daemon: Electron process detected on Wayland pid=12345
     relaunch with `--ozone-platform=wayland --enable-features=UseOzonePlatform`
     to enable IME input
```

Apply the suggestion by editing the application's `.desktop` launcher or
passing the flags on the command line:
```bash
code --ozone-platform=wayland --enable-features=UseOzonePlatform
```

**Flatpak apps** log an `INFO` line with the app-id.  If the
`org.freedesktop.portal.InputMethod` portal is not installed the daemon also
emits a `WARN`; install `xdg-desktop-portal` and a suitable backend (e.g.
`xdg-desktop-portal-gnome`) to resolve it.

### Checking sandbox status with vi-tools

```bash
vi-tools sandbox-status
```

Example output:

```
PID       TYPE          IME STATUS
-------------------------------------------------------
12345     electron      needs-flags (--ozone-platform=wayland)
67890     flatpak       OK (portal) [org.gnome.Gedit]
11223     snap          unsupported [firefox]
```

Status values:
- **needs-flags** — Electron process; relaunch with the displayed flags.
- **OK (portal)** — Flatpak app; portal input method route available.
- **unsupported** — Snap app; direct IME integration is not supported; use
  `XMODIFIERS=@im=ibus` as a workaround where the snap allows it.

### D-Bus diagnostics interface

When Electron processes are detected, vi-daemon registers a D-Bus object that
desktop environment components can query programmatically:

```bash
dbus-send --session --print-reply \
  --dest=org.freedesktop.vime \
  /org/freedesktop/vime/Diagnostics \
  org.freedesktop.vime.Diagnostics1.SuggestElectronFlags
```

The reply is a string array of recommended flags, e.g.
`["--ozone-platform=wayland", "--enable-features=UseOzonePlatform"]`.
