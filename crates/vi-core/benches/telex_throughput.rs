use std::time::{Duration, Instant};

use criterion::{black_box, criterion_group, criterion_main, Criterion, SamplingMode};
use vi_core::{CompositionEngine, InputEvent, InputMethod, Key, Modifiers, StandardEngine};

fn make_events(keys: &str) -> Vec<InputEvent> {
    keys.chars()
        .map(|c| InputEvent::KeyDown(Key::Char(c), Modifiers::none()))
        .collect()
}

fn bench_telex_1000(c: &mut Criterion) {
    // ~1000-key Telex sequence covering vowel substitutions and tones.
    let pattern = "tooi vieejt nam dduwowcj ";
    let keys: String = pattern.chars().cycle().take(1000).collect();
    let events = make_events(&keys);

    c.bench_function("telex_1000_keystrokes", |b| {
        b.iter(|| {
            let mut engine = StandardEngine::new(InputMethod::Telex);
            for event in &events {
                let _ = black_box(engine.process(event));
            }
        })
    });
}

/// Measure per-event p99 latency for 10,000 sequential key events and assert
/// it stays under 1 ms.  Uses `SamplingMode::Flat` so criterion collects one
/// sample per iteration without auto-scaling the iteration count.
fn bench_telex_10000_p99(c: &mut Criterion) {
    let pattern = "tooi vieejt nam dduwowcj ";
    let keys: String = pattern.chars().cycle().take(10_000).collect();
    let events = make_events(&keys);

    let mut group = c.benchmark_group("telex_latency_10k");
    group.sampling_mode(SamplingMode::Flat);
    group.sample_size(10);

    group.bench_function("p99_per_event_under_1ms", |b| {
        b.iter_custom(|iters| {
            // Criterion total-time measurement: sum wall time for all iters.
            let mut total = Duration::ZERO;

            // Per-event latency collection: one engine pass outside the timing
            // loop so criterion's reported numbers stay meaningful.
            let mut latencies: Vec<Duration> = Vec::with_capacity(events.len());
            {
                let mut engine = StandardEngine::new(InputMethod::Telex);
                for event in &events {
                    let t = Instant::now();
                    let _ = black_box(engine.process(event));
                    latencies.push(t.elapsed());
                }
            }

            // Assert p99 before adding to total so a violation fails fast.
            latencies.sort_unstable();
            let p99_idx = latencies.len() * 99 / 100;
            let p99 = latencies[p99_idx];
            assert!(
                p99 < Duration::from_millis(1),
                "p99 per-event latency {p99:?} exceeds 1 ms threshold"
            );

            // Now run the full iters for criterion's timing.
            for _ in 0..iters {
                let mut engine = StandardEngine::new(InputMethod::Telex);
                let t = Instant::now();
                for event in &events {
                    let _ = black_box(engine.process(event));
                }
                total += t.elapsed();
            }

            total
        })
    });

    group.finish();
}

/// Measure throughput for keystrokes that never match any Telex vowel or tone
/// rule (pure append path, no allocation on the rule-match branch).
fn bench_telex_nomatch_throughput(c: &mut Criterion) {
    // 'b','c','h','k','l','m','n','p','t','v' are not vowels, not tone keys
    // (s/f/r/x/j/z), and no pair of them triggers a Telex vowel rule.
    let pattern = "bchlmnptv ";
    let keys: String = pattern.chars().cycle().take(1000).collect();
    let events = make_events(&keys);

    c.bench_function("telex_nomatch_throughput", |b| {
        b.iter(|| {
            let mut engine = StandardEngine::new(InputMethod::Telex);
            for event in &events {
                let _ = black_box(engine.process(event));
            }
        })
    });
}

/// Measure p99 latency for 10,000 single-key process() calls and assert
/// p99 ≤ 1000 µs.  Uses `SamplingMode::Flat` so criterion does not rescale.
fn bench_p99_per_key_us(c: &mut Criterion) {
    let event = InputEvent::KeyDown(Key::Char('a'), Modifiers::none());

    let mut group = c.benchmark_group("p99_per_key_us");
    group.sampling_mode(SamplingMode::Flat);
    group.sample_size(10);

    group.bench_function("telex", |b| {
        b.iter_custom(|iters| {
            let mut total = Duration::ZERO;

            // Collect per-call latencies on a fresh engine and assert p99.
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
                p99 <= Duration::from_micros(1_000),
                "p99 per-key latency {p99:?} exceeds 1000 µs threshold"
            );

            // Criterion timing loop.
            for _ in 0..iters {
                let mut engine = StandardEngine::new(InputMethod::Telex);
                let t = Instant::now();
                for _ in 0..10_000 {
                    let _ = black_box(engine.process(&event));
                }
                total += t.elapsed();
            }

            total
        })
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_telex_1000,
    bench_telex_10000_p99,
    bench_telex_nomatch_throughput,
    bench_p99_per_key_us,
);
criterion_main!(benches);
