//! Multi-threaded stress helpers.

use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use unicode_normalization::UnicodeNormalization;
use vi_core::{
    CompositionEngine, InputEvent, InputMethod, Key, Modifiers, StandardEngine, StateTransition,
};

/// Minimal LCG PRNG: no external dependencies, deterministic.
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

/// Representative Telex key sequences covering all six tones and common vowels.
const SYLLABLES: &[&str] = &[
    "toi", "viet", "an", "em", "hay",
    "af", "as", "ar", "ax", "aj",   // a + tones
    "tooi", "toof", "toos", "toor", // tô + tones
    "uws", "uwf", "uwr",            // ư + tones
    "ees", "eef",                   // ê + tones
    "oof", "oos",                   // ô + tones
    "owf", "ows",                   // ơ + tones
    "awf", "aws",                   // ă + tones
    "ddi",                          // đi
];

fn compose_one(engine: &mut StandardEngine, syllable: &str) -> Option<String> {
    engine.reset();
    for ch in syllable.chars() {
        let _ = engine.process(&InputEvent::KeyDown(Key::Char(ch), Modifiers::none()));
        let _ = engine.process(&InputEvent::KeyUp(Key::Char(ch)));
    }
    match engine.process(&InputEvent::KeyDown(Key::Return, Modifiers::none())) {
        Ok(StateTransition::Commit(c) | StateTransition::CommitAndClear(c)) => {
            Some(c.as_str().to_owned())
        }
        _ => None,
    }
}

/// Run `thread_count` threads, each composing `syllables_per_thread` randomly
/// chosen Vietnamese syllables through a **shared** `Arc<Mutex<StandardEngine>>`.
///
/// Returns `Some(total_commits)` if the run completes within `timeout`, or
/// `None` if the timeout was exceeded (potential deadlock).  All committed
/// strings are validated to be valid NFC before returning.
pub fn multithreaded_stress_run(
    thread_count: usize,
    syllables_per_thread: usize,
    timeout: Duration,
) -> Option<usize> {
    let engine = Arc::new(Mutex::new(StandardEngine::new(InputMethod::Telex)));
    let start = Instant::now();

    let handles: Vec<_> = (0..thread_count)
        .map(|thread_id| {
            let engine = Arc::clone(&engine);
            thread::spawn(move || {
                let mut rng =
                    Lcg(0xDEAD_BEEF_u64.wrapping_add((thread_id as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)));
                let mut results: Vec<String> = Vec::with_capacity(syllables_per_thread);

                for _ in 0..syllables_per_thread {
                    let syllable = SYLLABLES[rng.next_usize(SYLLABLES.len())];
                    let mut guard = engine.lock().unwrap();
                    if let Some(s) = compose_one(&mut guard, syllable) {
                        results.push(s);
                    }
                }

                results
            })
        })
        .collect();

    let mut all: Vec<String> = Vec::new();
    for handle in handles {
        let results = handle.join().expect("stress thread panicked");
        all.extend(results);
    }

    if start.elapsed() > timeout {
        return None;
    }

    for s in &all {
        let nfc: String = s.nfc().collect();
        assert_eq!(s, &nfc, "committed string is not NFC: {s:?}");
    }

    Some(all.len())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    /// 8 threads × 10 000 syllables through a shared engine must complete within
    /// 30 seconds, produce no panics, and emit only valid NFC strings.
    #[test]
    fn multithreaded_8x10k_no_deadlock_all_nfc() {
        let total = multithreaded_stress_run(8, 10_000, Duration::from_secs(30))
            .expect("multithreaded stress test timed out (possible deadlock)");

        assert!(total > 0, "no syllables were committed during stress run");
    }
}
