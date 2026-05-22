use criterion::{black_box, criterion_group, criterion_main, Criterion};
use vi_core::{CompositionEngine, InputEvent, InputMethod, Key, Modifiers, StandardEngine};

fn make_key(c: char) -> InputEvent {
    InputEvent::KeyDown(Key::Char(c), Modifiers::none())
}

fn bench_telex_toi(c: &mut Criterion) {
    c.bench_function("telex_toi", |b| {
        b.iter(|| {
            let mut e = StandardEngine::new(InputMethod::Telex);
            for ch in "tooi".chars() {
                e.process(black_box(&make_key(ch))).ok();
            }
            e.process(black_box(&InputEvent::KeyDown(
                Key::Return,
                Modifiers::none(),
            )))
            .ok()
        })
    });
}

fn bench_telex_duoc(c: &mut Criterion) {
    c.bench_function("telex_duoc", |b| {
        b.iter(|| {
            let mut e = StandardEngine::new(InputMethod::Telex);
            for ch in "dduwowcj".chars() {
                e.process(black_box(&make_key(ch))).ok();
            }
            e.process(black_box(&InputEvent::KeyDown(
                Key::Return,
                Modifiers::none(),
            )))
            .ok()
        })
    });
}

fn bench_vni_toi(c: &mut Criterion) {
    c.bench_function("vni_toi", |b| {
        b.iter(|| {
            let mut e = StandardEngine::new(InputMethod::Vni);
            for ch in "to6i".chars() {
                e.process(black_box(&make_key(ch))).ok();
            }
            e.process(black_box(&InputEvent::KeyDown(
                Key::Return,
                Modifiers::none(),
            )))
            .ok()
        })
    });
}

fn bench_vni_duoc(c: &mut Criterion) {
    c.bench_function("vni_duoc", |b| {
        b.iter(|| {
            let mut e = StandardEngine::new(InputMethod::Vni);
            for ch in "d9u7o7c5".chars() {
                e.process(black_box(&make_key(ch))).ok();
            }
            e.process(black_box(&InputEvent::KeyDown(
                Key::Return,
                Modifiers::none(),
            )))
            .ok()
        })
    });
}

fn bench_viqr_toi(c: &mut Criterion) {
    c.bench_function("viqr_toi", |b| {
        b.iter(|| {
            let mut e = StandardEngine::new(InputMethod::Viqr);
            for ch in "to^i".chars() {
                e.process(black_box(&make_key(ch))).ok();
            }
            e.process(black_box(&InputEvent::KeyDown(
                Key::Return,
                Modifiers::none(),
            )))
            .ok()
        })
    });
}

fn bench_telex_throughput_1k_syllables(c: &mut Criterion) {
    let syllables: Vec<&str> = vec!["tooi", "aa", "ee", "uw", "ow", "dd", "as", "af", "ar"];
    c.bench_function("telex_1k_syllables", |b| {
        b.iter(|| {
            let mut total = 0usize;
            for _ in 0..1_000 {
                for &syl in &syllables {
                    let mut e = StandardEngine::new(InputMethod::Telex);
                    for ch in syl.chars() {
                        e.process(black_box(&make_key(ch))).ok();
                    }
                    if let Ok(t) = e.process(black_box(&InputEvent::KeyDown(
                        Key::Return,
                        Modifiers::none(),
                    ))) {
                        total += 1;
                        black_box(t);
                    }
                }
            }
            total
        })
    });
}

criterion_group!(
    composition,
    bench_telex_toi,
    bench_telex_duoc,
    bench_vni_toi,
    bench_vni_duoc,
    bench_viqr_toi,
    bench_telex_throughput_1k_syllables,
);
criterion_main!(composition);
