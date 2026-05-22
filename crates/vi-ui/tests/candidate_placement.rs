use vi_ui::candidate::{CandidateOrientation, CandidateWindow, MonitorInfo, cursor_monitor};

fn dual_monitor_setup() -> Vec<MonitorInfo> {
    vec![
        MonitorInfo { x: 0,    y: 0, width: 1920, height: 1080, scale_factor: 1.0 },
        MonitorInfo { x: 1920, y: 0, width: 2560, height: 1440, scale_factor: 2.0 },
    ]
}

/// Cursor at (2200, 100) lies inside monitor 2 (x=1920..4480, y=0..1440).
/// The returned monitor must have scale 2.0.
#[test]
fn cursor_on_second_monitor_selects_correct_monitor_and_scale() {
    let monitors = dual_monitor_setup();
    let m = cursor_monitor(&monitors, 2200, 100)
        .expect("cursor_monitor must return Some for a non-empty list");
    assert_eq!(m.x, 1920, "cursor should resolve to the second monitor");
    assert!(
        (m.scale_factor - 2.0).abs() < f32::EPSILON,
        "second monitor scale_factor must be 2.0, got {}",
        m.scale_factor
    );
}

/// Cursor exactly at the left edge of monitor 2 (x=1920) must still resolve
/// to monitor 2 (contains is inclusive of the left edge).
#[test]
fn cursor_at_monitor_boundary_selects_right_monitor() {
    let monitors = dual_monitor_setup();
    let m = cursor_monitor(&monitors, 1920, 0).unwrap();
    assert_eq!(m.x, 1920);
}

/// Cursor inside monitor 1 must resolve to monitor 1 (scale 1.0).
#[test]
fn cursor_on_first_monitor_selects_scale_1() {
    let monitors = dual_monitor_setup();
    let m = cursor_monitor(&monitors, 100, 100).unwrap();
    assert_eq!(m.x, 0);
    assert!((m.scale_factor - 1.0).abs() < f32::EPSILON);
}

/// Cursor outside every monitor falls back to the first monitor.
#[test]
fn cursor_outside_all_monitors_falls_back_to_first() {
    let monitors = dual_monitor_setup();
    let m = cursor_monitor(&monitors, 9999, 9999).unwrap();
    assert_eq!(m.x, 0, "fallback must be the first monitor");
}

/// Empty monitor list must return None.
#[test]
fn empty_monitor_list_returns_none() {
    assert!(cursor_monitor(&[], 0, 0).is_none());
}

// ── Scale-change mid-session ──────────────────────────────────────────────────

/// Simulates a wl_output scale change from 1.0 to 2.0 arriving mid-session.
///
/// Asserts:
/// - No panic during or after the scale update.
/// - `CandidateWindow::scale_factor()` reflects the new value immediately.
#[test]
fn scale_change_mid_session_no_crash_and_updates_scale() {
    let mut monitors = vec![
        MonitorInfo { x: 0, y: 0, width: 1920, height: 1080, scale_factor: 1.0 },
    ];
    let mut win = CandidateWindow::new(CandidateOrientation::Horizontal);

    win.set_cursor(100, 100, &monitors);
    assert!(
        (win.scale_factor() - 1.0).abs() < f32::EPSILON,
        "initial scale must be 1.0"
    );

    // Simulate wl_output Scale event arriving with factor = 2
    monitors[0].scale_factor = 2.0;

    // No crash: re-derive scale from updated monitor list
    win.set_cursor(100, 100, &monitors);

    assert!(
        (win.scale_factor() - 2.0).abs() < f32::EPSILON,
        "scale_factor must update to 2.0 after monitor scale change, got {}",
        win.scale_factor()
    );
}

/// set_candidates / is_visible round-trip.
#[test]
fn set_candidates_makes_window_visible() {
    let mut win = CandidateWindow::new(CandidateOrientation::Vertical);
    assert!(!win.is_visible());
    win.set_candidates(vec!["hoà".to_owned(), "hòa".to_owned()]);
    assert!(win.is_visible());
    win.set_candidates(vec![]);
    assert!(!win.is_visible());
}
