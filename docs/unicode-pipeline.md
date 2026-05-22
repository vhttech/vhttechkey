# Pipeline Unicode

## Vì sao dùng NFC?

Văn bản tiếng Việt dùng ký tự precomposed — một codepoint mang cả nguyên âm gốc
và dấu (ví dụ `ệ` = U+1EB9, LATIN SMALL LETTER E WITH CIRCUMFLEX AND DOT BELOW).
Hai dạng chuẩn hóa phổ biến:

| Dạng | Mô tả | Ví dụ cho "ệ" |
|---|---|---|
| **NFC** | Precomposed — một codepoint mỗi glyph | U+1EB9 |
| **NFD** | Decomposed — gốc + dấu kết hợp | U+0065 U+0323 U+0302 |

VHTTechKey luôn xuất **NFC** vì:

1. Widget văn bản GTK / Qt lưu và render NFC nội bộ; chèn NFD khiến
   dấu kết hợp hiện thành ký tự riêng (dấu đôi).
2. Clipboard Linux và hầu hết handler paste mong đợi NFC.
3. `String::len()` / offset byte dự đoán được khi mỗi grapheme là một codepoint.
4. So sánh và collation Unicode đơn giản hơn với NFC.

## Trình tự thao tác

```
Chuỗi preedit thô  (có thể là chuỗi ASCII dở, ví dụ "vie65t")
        │
        ▼  Bước 1 – áp quy tắc (vi-core::methods)
        │   Kiểu gõ đang active (Telex/VNI/VIQR) thay chuỗi kích hoạt
        │   bằng giá trị scalar Unicode tương ứng.
        │   "vie65t"  →  "viết"  (trung gian, có thể còn giống NFD)
        │
        ▼  Bước 2 – phân rã chuẩn  (unicode_normalization::nfd())
        │   Mọi ký tự precomposed tách thành gốc + dấu kết hợp.
        │   "viết" → "vie\u{0301}\u{0323}t"  (đơn giản hóa; thực tế khác)
        │
        ▼  Bước 3 – sắp xếp lại canonical combining class
        │   Dấu kết hợp sắp theo Canonical Combining Class (CCC).
        │   Dấu CCC thấp hơn đứng trước. Đảm bảo chuỗi byte duy nhất
        │   cho cùng ký tự logic bất kể thứ tự nhập.
        │
        ▼  Bước 4 – ghép NFC  (unicode_normalization::nfc())
        │   Cặp liền kề (gốc, dấu kết hợp) thay bằng codepoint precomposed
        │   nếu có trong bảng ghép Unicode.
        │   "…\u{0301}\u{0323}…" → "ệ" (U+1EB9)
        │
        ▼
NfcString  (newtype đảm bảo NFC; chỉ tạo bên trong UnicodePipeline)
```

## Codepoint tiếng Việt liên quan

134 codepoint precomposed tiếng Việt nằm trong hai khối Unicode:

| Khối | Phạm vi | Codepoint | Ví dụ |
|---|---|---|---|
| Latin Extended Additional | U+1E00–U+1EFF | 128 | ề ế ệ ổ ộ ợ ự ặ ắ ằ ẳ ẫ |
| Latin-1 Supplement | U+00C0–U+00FF | 6 | à á â ã è é |

Tất cả đều **ổn định NFC**: dạng precomposed là biểu diễn NFC chuẩn
và qua pipeline không đổi.

## Mã hóa sai thường gặp và dạng NFC đúng

Các mã hóa này xuất hiện trong tài liệu cũ và gây lỗi hiển thị trên
app Linux hiện đại. Pipeline VHTTechKey sửa chúng ở đầu ra.

| Mã hóa sai | Codepoint (hex) | Mô tả | NFC đúng | Codepoint |
|---|---|---|---|---|
| a + ◌̣ + ◌̂ | 0061 0323 0302 | NFD, sai thứ tự CCC | ậ | U+1EAD |
| a + ◌̂ + ◌̣ | 0061 0302 0323 | NFD, đúng thứ tự CCC | ậ | U+1EAD |
| ă + ◌́ | 0103 0301 | NFD một phần | ắ | U+1EAF |
| ă + ◌̀ | 0103 0300 | NFD một phần | ằ | U+1EB1 |
| ă + ◌̣ | 0103 0323 | NFD một phần | ặ | U+1EB7 |
| o + ◌̛ + ◌̣ | 006F 031B 0323 | NFD, COMBINING HORN | ợ | U+1EE3 |
| u + ◌̛ + ◌̣ | 0075 031B 0323 | NFD, COMBINING HORN | ự | U+1EF1 |
| VISCII cp 0xF5 | — | Mã hóa 8-bit cũ | ợ | U+1EE3 |
| VPS cp 0xD5 | — | Mã hóa 8-bit cũ | ợ | U+1EE3 |

## Ma trận phủ test

| Bộ test | Số lượng | Vị trí | Nội dung kiểm tra |
|---|---|---|---|
| Golden 216 | 216 | `vi-testing/tests/golden_216.rs` | Mọi nguyên âm Việt × 6 thanh × 3 kiểu gõ cho ký tự đúng VÀ hợp lệ NFC |
| Round-trip NFD | 10+ | `vi-testing/tests/unicode_torture.rs` | Đầu vào NFD (gốc + dấu kết hợp, kể cả thứ tự CCC đảo) chuẩn hóa đúng codepoint NFC |
| Dấu cùng CCC sai thứ tự | 1 | `vi-testing/tests/unicode_torture.rs` | `a+U+0301+U+0302` (sắc trước mũ, cùng CCC=230) cho NFC ổn định khác `ấ` — ghi nhận thứ tự nhập quan trọng với cặp dấu cùng CCC |
| Phát hiện mã hóa cũ | 4+ | `vi-testing/tests/unicode_torture.rs` | Codepoint điều khiển C1 (U+0080–U+009F) từ file TCVN3/VPS/VISCII decode Latin-1 bị từ chối với `CompositionError::LegacyEncoding` |
| Từ chối surrogate | 1 | `vi-testing/tests/unicode_torture.rs` | Biến thể `CompositionError::SurrogateCodepoint` cho đường FFI/CESU-8; đầu ra engine không chứa surrogate |
| Unicode torture (pipeline) | 40+ | `vi-testing/src/unicode_torture.rs` + `tests/unicode_torture_test.rs` | Bộ đầy đủ NFD→NFC, emoji ZWJ, phát hiện C1, dấu kết hợp mồ côi, codepoint non-character |
| Property test | ∞ | `vi-testing/tests/golden_exhaustive_test.rs` | Tiền tố chữ thường tùy ý + golden 216 qua proptest |

## Phát hiện mã hóa cũ

Pipeline từ chối chuỗi chứa **ký tự điều khiển C1** (U+0080–U+009F).
Các codepoint này xuất hiện khi tài liệu TCVN3, VPS hoặc VISCII (mã 8-bit)
được decode byte-by-byte thành Latin-1 rồi mã hóa lại UTF-8. Giá trị byte
0x80–0x9F trong các bảng đó ánh xạ ký tự Việt; đọc nhầm Latin-1 thành
U+0080–U+009F, không có ý nghĩa hữu ích trong văn bản Unicode.

| Lỗi | Kích hoạt | Ý nghĩa |
|---|---|---|
| `CompositionError::LegacyEncoding(cp)` | Bất kỳ codepoint trong U+0080–U+009F | Caller phải mã hóa lại nguồn UTF-8 bằng bảng code-page đúng |

Ký tự trong khối Latin-1 Supplement **trên** U+00A0 (ví dụ `à` = U+00E0,
`ô` = U+00F4) là Unicode hợp lệ và qua pipeline không đổi.

## Kiểm tra đầu ra NFC

```python
import unicodedata

text = "việt nam"
for ch in text:
    cp   = ord(ch)
    name = unicodedata.name(ch, "UNKNOWN")
    form = unicodedata.normalize("NFC", ch) == ch
    print(f"U+{cp:04X}  {ch!r:4}  {'NFC' if form else 'NOT NFC':7}  {name}")
```

Kết quả mong đợi cho `"việt nam"` (tất cả NFC):

```
U+0076  'v'  NFC     LATIN SMALL LETTER V
U+0069  'i'  NFC     LATIN SMALL LETTER I
U+1EC7  'ệ'  NFC     LATIN SMALL LETTER E WITH CIRCUMFLEX AND DOT BELOW
U+0074  't'  NFC     LATIN SMALL LETTER T
U+0020  ' '  NFC     SPACE
U+006E  'n'  NFC     LATIN SMALL LETTER N
U+0061  'a'  NFC     LATIN SMALL LETTER A
U+006D  'm'  NFC     LATIN SMALL LETTER M
```
