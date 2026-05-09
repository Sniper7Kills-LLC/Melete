use journal_core::Color;

#[cfg(feature = "desktop")]
use gtk4::cairo;
#[cfg(feature = "desktop")]
use crate::viewport_transform::ViewportTransform;

#[derive(Debug, Clone, Copy)]
pub struct GridSettings {
    pub base_spacing: f64,
    /// Visual subdivision ratio: every Nth line is drawn slightly thicker
    /// (a major grid line). `0` or `1` disables major-line emphasis.
    pub subdivisions: u32,
    pub color: Color,
}

impl Default for GridSettings {
    fn default() -> Self {
        Self {
            base_spacing: 20.0,
            subdivisions: 4,
            color: Color {
                r: 80,
                g: 80,
                b: 90,
                a: 255,
            },
        }
    }
}

/// Draw a square grid at fixed canvas spacing — no LOD subdivision. Major
/// lines (every `subdivisions`-th) are drawn thicker so the eye can still
/// pick out coarse structure without the grid auto-shifting on zoom.
#[cfg(feature = "desktop")]
pub fn draw_grid(ctx: &cairo::Context, transform: &ViewportTransform, settings: &GridSettings) {
    if settings.base_spacing <= 0.0 {
        return;
    }
    let visible = transform.visible_canvas_rect();
    let zoom = transform.zoom().max(1e-6);
    let spacing = settings.base_spacing;
    let sub = settings.subdivisions.max(1);

    let alpha = settings.color.a as f64 / 255.0;
    let r = settings.color.r as f64 / 255.0;
    let g = settings.color.g as f64 / 255.0;
    let b = settings.color.b as f64 / 255.0;

    let start_x = (visible.x / spacing).floor() * spacing;
    let end_x = visible.x + visible.width;
    let start_y = (visible.y / spacing).floor() * spacing;
    let end_y = visible.y + visible.height;

    let start_x_index = (start_x / spacing).round() as i64;
    let start_y_index = (start_y / spacing).round() as i64;

    ctx.set_source_rgba(r, g, b, alpha);

    // Minor lines first.
    ctx.set_line_width(1.0 / zoom);
    let mut x = start_x;
    let mut i = start_x_index;
    while x <= end_x {
        if sub <= 1 || i.rem_euclid(sub as i64) != 0 {
            ctx.move_to(x, visible.y);
            ctx.line_to(x, visible.y + visible.height);
        }
        x += spacing;
        i += 1;
    }
    let mut y = start_y;
    let mut j = start_y_index;
    while y <= end_y {
        if sub <= 1 || j.rem_euclid(sub as i64) != 0 {
            ctx.move_to(visible.x, y);
            ctx.line_to(visible.x + visible.width, y);
        }
        y += spacing;
        j += 1;
    }
    let _ = ctx.stroke();

    if sub <= 1 {
        return;
    }

    // Major lines (thicker).
    ctx.set_line_width(2.0 / zoom);
    let mut x = start_x;
    let mut i = start_x_index;
    while x <= end_x {
        if i.rem_euclid(sub as i64) == 0 {
            ctx.move_to(x, visible.y);
            ctx.line_to(x, visible.y + visible.height);
        }
        x += spacing;
        i += 1;
    }
    let mut y = start_y;
    let mut j = start_y_index;
    while y <= end_y {
        if j.rem_euclid(sub as i64) == 0 {
            ctx.move_to(visible.x, y);
            ctx.line_to(visible.x + visible.width, y);
        }
        y += spacing;
        j += 1;
    }
    let _ = ctx.stroke();
}
