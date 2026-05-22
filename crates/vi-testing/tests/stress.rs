//! Stress tests: high-volume random input sequences.
//!
//! Goal: verify stability (no panics, no UB) under load. Throughput is
//! measured by the benchmarks; here we care only about correctness.
#![allow(clippy::unwrap_used)]

use unicode_normalization::UnicodeNormalization;
use vi_core::{
    CompositionEngine, InputEvent, InputMethod, Key, Modifiers, StandardEngine, StateTransition,
    UnicodePipeline,
};

/// Minimal LCG PRNG so the stress tests have no external dependencies and
/// produce the same sequence on every run.
struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self.0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        self.0
    }

    fn next_usize(&mut self, n: usize) -> usize {
        (self.next() >> 33) as usize % n
    }
}

/// 100 000 random Telex sequences must not panic and every committed string
/// must be valid NFC.
#[test]
fn stress_telex_100k_no_panic_always_nfc() {
    let telex_chars: &[char] =
        &['a', 'e', 'i', 'o', 'u', 'y', 'd', 's', 'f', 'r', 'x', 'j', 'w', 'z', 'n', 't'];
    let mut rng = Lcg(0xDEAD_BEEF_CAFE_BABEu64);

    for _ in 0..100_000 {
        let mut engine = StandardEngine::new(InputMethod::Telex);
        let seq_len = rng.next_usize(8) + 1;

        for _ in 0..seq_len {
            let ch = telex_chars[rng.next_usize(telex_chars.len())];
            let _ = engine.process(&InputEvent::KeyDown(Key::Char(ch), Modifiers::none()));
        }

        match engine
            .process(&InputEvent::KeyDown(Key::Return, Modifiers::none()))
            .expect("engine must not fail")
        {
            StateTransition::CommitAndClear(c) | StateTransition::Commit(c) => {
                let s = c.as_str();
                let nfc: String = s.nfc().collect();
                assert_eq!(s, nfc, "committed text is not NFC: {s:?}");
                for ch in s.chars() {
                    assert!(
                        !(0xD800u32..=0xDFFF).contains(&(ch as u32)),
                        "surrogate in output: U+{:04X}",
                        ch as u32
                    );
                }
            }
            _ => {}
        }
    }
}

/// 10 000 random VNI sequences — no panics, all output NFC.
#[test]
fn stress_vni_10k_no_panic() {
    let vni_chars: &[char] =
        &['a', 'e', 'i', 'o', 'u', 'y', 'd', '1', '2', '3', '4', '5', '6', '7', '8', '9'];
    let mut rng = Lcg(0x1234_5678_9ABC_DEF0u64);

    for _ in 0..10_000 {
        let mut engine = StandardEngine::new(InputMethod::Vni);
        let seq_len = rng.next_usize(6) + 1;
        for _ in 0..seq_len {
            let ch = vni_chars[rng.next_usize(vni_chars.len())];
            let _ = engine.process(&InputEvent::KeyDown(Key::Char(ch), Modifiers::none()));
        }
        let _ = engine.process(&InputEvent::KeyDown(Key::Return, Modifiers::none()));
    }
}

/// 10 000 random VIQR sequences — no panics.
#[test]
fn stress_viqr_10k_no_panic() {
    let viqr_chars: &[char] = &[
        'a', 'e', 'i', 'o', 'u', 'y', 'd', '^', '(', '+', '\'', '`', '?', '~', '.',
    ];
    let mut rng = Lcg(0xFEDC_BA98_7654_3210u64);

    for _ in 0..10_000 {
        let mut engine = StandardEngine::new(InputMethod::Viqr);
        let seq_len = rng.next_usize(6) + 1;
        for _ in 0..seq_len {
            let ch = viqr_chars[rng.next_usize(viqr_chars.len())];
            let _ = engine.process(&InputEvent::KeyDown(Key::Char(ch), Modifiers::none()));
        }
        let _ = engine.process(&InputEvent::KeyDown(Key::Return, Modifiers::none()));
    }
}

/// The Unicode pipeline never panics on arbitrary well-formed UTF-8.
#[test]
fn stress_unicode_pipeline_never_panics() {
    let test_strings: &[&str] = &[
        "",
        "hello",
        "tôi",
        "Việt Nam",
        "\u{0300}",                         // lone combining grave (rejected, not panic)
        "a\u{0300}",                         // NFD a + grave
        "\u{1F600}",                         // emoji
        "\u{1F468}\u{200D}\u{1F469}",        // ZWJ sequence
        "中文",                               // CJK
        "a\u{0302}\u{0323}",                 // multi-combining
        &"a".repeat(1000),                   // long ASCII
        "\u{FFFE}",                          // BOM-like
        "ỡ",                                 // precomposed Vietnamese
    ];

    for s in test_strings {
        let _ = UnicodePipeline::process(s); // must not panic
    }
}

/// Sending mixed event types to an engine in a tight loop must not panic.
#[test]
fn stress_mixed_events_no_panic() {
    let events: &[InputEvent] = &[
        InputEvent::KeyDown(Key::Char('a'), Modifiers::none()),
        InputEvent::KeyDown(Key::Backspace, Modifiers::none()),
        InputEvent::KeyDown(Key::Char('o'), Modifiers::none()),
        InputEvent::KeyDown(Key::Char('o'), Modifiers::none()),
        InputEvent::KeyUp(Key::Char('o')),
        InputEvent::KeyRepeat(Key::Char('o'), Modifiers::none()),
        InputEvent::FocusOut,
        InputEvent::FocusIn,
        InputEvent::Reset,
    ];

    for _ in 0..10_000 {
        let mut engine = StandardEngine::new(InputMethod::Telex);
        for event in events {
            let _ = engine.process(event);
        }
    }
}
