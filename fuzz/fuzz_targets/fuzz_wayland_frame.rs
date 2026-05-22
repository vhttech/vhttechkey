//! Fuzz target: Wayland protocol frame deserialization.
//!
//! Exercises two pure functions in vi-wayland that process compositor-provided
//! byte streams without any Wayland socket connection:
//!
//!  - `adapter::clamp_preedit_cursor`: snaps a compositor-supplied byte-offset
//!    cursor to the nearest valid UTF-8 boundary.  KWin forwards raw byte
//!    offsets to Qt, which panics on non-boundary positions.
//!
//!  - `quirks::CompositorQuirks::from_global_pairs`: detects the compositor
//!    family from an arbitrary list of Wayland global interface names and
//!    versions.
//!
//! Invariant: neither function may panic on arbitrary input.
#![no_main]

use libfuzzer_sys::fuzz_target;
use vi_wayland::{adapter::clamp_preedit_cursor, quirks::CompositorQuirks};

fuzz_target!(|data: &[u8]| {
    if data.len() < 4 {
        return;
    }

    // ── clamp_preedit_cursor ─────────────────────────────────────────────────
    // Use the last 4 bytes as a little-endian i32 cursor; the rest as text.
    let split = data.len().saturating_sub(4);
    let cursor_bytes: [u8; 4] = data[split..].try_into().unwrap_or([0; 4]);
    let cursor = i32::from_le_bytes(cursor_bytes);
    if let Ok(text) = std::str::from_utf8(&data[..split]) {
        let _ = clamp_preedit_cursor(text, cursor);
    }

    // ── from_global_pairs ───────────────────────────────────────────────────
    // Parse length-prefixed (name, version) pairs from the byte stream.
    // Format per entry: 1-byte name_len (1..=64), name bytes, 4-byte version.
    let mut names: Vec<String> = Vec::new();
    let mut i = 0;
    while i < data.len() {
        let name_len = (data[i] as usize & 0x3f).saturating_add(1);
        i += 1;
        if i + name_len + 4 > data.len() {
            break;
        }
        if let Ok(s) = std::str::from_utf8(&data[i..i + name_len]) {
            names.push(s.to_owned());
        }
        i += name_len + 4; // skip name + 4-byte version field
    }
    let pairs: Vec<(&str, u32)> = names.iter().map(|s| (s.as_str(), 0u32)).collect();
    let _ = CompositorQuirks::from_global_pairs(&pairs);
});
