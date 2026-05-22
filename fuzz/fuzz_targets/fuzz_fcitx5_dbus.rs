//! Fuzz target: Fcitx5 D-Bus message deserialization.
//!
//! Attempts to decode arbitrary bytes as the D-Bus wire-format messages that
//! vi-fcitx5 receives from Fcitx5 via the `org.fcitx.Fcitx5.InputContext1`
//! interface.
//!
//! Signal types covered:
//!  - `CommitString(s)` → plain string
//!  - `UpdateFormattedPreedit(a(su) i)` → array of (text, format-flags) + cursor
//!  - `ForwardKey(u u b)` → keyval, state, is_release
//!  - `DeleteSurroundingText(i u)` → offset, n_chars
//!  - `ProcessKeyEvent` response: `b` → consumed flag
//!
//! Invariant: all deserialization paths must be panic-free; errors are OK.
#![no_main]

use libfuzzer_sys::fuzz_target;
use zvariant::{serialized::Context, Endian};

fn try_dbus(data: &[u8], endian: Endian) {
    let ctx = Context::new_dbus(endian, 0);

    // CommitString(s)
    let _: Result<String, _> = zvariant::from_slice(data, ctx);

    // UpdateFormattedPreedit: array of (String, u32) + i32 cursor
    let _: Result<(Vec<(String, u32)>, i32), _> = zvariant::from_slice(data, ctx);

    // ForwardKey: (u32, u32, bool)
    let _: Result<(u32, u32, bool), _> = zvariant::from_slice(data, ctx);

    // DeleteSurroundingText: (i32, u32)
    let _: Result<(i32, u32), _> = zvariant::from_slice(data, ctx);

    // ProcessKeyEvent response: bool
    let _: Result<bool, _> = zvariant::from_slice(data, ctx);
}

fuzz_target!(|data: &[u8]| {
    if data.is_empty() {
        return;
    }
    try_dbus(data, Endian::Little);
    try_dbus(data, Endian::Big);
});
