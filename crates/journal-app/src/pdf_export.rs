use std::collections::HashSet;
use std::path::Path;

use gtk4::cairo;
use gtk4::cairo::PdfSurface;
use journal_canvas::{paint, ViewportTransform};
use journal_core::{Viewport};
use journal_storage::StrokeStore;

use crate::state::SharedState;

const A4_W_PT: f64 = 595.28;
const A4_H_PT: f64 = 841.89;

pub fn export_page_to_pdf(state: &SharedState, path: &Path) -> anyhow::Result<()> {
    let (page_id, background, page_rect, backend, dark_mode) = {
        let s = state.borrow();
        let page_id = s.current_page_id.ok_or_else(|| anyhow::anyhow!("no page selected"))?;
        (page_id, s.background.clone(), s.page_rect, s.backend.clone(), s.dark_mode)
    };

    let strokes = backend.borrow_mut().list_strokes_for_page(page_id)
        .map_err(|e| anyhow::anyhow!("failed to load strokes: {}", e))?;

    let path_str = path.to_string_lossy();
    let surface = PdfSurface::new(A4_W_PT, A4_H_PT, path_str.as_ref())
        .map_err(|e| anyhow::anyhow!("failed to create PDF surface: {:?}", e))?;

    let ctx = cairo::Context::new(&surface)
        .map_err(|e| anyhow::anyhow!("failed to create Cairo context: {:?}", e))?;

    let zoom_x = A4_W_PT / page_rect.width;
    let zoom_y = A4_H_PT / page_rect.height;
    let zoom = zoom_x.min(zoom_y);

    let viewport = Viewport {
        center: journal_core::Point {
            x: page_rect.x + page_rect.width * 0.5,
            y: page_rect.y + page_rect.height * 0.5,
        },
        zoom,
        rotation: 0.0,
    };
    let transform = ViewportTransform::new(viewport, A4_W_PT, A4_H_PT);
    let empty_selected = HashSet::new();

    paint(&ctx, &transform, &background, page_rect, &strokes, &empty_selected, dark_mode);

    surface.finish();
    Ok(())
}
