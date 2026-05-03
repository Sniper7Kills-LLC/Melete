use gtk4::prelude::*;
use gtk4::DrawingArea;
use journal_canvas::paint;

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
                draw_placeholder(ctx, w as f64, h as f64);
                return;
            }

            if let Some(cs) = s.current_stroke.clone() {
                let mut frame: Vec<journal_core::Stroke> =
                    Vec::with_capacity(s.strokes.len() + 1);
                frame.extend_from_slice(&s.strokes);
                frame.push(cs);
                paint(ctx, &s.transform, &s.background, s.page_rect, &frame);
            } else {
                paint(ctx, &s.transform, &s.background, s.page_rect, &s.strokes);
            }
        });
    }

    area
}

fn draw_placeholder(ctx: &gtk4::cairo::Context, w: f64, h: f64) {
    ctx.set_source_rgb(0.97, 0.97, 0.98);
    let _ = ctx.paint();

    ctx.set_source_rgba(0.3, 0.3, 0.35, 0.6);
    ctx.set_font_size(20.0);
    let text = "Select a page to start drawing";
    let extents = match ctx.text_extents(text) {
        Ok(e) => e,
        Err(_) => return,
    };
    let x = (w - extents.width()) * 0.5 - extents.x_bearing();
    let y = (h - extents.height()) * 0.5 - extents.y_bearing();
    ctx.move_to(x, y);
    let _ = ctx.show_text(text);
}
