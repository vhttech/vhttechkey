//! Memory and throughput stress tests.
//!
//! Run with:
//!   cargo test -p vi-testing --test stress_test -- --nocapture
#![allow(clippy::unwrap_used)]

use std::time::Instant;
use vi_core::{CompositionEngine, InputEvent, InputMethod, Key, Modifiers, StandardEngine};

const EVENT_COUNT: usize = 10_000;

/// Key sequence that cycles through a realistic Telex composition pattern.
fn make_events() -> Vec<InputEvent> {
    // Repeating pattern: type "too" (→ "tô"), then Return to commit.
    let seq: &[InputEvent] = &[
        InputEvent::KeyDown(Key::Char('t'), Modifiers::none()),
        InputEvent::KeyDown(Key::Char('o'), Modifiers::none()),
        InputEvent::KeyDown(Key::Char('o'), Modifiers::none()),
        InputEvent::KeyDown(Key::Return, Modifiers::none()),
    ];
    seq.iter().cloned().cycle().take(EVENT_COUNT).collect()
}

/// Read the resident set size (RSS) in bytes from `/proc/self/statm`.
///
/// Returns 0 on non-Linux platforms.
fn rss_bytes() -> u64 {
    #[cfg(target_os = "linux")]
    {
        let statm = std::fs::read_to_string("/proc/self/statm").unwrap_or_default();
        // Fields: size  resident  shared  text  lib  data  dt  (all in pages)
        let resident_pages: u64 = statm
            .split_whitespace()
            .nth(1)
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);
        let page_size = libc_page_size();
        resident_pages * page_size
    }
    #[cfg(not(target_os = "linux"))]
    {
        0
    }
}

#[cfg(target_os = "linux")]
fn libc_page_size() -> u64 {
    // sysconf(_SC_PAGESIZE) without pulling in libc crate.
    // On x86-64 / aarch64 Linux the page size is almost universally 4096.
    4096
}

/// Run 10 000 key events through `StandardEngine` and assert RSS growth < 1 MB.
#[test]
fn no_memory_growth_under_10k_events() {
    let events = make_events();

    // Force a GC-equivalent: run a warm-up pass so allocator bookkeeping is
    // already stable before we take the baseline measurement.
    {
        let mut engine = StandardEngine::new(InputMethod::Telex);
        for ev in &events {
            let _ = engine.process(ev);
        }
    }

    let rss_before = rss_bytes();

    let mut engine = StandardEngine::new(InputMethod::Telex);
    for ev in &events {
        let _ = engine.process(ev);
    }

    let rss_after = rss_bytes();

    if rss_before > 0 {
        let growth = rss_after.saturating_sub(rss_before);
        const MAX_GROWTH: u64 = 1024 * 1024; // 1 MB
        assert!(
            growth <= MAX_GROWTH,
            "RSS grew by {} bytes (> 1 MB) under {} events",
            growth,
            EVENT_COUNT,
        );
    }
}

/// Run 10 000 key events and assert total elapsed time < 10 seconds (1 ms/event).
#[test]
fn throughput_meets_latency_target() {
    let events = make_events();

    let start = Instant::now();
    let mut engine = StandardEngine::new(InputMethod::Telex);
    for ev in &events {
        let _ = engine.process(ev);
    }
    let elapsed = start.elapsed();

    println!(
        "throughput: {} events in {:.3}s ({:.3} µs/event)",
        EVENT_COUNT,
        elapsed.as_secs_f64(),
        elapsed.as_micros() as f64 / EVENT_COUNT as f64,
    );

    assert!(
        elapsed.as_secs() < 10,
        "{} events took {:.3}s (> 10 s budget)",
        EVENT_COUNT,
        elapsed.as_secs_f64(),
    );
}
