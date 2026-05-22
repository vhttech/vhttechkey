//! Fuzz target: feed arbitrary bytes as TOML to the vi-config parser.
//!
//! Invariant: the parser must never panic. `Err` is always acceptable.
//!
//! NOTE: superseded by fuzz_config.rs — kept for historical corpus compatibility.
#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(s) = std::str::from_utf8(data) {
        // load_str must not panic regardless of input.
        let _ = vi_config::load_str(s);
    }
});
