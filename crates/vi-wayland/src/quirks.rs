use std::collections::HashSet;

use wayland_client::globals::GlobalList;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CompositorProfile {
    #[default]
    Standard,
    Gnome,
    KWin,
    Hyprland,
    /// Niri requires zwp_input_method_v2 and zwp_text_input_v3 to be managed
    /// together (dual-protocol path).
    Niri,
    Labwc,
    Mir,
    /// Weston: reference compositor; strict protocol ordering, no quirks required.
    Weston,
    Sway,
    Xfce,
    Cinnamon,
    LxQt,
    River,
}

/// Detected version of the primary compositor-identifying Wayland global.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CompositorVersion(pub u32);

struct CompositorSignature {
    required: &'static [&'static str],
    absent: &'static [&'static str],
    weight: u32,
}

static PROFILES: &[(CompositorProfile, CompositorSignature)] = &[
    (
        CompositorProfile::KWin,
        CompositorSignature {
            required: &["kde_output_management_v2"],
            absent: &[],
            weight: 10,
        },
    ),
    (
        CompositorProfile::Hyprland,
        CompositorSignature {
            required: &["hyprland_global_shortcuts_manager_v1"],
            absent: &[],
            weight: 10,
        },
    ),
    (
        CompositorProfile::Sway,
        CompositorSignature {
            // Sway (wlroots) exposes zwlr_output_manager_v1; the absent guard
            // ensures Hyprland (which also exposes it) takes priority.
            required: &["zwlr_output_manager_v1"],
            absent: &["hyprland_global_shortcuts_manager_v1"],
            weight: 6,
        },
    ),
    (
        CompositorProfile::Niri,
        CompositorSignature {
            // Niri advertises cursor-shape-v1 but ships without DRM/KMS or
            // wlr-output globals, which distinguish it from GNOME (which also
            // gained cursor-shape-v1 in recent releases).  Weight 6 ensures
            // Niri beats Labwc (weight 5) when only cursor-shape is present.
            required: &["wp_cursor_shape_manager_v1"],
            absent: &[
                "wl_drm",
                "zwlr_output_manager_v1",
                "kde_output_management_v2",
                "hyprland_global_shortcuts_manager_v1",
            ],
            weight: 6,
        },
    ),
    (
        CompositorProfile::Labwc,
        CompositorSignature {
            // Labwc advertises cursor-shape but not text-input-v3 or wlr-layer-shell;
            // those absent globals prevent false-positive matches against Niri/GNOME.
            required: &["wp_cursor_shape_manager_v1"],
            absent: &["zwlr_layer_shell_v1", "zwp_text_input_manager_v3"],
            weight: 5,
        },
    ),
    (
        CompositorProfile::Mir,
        CompositorSignature {
            required: &["mir_shell"],
            absent: &[],
            weight: 10,
        },
    ),
    (
        CompositorProfile::Mir,
        CompositorSignature {
            // Fallback: Mir exposes mir_display_configuration without wlr output management.
            required: &["mir_display_configuration"],
            absent: &["zwlr_output_manager_v1"],
            weight: 5,
        },
    ),
    (
        CompositorProfile::Xfce,
        CompositorSignature {
            // xfwm4-wayland exposes wp_viewporter but not KDE, Hyprland, or cursor-shape
            // globals; the absent guards prevent false positives against compositors that
            // advertise cursor-shape (GNOME, Niri, Sway/wlroots).
            required: &["wp_viewporter"],
            absent: &[
                "kde_output_management_v2",
                "hyprland_global_shortcuts_manager_v1",
                "wp_cursor_shape_manager_v1",
            ],
            weight: 6,
        },
    ),
    (
        CompositorProfile::River,
        CompositorSignature {
            required: &["river_control_v1"],
            absent: &[],
            weight: 10,
        },
    ),
    (
        CompositorProfile::Weston,
        CompositorSignature {
            // Weston (reference compositor) advertises weston_screenshooter in its
            // global registry; no other production compositor exposes this interface.
            required: &["weston_screenshooter"],
            absent: &[],
            weight: 10,
        },
    ),
];

const MIN_SCORE: i32 = 1;

#[derive(Debug, Clone, Copy, Default)]
pub struct CompositorQuirks {
    pub profile: CompositorProfile,
    /// Version of the primary compositor-identifying global; zero when unknown.
    pub version: CompositorVersion,
    /// GNOME/Mutter: send empty preedit + commit before the actual commit_string.
    pub empty_preedit_before_commit: bool,
    /// KWin: snap cursor byte offset to the nearest UTF-8 char boundary.
    pub snap_cursor_to_char_boundary: bool,
    /// Niri: manage the zwp_text_input_v3 lifecycle (enable/done/disable)
    /// alongside zwp_input_method_v2 events.
    pub niri_dual_protocol: bool,
    /// Labwc: do not attempt to position the candidate window at the cursor.
    pub suppress_candidate_position: bool,
    /// Mir: text-input surrounding-text is not delivered reliably.
    pub no_surrounding_text: bool,
    /// XFCE/xfwm4: delay the preedit clear by one roundtrip after commit_string.
    pub delay_preedit_clear: bool,
    /// LXQt/Openbox-Wayland: no input-method-v2 present; use virtual-keyboard fallback.
    pub virtual_keyboard_fallback: bool,
    /// Hyprland/Labwc: flush after each preedit commit so the compositor receives
    /// updates in real time; without an explicit flush the messages sit in the
    /// socket buffer until the next commit.
    pub buffer_preedit_updates: bool,
}

impl CompositorQuirks {
    pub fn detect(globals: &GlobalList) -> Self {
        if let Ok(val) = std::env::var("VIME_COMPOSITOR_PROFILE") {
            if let Some(q) = Self::from_profile_name(&val) {
                return q;
            }
        }
        let list = globals.contents().clone_list();
        let pairs: Vec<(&str, u32)> = list
            .iter()
            .map(|g| (g.interface.as_str(), g.version))
            .collect();
        Self::from_global_pairs(&pairs)
    }

    /// Build quirks from the named profile string (value of `VIME_COMPOSITOR_PROFILE`).
    /// Accepted values (case-insensitive): standard, gnome, kwin, hyprland, niri.
    /// Returns `None` for unrecognised values so callers can fall through to heuristics.
    fn from_profile_name(val: &str) -> Option<Self> {
        let profile = match val.to_ascii_lowercase().as_str() {
            "standard" => CompositorProfile::Standard,
            "gnome" => CompositorProfile::Gnome,
            "kwin" => CompositorProfile::KWin,
            "hyprland" => CompositorProfile::Hyprland,
            "niri" => CompositorProfile::Niri,
            "sway" => CompositorProfile::Sway,
            "weston" => CompositorProfile::Weston,
            "xfce" => CompositorProfile::Xfce,
            "cinnamon" => CompositorProfile::Cinnamon,
            "lxqt" => CompositorProfile::LxQt,
            "labwc" => CompositorProfile::Labwc,
            "mir" => CompositorProfile::Mir,
            "river" => CompositorProfile::River,
            _ => return None,
        };
        tracing::info!("VIME_COMPOSITOR_PROFILE={val}: bypassing heuristic compositor detection");
        Some(Self::from_profile(profile))
    }

    /// Build quirks directly from a profile without Wayland global inspection.
    pub fn from_profile(profile: CompositorProfile) -> Self {
        Self {
            profile,
            version: CompositorVersion(0),
            empty_preedit_before_commit: matches!(
                profile,
                CompositorProfile::Gnome | CompositorProfile::Cinnamon
            ),
            snap_cursor_to_char_boundary: profile == CompositorProfile::KWin,
            niri_dual_protocol: profile == CompositorProfile::Niri,
            suppress_candidate_position: profile == CompositorProfile::Labwc,
            no_surrounding_text: profile == CompositorProfile::Mir,
            delay_preedit_clear: profile == CompositorProfile::Xfce,
            virtual_keyboard_fallback: profile == CompositorProfile::LxQt,
            buffer_preedit_updates: matches!(
                profile,
                CompositorProfile::Hyprland | CompositorProfile::Labwc
            ),
        }
    }

    pub fn from_global_pairs(globals: &[(&str, u32)]) -> Self {
        if let Ok(val) = std::env::var("VIME_COMPOSITOR_PROFILE") {
            if let Some(q) = Self::from_profile_name(&val) {
                return q;
            }
        }

        let global_set: HashSet<&str> = globals.iter().map(|(iface, _)| *iface).collect();
        let has = |name: &str| global_set.contains(name);
        let version_of = |name: &str| {
            globals
                .iter()
                .find(|(iface, _)| *iface == name)
                .map(|(_, v)| *v)
        };

        // Niri exposes a dedicated IPC global; trust it before the fragile heuristic.
        if has("niri_ipc") {
            return Self {
                profile: CompositorProfile::Niri,
                version: CompositorVersion(version_of("niri_ipc").unwrap_or(0)),
                niri_dual_protocol: true,
                ..Default::default()
            };
        }

        // labwc exposes labwc_options_v1 from version 0.7+; presence is definitive.
        if has("labwc_options_v1") {
            return Self {
                profile: CompositorProfile::Labwc,
                version: CompositorVersion(version_of("labwc_options_v1").unwrap_or(0)),
                suppress_candidate_position: true,
                buffer_preedit_updates: true,
                ..Default::default()
            };
        }

        // Cinnamon (Muffin fork): cinnamon_shell_v1 is present on Cinnamon >= 6.0.
        if has("cinnamon_shell_v1") {
            return Self {
                profile: CompositorProfile::Cinnamon,
                version: CompositorVersion(version_of("cinnamon_shell_v1").unwrap_or(0)),
                empty_preedit_before_commit: true,
                ..Default::default()
            };
        }

        // GNOME/Mutter: text-input-v3 on wl_compositor >= 5 is the reliable signal.
        // Exclude wp_viewporter (Xfce/xfwm4) so Xfce can be scored by PROFILES.
        // Older or generic compositors fall through to Standard.
        if has("zwp_text_input_manager_v3")
            && !has("kde_output_management_v2")
            && !has("hyprland_global_shortcuts_manager_v1")
            && !has("wp_viewporter")
            && version_of("wl_compositor").unwrap_or(0) >= 5
        {
            return Self {
                profile: CompositorProfile::Gnome,
                version: CompositorVersion(version_of("zwp_text_input_manager_v3").unwrap_or(0)),
                empty_preedit_before_commit: true,
                ..Default::default()
            };
        }

        let score_sig = |sig: &CompositorSignature| -> i32 {
            let w = sig.weight as i32;
            let present = sig
                .required
                .iter()
                .filter(|g| global_set.contains(*g))
                .count() as i32;
            let penalty = sig
                .absent
                .iter()
                .filter(|g| global_set.contains(*g))
                .count() as i32;
            (present - penalty) * w
        };

        let best = PROFILES
            .iter()
            .filter_map(|(profile, sig)| {
                let s = score_sig(sig);
                if s >= MIN_SCORE {
                    Some((profile, sig, s))
                } else {
                    None
                }
            })
            .max_by_key(|(_, _, s)| *s);

        let (profile, version) = match best {
            Some((profile, sig, _)) => {
                let ver = sig
                    .required
                    .first()
                    .and_then(|g| version_of(g))
                    .unwrap_or(0);
                (*profile, CompositorVersion(ver))
            }
            None => {
                // LXQt/Openbox-Wayland: modern compositor with XDG shell but no IME
                // protocols.  xdg_wm_base is required to distinguish LXQt from
                // minimal compositors that also lack it.
                if !has("zwp_text_input_manager_v3")
                    && !has("zwp_input_method_manager_v2")
                    && !has("zwp_virtual_keyboard_manager_v1")
                    && has("xdg_wm_base")
                    && version_of("wl_compositor").unwrap_or(0) >= 4
                {
                    (
                        CompositorProfile::LxQt,
                        CompositorVersion(version_of("wl_compositor").unwrap_or(0)),
                    )
                } else {
                    (CompositorProfile::Standard, CompositorVersion(0))
                }
            }
        };

        Self {
            profile,
            version,
            empty_preedit_before_commit: matches!(
                profile,
                CompositorProfile::Gnome | CompositorProfile::Cinnamon
            ),
            snap_cursor_to_char_boundary: profile == CompositorProfile::KWin,
            niri_dual_protocol: profile == CompositorProfile::Niri,
            suppress_candidate_position: profile == CompositorProfile::Labwc,
            no_surrounding_text: profile == CompositorProfile::Mir,
            delay_preedit_clear: profile == CompositorProfile::Xfce,
            virtual_keyboard_fallback: profile == CompositorProfile::LxQt,
            buffer_preedit_updates: matches!(
                profile,
                CompositorProfile::Hyprland | CompositorProfile::Labwc
            ),
        }
    }
}

/// Snap `byte_offset` back to the nearest UTF-8 character boundary in `s`.
///
/// # Precondition
/// `s` is valid UTF-8 (guaranteed by `&str`), but `byte_offset` may fall in
/// the middle of a multi-byte codepoint; this function walks backwards to the
/// nearest boundary so callers never produce a split codepoint.
pub fn snap_to_char_boundary(s: &str, byte_offset: usize) -> usize {
    let mut pos = byte_offset.min(s.len());
    // UTF-8 guarantees byte 0 is always a char boundary, so this loop terminates
    // in at most 3 steps (the maximum continuation-byte tail of any codepoint).
    while !s.is_char_boundary(pos) {
        pos -= 1;
    }
    pos
}

#[cfg(test)]
mod tests {
    use parking_lot::Mutex;

    use super::*;

    // Serialise tests that mutate the process environment.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn globals(names: &[&'static str]) -> Vec<(&'static str, u32)> {
        names.iter().map(|n| (*n, 1u32)).collect()
    }

    // ── profile detection ────────────────────────────────────────────────────

    #[test]
    fn detect_gnome() {
        // GNOME detection requires wl_compositor >= 5 and zwp_text_input_manager_v3.
        let g: &[(&str, u32)] = &[
            ("wl_compositor", 5),
            ("zwp_text_input_manager_v3", 1),
            ("xdg_wm_base", 1),
            ("wl_drm", 1),
            ("wp_cursor_shape_manager_v1", 1),
        ];
        let q = CompositorQuirks::from_global_pairs(g);
        assert_eq!(q.profile, CompositorProfile::Gnome);
        assert!(q.empty_preedit_before_commit);
        assert!(!q.snap_cursor_to_char_boundary);
        assert!(!q.niri_dual_protocol);
    }

    #[test]
    fn detect_kwin() {
        let g = globals(&[
            "kde_output_management_v2",
            "zwp_text_input_manager_v3",
            "xdg_wm_base",
        ]);
        let q = CompositorQuirks::from_global_pairs(&g);
        assert_eq!(q.profile, CompositorProfile::KWin);
        assert!(q.snap_cursor_to_char_boundary);
        assert!(!q.empty_preedit_before_commit);
        assert!(!q.niri_dual_protocol);
    }

    #[test]
    fn detect_hyprland() {
        let g = globals(&[
            "hyprland_global_shortcuts_manager_v1",
            "wl_drm",
            "zwlr_output_manager_v1",
            "xdg_wm_base",
        ]);
        let q = CompositorQuirks::from_global_pairs(&g);
        assert_eq!(q.profile, CompositorProfile::Hyprland);
        assert!(!q.empty_preedit_before_commit);
        assert!(!q.snap_cursor_to_char_boundary);
        assert!(!q.niri_dual_protocol);
    }

    #[test]
    fn detect_niri() {
        // No wl_drm or zwlr_output_manager_v1: Niri outscores GNOME (4 vs 3).
        let g = globals(&[
            "wp_cursor_shape_manager_v1",
            "zwp_text_input_manager_v3",
            "xdg_wm_base",
        ]);
        let q = CompositorQuirks::from_global_pairs(&g);
        assert_eq!(q.profile, CompositorProfile::Niri);
        assert!(q.niri_dual_protocol);
        assert!(!q.empty_preedit_before_commit);
        assert!(!q.snap_cursor_to_char_boundary);
    }

    #[test]
    fn detect_niri_ipc_fastpath() {
        // niri_ipc bypasses the scoring heuristic entirely.
        let g = globals(&["niri_ipc", "wp_cursor_shape_manager_v1"]);
        let q = CompositorQuirks::from_global_pairs(&g);
        assert_eq!(q.profile, CompositorProfile::Niri);
        assert!(q.niri_dual_protocol);
    }

    #[test]
    fn detect_labwc() {
        // labwc_options_v1 is present from Labwc 0.7+; the early-return path is definitive.
        let g = globals(&[
            "wp_cursor_shape_manager_v1",
            "xdg_wm_base",
            "labwc_options_v1",
        ]);
        let q = CompositorQuirks::from_global_pairs(&g);
        assert_eq!(q.profile, CompositorProfile::Labwc);
        assert!(q.suppress_candidate_position);
        assert!(!q.empty_preedit_before_commit);
        assert!(!q.niri_dual_protocol);
    }

    #[test]
    fn detect_mir() {
        let g = globals(&["mir_shell", "xdg_wm_base"]);
        let q = CompositorQuirks::from_global_pairs(&g);
        assert_eq!(q.profile, CompositorProfile::Mir);
        assert!(q.no_surrounding_text);
        assert!(!q.empty_preedit_before_commit);
        assert!(!q.niri_dual_protocol);
    }

    #[test]
    fn detect_weston() {
        let q = CompositorQuirks::from_global_pairs(&[("weston_screenshooter", 1)]);
        assert_eq!(q.profile, CompositorProfile::Weston);
        // Weston is protocol-correct: no quirk flags should be set.
        assert!(!q.empty_preedit_before_commit);
        assert!(!q.snap_cursor_to_char_boundary);
        assert!(!q.niri_dual_protocol);
        assert!(!q.suppress_candidate_position);
        assert!(!q.no_surrounding_text);
        assert!(!q.delay_preedit_clear);
        assert!(!q.virtual_keyboard_fallback);
    }

    #[test]
    fn detect_cinnamon() {
        // cinnamon_shell_v1 is the fast-path: present on Cinnamon >= 6.0.
        let q = CompositorQuirks::from_global_pairs(&[("cinnamon_shell_v1", 1)]);
        assert_eq!(q.profile, CompositorProfile::Cinnamon);
        assert!(q.empty_preedit_before_commit);
        assert!(!q.snap_cursor_to_char_boundary);
        assert!(!q.niri_dual_protocol);
    }

    #[test]
    fn detect_xfce() {
        // wp_viewporter without KDE/Hyprland/cursor-shape globals → Xfce wins.
        let g = globals(&["wp_viewporter", "xdg_wm_base"]);
        let q = CompositorQuirks::from_global_pairs(&g);
        assert_eq!(q.profile, CompositorProfile::Xfce);
        assert!(q.delay_preedit_clear);
        assert!(!q.empty_preedit_before_commit);
        assert!(!q.snap_cursor_to_char_boundary);
    }

    #[test]
    fn detect_sway() {
        let q = CompositorQuirks::from_global_pairs(&[("zwlr_output_manager_v1", 4)]);
        assert_eq!(q.profile, CompositorProfile::Sway);
        assert!(!q.empty_preedit_before_commit);
        assert!(!q.snap_cursor_to_char_boundary);
        assert!(!q.niri_dual_protocol);
        assert!(!q.suppress_candidate_position);
        assert!(!q.no_surrounding_text);
    }

    #[test]
    fn hyprland_beats_sway_when_both_globals_present() {
        let g = globals(&[
            "hyprland_global_shortcuts_manager_v1",
            "zwlr_output_manager_v1",
        ]);
        let q = CompositorQuirks::from_global_pairs(&g);
        assert_eq!(q.profile, CompositorProfile::Hyprland);
    }

    #[test]
    fn detect_standard() {
        let g = globals(&["wl_compositor", "xdg_wm_base"]);
        let q = CompositorQuirks::from_global_pairs(&g);
        assert_eq!(q.profile, CompositorProfile::Standard);
        assert!(!q.empty_preedit_before_commit);
        assert!(!q.snap_cursor_to_char_boundary);
        assert!(!q.niri_dual_protocol);
    }

    #[test]
    fn detect_standard_ambiguous() {
        // No globals match any profile signature → falls back to Standard.
        let q = CompositorQuirks::from_global_pairs(&[]);
        assert_eq!(q.profile, CompositorProfile::Standard);
        assert!(!q.empty_preedit_before_commit);
        assert!(!q.snap_cursor_to_char_boundary);
    }

    // ── VIME_COMPOSITOR_PROFILE env-var override ─────────────────────────────

    #[test]
    fn env_override_profiles() {
        let cases: &[(&str, CompositorProfile)] = &[
            ("standard", CompositorProfile::Standard),
            ("gnome", CompositorProfile::Gnome),
            ("GNOME", CompositorProfile::Gnome),
            ("kwin", CompositorProfile::KWin),
            ("hyprland", CompositorProfile::Hyprland),
            ("niri", CompositorProfile::Niri),
            ("sway", CompositorProfile::Sway),
            ("xfce", CompositorProfile::Xfce),
            ("cinnamon", CompositorProfile::Cinnamon),
            ("weston", CompositorProfile::Weston),
            ("labwc", CompositorProfile::Labwc),
            ("mir", CompositorProfile::Mir),
        ];
        // Pass KWin globals so heuristics would produce KWin without the override.
        let kwin_globals = globals(&["kde_output_management_v2"]);
        let _guard = ENV_LOCK.lock();
        for &(val, expected) in cases {
            // SAFETY: single-threaded under ENV_LOCK; variable is removed before next iteration.
            #[allow(unused_unsafe)]
            unsafe {
                std::env::set_var("VIME_COMPOSITOR_PROFILE", val)
            }
            let q = CompositorQuirks::from_global_pairs(&kwin_globals);
            #[allow(unused_unsafe)]
            unsafe {
                std::env::remove_var("VIME_COMPOSITOR_PROFILE")
            }
            assert_eq!(q.profile, expected, "env={val}");
        }
    }

    #[test]
    fn env_override_unknown_falls_through_to_heuristics() {
        let kwin_globals = globals(&["kde_output_management_v2"]);
        let _guard = ENV_LOCK.lock();
        #[allow(unused_unsafe)]
        unsafe {
            std::env::set_var("VIME_COMPOSITOR_PROFILE", "bogus_compositor")
        }
        let q = CompositorQuirks::from_global_pairs(&kwin_globals);
        #[allow(unused_unsafe)]
        unsafe {
            std::env::remove_var("VIME_COMPOSITOR_PROFILE")
        }
        assert_eq!(q.profile, CompositorProfile::KWin);
    }

    // ── snap_to_char_boundary ────────────────────────────────────────────────

    #[test]
    fn snap_ascii_stays_in_place() {
        let s = "hello";
        assert_eq!(snap_to_char_boundary(s, 0), 0);
        assert_eq!(snap_to_char_boundary(s, 3), 3);
        assert_eq!(snap_to_char_boundary(s, 5), 5);
        assert_eq!(snap_to_char_boundary(s, 99), 5); // clamped to len
    }

    #[test]
    fn snap_vietnamese_multibyte() {
        // "việt nam" (NFC):
        //   v=0  i=1  ệ(U+1EC7, E1 BB 87)=2,3,4  t=5  ' '=6  n=7  a=8  m=9
        //   len=10; char boundaries at 0,1,2,5,6,7,8,9,10
        let s = "việt nam";
        assert_eq!(s.len(), 10, "NFC encoding should be 10 bytes");
        assert_eq!(snap_to_char_boundary(s, 0), 0);
        assert_eq!(snap_to_char_boundary(s, 2), 2); // start of ệ
        assert_eq!(snap_to_char_boundary(s, 3), 2); // inside ệ → snaps to start
        assert_eq!(snap_to_char_boundary(s, 4), 2); // inside ệ → snaps to start
        assert_eq!(snap_to_char_boundary(s, 5), 5); // 't'
        assert_eq!(snap_to_char_boundary(s, 99), 10); // beyond end → clamped
    }

    #[test]
    fn snap_empty_string() {
        assert_eq!(snap_to_char_boundary("", 0), 0);
        assert_eq!(snap_to_char_boundary("", 5), 0);
    }
}
