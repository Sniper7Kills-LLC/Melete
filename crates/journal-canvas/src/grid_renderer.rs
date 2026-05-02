use gtk4::cairo;
use journal_core::Color;

use crate::viewport_transform::ViewportTransform;

#[derive(Debug, Clone, Copy)]
pub struct GridSettings {
    pub base_spacing: f64,
    pub subdivisions: u32,
    pub color: Color,
}

impl Default for GridSettings {
    fn default() -> Self {
        Self {
            base_spacing: 100.0,
            subdivisions: 5,
            color: Color { r: 80, g: 80, b: 90, a: 255 },
        }
    }
}

pub fn draw_grid(ctx: &cairo::Context, transform: &ViewportTransform, settings: &GridSettings) {
    if settings.base_spacing <= 0.0 || settings.subdivisions == 0 {
        return;
    }

    let visible = transform.visible_canvas_rect();
    let zoom = transform.zoom().max(1e-6);
    let sub = settings.subdivisions.max(2) as f64;

    let level_zoom = zoom.log(sub).max(0.0);
    let max_level = level_zoom.ceil() as i32;
    let min_level: i32 = -3;

    let base_alpha = settings.color.a as f64 / 255.0;
    let r = settings.color.r as f64 / 255.0;
    let g = settings.color.g as f64 / 255.0;
    let b = settings.color.b as f64 / 255.0;

    for level in min_level..=max_level {
        let spacing = settings.base_spacing * sub.powi(-level);
        if spacing <= 0.0 {
            continue;
        }
        let on_screen_spacing_px = spacing * zoom;
        if on_screen_spacing_px < 4.0 {
            continue;
        }

        let distance_from_target = (level as f64 - level_zoom).abs();
        let alpha_factor = (1.0 - (distance_from_target / 1.5)).clamp(0.0, 1.0);
        let alpha = base_alpha * alpha_factor;
        if alpha <= 0.001 {
            continue;
        }

        let thickness_px = if level <= 0 { 1.5 } else { 1.0 };
        let line_width = thickness_px / zoom;

        ctx.set_source_rgba(r, g, b, alpha);
        ctx.set_line_width(line_width);

        let start_x = (visible.x / spacing).floor() * spacing;
        let end_x = visible.x + visible.width;
        let start_y = (visible.y / spacing).floor() * spacing;
        let end_y = visible.y + visible.height;

        let mut x = start_x;
        while x <= end_x {
            ctx.move_to(x, visible.y);
            ctx.line_to(x, visible.y + visible.height);
            x += spacing;
        }

        let mut y = start_y;
        while y <= end_y {
            ctx.move_to(visible.x, y);
            ctx.line_to(visible.x + visible.width, y);
            y += spacing;
        }

        let _ = ctx.stroke();
    }
}
