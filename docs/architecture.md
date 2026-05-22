# Kiến trúc

## Sơ đồ phân lớp

```
┌──────────────────────────────────────────────────────────────────────────┐
│                          Ứng dụng                                        │
│              (gedit, Kate, foot, xterm, app Electron, …)                 │
└────────────────────────────┬─────────────────────────────────────────────┘
                             │  text-input-v3 / XIM / IBus API / Fcitx5 API
┌────────────────────────────▼─────────────────────────────────────────────┐
│                     Lớp framework IME                                    │
│         ┌─────────────┐              ┌─────────────────┐                │
│         │  vi-ibus    │              │   vi-fcitx5     │                │
│         │  (IBus IME) │              │  (Fcitx5 IME)   │                │
│         └──────┬──────┘              └────────┬────────┘                │
└────────────────┼────────────────────────────────┼────────────────────────┘
                 │                                │  vi-platform::PlatformEngine trait
┌────────────────▼────────────────────────────────▼────────────────────────┐
│                     Trừu tượng hóa nền tảng (vi-platform)                  │
│   ┌─────────────┐   ┌─────────────────┐   ┌───────────┐                │
│   │ vi-wayland  │   │    vi-x11       │   │  (tương   │                │
│   │ (text-input │   │  (XIM/XKBC)     │   │   lai)    │                │
│   │  -v3 proto) │   │                 │   │           │                │
│   └──────┬──────┘   └────────┬────────┘   └───────────┘                │
└──────────┼────────────────────┼────────────────────────────────────────-─┘
           │                    │  KeyEvent / StateTransition
┌──────────▼────────────────────▼──────────────────────────────────────────┐
│                       vi-core (engine ghép chữ)                            │
│                                                                          │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐  ┌─────────────┐ │
│  │ methods/     │  │ preedit_     │  │ unicode_     │  │  engine /   │ │
│  │ telex.rs     │  │ buffer.rs    │  │ pipeline.rs  │  │  commit_    │ │
│  │ vni.rs       │  │              │  │  (đầu ra NFC)│  │  engine.rs  │ │
│  │ viqr.rs      │  │              │  │              │  │             │ │
│  └──────────────┘  └──────────────┘  └──────────────┘  └─────────────┘ │
└──────────────────────────────────────────────────────────────────────────┘
           │
           │  IPC Unix socket (JSON phân tách bằng newline)
┌──────────▼──────────────────────────────────────────────────────────────┐
│                     vi-daemon (điều phối)                                 │
│   detect.rs   ipc.rs   signal.rs   watchdog.rs                         │
└──────────────────┬──────────────────────────────────────────────────────┘
                   │  Unix socket
┌──────────────────▼──────────────────────────────────────────────────────┐
│                     vi-ui (ứng dụng cài đặt egui)                         │
│   Kiểu gõ · Bản đồ phím · Xem trước bàn phím · Bài test gõ               │
│   Profile · Chẩn đoán · Xem log                                           │
└─────────────────────────────────────────────────────────────────────────┘
```

## Luồng dữ liệu: sự kiện phím → văn bản NFC đã commit

```
 Người dùng bấm phím
        │
        ▼
 Backend nền tảng (vi-wayland / vi-x11)
   • Nhận sự kiện phím thô từ compositor / X server
   • Chuyển thành vi_core::InputEvent (Key, Modifiers)
        │
        ▼
 PlatformEngine trait (vi-platform)
   • Gửi InputEvent tới vi-core Engine
        │
        ▼
 vi-core::Engine::process(event)
   • InputMethod đang active (Telex / VNI / VIQR) áp quy tắc
   • PreeditBuffer được cập nhật
   • Trả về StateTransition:
       - PreeditUpdated(text)  → gửi preedit tới app, chờ thêm phím
       - CommitAndClear(text)  → văn bản sẵn sàng qua pipeline NFC, buffer xóa
       - Cleared               → bỏ composition (Escape / Reset)
       - PassThrough           → gửi phím thô tới app không đổi
       - Consumed              → sự kiện đã xử lý, không có đầu ra (KeyUp / FocusIn)
        │
        ▼  (khi Commit)
 UnicodePipeline::process(raw_str) → NfcString
   • Phân rã chuẩn Unicode (NFD)
   • Sắp xếp lại canonical combining class
   • Ghép NFC (dạng precomposed)
        │
        ▼
 CommittedText(NfcString) gửi tới framework IME
        │
        ▼
 IBus / Fcitx5 commit văn bản tới ứng dụng
        │
        ▼
 Ứng dụng nhận văn bản tiếng Việt NFC cuối cùng
```

## Đồ thị phụ thuộc crate

```
vi-ui ──► vi-daemon ──► vi-ibus ──────────────────────► vi-core
                    └── vi-fcitx5 ───────────────────► vi-core
                    └── vi-wayland ──────────────────► vi-core
                    └── vi-x11 ─────────────────────► vi-core
                    └── vi-config ───────────────────► vi-core

vi-platform ────────────────────────────────────────► vi-core
  ▲
  ├── vi-ibus
  ├── vi-fcitx5
  ├── vi-wayland
  └── vi-x11

vi-testing ──► vi-core, vi-config
```

### Engine Telex (Rust thuần của VHTTechKey)

Luật Telex trong **`StandardEngine`** nằm tại `crates/vi-core/src/methods/telex.rs`
(cùng `syllable.rs`, `composition_gate.rs`). Không phụ thuộc crate ngoài cho logic ghép chữ.

Module **`vi_core::vi_engine`** triển khai thuần Rust: parser quy tắc, flatten (NFC),
kiểm tra chính tả CVC, và `ViEngine::process_key` / `get_processed_string`.
Logic composition được kiểm tra trong `vi-core/tests/`.

### Quyết định thiết kế chính

| Quyết định | Lý do |
|---|---|
| Telex/VNI/VIQR native trong `vi-core` | Không dùng crate thuật toán IME ngoài; kiểm soát đầy đủ edge case thứ tự thanh linh hoạt và cổng composition |
| `vi-platform` trait object | Backend nền tảng chọn lúc runtime theo `$DISPLAY` / `$WAYLAND_DISPLAY`; static dispatch cần chọn lúc biên dịch |
| IPC JSON phân tách bằng newline | Đơn giản, debug được bằng `nc`/`socat`, không phụ thuộc protobuf |
| Chỉ đầu ra NFC | Hầu hết app Linux mong đợi NFC; NFD gây dấu đôi trên một số widget GTK |
| `NfcString` newtype | Biến lỗi kiểu khi truyền văn bản chưa chuẩn hóa vào đường commit |
| `PreeditBuffer` trong vi-core | Giữ logic composition test được mà không cần framework IME đang chạy |

## Ổn định giao thức IPC

Giao thức IPC ngầm định ở phiên bản 1. Không có bắt tay đàm phán phiên bản —
client và daemon phải build từ cùng một bản phát hành.

**Thay đổi tương thích ngược** (an toàn, không cần cập nhật client):

- Thêm trường mới vào biến thể `Request` hiện có. `serde` bỏ qua trường JSON
  không biết khi deserialize, nên client cũ không lỗi.

**Thay đổi phá vỡ** (cần client nhận biết):

- Thêm biến thể `Request` mới. Client cũ gửi biến thể daemon không nhận ra sẽ nhận
  phản hồi `Error`; daemon cũ nhận biến thể lạ từ client mới cũng lỗi tương tự.

**Khoảng trống đã biết**: chưa có bắt tay `{"type":"hello","version":1}`. Client
không phát hiện lệch phiên bản lúc kết nối. Công việc tương lai nên thêm trao đổi
`Hello` làm message đầu tiên trên mỗi kết nối mới để hai bên đàm phán tương thích
hoặc fail fast.

## Vòng đời preedit

Chuỗi preedit biểu diễn composition đang diễn ra — văn bản người dùng đã bắt đầu
gõ nhưng chưa commit vào ứng dụng.

| Sự kiện | Tác động lên preedit |
|---|---|
| Phím composition đầu tiên | Preedit được tạo; gửi transition `PreeditUpdated` tới backend |
| Các phím composition tiếp theo | Preedit cập nhật tại chỗ; lặp lại transition `PreeditUpdated` |
| `Escape` | Preedit xóa; transition `Cleared` (không commit) |
| `FocusOut` / compositor deactivate | Preedit commit nguyên trạng rồi xóa; transition `CommitAndClear` hoặc `Commit` |
| Reset do app khởi tạo (`InputEvent::Reset`) | Preedit xóa; transition `Cleared` |
| Xả buffer (ví dụ gọi `set_method`) | Preedit xóa; transition `Cleared` |
| `Enter` | Preedit commit rồi xóa; transition `CommitAndClear` |
| `Space` hoặc dấu câu kết thúc âm tiết | Âm tiết hiện tại commit; preedit có thể bắt đầu lại cho âm tiết kế; transition `CommitThenPreedit` hoặc `CommitAndClear` |
| Đổi focus giữa các cửa sổ | Giống `FocusOut` — preedit đang chờ được commit |
