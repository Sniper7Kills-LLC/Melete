use std::collections::HashSet;

use gtk4::cairo;
use journal_core::{Rect, Stroke, TemplateWidget};
use uuid::Uuid;

use crate::background_renderer::{draw_background, BackgroundConfig};
use crate::stroke_renderer::draw_stroke;
use crate::viewport_transform::ViewportTransform;
use crate::widget_renderer::draw_widgets;

/// Paint a frame to the supplied Cairo context. The context is left in canvas
/// space (transform applied) on return; callers that share the context with
/// other widgets should `save`/`restore` around this call themselves.
pub fn paint(
    ctx: &cairo::Context,
    transform: &ViewportTransform,
    background: &BackgroundConfig,
    page_rect: Rect,
    strokes: &[Stroke],
    selected_ids: &HashSet<Uuid>,
    dark_mode: bool,
) {
    paint_with_widgets(ctx, transform, background, page_rect, &[], strokes, selected_ids, dark_mode);
}

/// Paint a frame including template widgets between background and strokes.
pub fn paint_with_widgets(
    ctx: &cairo::Context,
    transform: &ViewportTransform,
    background: &BackgroundConfig,
    page_rect: Rect,
    widgets: &[TemplateWidget],
    strokes: &[Stroke],
    selected_ids: &HashSet<Uuid>,
    dark_mode: bool,
) {
    let (sw, sh) = transform.screen_size();
    let center = transform.center();
    let zoom = transform.zoom().max(1e-6);

    if dark_mode {
        ctx.set_source_rgb(0.15, 0.15, 0.18);
    } else {
        ctx.set_source_rgb(1.0, 1.0, 1.0);
    }
    let _ = ctx.paint();

    ctx.translate(sw * 0.5, sh * 0.5);
    ctx.scale(zoom, zoom);
    ctx.translate(-center.x, -center.y);

    draw_background(ctx, transform, background, page_rect);
    if !widgets.is_empty() {
        draw_widgets(ctx, transform, widgets, page_rect);
    }
    for stroke in strokes {
        draw_stroke(ctx, transform, stroke);
        if selected_ids.contains(&stroke.id) {
            draw_selection_highlight(ctx, transform, stroke);
        }
    }
}

fn draw_selection_highlight(ctx: &cairo::Context, transform: &ViewportTransform, stroke: &Stroke) {
    let zoc = stroke.zoom_at_creation.max(1e-6);
    let base_w = stroke.pen.base_width / zoc;

    ctx.save().ok();
    ctx.set_source_rgba(0.2, 0.5, 1.0, 0.45);
    ctx.set_line_cap(cairo::LineCap::Round);
    ctx.set_line_join(cairo::LineJoin::Round);

    let visible = transform.visible_canvas_rect();
    let a_x = visible.x;
    let a_y = visible.y;
    let b_x = a_x + visible.width;
    let b_y = a_y + visible.height;
    let _ = (a_x, a_y, b_x, b_y);

    if stroke.points.len() == 1 {
        let p = &stroke.points[0];
        let r = base_w * (p.pressure.max(0.05) as f64) * 0.5 + 2.0 / transform.zoom().max(1e-6);
        ctx.arc(p.x, p.y, r, 0.0, std::f64::consts::TAU);
        let _ = ctx.fill();
    } else {
        for window in stroke.points.windows(2) {
            let a = &window[0];
            let b = &window[1];
            let avg_pressure = ((a.pressure + b.pressure) * 0.5).max(0.05) as f64;
            let w = base_w * avg_pressure + 4.0 / transform.zoom().max(1e-6);
            ctx.set_line_width(w);
            ctx.move_to(a.x, a.y);
            ctx.line_to(b.x, b.y);
            let _ = ctx.stroke();
        }
    }
    ctx.restore().ok();
}

/// Draw a lasso polygon overlay in screen coordinates.
/// Called after the main canvas transform is restored, so uses raw screen coords.
pub fn draw_lasso_overlay(ctx: &cairo::Context, points: &[(f64, f64)]) {
    if points.len() < 2 {
        return;
    }
    ctx.save().ok();
    ctx.new_path();
    ctx.move_to(points[0].0, points[0].1);
    for &(x, y) in &points[1..] {
        ctx.line_to(x, y);
    }
    ctx.close_path();
    ctx.set_source_rgba(0.2, 0.5, 1.0, 0.15);
    let _ = ctx.fill_preserve();
    ctx.set_source_rgba(0.2, 0.5, 1.0, 0.6);
    ctx.set_line_width(1.5);
    let _ = ctx.stroke();
    ctx.restore().ok();
}
