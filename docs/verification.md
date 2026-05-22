# Checklist xác minh

Chạy các kiểm tra sau trước mỗi bản phát hành. Mọi mục phải pass với
không lỗi, không cảnh báo, không crash.

---

## 1. Bộ test tự động

```bash
cargo test --workspace
```

**Tiêu chí pass**: 0 test fail, không có test bị ignore hồi quy so với baseline.

---

## 2. Clippy — không cảnh báo

```bash
cargo clippy --workspace -- -D warnings
```

**Tiêu chí pass**: không có dòng `warning:` hoặc `error:` trong output.

---

## 3. Miri — an toàn bộ nhớ (nightly)

```bash
cargo +nightly miri test -p vi-core
```

**Tiêu chí pass**: `Miri: test result: ok.  N passed; 0 failed` và không có
output `error: Undefined Behavior`.

Lưu ý: Miri không hỗ trợ crate I/O nặng (vi-daemon, vi-wayland, v.v.).
Chỉ vi-core (logic thuần) được kỳ vọng chạy sạch dưới Miri.

---

## 4. Fuzz — không crash

```bash
cargo fuzz run fuzz_key_sequence  -- -max_total_time=60
cargo fuzz run fuzz_config        -- -max_total_time=60
cargo fuzz run fuzz_unicode_pipeline -- -max_total_time=60
```

**Tiêu chí pass**: không có dòng `CRASH`, `TIMEOUT`, hoặc `OOM`; mỗi lần chạy
kết thúc bằng `Done N runs in 60 second(s)`.

---

## 5. E2E thủ công: gõ Telex trong bốn môi trường

Với mỗi ứng dụng dưới đây:

1. Đảm bảo vi-daemon đang chạy và kiểu gõ là **Telex**.
2. Mở app và focus vào ô văn bản.
3. Gõ `viet nam` (9 phím, không phím đặc biệt).
4. Xác minh kết quả trên màn hình là **`việt nam`**.

| Ứng dụng | Backend | Kết quả mong đợi |
|---|---|---|
| gedit | IBus | `việt nam` |
| Kate | Fcitx5 | `việt nam` |
| foot terminal (`nano /tmp/t.txt`) | Wayland text-input-v3 | `việt nam` |
| xterm | X11 / XIM | `việt nam` |

---

## 6. Xác minh NFC bằng Python

Sau bước 5, kiểm tra file đã lưu (dùng output foot/nano):

```bash
python3 -c "
import unicodedata
text = 'việt nam'
for ch in text:
    cp   = ord(ch)
    name = unicodedata.name(ch, 'UNKNOWN')
    nfc  = unicodedata.normalize('NFC', ch) == ch
    print(f'U+{cp:04X}  {ch!r:3}  {\"NFC\" if nfc else \"NOT NFC\"}  {name}')
"
```

**Tiêu chí pass**: mọi dòng ghi `NFC`; không có mục `NOT NFC`.

Kết quả mong đợi:

```
U+0076  'v'  NFC  LATIN SMALL LETTER V
U+0069  'i'  NFC  LATIN SMALL LETTER I
U+1EC7  'ệ'  NFC  LATIN SMALL LETTER E WITH CIRCUMFLEX AND DOT BELOW
U+0074  't'  NFC  LATIN SMALL LETTER T
U+0020  ' '  NFC  SPACE
U+006E  'n'  NFC  LATIN SMALL LETTER N
U+0061  'a'  NFC  LATIN SMALL LETTER A
U+006D  'm'  NFC  LATIN SMALL LETTER M
```

---

## 7. Valgrind — không rò rỉ bộ nhớ

```bash
valgrind \
  --leak-check=full \
  --error-exitcode=1 \
  --suppressions=/usr/share/glib-2.0/valgrind/glib.supp \
  vi-daemon &
DAEMON_PID=$!

# Gửi 1000 sự kiện phím giả lập
for i in $(seq 1 1000); do
  echo '{"cmd":"set_method","method":"telex"}' | \
    nc -q1 -U "$XDG_RUNTIME_DIR/vi-daemon.sock" > /dev/null
done

kill $DAEMON_PID
wait $DAEMON_PID
```

**Tiêu chí pass**: valgrind thoát với mã 0 và báo
`definitely lost: 0 bytes in 0 blocks`.

---

## 8. Smoke test UI — chế độ VNI

1. Khởi chạy `vi-ui`.
2. Mở panel **Input Method**.
3. Chuyển sang **VNI** từ dropdown. Xác minh tóm tắt quy tắc cập nhật.
4. Mở panel **Typing Test**.
5. Gõ `81 82 83` (chuỗi số cách nhau bằng space) — với VNI bật trong IME hệ thống,
   mỗi chuỗi phải cho một ký tự tiếng Việt.
6. Vùng test gõ phải chứa `ặ ắ ẳ` (U+1EB7 U+0020 U+1EAF
   U+0020 U+1EB3).

**Tiêu chí pass**: cả ba ký tự hiển thị đúng; bảng phân tích NFC
hiện `✓` cho mỗi ký tự và không có dấu kết hợp `U+03xx`.

---

## Ký duyệt

| Mục | Trạng thái | Người test | Ngày |
|---|---|---|---|
| 1. `cargo test` | | | |
| 2. `cargo clippy` | | | |
| 3. Miri | | | |
| 4. Fuzz | | | |
| 5. E2E thủ công | | | |
| 6. Python NFC | | | |
| 7. Valgrind | | | |
| 8. Smoke test UI | | | |
