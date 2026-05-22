// Latency regression gate for the composition engine.
//
// Measures per-syllable p50 and p99 latency across 10,000 iterations of a
// representative Telex composition sequence and fails (exit code 1) if either
// value exceeds the threshold.  `cargo bench -p vi-testing --bench latency_bench`
// returns non-zero, which fails CI.
//
// Thresholds are generous (1 ms / 5 ms) to catch only catastrophic regressions
// such as accidentally-added blocking I/O or exponential-time algorithms.

use std::time::Instant;
use vi_core::{CompositionEngine, InputEvent, InputMethod, Key, Modifiers, StandardEngine};

const WARMUP: usize = 500;
const ITERS: usize = 10_000;
const P50_LIMIT_NS: u128 = 1_000_000; // 1 ms
const P99_LIMIT_NS: u128 = 5_000_000; // 5 ms

fn syllable_events() -> Vec<InputEvent> {
    // "tôi" via Telex: t o o i Return — 5 events, commits one word.
    let mut evs: Vec<InputEvent> = "tooi"
        .chars()
        .map(|c| InputEvent::KeyDown(Key::Char(c), Modifiers::none()))
        .collect();
    evs.push(InputEvent::KeyDown(Key::Return, Modifiers::none()));
    evs
}

fn measure_ns(engine: &mut StandardEngine, evs: &[InputEvent]) -> u128 {
    let start = Instant::now();
    for ev in evs {
        let _ = engine.process(ev);
    }
    start.elapsed().as_nanos()
}

fn main() {
    let evs = syllable_events();
    let mut engine = StandardEngine::new(InputMethod::Telex);

    // Warmup — fills branch predictor and instruction caches.
    for _ in 0..WARMUP {
        measure_ns(&mut engine, &evs);
    }

    // Measurement.
    let mut times: Vec<u128> = (0..ITERS)
        .map(|_| measure_ns(&mut engine, &evs))
        .collect();
    times.sort_unstable();

    let p50 = times[ITERS * 50 / 100];
    let p99 = times[ITERS * 99 / 100];

    println!(
        "latency  p50={p50}ns ({:.3}ms)  p99={p99}ns ({:.3}ms)",
        p50 as f64 / 1_000_000.0,
        p99 as f64 / 1_000_000.0,
    );

    let mut fail = false;
    if p50 > P50_LIMIT_NS {
        eprintln!(
            "FAIL: p50 {p50}ns exceeds {P50_LIMIT_NS}ns threshold (1ms)"
        );
        fail = true;
    }
    if p99 > P99_LIMIT_NS {
        eprintln!(
            "FAIL: p99 {p99}ns exceeds {P99_LIMIT_NS}ns threshold (5ms)"
        );
        fail = true;
    }
    if fail {
        std::process::exit(1);
    }
}
