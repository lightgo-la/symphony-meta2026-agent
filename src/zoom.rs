//! Canvas wheel-zoom math. Pure functions detached from egui widgets, so unit-testable.

/// Zoom lower bound. A safety net against 0/negative/infinity; no practical upper bound.
pub const MIN_ZOOM: f32 = 0.001;

/// One wheel-delta unit. Based on egui `raw_scroll_delta` (screen pt).
const WHEEL_UNIT: f32 = 50.0;
/// Base making delta +50 mean ×1.1 and −50 mean ÷1.1.
const ZOOM_BASE: f32 = 1.1;

/// Converts a wheel delta to a zoom factor. Positive = zoom in, negative = out, 0 = unchanged.
pub fn zoom_factor_for_wheel(wheel: f32) -> f32 {
    ZOOM_BASE.powf(wheel / WHEEL_UNIT)
}

/// Normalizes a zoom factor. No upper bound; only fixes <=0, NaN, and infinity.
pub fn clamp_zoom(zoom: f32) -> f32 {
    if zoom.is_finite() {
        zoom.max(MIN_ZOOM)
    } else {
        1.0
    }
}

/// Adjusts the pan offset so the point under the cursor stays fixed.
pub fn anchored_canvas(
    canvas: [f32; 2],
    mouse: [f32; 2],
    old_zoom: f32,
    new_zoom: f32,
) -> [f32; 2] {
    let k = new_zoom / old_zoom.max(0.001);
    [
        mouse[0] - (mouse[0] - canvas[0]) * k,
        mouse[1] - (mouse[1] - canvas[1]) * k,
    ]
}

/// Tests whether a point is inside a rect. rect is [x0, y0, x1, y1].
pub fn point_in_rect(p: [f32; 2], r: [f32; 4]) -> bool {
    p[0] >= r[0] && p[0] <= r[2] && p[1] >= r[1] && p[1] <= r[3]
}

/// Tests whether any of several rects covers a point.
pub fn point_in_zones(p: [f32; 2], zones: &[[f32; 4]]) -> bool {
    zones.iter().any(|&r| point_in_rect(p, r))
}

/// Decides whether to apply plain-wheel (raw) zoom. Ctrl+wheel/pinch goes through
/// the smoothed-zoom (`zoom_delta`) path, so only plain wheel is considered here.
/// No zoom over in-card scroll areas (keeps content scrolling) or
/// over overlays like modals/minimaps.
pub fn wheel_should_zoom(in_canvas: bool, over_scroll_zone: bool, over_overlay: bool) -> bool {
    in_canvas && !over_overlay && !over_scroll_zone
}

/// Applies one zoom step around the screen center (or cursor).
/// Keeps button/shortcut zoom from sliding toward the origin.
pub fn step_zoom_at_point(
    canvas: [f32; 2],
    zoom: f32,
    factor: f32,
    center: [f32; 2],
) -> ([f32; 2], f32) {
    let new_zoom = clamp_zoom(zoom * factor);
    (anchored_canvas(canvas, center, zoom, new_zoom), new_zoom)
}

/// Builds a selection-band rect from two drag points. [x0, y0, x1, y1] (normalized).
pub fn band_rect(a: [f32; 2], b: [f32; 2]) -> [f32; 4] {
    [
        a[0].min(b[0]),
        a[1].min(b[1]),
        a[0].max(b[0]),
        a[1].max(b[1]),
    ]
}

/// Picks node ids overlapping the band. rect is in screen coords [x0, y0, x1, y1].
pub fn select_in_rect(nodes: &[(usize, [f32; 4])], band: [f32; 4]) -> Vec<usize> {
    nodes
        .iter()
        .filter(|(_, r)| r[0] <= band[2] && r[2] >= band[0] && r[1] <= band[3] && r[3] >= band[1])
        .map(|(id, _)| *id)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wheel_up_zooms_in_and_down_zooms_out() {
        assert!((zoom_factor_for_wheel(50.0) - 1.1).abs() < 1e-6);
        assert!((zoom_factor_for_wheel(-50.0) - 1.0 / 1.1).abs() < 1e-6);
        assert_eq!(zoom_factor_for_wheel(0.0), 1.0);
    }

    #[test]
    fn zoom_has_no_upper_limit_and_guards_invalid() {
        assert_eq!(clamp_zoom(100.0), 100.0);
        assert_eq!(clamp_zoom(1.5), 1.5);
        assert_eq!(clamp_zoom(0.0), MIN_ZOOM);
        assert_eq!(clamp_zoom(-5.0), MIN_ZOOM);
        assert_eq!(clamp_zoom(f32::NAN), 1.0);
        assert_eq!(clamp_zoom(f32::INFINITY), 1.0);
    }

    #[test]
    fn zoom_anchor_keeps_point_under_cursor() {
        // The canvas point under the cursor must map to the same screen before and after zoom.
        let canvas = [120.0, 80.0];
        let mouse = [400.0, 300.0];
        let (old_zoom, new_zoom) = (1.0, 1.5);
        let canvas_point = [
            (mouse[0] - canvas[0]) / old_zoom,
            (mouse[1] - canvas[1]) / old_zoom,
        ];
        let next = anchored_canvas(canvas, mouse, old_zoom, new_zoom);
        let screen_after = [
            canvas_point[0] * new_zoom + next[0],
            canvas_point[1] * new_zoom + next[1],
        ];
        assert!((screen_after[0] - mouse[0]).abs() < 1e-4);
        assert!((screen_after[1] - mouse[1]).abs() < 1e-4);
    }

    #[test]
    fn zoom_gate_truth_table() {
        // Empty space zooms.
        assert!(wheel_should_zoom(true, false, false));
        // No zoom over scroll areas, to keep content scrolling.
        assert!(!wheel_should_zoom(true, true, false));
        // No zoom over overlays (modal/minimap).
        assert!(!wheel_should_zoom(true, false, true));
        assert!(!wheel_should_zoom(true, true, true));
        // Never zoom outside the canvas.
        assert!(!wheel_should_zoom(false, false, false));
        assert!(!wheel_should_zoom(false, true, false));
        assert!(!wheel_should_zoom(false, false, true));
    }

    #[test]
    fn anchored_step_keeps_center_still() {
        // Button zoom pins the screen center (prevents origin sliding).
        let (canvas, zoom) = step_zoom_at_point([100.0, 50.0], 1.0, 1.2, [600.0, 350.0]);
        assert!((zoom - 1.2).abs() < 1e-6);
        // The center's canvas coords must be identical before and after.
        let before = [(600.0 - 100.0) / 1.0, (350.0 - 50.0) / 1.0];
        let after = [(600.0 - canvas[0]) / zoom, (350.0 - canvas[1]) / zoom];
        assert!((before[0] - after[0]).abs() < 1e-4);
        assert!((before[1] - after[1]).abs() < 1e-4);
    }

    #[test]
    fn band_select_picks_overlapping_nodes() {
        let nodes = [
            (1, [0.0, 0.0, 100.0, 100.0]),
            (2, [200.0, 200.0, 300.0, 300.0]),
            (3, [90.0, 90.0, 150.0, 150.0]),
        ];
        assert_eq!(
            band_rect([150.0, 150.0], [50.0, 50.0]),
            [50.0, 50.0, 150.0, 150.0]
        );
        let hit = select_in_rect(&nodes, [50.0, 50.0, 150.0, 150.0]);
        assert_eq!(hit, vec![1, 3]);
        let empty = select_in_rect(&nodes, [400.0, 400.0, 500.0, 500.0]);
        assert!(empty.is_empty());
    }

    #[test]
    fn scroll_zone_hit_uses_screen_rects() {
        let zones = [[10.0, 10.0, 100.0, 60.0]];
        assert!(point_in_zones([50.0, 30.0], &zones));
        assert!(point_in_zones([10.0, 10.0], &zones));
        assert!(!point_in_zones([200.0, 30.0], &zones));
        assert!(!point_in_zones([50.0, 30.0], &[]));
    }

    #[test]
    fn mouse_wheel_event_reaches_raw_scroll_delta() {
        // Pins that real wheel events reach `raw_scroll_delta`, which app wheel zoom reads,
        // via a headless egui pass. If this breaks, all wheel zoom stops.
        use eframe::egui;
        let ctx = egui::Context::default();
        let mut raw = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_max(
                egui::pos2(0.0, 0.0),
                egui::pos2(1100.0, 700.0),
            )),
            ..Default::default()
        };
        raw.events.push(egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Line,
            delta: egui::vec2(0.0, 1.0),
            modifiers: egui::Modifiers::NONE,
        });
        let mut seen = egui::Vec2::ZERO;
        let _ = ctx.run(raw, |ctx| {
            seen = ctx.input(|i| i.raw_scroll_delta);
        });
        // One native notch = 1 line × 40pt line-scroll speed.
        assert_eq!(seen, egui::vec2(0.0, 40.0));
        assert!(zoom_factor_for_wheel(seen.y) > 1.0);
    }
}
