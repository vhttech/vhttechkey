//! Fuzz target: feed arbitrary byte sequences through `UnicodePipeline`.
//!
//! Invariants checked:
//! - Must never panic on any input, including invalid UTF-8.
//! - On `Ok`, the output must be valid NFC.
//! - No output codepoint may be in the surrogate range D800–DFFF.
//! - Lossy UTF-8 conversion (arbitrary bytes → string with U+FFFD replacements)
//!   must also not panic and must produce NFC output on `Ok`.
#![no_main]

use libfuzzer_sys::fuzz_target;
use unicode_normalization::UnicodeNormalization;
use vi_core::UnicodePipeline;

fn check_invariants(s: &str) {
    match UnicodePipeline::process(s) {
        Ok(result) => {
            let out = result.as_str();
            // Output must be NFC.
            let nfc: String = out.nfc().collect();
            assert_eq!(out, nfc, "pipeline output is not NFC for input {s:?}");
            // No surrogate codepoints in output.
            for ch in out.chars() {
                let cp = ch as u32;
                assert!(
                    !(0xD800..=0xDFFF).contains(&cp),
                    "surrogate U+{cp:04X} in pipeline output"
                );
            }
        }
        Err(_) => {
            // Legitimate rejection — not a panic.
        }
    }
}

fuzz_target!(|data: &[u8]| {
    // Path 1: valid UTF-8 strings — full invariant check.
    if let Ok(s) = std::str::from_utf8(data) {
        check_invariants(s);
    }

    // Path 2: arbitrary byte sequences decoded lossily (invalid bytes become
    // U+FFFD).  The pipeline must not panic and, on Ok, must produce NFC.
    // This exercises U+FFFD handling and ensures no combination of replacement
    // characters can trigger undefined behavior or panics.
    let lossy = String::from_utf8_lossy(data);
    check_invariants(&lossy);
});
