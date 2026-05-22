//! Candidate window placement, scaling, and egui rendering.
//!
//! `MonitorInfo` and the pure placement helpers are `#[cfg(test)]`-safe — they
//! do not require a display server.  The egui rendering methods (`show`,
//! `native_options`) are only meaningful inside an eframe event loop.

pub use vi_config::schema::CandidateOrientation;

// ── Monitor geometry ──────────────────────────────────────────────────────────

/// Display metrics for a connected monitor.
#[derive(Debug, Clone, PartialEq)]
pub struct MonitorInfo {
    /// Left edge in global screen coordinates.
    pub x: i32,
    /// Top edge in global screen coordinates.
    pub y: i32,
    /// Width in logical pixels.
    pub width: i32,
    /// Height in logical pixels.
    pub height: i32,
    /// HiDPI scale factor (1.0 = no scaling, 2.0 = 2× HiDPI).
    pub scale_factor: f32,
}

impl MonitorInfo {
    fn contains(&self, cx: i32, cy: i32) -> bool {
        cx >= self.x
            && cx < self.x + self.width
            && cy >= self.y
            && cy < self.y + self.height
    }
}

// ── Placement helpers ─────────────────────────────────────────────────────────

/// Return the monitor that contains `(cx, cy)`, falling back to the first
/// monitor in the list when the point is outside every monitor's rectangle.
///
/// Returns `None` only if `monitors` is empty.
pub fn cursor_monitor(
    monitors: &[MonitorInfo],
    cx: i32,
    cy: i32,
) -> Option<&MonitorInfo> {
    monitors
        .iter()
        .find(|m| m.contains(cx, cy))
        .or_else(|| monitors.first())
}

/// Compute the top-left screen position for a popup window of size `(w, h)`
/// placed just below the cursor, clamped entirely within `monitor`.
///
/// The vertical gap between the cursor and the window top is scaled by
/// `monitor.scale_factor` so it looks consistent across DPI settings.
pub fn compute_placement(
    monitor: &MonitorInfo,
    cursor_x: i32,
    cursor_y: i32,
    w: i32,
    h: i32,
) -> (i32, i32) {
    let gap = (16.0 * monitor.scale_factor) as i32;
    let w = w.max(1);
    let h = h.max(1);
    let x = cursor_x
        .max(monitor.x)
        .min(monitor.x + monitor.width - w);
    let y = (cursor_y + gap)
        .max(monitor.y)
        .min(monitor.y + monitor.height - h);
    (x, y)
}

// ── Candidate window ──────────────────────────────────────────────────────────

/// Orientation-aware, multi-monitor-aware candidate window state.
///
/// Call [`set_cursor`] whenever the cursor moves so the window tracks the
/// correct monitor and adopts that monitor's scale factor.  Pass the
/// candidate list to [`set_candidates`].  Render by calling [`show`] from
/// inside an egui viewport frame.
pub struct CandidateWindow {
    candidates: Vec<String>,
    orientation: CandidateOrientation,
    cursor_x: i32,
    cursor_y: i32,
    scale_factor: f32,
}

impl CandidateWindow {
    pub fn new(orientation: CandidateOrientation) -> Self {
        Self {
            candidates: Vec::new(),
            orientation,
            cursor_x: 0,
            cursor_y: 0,
            scale_factor: 1.0,
        }
    }

    /// Update cursor position and re-derive the scale factor from the monitor
    /// that now contains the cursor.
    pub fn set_cursor(&mut self, x: i32, y: i32, monitors: &[MonitorInfo]) {
        self.cursor_x = x;
        self.cursor_y = y;
        if let Some(m) = cursor_monitor(monitors, x, y) {
            self.scale_factor = m.scale_factor;
        }
    }

    /// Replace the displayed candidate strings.
    pub fn set_candidates(&mut self, candidates: Vec<String>) {
        self.candidates = candidates;
    }

    /// Current HiDPI scale factor derived from the containing monitor.
    pub fn scale_factor(&self) -> f32 {
        self.scale_factor
    }

    /// `true` when there are candidates to display.
    pub fn is_visible(&self) -> bool {
        !self.candidates.is_empty()
    }

    /// Render the candidate list into `ui`.
    ///
    /// Respects [`CandidateOrientation`]: horizontal lays candidates out in a
    /// row separated by vertical rules; vertical stacks them.
    pub fn show(&self, ui: &mut egui::Ui) {
        match self.orientation {
            CandidateOrientation::Horizontal => {
                ui.horizontal(|ui| {
                    for (i, candidate) in self.candidates.iter().enumerate() {
                        if i > 0 {
                            ui.separator();
                        }
                        ui.label(candidate.as_str());
                    }
                });
            }
            CandidateOrientation::Vertical => {
                for candidate in &self.candidates {
                    ui.label(candidate.as_str());
                }
            }
        }
    }

    /// Build [`eframe::NativeOptions`] for a borderless popup window placed
    /// below the cursor on the correct monitor, scaled to the monitor's DPI.
    pub fn native_options(&self, monitors: &[MonitorInfo]) -> eframe::NativeOptions {
        const W: i32 = 300;
        const H: i32 = 64;

        let (px, py) = cursor_monitor(monitors, self.cursor_x, self.cursor_y)
            .map(|m| compute_placement(m, self.cursor_x, self.cursor_y, W, H))
            .unwrap_or((self.cursor_x, self.cursor_y));

        eframe::NativeOptions {
            renderer: crate::preferred_renderer(),
            viewport: egui::ViewportBuilder::default()
                .with_decorations(false)
                .with_resizable(false)
                .with_inner_size(egui::vec2(W as f32, H as f32))
                .with_position(egui::pos2(px as f32, py as f32)),
            ..Default::default()
        }
    }
}
