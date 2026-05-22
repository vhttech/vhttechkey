# Architecture

## Layered diagram

```
┌──────────────────────────────────────────────────────────────────────────┐
│                          Applications                                    │
│              (gedit, Kate, foot, xterm, Electron apps, …)               │
└────────────────────────────┬─────────────────────────────────────────────┘
                             │  text-input-v3 / XIM / IBus API / Fcitx5 API
┌────────────────────────────▼─────────────────────────────────────────────┐
│                     IME Framework Layer                                  │
│         ┌─────────────┐              ┌─────────────────┐                │
│         │  vi-ibus    │              │   vi-fcitx5     │                │
│         │  (IBus IME) │              │  (Fcitx5 IME)   │                │
│         └──────┬──────┘              └────────┬────────┘                │
└────────────────┼────────────────────────────────┼────────────────────────┘
                 │                                │  vi-platform::PlatformEngine trait
┌────────────────▼────────────────────────────────▼────────────────────────┐
│                     Platform Abstraction (vi-platform)                   │
│   ┌─────────────┐   ┌─────────────────┐   ┌───────────┐                │
│   │ vi-wayland  │   │    vi-x11       │   │  (future) │                │
│   │ (text-input │   │  (XIM/XKBC)     │   │           │                │
│   │  -v3 proto) │   │                 │   │           │                │
│   └──────┬──────┘   └────────┬────────┘   └───────────┘                │
└──────────┼────────────────────┼────────────────────────────────────────-─┘
           │                    │  KeyEvent / StateTransition
┌──────────▼────────────────────▼──────────────────────────────────────────┐
│                       vi-core (composition engine)                       │
│                                                                          │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐  ┌─────────────┐ │
│  │ methods/     │  │ preedit_     │  │ unicode_     │  │  engine /   │ │
│  │ telex.rs     │  │ buffer.rs    │  │ pipeline.rs  │  │  commit_    │ │
│  │ vni.rs       │  │              │  │  (NFC out)   │  │  engine.rs  │ │
│  │ viqr.rs      │  │              │  │              │  │             │ │
│  └──────────────┘  └──────────────┘  └──────────────┘  └─────────────┘ │
└──────────────────────────────────────────────────────────────────────────┘
           │
           │  Unix socket IPC (newline-delimited JSON)
┌──────────▼──────────────────────────────────────────────────────────────┐
│                     vi-daemon (orchestrator)                             │
│   detect.rs   ipc.rs   signal.rs   watchdog.rs                         │
└──────────────────┬──────────────────────────────────────────────────────┘
                   │  Unix socket
┌──────────────────▼──────────────────────────────────────────────────────┐
│                     vi-ui (egui settings app)                           │
│   Input Method · Key Map · Keyboard Preview · Typing Test               │
│   Profiles · Diagnostics · Log Viewer                                   │
└─────────────────────────────────────────────────────────────────────────┘
```

## Data flow: key event → committed NFC text

```
 User presses key
        │
        ▼
 Platform backend (vi-wayland / vi-x11)
   • Receives raw key event from compositor / X server
   • Converts to vi_core::InputEvent (Key, Modifiers)
        │
        ▼
 PlatformEngine trait (vi-platform)
   • Dispatches InputEvent to vi-core Engine
        │
        ▼
 vi-core::Engine::process(event)
   • Active InputMethod (Telex / VNI / VIQR) applies rules
   • PreeditBuffer updated
   • Returns StateTransition:
       - PreeditUpdated(text)  → send preedit to app, wait for more keys
       - CommitAndClear(text)  → text ready for NFC pipeline, buffer cleared
       - Cleared               → composition discarded (Escape / Reset)
       - PassThrough           → send raw key to app unchanged
       - Consumed              → event handled, no output (KeyUp / FocusIn)
        │
        ▼  (on Commit)
 UnicodePipeline::process(raw_str) → NfcString
   • Unicode canonical decomposition (NFD)
   • Canonical combining class reorder
   • NFC composition (precomposed form)
        │
        ▼
 CommittedText(NfcString) sent to IME framework
        │
        ▼
 IBus / Fcitx5 commits text to application
        │
        ▼
 Application receives final NFC Vietnamese text
```

## Crate dependency graph

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

### Telex engine (Rust riêng của vhttechkey)

Luật Telex trong **`StandardEngine`** được áp dụng qua `crates/vi-core/src/methods/telex.rs` (+ `syllable.rs`, `composition_gate.rs`).
Không phụ thuộc crate ngoài nào cho logic composition.

Engine **`vi_core::vi_engine`** triển khai thuần Rust: parser luật, flatten (NFC), kiểm tra chính tả CVC, và `ViEngine::process_key` / `get_processed_string`.
Logic composition được kiểm tra bởi bộ test trong `vi-core/tests/`.

### Key design decisions

| Decision | Rationale |
|---|---|
| Native Telex/VNI/VIQR in `vi-core` | No external IME algorithm crate; full control over flexible tone ordering edge cases and composition gating |
| `vi-platform` trait object | Platform backends are selected at runtime based on `$DISPLAY` / `$WAYLAND_DISPLAY`; static dispatch would require compile-time selection |
| Newline-delimited JSON IPC | Simple, debuggable with `nc`/`socat`, no external protobuf dep |
| NFC output only | All major Linux apps expect NFC; NFD causes doubled marks in some GTK widgets |
| `NfcString` newtype | Makes it a type error to pass non-normalized text to commit path |
| `PreeditBuffer` in vi-core | Keeps composition logic testable without a running IME framework |

## IPC Protocol Stability

The IPC protocol is implicitly at version 1.  There is no version-negotiation
handshake — the client and daemon must be built from the same release.

**Backward-compatible changes** (safe to make without client updates):

- Adding new fields to existing `Request` variants.  `serde` ignores unknown
  JSON fields on deserialization, so older clients will not error.

**Breaking changes** (require client awareness):

- Adding new `Request` variants.  An older client that sends a variant the
  daemon does not recognise will receive an `Error` response; an older daemon
  that receives an unknown variant from a newer client will likewise error.

**Known gap**: there is no `{"type":"hello","version":1}` handshake.  Clients
cannot detect a version mismatch at connect time.  Future work should add a
`Hello` exchange as the first message on every new connection so both sides can
negotiate compatibility or fail fast.

## Preedit Lifecycle

A preedit string represents composition in progress — text the user has started
typing but not yet committed to the application.

| Event | Effect on preedit |
|---|---|
| First composition keypress | Preedit created; `PreeditUpdated` transition sent to backend |
| Subsequent composition keypresses | Preedit updated in place; repeated `PreeditUpdated` transitions |
| `Escape` | Preedit cleared; `Cleared` transition (no commit) |
| `FocusOut` / compositor deactivate | Preedit committed as-is then cleared; `CommitAndClear` or `Commit` transition |
| App-initiated reset (`InputEvent::Reset`) | Preedit cleared; `Cleared` transition |
| Buffer flush (e.g. `set_method` call) | Preedit cleared; `Cleared` transition |
| `Enter` | Preedit committed then cleared; `CommitAndClear` transition |
| `Space` or punctuation terminating a syllable | Current syllable committed; preedit may start afresh for the next syllable; `CommitThenPreedit` or `CommitAndClear` transition |
| Focus change between windows | Same as `FocusOut` — pending preedit is committed |
