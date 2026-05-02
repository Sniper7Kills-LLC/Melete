use gtk4::cairo;
use journal_core::{Color, Rect};

use crate::grid_renderer::{draw_grid, GridSettings};
use crate::viewport_transform::ViewportTransform;

#[derive(Debug, Clone, Copy)]
pub enum BackgroundConfig {
    Blank,
    Dots { spacing: f64 },
    Lines { spacing: f64 },
    Grid(GridSettings),
}

fn pattern_color() -> Color {
    Color { r: 90, g: 90, b: 100, a: 200 }
}

pub fn draw_background(
    ctx: &cairo::Context,
    transform: &ViewportTransform,
    config: &BackgroundConfig,
    page_rect: Rect,
) {
    match config {
        BackgroundConfig::Blank => {}
        BackgroundConfig::Grid(settings) => {
            draw_grid(ctx, transform, settings);
        }
        BackgroundConfig::Dots { spacing } => {
            draw_dots(ctx, transform, page_rect, *spacing);
        }
        BackgroundConfig::Lines { spacing } => {
            draw_lines(ctx, transform, page_rect, *spacing);
        }
    }
}

fn set_pattern_source(ctx: &cairo::Context) {
    let c = pattern_color();
    ctx.set_source_rgba(
        c.r as f64 / 255.0,
        c.g as f64 / 255.0,
        c.b as f64 / 255.0,
        c.a as f64 / 255.0,
    );
}

fn draw_dots(ctx: &cairo::Context, transform: &ViewportTransform, page_rect: Rect, spacing: f64) {
    if spacing <= 0.0 {
        return;
    }
    let zoom = transform.zoom().max(1e-6);
    let radius_canvas = (1.5 / zoom).clamp(0.5, 3.0 / zoom.max(0.1));

    ctx.save().ok();
    ctx.rectangle(page_rect.x, page_rect.y, page_rect.width, page_rect.height);
    ctx.clip();

    set_pattern_source(ctx);

    let mut y = page_rect.y;
    while y <= page_rect.y + page_rect.height {
        let mut x = page_rect.x;
        while x <= page_rect.x + page_rect.width {
            ctx.arc(x, y, radius_canvas, 0.0, std::f64::consts::TAU);
            let _ = ctx.fill();
            x += spacing;
        }
        y += spacing;
    }

    ctx.restore().ok();
}

fn draw_lines(ctx: &cairo::Context, transform: &ViewportTransform, page_rect: Rect, spacing: f64) {
    if spacing <= 0.0 {
        return;
    }
    let zoom = transform.zoom().max(1e-6);

    ctx.save().ok();
    ctx.rectangle(page_rect.x, page_rect.y, page_rect.width, page_rect.height);
    ctx.clip();

    set_pattern_source(ctx);
    ctx.set_line_width(1.0 / zoom);

    let mut y = page_rect.y;
    while y <= page_rect.y + page_rect.height {
        ctx.move_to(page_rect.x, y);
        ctx.line_to(page_rect.x + page_rect.width, y);
        y += spacing;
    }
    let _ = ctx.stroke();

    ctx.restore().ok();
}
