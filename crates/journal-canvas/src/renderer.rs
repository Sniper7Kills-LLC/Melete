use std::collections::HashSet;

use gtk4::cairo;
use journal_core::{Point, Rect, Stroke, TemplateWidget};
use uuid::Uuid;

use crate::background_renderer::{draw_background, BackgroundConfig};
use crate::stroke_renderer::draw_stroke;
use crate::viewport_transform::ViewportTransform;
use crate::widget_renderer::draw_widgets;

const HANDLE_SIZE: f64 = 8.0;

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

/// Compute the combined bounding box of all selected strokes (canvas space).
pub fn selection_combined_bbox(strokes: &[Stroke], selected_ids: &HashSet<Uuid>) -> Option<Rect> {
    let selected: Vec<&Stroke> = strokes.iter().filter(|s| selected_ids.contains(&s.id)).collect();
    if selected.is_empty() {
        return None;
    }
    let mut min_x = f64::MAX;
    let mut min_y = f64::MAX;
    let mut max_x = f64::MIN;
    let mut max_y = f64::MIN;
    for s in &selected {
        let bb = s.bounding_box;
        min_x = min_x.min(bb.x);
        min_y = min_y.min(bb.y);
        max_x = max_x.max(bb.x + bb.width);
        max_y = max_y.max(bb.y + bb.height);
    }
    Some(Rect { x: min_x, y: min_y, width: max_x - min_x, height: max_y - min_y })
}

fn handle_anchors(transform: &ViewportTransform, selection_bbox: Rect) -> [(f64, f64); 8] {
    let to_screen = |cx: f64, cy: f64| -> (f64, f64) {
        transform.canvas_to_screen(Point { x: cx, y: cy })
    };
    let bb = selection_bbox;
    let mx = bb.x + bb.width * 0.5;
    let my = bb.y + bb.height * 0.5;
    [
        to_screen(bb.x, bb.y),
        to_screen(mx, bb.y),
        to_screen(bb.x + bb.width, bb.y),
        to_screen(bb.x + bb.width, my),
        to_screen(bb.x + bb.width, bb.y + bb.height),
        to_screen(mx, bb.y + bb.height),
        to_screen(bb.x, bb.y + bb.height),
        to_screen(bb.x, my),
    ]
}

/// Draw the 8 resize handles around the selection bounding box in screen coords.
pub fn draw_selection_handles(
    ctx: &cairo::Context,
    transform: &ViewportTransform,
    selection_bbox: Rect,
) {
    let anchors = handle_anchors(transform, selection_bbox);
    let h = HANDLE_SIZE * 0.5;
    ctx.save().ok();
    for (sx, sy) in &anchors {
        ctx.rectangle(sx - h, sy - h, HANDLE_SIZE, HANDLE_SIZE);
    }
    ctx.set_source_rgba(1.0, 1.0, 1.0, 0.9);
    let _ = ctx.fill_preserve();
    ctx.set_source_rgba(0.2, 0.5, 1.0, 1.0);
    ctx.set_line_width(1.5);
    let _ = ctx.stroke();
    ctx.restore().ok();
}

/// Hit-test screen point against the 8 handle squares.
/// Returns the index (0=TL,1=T,2=TR,3=R,4=BR,5=B,6=BL,7=L) or None.
pub fn hit_test_handle(
    transform: &ViewportTransform,
    selection_bbox: Rect,
    sx: f64,
    sy: f64,
) -> Option<usize> {
    let anchors = handle_anchors(transform, selection_bbox);
    let h = HANDLE_SIZE;
    for (i, (hx, hy)) in anchors.iter().enumerate() {
        if (sx - hx).abs() <= h && (sy - hy).abs() <= h {
            return Some(i);
        }
    }
    None
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
