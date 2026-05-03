use gtk4::prelude::*;
use gtk4::DrawingArea;
use journal_canvas::{
    draw_lasso_overlay, draw_page_bounds_outline, draw_selection_handles,
    paint_with_widgets_ctx, scale_background, selection_combined_bbox, WidgetRenderContext,
};

use crate::state::SharedState;

pub fn build_canvas(state: SharedState) -> DrawingArea {
    let area = DrawingArea::builder()
        .hexpand(true)
        .vexpand(true)
        .build();

    {
        let state = state.clone();
        area.set_draw_func(move |_area, ctx, w, h| {
            let mut s = state.borrow_mut();
            s.transform.set_size(w as f64, h as f64);

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

            if let Some(cs) = s.current_stroke.clone() {
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
        });
    }

    area
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
