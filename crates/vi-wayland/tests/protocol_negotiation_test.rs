use vi_wayland::quirks::{CompositorProfile, CompositorQuirks};

// ── Profile detection from global lists ──────────────────────────────────────

#[test]
fn test_profile_detection_kwin() {
    let globals = [("kde_output_management_v2", 1u32), ("wl_compositor", 4)];
    let q = CompositorQuirks::from_global_pairs(&globals);
    assert_eq!(q.profile, CompositorProfile::KWin);
    assert!(q.snap_cursor_to_char_boundary);
    assert!(!q.empty_preedit_before_commit);
}

#[test]
fn test_profile_detection_hyprland() {
    let globals = [
        ("hyprland_global_shortcuts_manager_v1", 1u32),
        ("wl_compositor", 4),
    ];
    let q = CompositorQuirks::from_global_pairs(&globals);
    assert_eq!(q.profile, CompositorProfile::Hyprland);
    assert!(!q.snap_cursor_to_char_boundary);
    assert!(!q.empty_preedit_before_commit);
}

#[test]
fn test_profile_detection_gnome() {
    let globals = [
        ("wl_compositor", 5u32),
        ("zwp_text_input_manager_v3", 1),
        ("wl_seat", 7),
    ];
    let q = CompositorQuirks::from_global_pairs(&globals);
    assert_eq!(q.profile, CompositorProfile::Gnome);
    assert!(q.empty_preedit_before_commit);
    assert!(!q.snap_cursor_to_char_boundary);
}

#[test]
fn test_profile_detection_gnome_requires_compositor_v5() {
    // wl_compositor version 4 is not enough for GNOME detection.
    let globals = [("wl_compositor", 4u32), ("zwp_text_input_manager_v3", 1)];
    let q = CompositorQuirks::from_global_pairs(&globals);
    assert_eq!(q.profile, CompositorProfile::Standard);
}

#[test]
fn test_profile_detection_standard() {
    let globals = [("wl_compositor", 4u32), ("wl_seat", 7), ("wl_shm", 1)];
    let q = CompositorQuirks::from_global_pairs(&globals);
    assert_eq!(q.profile, CompositorProfile::Standard);
    assert!(!q.empty_preedit_before_commit);
    assert!(!q.snap_cursor_to_char_boundary);
}

#[test]
fn test_profile_detection_empty_globals() {
    let globals: &[(&str, u32)] = &[];
    let q = CompositorQuirks::from_global_pairs(globals);
    assert_eq!(q.profile, CompositorProfile::Standard);
}

// ── KWin takes precedence over other indicators ───────────────────────────────

#[test]
fn test_kwin_wins_over_gnome_markers() {
    // KWin can expose zwp_text_input_manager_v3 and a high wl_compositor
    // version; the kde_output_management_v2 probe must win.
    let globals = [
        ("kde_output_management_v2", 1u32),
        ("wl_compositor", 5),
        ("zwp_text_input_manager_v3", 1),
    ];
    let q = CompositorQuirks::from_global_pairs(&globals);
    assert_eq!(q.profile, CompositorProfile::KWin);
}

// ── Fallback to virtual-keyboard path ────────────────────────────────────────

#[test]
fn test_fallback_to_virtual_keyboard() {
    // A compositor that does NOT expose input-method-v2 should still result in
    // a Standard (or other) profile — the connect() code falls back to
    // virtual-keyboard-v1.  Here we verify the profile detection itself does
    // not make assumptions about input-method-v2 presence.
    let globals_vk_only = [
        ("wl_compositor", 4u32),
        ("zwp_virtual_keyboard_manager_v1", 1),
    ];
    let q = CompositorQuirks::from_global_pairs(&globals_vk_only);
    // Profile is Standard; the actual fallback is handled in WaylandBackend::connect.
    assert_eq!(q.profile, CompositorProfile::Standard);
    assert!(!q.empty_preedit_before_commit);
    assert!(!q.snap_cursor_to_char_boundary);
}
