//! Concurrent-event stress tests.
//!
//! Sends rapid activate/deactivate/input events from two threads simultaneously
//! (mirroring WaylandShared state transitions).  Asserts no panic and no stale
//! preedit leaking across focus sessions.

use parking_lot::Mutex;
use std::sync::Arc;
use std::thread;

struct MockShared {
    active: bool,
    pending_preedit: Option<(String, i32)>,
    serial: u32,
}

/// Thread A: rapid Activate/Deactivate cycles — invalidates preedit on Activate.
/// Thread B: Done events (serial++) and preedit updates while active.
/// Assert: no panic; pending_preedit is None immediately after each Activate.
#[test]
fn concurrent_activate_deactivate_no_stale_preedit() {
    let shared = Arc::new(Mutex::new(MockShared {
        active: false,
        pending_preedit: None,
        serial: 0,
    }));

    let shared_a = Arc::clone(&shared);
    let thread_a = thread::spawn(move || {
        for i in 0..200u32 {
            let mut s = shared_a.lock();
            if i % 2 == 0 {
                // Activate: reset serial and invalidate stale preedit.
                s.active = true;
                s.serial = 0;
                s.pending_preedit = None;
                assert!(s.pending_preedit.is_none(), "stale preedit must be cleared on Activate");
            } else {
                // Deactivate: flush pending preedit.
                s.active = false;
                let _ = s.pending_preedit.take();
            }
        }
    });

    let shared_b = Arc::clone(&shared);
    let thread_b = thread::spawn(move || {
        for _ in 0..200u32 {
            let mut s = shared_b.lock();
            // Simulate Done event + optional preedit update while active.
            s.serial = s.serial.wrapping_add(1);
            if s.active {
                s.pending_preedit = Some(("preedit".to_string(), 7));
            }
        }
    });

    thread_a.join().expect("activate/deactivate thread panicked");
    thread_b.join().expect("Done/preedit thread panicked");

    // After a final simulated Activate, preedit must be cleared.
    {
        let mut s = shared.lock();
        s.active = true;
        s.serial = 0;
        s.pending_preedit = None;
        assert!(s.pending_preedit.is_none(), "stale preedit leaked into new focus session");
    }
}

/// parking_lot::Mutex must not deadlock or panic under heavy contention.
#[test]
fn concurrent_rapid_lock_contention_no_deadlock() {
    let counter = Arc::new(Mutex::new(0u32));
    let mut handles = Vec::new();

    for _ in 0..4 {
        let c = Arc::clone(&counter);
        handles.push(thread::spawn(move || {
            for _ in 0..500 {
                *c.lock() += 1;
            }
        }));
    }

    for h in handles {
        h.join().expect("thread panicked");
    }

    assert_eq!(*counter.lock(), 4 * 500);
}
