//! Fuzz target: feed arbitrary bytes to the vi-daemon IPC JSON command parser.
//!
//! Invariant: `serde_json::from_str::<Command>` must never panic.
//! Returning `Err` is always acceptable.
//!
//! NOTE: The `Command` enum is mirrored here to avoid a workspace dependency
//! from the fuzz package.
#![no_main]

use libfuzzer_sys::fuzz_target;
use serde::{Deserialize, Serialize};

/// Mirror of `vi_daemon::ipc::Command`.
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "cmd", rename_all = "snake_case")]
enum Command {
    SetInputMethod { method: String },
    GetStatus,
    ReloadConfig,
    Quit,
}

fuzz_target!(|data: &[u8]| {
    if let Ok(s) = std::str::from_utf8(data) {
        // The parser must not panic on arbitrary JSON input.
        let _: Result<Command, _> = serde_json::from_str(s);

        // Also fuzz line-splitting (the IPC protocol is newline-delimited).
        for line in s.lines() {
            let _: Result<Command, _> = serde_json::from_str(line);
        }
    }
});
