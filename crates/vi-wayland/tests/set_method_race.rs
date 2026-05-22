//! Race-condition tests for IPC SetMethod concurrent with Wayland Deactivate.
//!
//! Lock ordering enforced here:
//!   Lock A = engine method
//!   Lock B = wayland state
//! Any path that needs both must acquire A before B.

use parking_lot::Mutex;
use std::sync::Arc;
use std::thread;

struct MockEngine {
    method: String,
}

struct MockWayland {
    active: bool,
    pending_preedit: Option<(String, i32)>,
    serial: u32,
}

/// IPC SetMethod (Lock A then B) races with Wayland Deactivate (Lock B only).
/// Assert: engine method reflects the last SetMethod; preedit is cleared.
#[test]
fn set_method_race_consistent_state() {
    let engine = Arc::new(Mutex::new(MockEngine {
        method: "vi-telex".to_string(),
    }));
    let wayland = Arc::new(Mutex::new(MockWayland {
        active: true,
        pending_preedit: Some(("đ".to_string(), 2)),
        serial: 3,
    }));

    // Thread 1: IPC SetMethod — acquire Lock A then Lock B (defined order).
    let eng1 = Arc::clone(&engine);
    let wl1 = Arc::clone(&wayland);
    let t1 = thread::spawn(move || {
        for _ in 0..100 {
            let mut eng = eng1.lock(); // Lock A
            eng.method = "vi-viqr".to_string();
            let mut wl = wl1.lock(); // Lock B after A
            wl.pending_preedit = None; // flush preedit after method change
        }
    });

    // Thread 2: Wayland Deactivate — Lock B only.
    let wl2 = Arc::clone(&wayland);
    let t2 = thread::spawn(move || {
        for _ in 0..100 {
            let mut wl = wl2.lock(); // Lock B only
            wl.active = false;
            let _ = wl.pending_preedit.take();
            // Re-activate for next iteration.
            wl.active = true;
            wl.serial = 0;
            wl.pending_preedit = None;
        }
    });

    t1.join().expect("SetMethod thread panicked");
    t2.join().expect("Deactivate thread panicked");

    let eng = engine.lock();
    let wl = wayland.lock();
    assert_eq!(
        eng.method, "vi-viqr",
        "engine method must reflect last SetMethod"
    );
    assert!(
        wl.pending_preedit.is_none(),
        "stale preedit must be cleared"
    );
}

/// Acquiring locks in the defined order (A then B) must never deadlock.
#[test]
fn lock_ordering_no_deadlock() {
    let lock_a = Arc::new(Mutex::new(0u32));
    let lock_b = Arc::new(Mutex::new(0u32));
    let mut handles = Vec::new();

    for _ in 0..4 {
        let a = Arc::clone(&lock_a);
        let b = Arc::clone(&lock_b);
        handles.push(thread::spawn(move || {
            for _ in 0..500 {
                let mut ga = a.lock(); // Lock A first
                let mut gb = b.lock(); // Lock B second
                *ga += 1;
                *gb += 1;
            }
        }));
    }

    for h in handles {
        h.join().expect("thread panicked");
    }

    assert_eq!(*lock_a.lock(), 4 * 500);
    assert_eq!(*lock_b.lock(), 4 * 500);
}
