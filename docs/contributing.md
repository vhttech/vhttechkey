# Contributing

## Build from source

### Prerequisites

```bash
# Rust toolchain (stable + nightly for Miri/fuzz)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
rustup toolchain install nightly
rustup component add miri --toolchain nightly

# System libraries (Ubuntu/Debian)
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

# System libraries (Fedora)
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

# Debug build (all crates)
cargo build --workspace

# Release build
cargo build --workspace --release

# Build only the settings UI
cargo build -p vi-ui --release

# Build only the daemon
cargo build -p vi-daemon --release
```

The daemon binary is at `target/release/vi-daemon` and the UI at
`target/release/vi-ui`.  The CLI tool is at `target/release/vi-tools`.

## Running tests

```bash
# All unit and integration tests
cargo test --workspace

# Single crate
cargo test -p vi-core

# With output (useful for debugging)
cargo test --workspace -- --nocapture

# Linting (must be clean — CI enforces -D warnings)
cargo clippy --workspace -- -D warnings

# Formatting check
cargo fmt --check

# Miri (memory safety, nightly only)
cargo +nightly miri test -p vi-core

# Fuzzing (requires cargo-fuzz)
cargo install cargo-fuzz
cargo fuzz run fuzz_key_sequence -- -max_total_time=60
cargo fuzz run fuzz_config -- -max_total_time=60
cargo fuzz run fuzz_unicode_pipeline -- -max_total_time=60
```

## Adding a new input method rule set

Input methods live in `crates/vi-core/src/methods/`.

1. **Create the rule file** — copy `telex.rs` as a template:
   ```bash
   cp crates/vi-core/src/methods/telex.rs crates/vi-core/src/methods/mymethod.rs
   ```

2. **Implement the trait**:
   ```rust
   // crates/vi-core/src/methods/mymethod.rs
   use crate::{InputEvent, StateTransition};
   use super::MethodEngine;

   pub struct MyMethod { /* rule table fields */ }

   impl MethodEngine for MyMethod {
       fn name(&self) -> &'static str { "mymethod" }
       fn process(&mut self, event: &InputEvent) -> StateTransition { /* … */ }
       fn reset(&mut self) { /* clear state */ }
   }
   ```

3. **Register it** in `crates/vi-core/src/methods/mod.rs`:
   ```rust
   mod mymethod;
   pub use mymethod::MyMethod;

   impl InputMethod {
       pub fn engine(&self) -> Box<dyn MethodEngine> {
           match self {
               // … existing arms …
               InputMethod::MyMethod => Box::new(MyMethod::new()),
           }
       }
   }
   ```

4. **Add the variant** to the `InputMethod` enum in `types.rs`.

5. **Write golden tests** in `crates/vi-core/tests/` — see `syllables.rs` for
   the pattern.  Add at least:
   - All tone marks on at least 3 vowel classes
   - Backspace handling
   - Round-trip: commit text is NFC

6. **Add a fuzz target** in `fuzz/fuzz_targets/` that feeds random bytes into
   the new method and asserts the output is NFC.

7. **Document** the rule table in `docs/unicode-pipeline.md`.

## Adding a compositor quirk

Compositor-specific workarounds live in `crates/vi-wayland/src/lib.rs` behind
the `CompositorQuirks` bitflag struct.

1. **Reproduce** the bug in `crates/vi-wayland/tests/integration_test.rs` using
   the mock compositor harness:
   ```rust
   #[test]
   fn gnome_commit_without_preedit_quirk() {
       let mut session = MockCompositorSession::new(CompositorKind::Gnome);
       session.quirks |= CompositorQuirks::GNOME_PREEDIT_REQUIRED_BEFORE_COMMIT;
       // … assert correct behaviour with/without the workaround
   }
   ```

2. **Detect the compositor** in `detect_compositor()` (already reads
   `$XDG_CURRENT_DESKTOP` and the `wl_compositor` name advertised by the server).

3. **Apply the workaround** in the relevant protocol handler:
   ```rust
   if self.quirks.contains(CompositorQuirks::GNOME_PREEDIT_REQUIRED) {
       self.send_empty_preedit_before_commit();
   }
   ```

4. **Update** `docs/wayland-compat.md` with the new row in the compatibility table.

5. **Update** `docs/troubleshooting.md` if the quirk causes a user-visible
   symptom that warrants a troubleshooting entry.

## Commit conventions

- Subject line: imperative mood, ≤72 chars, no period.
- Body: explain *why*, not *what* (the diff shows what).
- Reference issues: `Fixes #123` or `Part of #456`.
- No merge commits on `main`; rebase your branch before opening a PR.

## CI

GitHub Actions runs on every PR:

1. `cargo test --workspace`
2. `cargo clippy --workspace -- -D warnings`
3. `cargo fmt --check`
4. `cargo +nightly miri test -p vi-core`
5. Fuzz for 60 s on each target

All checks must pass before merge.
