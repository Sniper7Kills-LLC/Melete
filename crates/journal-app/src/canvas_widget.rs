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
