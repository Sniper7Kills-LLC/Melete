use gtk4::cairo;
use journal_core::{BlendMode, Rect, Stroke};

use crate::viewport_transform::ViewportTransform;

fn rects_intersect(a: &Rect, b: &Rect) -> bool {
    a.x < b.x + b.width
        && b.x < a.x + a.width
        && a.y < b.y + b.height
        && b.y < a.y + a.height
}

fn set_color(ctx: &cairo::Context, c: journal_core::Color, opacity: f32) {
    let alpha = (c.a as f64 / 255.0) * opacity.clamp(0.0, 1.0) as f64;
    ctx.set_source_rgba(
        c.r as f64 / 255.0,
        c.g as f64 / 255.0,
        c.b as f64 / 255.0,
        alpha,
    );
}

fn blend_to_operator(mode: BlendMode) -> cairo::Operator {
    match mode {
        BlendMode::Normal => cairo::Operator::Over,
        BlendMode::Multiply => cairo::Operator::Multiply,
        BlendMode::Screen => cairo::Operator::Screen,
        BlendMode::Overlay => cairo::Operator::Overlay,
        BlendMode::Darken => cairo::Operator::Darken,
        BlendMode::Lighten => cairo::Operator::Lighten,
        BlendMode::Erase => cairo::Operator::DestOut,
    }
}

/// Draw a single stroke. Cairo context must already have viewport transform
/// applied (canvas-space drawing).
pub fn draw_stroke(ctx: &cairo::Context, transform: &ViewportTransform, stroke: &Stroke) -> bool {
    let visible = transform.visible_canvas_rect();
    if !rects_intersect(&visible, &stroke.bounding_box) {
        return false;
    }
    if stroke.points.is_empty() {
        return false;
    }

    let pen = stroke.pen;
    let zoc = stroke.zoom_at_creation.max(1e-6);
    let canvas_width_at_full_pressure = pen.base_width / zoc;

    ctx.save().ok();
    ctx.set_operator(blend_to_operator(pen.blend_mode));
    set_color(ctx, pen.color, pen.opacity);
    ctx.set_line_cap(cairo::LineCap::Round);
    ctx.set_line_join(cairo::LineJoin::Round);

    if stroke.points.len() == 1 {
        let p = &stroke.points[0];
        let r = canvas_width_at_full_pressure * (p.pressure.max(0.05) as f64) * 0.5;
        ctx.arc(p.x, p.y, r, 0.0, std::f64::consts::TAU);
        let _ = ctx.fill();
        ctx.restore().ok();
        return true;
    }

    for window in stroke.points.windows(2) {
        let a = &window[0];
        let b = &window[1];
        let avg_pressure = ((a.pressure + b.pressure) * 0.5).max(0.05) as f64;
        let w = canvas_width_at_full_pressure * avg_pressure;
        ctx.set_line_width(w);
        ctx.move_to(a.x, a.y);
        ctx.line_to(b.x, b.y);
        let _ = ctx.stroke();
    }

    ctx.restore().ok();
    true
}
