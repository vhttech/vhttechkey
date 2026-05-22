//! Per-key latency benchmark for `StandardEngine::process()`.
//!
//! Target: p99 < 100 µs per key (engine only, no I/O).
//! This benchmark catches performance regressions in the composition pipeline
//! before they affect interactive typing latency.

use std::time::{Duration, Instant};

use criterion::{black_box, criterion_group, criterion_main, Criterion, SamplingMode};
use vi_core::{CompositionEngine, InputEvent, InputMethod, Key, Modifiers, StandardEngine};

/// Measure p99 latency for 10 000 consecutive `process()` calls on a fresh
/// engine and assert it stays below 100 µs.
///
/// Uses `SamplingMode::Flat` so criterion collects one sample per iteration
/// without auto-scaling the iteration count, keeping the assertion meaningful.
fn bench_single_key_p99_under_100us(c: &mut Criterion) {
    let event = InputEvent::KeyDown(Key::Char('t'), Modifiers::none());

    let mut group = c.benchmark_group("key_latency");
    group.sampling_mode(SamplingMode::Flat);
    group.sample_size(10);

    group.bench_function("p99_under_100us", |b| {
        b.iter_custom(|iters| {
            let mut total = Duration::ZERO;

            // Collect per-call latencies outside the timing loop so the
            // assertion overhead doesn't skew criterion's numbers.
            let mut latencies: Vec<Duration> = Vec::with_capacity(10_000);
            {
                let mut engine = StandardEngine::new(InputMethod::Telex);
                for _ in 0..10_000 {
                    let t = Instant::now();
                    let _ = black_box(engine.process(&event));
                    latencies.push(t.elapsed());
                }
            }
            latencies.sort_unstable();
            let p99_idx = latencies.len() * 99 / 100;
            let p99 = latencies[p99_idx];
            assert!(
                p99 < Duration::from_micros(100),
                "p99 per-key latency {p99:?} exceeds 100 µs regression threshold \
                 (engine only, no I/O)"
            );

            // Criterion timing loop — separate from the assertion pass.
            for _ in 0..iters {
                let mut engine = StandardEngine::new(InputMethod::Telex);
                let t = Instant::now();
                let _ = black_box(engine.process(&event));
                total += t.elapsed();
            }
            total
        })
    });

    group.finish();
}

/// Throughput benchmark across all three input methods for a single composing
/// key.  Provides a quick per-method comparison without latency assertions.
fn bench_single_key_throughput(c: &mut Criterion) {
    let event = InputEvent::KeyDown(Key::Char('a'), Modifiers::none());

    let mut group = c.benchmark_group("key_latency");

    for method in [InputMethod::Telex, InputMethod::Vni, InputMethod::Viqr] {
        let label = format!("{method:?}_single_key");
        group.bench_function(&label, |b| {
            b.iter(|| {
                let mut engine = StandardEngine::new(method);
                let _ = black_box(engine.process(&event));
            })
        });
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_single_key_p99_under_100us,
    bench_single_key_throughput
);
criterion_main!(benches);
