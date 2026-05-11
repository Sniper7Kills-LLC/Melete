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
