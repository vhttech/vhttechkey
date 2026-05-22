use std::time::{Duration, Instant};

use criterion::{black_box, criterion_group, criterion_main, Criterion, SamplingMode};

/// Benchmark the Unix IPC socket bind — the final step before the daemon is
/// ready to accept connections.  Measured with a temp path to avoid conflicts
/// with any running daemon instance.
fn bench_socket_startup(c: &mut Criterion) {
    let mut group = c.benchmark_group("socket_startup");
    group.sampling_mode(SamplingMode::Flat);
    group.sample_size(10);

    group.bench_function("bind", |b| {
        b.iter_custom(|iters| {
            let mut total = Duration::ZERO;
            for _ in 0..iters {
                let dir = tempfile::tempdir().unwrap();
                let path = dir.path().join("vi-daemon.sock");
                let t = Instant::now();
                let _listener = black_box(std::os::unix::net::UnixListener::bind(&path).unwrap());
                total += t.elapsed();
            }
            total
        })
    });

    group.finish();
}

criterion_group!(benches, bench_socket_startup);
criterion_main!(benches);
