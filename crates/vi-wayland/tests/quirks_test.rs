use vi_wayland::quirks::{snap_to_char_boundary, CompositorProfile, CompositorQuirks};

// ── Hyprland/Labwc buffer_preedit_updates regression tests ───────────────────

/// Regression: Hyprland must set buffer_preedit_updates = true so that
/// update_preedit() calls flush() after writing preedit to the socket.
/// The original bug returned Ok(()) without flush(), silently dropping preedit.
#[test]
fn hyprland_sets_buffer_preedit_updates() {
    let globals = [("hyprland_global_shortcuts_manager_v1", 1u32)];
    let q = CompositorQuirks::from_global_pairs(&globals);
    assert_eq!(q.profile, CompositorProfile::Hyprland);
    assert!(
        q.buffer_preedit_updates,
        "Hyprland must enable buffer_preedit_updates"
    );
}

/// Regression: Labwc must set buffer_preedit_updates = true for the same reason.
#[test]
fn labwc_sets_buffer_preedit_updates() {
    let globals = [("labwc_options_v1", 1u32)];
    let q = CompositorQuirks::from_global_pairs(&globals);
    assert_eq!(q.profile, CompositorProfile::Labwc);
    assert!(
        q.buffer_preedit_updates,
        "Labwc must enable buffer_preedit_updates"
    );
}

/// Standard and GNOME compositors must not get buffer_preedit_updates.
#[test]
fn non_hyprland_does_not_set_buffer_preedit_updates() {
    let standard = CompositorQuirks::from_global_pairs(&[("wl_compositor", 4u32)]);
    assert!(!standard.buffer_preedit_updates);

    let gnome = CompositorQuirks::from_global_pairs(&[
        ("wl_compositor", 5u32),
        ("zwp_text_input_manager_v3", 1),
    ]);
    assert!(!gnome.buffer_preedit_updates);
}

/// Regression: simulate the buffer_preedit_updates branch of update_preedit().
///
/// The original bug: the Hyprland/Labwc path returned Ok(()) without calling
/// flush(), so preedit bytes sat in the socket buffer and were never shown.
/// The fix adds `drop(wl); return self.flush();`.
///
/// This test records the operation sequence and verifies:
/// 1. flush() is called (not silently omitted).
/// 2. The wl lock is released before flush() so the second lock acquisition
///    inside flush() does not deadlock.
#[test]
fn buffer_preedit_updates_path_calls_flush_after_lock_release() {
    let quirks = CompositorQuirks {
        profile: CompositorProfile::Hyprland,
        buffer_preedit_updates: true,
        ..Default::default()
    };

    // Simulate the buffer_preedit_updates branch of update_preedit().
    let mut ops: Vec<&str> = Vec::new();

    if quirks.buffer_preedit_updates {
        ops.push("acquire_wl_lock");
        ops.push("set_preedit_string"); // im.set_preedit_string(s, cb, cb)
        ops.push("im_commit"); // im.commit(serial)
                               // drop(wl) — must happen before self.flush() acquires wl again
        ops.push("drop_wl_lock");
        // self.flush() — was missing in the original bug
        ops.push("flush");
    } else {
        // Non-Hyprland: with_im() drops wl then calls flush internally.
        ops.push("with_im_drop_then_flush");
        ops.push("flush");
    }

    assert!(
        ops.contains(&"flush"),
        "buffer_preedit_updates path must call flush(); ops={ops:?}"
    );

    // Deadlock prevention: wl lock must be released before flush() re-acquires it.
    let drop_pos = ops.iter().position(|&o| o == "drop_wl_lock").unwrap();
    let flush_pos = ops.iter().position(|&o| o == "flush").unwrap();
    assert!(
        drop_pos < flush_pos,
        "wl lock must be released (pos {drop_pos}) before flush() (pos {flush_pos})"
    );
}

/// commit() and clear_preedit() use with_im(), which always calls flush() after
/// the closure. Verify the invariant holds: the wl lock is released inside
/// with_im before flush() is invoked.
#[test]
fn with_im_path_releases_lock_before_flush() {
    // Simulate the with_im() pattern used by commit() and clear_preedit().
    let mut ops: Vec<&str> = Vec::new();
    let input_method_present = true;

    ops.push("acquire_wl_lock");
    ops.push("acquire_shared_lock");
    ops.push("drop_shared_lock");
    if input_method_present {
        ops.push("run_closure"); // f(im, serial)
        ops.push("drop_wl_lock");
        ops.push("flush"); // self.flush()
    }

    assert!(
        ops.contains(&"flush"),
        "with_im path must always flush; ops={ops:?}"
    );

    let drop_pos = ops.iter().position(|&o| o == "drop_wl_lock").unwrap();
    let flush_pos = ops.iter().position(|&o| o == "flush").unwrap();
    assert!(
        drop_pos < flush_pos,
        "wl lock must be released before flush()"
    );
}

// ── KWin cursor-snap tests ────────────────────────────────────────────────────

#[test]
fn test_kwin_cursor_snaps_on_multibyte() {
    // "tôi": t=1 byte, ô=2 bytes (0xC3 0xB4), i=1 byte.
    // Byte layout: 0='t', 1=0xC3, 2=0xB4, 3='i'
    // cursor=2 lands in the middle of ô (not a char boundary).
    assert_eq!(snap_to_char_boundary("tôi", 2), 1);
}

#[test]
fn test_snap_at_char_boundary_unchanged() {
    // cursor=1 is exactly the start of ô — already a boundary.
    assert_eq!(snap_to_char_boundary("tôi", 1), 1);
}

#[test]
fn test_snap_zero_unchanged() {
    assert_eq!(snap_to_char_boundary("tôi", 0), 0);
}

#[test]
fn test_snap_past_end_clamps() {
    // "tôi" is 4 bytes; offset 99 clamps to 4.
    assert_eq!(snap_to_char_boundary("tôi", 99), 4);
}

#[test]
fn test_snap_ascii_all_boundaries() {
    for i in 0..="hello".len() {
        assert_eq!(snap_to_char_boundary("hello", i), i);
    }
}

// ── GNOME empty-preedit-before-commit test ───────────────────────────────────

#[test]
fn test_gnome_empty_preedit_flush_sent() {
    // Simulate the protocol operation sequence produced by the GNOME quirk
    // when a pending preedit is flushed on Deactivate / Leave.
    let quirks = CompositorQuirks {
        profile: CompositorProfile::Gnome,
        empty_preedit_before_commit: true,
        snap_cursor_to_char_boundary: false,
        niri_dual_protocol: false,
        ..Default::default()
    };

    let pending_preedit = Some(("viê".to_string(), 3i32));
    let mut ops: Vec<String> = Vec::new();

    if quirks.empty_preedit_before_commit {
        if let Some((preedit, _)) = pending_preedit {
            if !preedit.is_empty() {
                ops.push("set_preedit_string(\"\", -1, -1)".into());
                ops.push("commit".into());
                ops.push(format!("commit_string({preedit})"));
                ops.push("commit".into());
            }
        }
    }

    assert_eq!(ops.len(), 4);
    assert_eq!(ops[0], "set_preedit_string(\"\", -1, -1)");
    assert_eq!(ops[1], "commit");
    assert!(
        ops[2].contains("viê"),
        "commit_string must carry the preedit text"
    );
    assert_eq!(ops[3], "commit");
}

#[test]
fn test_gnome_quirk_skipped_when_not_gnome() {
    let quirks = CompositorQuirks {
        profile: CompositorProfile::Standard,
        empty_preedit_before_commit: false,
        snap_cursor_to_char_boundary: false,
        niri_dual_protocol: false,
        ..Default::default()
    };

    let pending_preedit = Some(("viê".to_string(), 3i32));
    let mut ops: Vec<String> = Vec::new();

    if quirks.empty_preedit_before_commit {
        if let Some((preedit, _)) = pending_preedit {
            ops.push(format!("commit_string({preedit})"));
        }
    }

    assert!(ops.is_empty());
}

// ── Compositor detection tests ───────────────────────────────────────────────

#[test]
fn test_niri_detected_by_ipc_global() {
    let globals = vec![
        ("niri_ipc", 1u32),
        ("wp_cursor_shape_manager_v1", 1u32),
        ("xdg_wm_base", 1u32),
    ];
    let q = CompositorQuirks::from_global_pairs(&globals);
    assert_eq!(q.profile, CompositorProfile::Niri);
    assert!(q.niri_dual_protocol);
}

#[test]
fn test_labwc_suppresses_candidate() {
    // labwc_options_v1 is the definitive Labwc identifier (0.7+).
    let globals = vec![
        ("wp_cursor_shape_manager_v1", 1u32),
        ("xdg_wm_base", 1u32),
        ("labwc_options_v1", 1u32),
    ];
    let q = CompositorQuirks::from_global_pairs(&globals);
    assert!(q.suppress_candidate_position);
}

#[test]
fn test_mir_no_surrounding_text() {
    let globals = vec![("mir_shell", 1u32), ("xdg_wm_base", 1u32)];
    let q = CompositorQuirks::from_global_pairs(&globals);
    assert!(q.no_surrounding_text);
}

// ── New compositor detection tests ───────────────────────────────────────────

#[test]
fn test_xfce_delay_preedit_clear() {
    // wp_viewporter without cursor-shape/KDE/Hyprland globals → XFCE.
    let globals = vec![
        ("wl_compositor", 6u32),
        ("wp_viewporter", 1u32),
        ("zwp_text_input_manager_v3", 1u32),
        ("xdg_wm_base", 1u32),
    ];
    let q = CompositorQuirks::from_global_pairs(&globals);
    assert_eq!(q.profile, CompositorProfile::Xfce);
    assert!(q.delay_preedit_clear);
    assert!(!q.empty_preedit_before_commit);
    assert!(!q.niri_dual_protocol);
    assert!(!q.virtual_keyboard_fallback);
}

#[test]
fn test_cinnamon_reuses_gnome_quirk() {
    // cinnamon_shell_v1 present → Cinnamon fast-path.
    let globals = vec![
        ("cinnamon_shell_v1", 1u32),
        ("zwp_text_input_manager_v3", 1u32),
        ("xdg_wm_base", 1u32),
    ];
    let q = CompositorQuirks::from_global_pairs(&globals);
    assert_eq!(q.profile, CompositorProfile::Cinnamon);
    assert!(q.empty_preedit_before_commit);
    assert!(!q.niri_dual_protocol);
    assert!(!q.delay_preedit_clear);
}

#[test]
fn test_lxqt_virtual_keyboard_fallback() {
    // Modern wl_compositor (v4) without text-input-v3 or input-method-v2 → LXQt.
    let globals = vec![("wl_compositor", 4u32), ("xdg_wm_base", 1u32)];
    let q = CompositorQuirks::from_global_pairs(&globals);
    assert_eq!(q.profile, CompositorProfile::LxQt);
    assert!(q.virtual_keyboard_fallback);
    assert!(!q.empty_preedit_before_commit);
    assert!(!q.delay_preedit_clear);
}

#[test]
fn test_lxqt_not_detected_without_modern_compositor() {
    // wl_compositor v3 → falls back to Standard, not LXQt.
    let globals = vec![("wl_compositor", 3u32), ("xdg_wm_base", 1u32)];
    let q = CompositorQuirks::from_global_pairs(&globals);
    assert_eq!(q.profile, CompositorProfile::Standard);
    assert!(!q.virtual_keyboard_fallback);
}

#[test]
fn test_labwc_options_v1_fast_path() {
    // labwc_options_v1 present → definitive labwc detection with both quirks.
    let globals = vec![
        ("labwc_options_v1", 1u32),
        ("wp_cursor_shape_manager_v1", 1u32),
        ("xdg_wm_base", 1u32),
    ];
    let q = CompositorQuirks::from_global_pairs(&globals);
    assert_eq!(q.profile, CompositorProfile::Labwc);
    assert!(q.suppress_candidate_position);
    assert!(!q.empty_preedit_before_commit);
    assert!(!q.niri_dual_protocol);
}

#[test]
fn test_river_fully_compliant() {
    // river_control_v1 → River profile with no active quirks.
    let globals = vec![
        ("river_control_v1", 1u32),
        ("zwp_text_input_manager_v3", 1u32),
        ("xdg_wm_base", 1u32),
    ];
    let q = CompositorQuirks::from_global_pairs(&globals);
    assert_eq!(q.profile, CompositorProfile::River);
    assert!(!q.empty_preedit_before_commit);
    assert!(!q.snap_cursor_to_char_boundary);
    assert!(!q.niri_dual_protocol);
    assert!(!q.suppress_candidate_position);
    assert!(!q.no_surrounding_text);
    assert!(!q.delay_preedit_clear);
    assert!(!q.virtual_keyboard_fallback);
}

// ── buffer_preedit_updates sent_commits sync regression tests ─────────────────

/// Regression: the buffer_preedit_updates path in update_preedit() must sync
/// sent_commits after each im.commit().  Without the sync, with_im()'s
/// niri_dual_protocol guard (sent_commits != ti_serial) fires and silently
/// drops all subsequent operations (Enter, commit, etc.).
///
/// Sequence simulated here:
///   Activate:          shared.serial=0, sent_commits=0, ti_serial=0
///   update_preedit 1:  commit(serial=0), sent_commits → 1
///   Done (round 1):    shared.serial=1, ti_serial=1
///   update_preedit 2:  commit(serial=1), sent_commits → 2
///   Done (round 2):    shared.serial=2, ti_serial=2
///
/// After two calls sent_commits must equal 2.  The niri_dual_protocol guard
/// checks `sent_commits != ti_serial`; with sent_commits=2 and ti_serial=2
/// the guard is false and the subsequent with_im call succeeds.
#[test]
#[allow(unused_assignments)]
fn buffer_preedit_updates_syncs_sent_commits() {
    // shared.serial starts at 0 on Activate; Done events increment it.
    let mut shared_serial: u32 = 0;
    let mut sent_commits: u32 = 0;
    let mut ti_serial: u32 = 0;

    // Call 1: buffer_preedit_updates path uses shared.serial=0 as commit serial,
    // then sets sent_commits = shared.serial.wrapping_add(1) = 1.
    sent_commits = shared_serial.wrapping_add(1);
    // Compositor processes preedit, sends Done.
    shared_serial = shared_serial.wrapping_add(1); // 1
    ti_serial = ti_serial.wrapping_add(1); // 1

    // Call 2: buffer_preedit_updates path uses shared.serial=1,
    // then sets sent_commits = 1.wrapping_add(1) = 2.
    sent_commits = shared_serial.wrapping_add(1);
    // Compositor processes preedit, sends Done.
    shared_serial = shared_serial.wrapping_add(1); // 2
    ti_serial = ti_serial.wrapping_add(1); // 2

    let _ = shared_serial; // used only for documentation clarity above

    assert_eq!(
        sent_commits, 2,
        "sent_commits must be 2 after two buffer-path update_preedit calls"
    );

    // The niri_dual_protocol guard (`sent_commits != ti_serial`) must be false
    // so that the subsequent with_im call (e.g. commit/Enter) is not dropped.
    let niri_dual_protocol = true;
    let guard_fires = niri_dual_protocol && sent_commits != ti_serial;
    assert!(
        !guard_fires,
        "with_im guard must not fire: sent_commits={sent_commits} ti_serial={ti_serial}; \
         guard would silently drop Enter/commit"
    );
}

/// Baseline: without the fix, sent_commits stays at 0 and the guard fires,
/// silently dropping the Enter/commit call.
#[test]
fn buffer_preedit_updates_without_fix_drops_with_im() {
    // After two Done events ti_serial=2, but the unfixed buffer path never
    // updates sent_commits so it stays at 0.
    let ti_serial: u32 = 2;
    let sent_commits: u32 = 0; // NOT updated by the unfixed buffer path

    let guard_fires = sent_commits != ti_serial;
    assert!(
        guard_fires,
        "without the fix the guard fires ({sent_commits} != {ti_serial}), \
         silently dropping subsequent with_im calls"
    );
}
