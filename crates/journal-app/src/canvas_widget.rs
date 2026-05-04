use gtk4::prelude::*;
use gtk4::DrawingArea;
use journal_canvas::{
    draw_lasso_overlay, draw_page_bounds_outline, draw_selection_handles,
    paint_with_widgets_ctx, scale_background, selection_combined_bbox,
    WidgetRenderContext,
};

use crate::state::{tool_brush_params, tool_is_drawing, SharedState, Tool};

pub fn build_canvas(state: SharedState) -> DrawingArea {
    let area = DrawingArea::builder()
        .hexpand(true)
        .vexpand(true)
        .build();
    // Hide the system pointer cursor over the canvas so the custom
    // brush-circle cursor we draw in `set_draw_func` is the only indicator.
    area.set_cursor_from_name(Some("none"));

    {
        let state = state.clone();
        area.set_draw_func(move |_area, ctx, w, h| {
            let mut s = state.borrow_mut();
            s.transform.set_size(w as f64, h as f64);

            // Vello (GLArea below) owns all canvas rendering when active —
            // bg, widgets, strokes, selection handles, lasso, page bounds,
            // and the brush cursor. The DrawingArea is left transparent so
            // input still routes through it.
            #[cfg(feature = "vello")]
            if crate::vello_glarea::enabled() {
                if s.current_page_id.is_none() {
                    draw_placeholder(
                        ctx,
                        w as f64,
                        h as f64,
                        s.dark_mode,
                        s.placeholder_image.as_ref(),
                        &s.placeholder_text,
                    );
                }
                return;
            }

            if s.current_page_id.is_none() {
                draw_placeholder(
                    ctx,
                    w as f64,
                    h as f64,
                    s.dark_mode,
                    s.placeholder_image.as_ref(),
                    &s.placeholder_text,
                );
                return;
            }

            let dark_mode = s.dark_mode;
            let selected_ids = s.selected_stroke_ids.clone();
            let lasso_points = s.lasso_points.clone();
            let widgets: Vec<journal_core::TemplateWidget> = s
                .current_template
                .as_ref()
                .map(|t| t.widgets.clone())
                .unwrap_or_default();

            let show_bounds = s.show_page_bounds;
            let page_rect = s.page_rect;
            let background = scale_background(&s.background, s.bg_scale);
            let transform = s.transform;
            let render_ctx = WidgetRenderContext {
                date: s.current_page_date,
                overrides: s.current_page_overrides.clone(),
            };

            // When the Vello GLArea overlay is active it renders the page
            // fill, background pattern, and strokes — Cairo here paints
            // widgets only, leaving everything else transparent so the
            // GLArea below shows through. With Vello off, fall back to the
            // full Cairo paint path.
            #[cfg(feature = "vello")]
            let vello_owns_canvas = crate::vello_glarea::enabled();
            #[cfg(not(feature = "vello"))]
            let vello_owns_canvas = false;

            if vello_owns_canvas {
                // Vello (GLArea below) now renders bg + widgets + strokes.
                // Cairo here paints overlays only (selection handles, lasso,
                // brush cursor, page bounds) — handled below the if/else.
            } else if let Some(cs) = s.current_stroke.clone() {
                let mut frame: Vec<journal_core::Stroke> =
                    Vec::with_capacity(s.strokes.len() + 1);
                frame.extend_from_slice(&s.strokes);
                frame.push(cs);
                paint_with_widgets_ctx(ctx, &s.transform, &background, page_rect, &widgets, &frame, &selected_ids, dark_mode, &render_ctx);
            } else {
                paint_with_widgets_ctx(ctx, &s.transform, &background, page_rect, &widgets, &s.strokes, &selected_ids, dark_mode, &render_ctx);
            }

            if show_bounds {
                draw_page_bounds_outline(ctx, &transform, &background, page_rect, dark_mode);
            }

            if !selected_ids.is_empty() {
                if let Some(sel_bbox) = selection_combined_bbox(&s.strokes, &selected_ids) {
                    ctx.identity_matrix();
                    draw_selection_handles(ctx, &s.transform, sel_bbox);
                }
            }

            if !lasso_points.is_empty() {
                ctx.identity_matrix();
                draw_lasso_overlay(ctx, &lasso_points);
            }

            if let Some((px, py)) = s.pointer_screen {
                ctx.identity_matrix();
                draw_brush_cursor(
                    ctx,
                    px,
                    py,
                    s.tool,
                    s.pen,
                    s.pointer_drawing,
                    dark_mode,
                );
            }
        });
    }

    area
}

/// Draw the floating brush cursor at the pointer position. Outlined when
/// just hovering, filled with the pen color while a stroke is in progress.
/// Radius matches the tool's effective on-screen brush diameter so the user
/// sees the actual painted area.
fn draw_brush_cursor(
    ctx: &gtk4::cairo::Context,
    px: f64,
    py: f64,
    tool: Tool,
    pen: journal_core::PenSettings,
    drawing: bool,
    dark_mode: bool,
) {
    // pen.base_width is in screen px at the zoom the stroke is created at —
    // tool_brush_params layers a multiplier per tool. Eraser/Selection get
    // a constant marker so the user still sees a hit indicator.
    let radius = if tool_is_drawing(tool) {
        let (_, mult, _, _) = tool_brush_params(tool);
        (pen.base_width * mult * 0.5).max(2.0)
    } else {
        match tool {
            Tool::Eraser(crate::state::EraserMode::Stroke) => 6.0,
            Tool::Eraser(crate::state::EraserMode::Partial) => 11.0,
            _ => 5.0,
        }
    };

    ctx.save().ok();
    ctx.set_operator(gtk4::cairo::Operator::Over);

    if drawing && tool_is_drawing(tool) {
        let c = pen.color;
        ctx.set_source_rgba(
            c.r as f64 / 255.0,
            c.g as f64 / 255.0,
            c.b as f64 / 255.0,
            (pen.opacity as f64).clamp(0.2, 1.0),
        );
        ctx.arc(px, py, radius, 0.0, std::f64::consts::TAU);
        let _ = ctx.fill();
    }

    // Outline ring — always drawn so the cursor remains visible against the
    // fill (and is the only mark shown when hovering / for non-drawing tools).
    if dark_mode {
        ctx.set_source_rgba(1.0, 1.0, 1.0, 0.85);
    } else {
        ctx.set_source_rgba(0.0, 0.0, 0.0, 0.75);
    }
    ctx.set_line_width(1.25);
    ctx.arc(px, py, radius, 0.0, std::f64::consts::TAU);
    let _ = ctx.stroke();

    // Contrast halo so the ring stays visible on similar-color backgrounds.
    if dark_mode {
        ctx.set_source_rgba(0.0, 0.0, 0.0, 0.4);
    } else {
        ctx.set_source_rgba(1.0, 1.0, 1.0, 0.6);
    }
    ctx.set_line_width(0.5);
    ctx.arc(px, py, radius + 0.9, 0.0, std::f64::consts::TAU);
    let _ = ctx.stroke();

    ctx.restore().ok();
}

fn draw_placeholder(
    ctx: &gtk4::cairo::Context,
    w: f64,
    h: f64,
    dark_mode: bool,
    image: Option<&gtk4::cairo::ImageSurface>,
    text: &str,
) {
    if dark_mode {
        ctx.set_source_rgb(0.13, 0.13, 0.15);
    } else {
        ctx.set_source_rgb(0.97, 0.97, 0.98);
    }
    let _ = ctx.paint();

    if let Some(surface) = image {
        let iw = surface.width() as f64;
        let ih = surface.height() as f64;
        if iw > 0.0 && ih > 0.0 {
            let max_w = w * 0.6;
            let max_h = h * 0.6;
            let scale = (max_w / iw).min(max_h / ih).min(1.0);
            let dst_w = iw * scale;
            let dst_h = ih * scale;
            let x = (w - dst_w) * 0.5;
            let y = (h - dst_h) * 0.5;
            ctx.save().ok();
            ctx.translate(x, y);
            ctx.scale(scale, scale);
            let _ = ctx.set_source_surface(surface, 0.0, 0.0);
            let _ = ctx.paint();
            ctx.restore().ok();
            return;
        }
    }

    if dark_mode {
        ctx.set_source_rgba(0.7, 0.7, 0.75, 0.6);
    } else {
        ctx.set_source_rgba(0.3, 0.3, 0.35, 0.6);
    }
    ctx.set_font_size(20.0);
    let extents = match ctx.text_extents(text) {
        Ok(e) => e,
        Err(_) => return,
    };
    let x = (w - extents.width()) * 0.5 - extents.x_bearing();
    let y = (h - extents.height()) * 0.5 - extents.y_bearing();
    ctx.move_to(x, y);
    let _ = ctx.show_text(text);
}
