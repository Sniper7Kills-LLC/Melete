use gtk4::cairo;
use journal_core::{Rect, Stroke};

use crate::background_renderer::{draw_background, BackgroundConfig};
use crate::stroke_renderer::draw_stroke;
use crate::viewport_transform::ViewportTransform;

/// Paint a frame to the supplied Cairo context. The context is left in canvas
/// space (transform applied) on return; callers that share the context with
/// other widgets should `save`/`restore` around this call themselves.
pub fn paint(
    ctx: &cairo::Context,
    transform: &ViewportTransform,
    background: &BackgroundConfig,
    page_rect: Rect,
    strokes: &[Stroke],
) {
    let (sw, sh) = transform.screen_size();
    let center = transform.center();
    let zoom = transform.zoom().max(1e-6);

    ctx.set_source_rgb(1.0, 1.0, 1.0);
    let _ = ctx.paint();

    ctx.translate(sw * 0.5, sh * 0.5);
    ctx.scale(zoom, zoom);
    ctx.translate(-center.x, -center.y);

    draw_background(ctx, transform, background, page_rect);
    for stroke in strokes {
        draw_stroke(ctx, transform, stroke);
    }
}
