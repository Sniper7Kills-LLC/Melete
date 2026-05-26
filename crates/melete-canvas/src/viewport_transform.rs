use melete_core::{Point, Rect, Viewport};

/// Wraps a [`Viewport`] together with a screen size (in physical pixels)
/// and provides conversions between canvas-space (world) and screen-space.
///
/// The viewport's `center` is the canvas-space point at the centre of the
/// screen. `zoom` is screen-pixels per canvas-unit. Rotation is currently
/// preserved on the inner viewport but not applied (Phase 1).
#[derive(Debug, Clone, Copy)]
pub struct ViewportTransform {
    viewport: Viewport,
    screen_width: f64,
    screen_height: f64,
}

impl ViewportTransform {
    pub fn new(viewport: Viewport, screen_width: f64, screen_height: f64) -> Self {
        Self {
            viewport,
            screen_width,
            screen_height,
        }
    }

    pub fn viewport(&self) -> Viewport {
        self.viewport
    }

    pub fn zoom(&self) -> f64 {
        self.viewport.zoom
    }

    pub fn center(&self) -> Point {
        self.viewport.center
    }

    pub fn screen_size(&self) -> (f64, f64) {
        (self.screen_width, self.screen_height)
    }

    pub fn set_size(&mut self, width: f64, height: f64) {
        self.screen_width = width;
        self.screen_height = height;
    }

    pub fn set_viewport(&mut self, viewport: Viewport) {
        self.viewport = viewport;
    }

    pub fn canvas_to_screen(&self, p: Point) -> (f64, f64) {
        let dx = (p.x - self.viewport.center.x) * self.viewport.zoom;
        let dy = (p.y - self.viewport.center.y) * self.viewport.zoom;
        (self.screen_width * 0.5 + dx, self.screen_height * 0.5 + dy)
    }

    pub fn screen_to_canvas(&self, screen_xy: (f64, f64)) -> Point {
        let (sx, sy) = screen_xy;
        let dx = (sx - self.screen_width * 0.5) / self.viewport.zoom;
        let dy = (sy - self.screen_height * 0.5) / self.viewport.zoom;
        Point {
            x: self.viewport.center.x + dx,
            y: self.viewport.center.y + dy,
        }
    }

    /// Pan the viewport by a delta expressed in screen pixels.
    pub fn pan(&mut self, dx: f64, dy: f64) {
        self.viewport.center.x -= dx / self.viewport.zoom;
        self.viewport.center.y -= dy / self.viewport.zoom;
    }

    /// Zoom toward a screen-space point. The canvas-space point currently
    /// under `screen_xy` stays under `screen_xy` after the zoom.
    pub fn zoom_at(&mut self, screen_xy: (f64, f64), factor: f64) {
        if factor <= 0.0 || !factor.is_finite() {
            return;
        }
        let anchor = self.screen_to_canvas(screen_xy);
        self.viewport.zoom *= factor;
        let (sx, sy) = screen_xy;
        // Recompute centre so that `anchor` maps back to `screen_xy`.
        self.viewport.center.x = anchor.x - (sx - self.screen_width * 0.5) / self.viewport.zoom;
        self.viewport.center.y = anchor.y - (sy - self.screen_height * 0.5) / self.viewport.zoom;
    }

    /// The visible canvas-space rectangle.
    pub fn visible_canvas_rect(&self) -> Rect {
        let half_w = self.screen_width * 0.5 / self.viewport.zoom;
        let half_h = self.screen_height * 0.5 / self.viewport.zoom;
        Rect {
            x: self.viewport.center.x - half_w,
            y: self.viewport.center.y - half_h,
            width: half_w * 2.0,
            height: half_h * 2.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vt(center_x: f64, center_y: f64, zoom: f64, w: f64, h: f64) -> ViewportTransform {
        ViewportTransform::new(
            Viewport {
                center: Point {
                    x: center_x,
                    y: center_y,
                },
                zoom,
                rotation: 0.0,
            },
            w,
            h,
        )
    }

    #[test]
    fn screen_to_canvas_centre_is_viewport_centre() {
        let t = vt(10.0, 20.0, 1.0, 800.0, 600.0);
        let p = t.screen_to_canvas((400.0, 300.0));
        assert!((p.x - 10.0).abs() < 1e-9);
        assert!((p.y - 20.0).abs() < 1e-9);
    }

    #[test]
    fn canvas_to_screen_round_trip() {
        let t = vt(50.0, -25.0, 1.5, 1024.0, 768.0);
        for (cx, cy) in [(0.0, 0.0), (100.0, 200.0), (-300.0, 50.0), (50.0, -25.0)] {
            let (sx, sy) = t.canvas_to_screen(Point { x: cx, y: cy });
            let back = t.screen_to_canvas((sx, sy));
            assert!(
                (back.x - cx).abs() < 1e-9 && (back.y - cy).abs() < 1e-9,
                "round trip drift at ({cx},{cy}) → ({sx},{sy}) → ({},{})",
                back.x,
                back.y
            );
        }
    }

    #[test]
    fn zoom_scales_screen_distance_from_centre() {
        let t = vt(0.0, 0.0, 2.0, 1000.0, 500.0);
        // A point 10 canvas-units right of centre should land 20 screen
        // pixels right of the centre at zoom=2.
        let (sx, _sy) = t.canvas_to_screen(Point { x: 10.0, y: 0.0 });
        assert!((sx - 520.0).abs() < 1e-9);
    }

    #[test]
    fn pan_shifts_canvas_under_cursor() {
        let mut t = vt(0.0, 0.0, 1.0, 800.0, 600.0);
        // `pan(dx, dy)` interprets (dx, dy) as user drag in screen
        // pixels — drag 50px right + 25px down means the canvas point
        // that was at screen (100, 100) ends up at screen (150, 125).
        let before = t.screen_to_canvas((100.0, 100.0));
        t.pan(50.0, 25.0);
        let after = t.screen_to_canvas((150.0, 125.0));
        assert!((before.x - after.x).abs() < 1e-9);
        assert!((before.y - after.y).abs() < 1e-9);
    }

    #[test]
    fn zoom_at_anchor_pins_canvas_point_under_cursor() {
        let mut t = vt(0.0, 0.0, 1.0, 800.0, 600.0);
        let anchor_screen = (200.0, 150.0);
        let anchor_canvas_before = t.screen_to_canvas(anchor_screen);
        t.zoom_at(anchor_screen, 2.5);
        let anchor_canvas_after = t.screen_to_canvas(anchor_screen);
        assert!((anchor_canvas_before.x - anchor_canvas_after.x).abs() < 1e-9);
        assert!((anchor_canvas_before.y - anchor_canvas_after.y).abs() < 1e-9);
        assert!((t.zoom() - 2.5).abs() < 1e-9);
    }

    #[test]
    fn zoom_at_rejects_non_positive_or_non_finite_factor() {
        let mut t = vt(0.0, 0.0, 1.0, 800.0, 600.0);
        let z0 = t.zoom();
        t.zoom_at((0.0, 0.0), 0.0);
        assert_eq!(t.zoom(), z0);
        t.zoom_at((0.0, 0.0), -1.0);
        assert_eq!(t.zoom(), z0);
        t.zoom_at((0.0, 0.0), f64::NAN);
        assert_eq!(t.zoom(), z0);
        t.zoom_at((0.0, 0.0), f64::INFINITY);
        assert_eq!(t.zoom(), z0);
    }

    #[test]
    fn visible_canvas_rect_centred_on_viewport_centre() {
        let t = vt(50.0, 100.0, 2.0, 400.0, 200.0);
        let r = t.visible_canvas_rect();
        // half_w = 400 * 0.5 / 2 = 100; half_h = 200 * 0.5 / 2 = 50
        assert!((r.x - (50.0 - 100.0)).abs() < 1e-9);
        assert!((r.y - (100.0 - 50.0)).abs() < 1e-9);
        assert!((r.width - 200.0).abs() < 1e-9);
        assert!((r.height - 100.0).abs() < 1e-9);
    }

    #[test]
    fn set_size_updates_screen_dimensions() {
        let mut t = vt(0.0, 0.0, 1.0, 800.0, 600.0);
        t.set_size(1024.0, 768.0);
        let (w, h) = t.screen_size();
        assert_eq!(w, 1024.0);
        assert_eq!(h, 768.0);
    }

    #[test]
    fn screen_to_canvas_at_origin_zoom_two() {
        let t = vt(0.0, 0.0, 2.0, 200.0, 200.0);
        // 50 px right of centre → 25 canvas units right.
        let p = t.screen_to_canvas((150.0, 100.0));
        assert!((p.x - 25.0).abs() < 1e-9);
        assert!((p.y - 0.0).abs() < 1e-9);
    }
}
