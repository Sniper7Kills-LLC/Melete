use std::collections::HashSet;

use gtk4::cairo;
use journal_canvas::{paint_with_widgets, BackgroundConfig, ViewportTransform};
use journal_core::{PageId, PageTemplate, Rect, Stroke, TemplateWidget, Viewport};
// StrokeStore methods reached via dyn JournalBackend.

use crate::state::SharedState;

pub const THUMB_W: i32 = 36;
pub const THUMB_H: i32 = 48;

pub fn render_thumbnail(
    background: &BackgroundConfig,
    page_rect: Rect,
    widgets: &[TemplateWidget],
    strokes: &[Stroke],
    dark_mode: bool,
) -> Option<cairo::ImageSurface> {
    let surface = cairo::ImageSurface::create(cairo::Format::ARgb32, THUMB_W, THUMB_H).ok()?;
    let ctx = cairo::Context::new(&surface).ok()?;

    let margin = 0.95;
    let zoom_x = THUMB_W as f64 / page_rect.width;
    let zoom_y = THUMB_H as f64 / page_rect.height;
    let zoom = zoom_x.min(zoom_y) * margin;

    let viewport = Viewport {
        center: journal_core::Point {
            x: page_rect.x + page_rect.width * 0.5,
            y: page_rect.y + page_rect.height * 0.5,
        },
        zoom,
        rotation: 0.0,
    };
    let transform = ViewportTransform::new(viewport, THUMB_W as f64, THUMB_H as f64);
    let empty_selected = HashSet::new();
    paint_with_widgets(&ctx, &transform, background, page_rect, widgets, strokes, &empty_selected, dark_mode);

    Some(surface)
}

pub fn get_or_generate_thumbnail(
    state: &SharedState,
    page_id: PageId,
    template: Option<&PageTemplate>,
    dark_mode: bool,
) -> Option<cairo::ImageSurface> {
    {
        let s = state.borrow();
        if s.thumbnail_cache.contains_key(&page_id) {
            return None;
        }
    }

    let (background, page_rect, widgets) = if let Some(t) = template {
        let bg = journal_templates::page_template_to_background_config(t);
        let rect = Rect { x: 0.0, y: 0.0, width: t.size_mm.0, height: t.size_mm.1 };
        (bg, rect, t.widgets.clone())
    } else {
        (crate::state::default_background(), crate::state::default_page_rect(), Vec::new())
    };

    let backend = state.borrow().backend.clone();
    let strokes = match backend.borrow_mut().list_strokes_for_page(page_id) {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!("thumbnail: {:?}: {}", page_id, e);
            return None;
        }
    };

    render_thumbnail(&background, page_rect, &widgets, &strokes, dark_mode)
}
