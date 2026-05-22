# Sơ đồ trình tự

## 1. Luồng gõ bình thường

Sự kiện phím đi từ compositor tới `vi-daemon`, qua engine `vi-core`,
trả về dưới dạng cập nhật preedit, cuối cùng là commit tới ứng dụng.

```mermaid
sequenceDiagram
    participant User
    participant Compositor
    participant Daemon as vi-daemon
    participant Engine as vi-core Engine
    participant App as Application

    User->>Compositor: key press ('t')
    Compositor->>Daemon: key event (zwp_input_method_v2)
    Daemon->>Engine: InputEvent::KeyDown(Char('t'), mods)
    Engine-->>Daemon: StateTransition::PreeditUpdated("t")
    Daemon->>Compositor: set_preedit_string("t")
    Compositor-->>App: preedit shown in text field

    User->>Compositor: key press ('o')
    Compositor->>Daemon: key event
    Daemon->>Engine: InputEvent::KeyDown(Char('o'), mods)
    Engine-->>Daemon: StateTransition::PreeditUpdated("to")
    Daemon->>Compositor: set_preedit_string("to")
    Compositor-->>App: preedit updated

    User->>Compositor: key press ('o')
    Compositor->>Daemon: key event
    Daemon->>Engine: InputEvent::KeyDown(Char('o'), mods)
    Engine-->>Daemon: StateTransition::PreeditUpdated("tô")
    Daemon->>Compositor: set_preedit_string("tô")
    Compositor-->>App: preedit updated

    User->>Compositor: Return
    Compositor->>Daemon: key event (Return)
    Daemon->>Engine: InputEvent::KeyDown(Return, mods)
    Engine-->>Daemon: StateTransition::CommitAndClear("tô")
    Daemon->>Compositor: commit_string("tô") + commit(serial)
    Compositor-->>App: "tô" inserted into document
```

## 2. Commit khi mất focus

Khi người dùng chuyển focus đi chỗ khác, `vi-daemon` nhận sự kiện `FocusOut` và
tự commit preedit đang chờ để không mất chữ.

```mermaid
sequenceDiagram
    participant User
    participant Compositor
    participant Daemon as vi-daemon
    participant Engine as vi-core Engine
    participant App as Application

    Note over Engine: Buffer contains "vie" (Composing state)

    User->>Compositor: click outside text field
    Compositor->>Daemon: text-input-v3 Leave / input-method-v2 Deactivate
    Daemon->>Engine: InputEvent::FocusOut
    Engine-->>Daemon: StateTransition::CommitAndClear("viê")
    Daemon->>Compositor: commit_string("viê") + commit(serial)
    Daemon->>Compositor: set_preedit_string("") [clear preedit display]
    Compositor-->>App: "viê" committed before focus leaves
    Note over Engine: Buffer cleared → Idle state
```

## 3. Phục hồi sau khởi động lại daemon

`vi-daemon` tuần tự hóa buffer preedit đang soạn trước khi tắt để
composition có thể khôi phục sau khi khởi động lại.

```mermaid
sequenceDiagram
    participant OS
    participant Daemon as vi-daemon
    participant Engine as vi-core Engine
    participant Config as vi-config (disk)
    participant Socket as Unix socket

    OS->>Daemon: SIGTERM
    Daemon->>Engine: request preedit state
    Engine-->>Daemon: { preedit: "tô", cursor: 2 }
    Daemon->>Config: persist state to disk
    Daemon->>Socket: close IPC socket gracefully
    OS->>Daemon: process exits

    OS->>Daemon: restart (watchdog / systemd)
    Daemon->>Config: load persisted state
    Config-->>Daemon: { preedit: "tô", cursor: 2 }
    Daemon->>Engine: restore preedit buffer
    Daemon->>Socket: re-open and advertise
    Daemon->>OS: set_preedit_string("tô") [restore preedit in compositor]
    Note over Daemon: Input method active, composition resumed
```

## 4. Chuyển kiểu gõ

Người dùng bật/tắt kiểu gõ đang active qua phím tắt. `vi-daemon` nhận
yêu cầu qua IPC, reset engine và nạp kiểu gõ mới.

```mermaid
sequenceDiagram
    participant User
    participant Compositor
    participant Daemon as vi-daemon
    participant Engine as vi-core Engine

    User->>Daemon: toggle key (e.g. Ctrl+Space) via IPC socket
    Daemon->>Engine: InputEvent::Reset
    Engine-->>Daemon: StateTransition::Cleared
    Daemon->>Compositor: set_preedit_string("") [clear any visible preedit]
    Daemon->>Daemon: select next InputMethod (e.g. Telex → VNI)
    Daemon->>Engine: reinitialize with new InputMethod
    Note over Engine: State = Idle, method = VNI
    Daemon-->>User: new method active (vi-ui notification / tray icon)
```
