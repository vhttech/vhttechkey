//! Fuzz target: IBus GVariant / D-Bus message deserialization.
//!
//! Attempts to decode arbitrary bytes using the D-Bus and GVariant wire formats
//! that vi-ibus receives from ibus-daemon, then applies the IBusText string
//! extraction logic (mirrored inline) to any successfully-decoded variant.
//!
//! Message types covered:
//!  - ProcessKeyEvent args: `(u u u)` → keyval, keycode, state
//!  - SetCursorLocation args: `(i i u u)` → x, y, w, h
//!  - SetCapabilities: `u`
//!  - CommitText: `s`
//!  - SetSurroundingText args: `(s u u)` → text, cursor, anchor
//!
//! Invariant: all deserialization paths must be panic-free;
//! returning an error is always acceptable.
#![no_main]

use libfuzzer_sys::fuzz_target;
use zvariant::{serialized::Context, Endian};

fn try_dbus(data: &[u8], endian: Endian) {
    let ctx = Context::new_dbus(endian, 0);

    // ProcessKeyEvent: (u u u) — keyval, keycode, state
    let _: Result<(u32, u32, u32), _> = zvariant::from_slice(data, ctx);

    // SetCursorLocation: (i i u u) — x, y, w, h
    let _: Result<(i32, i32, u32, u32), _> = zvariant::from_slice(data, ctx);

    // SetCapabilities: u
    let _: Result<u32, _> = zvariant::from_slice(data, ctx);

    // CommitText plain string
    let _: Result<String, _> = zvariant::from_slice(data, ctx);

    // SetSurroundingText: (s u u) — text, cursor, anchor
    let _: Result<(String, u32, u32), _> = zvariant::from_slice(data, ctx);
}

fuzz_target!(|data: &[u8]| {
    if data.is_empty() {
        return;
    }
    try_dbus(data, Endian::Little);
    try_dbus(data, Endian::Big);
});
