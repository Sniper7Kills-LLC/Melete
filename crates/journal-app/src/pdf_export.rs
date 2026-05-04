use std::cell::RefCell;
use std::collections::HashSet;
use std::path::Path;
use std::rc::Rc;

use gtk4::cairo;
use gtk4::cairo::PdfSurface;
use journal_canvas::{paint, paint_with_widgets_ctx, ViewportTransform, WidgetRenderContext};
use journal_core::{NotebookId, Rect, SectionId, Viewport};
use journal_storage::JournalBackend;

use crate::state::SharedState;

const A4_W_PT: f64 = 595.28;
const A4_H_PT: f64 = 841.89;

pub fn export_page_to_pdf(state: &SharedState, path: &Path) -> anyhow::Result<()> {
    let (page_id, background, page_rect, backend) = {
        let s = state.borrow();
        let page_id = s
            .current_page_id
            .ok_or_else(|| anyhow::anyhow!("no page selected"))?;
        (
            page_id,
            s.background.clone(),
            s.page_rect,
            s.backend.clone(),
        )
    };
    let dark_mode = crate::is_dark_mode();

    let strokes = backend
        .borrow_mut()
        .list_strokes_for_page(page_id)
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

    paint(
        &ctx,
        &transform,
        &background,
        page_rect,
        &strokes,
        &empty_selected,
        dark_mode,
    );

    surface.finish();
    Ok(())
}

/// Export every page in `notebook_id` to a multi-page A4 PDF at `path`.
///
/// Pages are visited depth-first in display order: root sections sorted by
/// `position`, each section's pages sorted by `position`, then child sections
/// in the same order. Each page gets its own PDF page sized to A4 with a small
/// margin (fit_zoom = min(A4_W/pw, A4_H/ph) × 0.95). Template widgets are
/// rendered so planner pages look correct.
pub fn export_notebook_to_pdf(
    state: &SharedState,
    notebook_id: NotebookId,
    path: &Path,
) -> anyhow::Result<()> {
    let (backend, templates) = {
        let s = state.borrow();
        (s.backend.clone(), s.templates.clone())
    };
    let dark_mode = crate::is_dark_mode();

    let path_str = path.to_string_lossy();
    let surface = PdfSurface::new(A4_W_PT, A4_H_PT, path_str.as_ref())
        .map_err(|e| anyhow::anyhow!("failed to create PDF surface: {:?}", e))?;
    let ctx = cairo::Context::new(&surface)
        .map_err(|e| anyhow::anyhow!("failed to create Cairo context: {:?}", e))?;

    let empty_selected = HashSet::new();

    let mut ordered_page_ids: Vec<journal_core::PageId> = Vec::new();

    // Helper: depth-first collect of all page ids within a section tree.
    fn collect_pages_for_section(
        section_id: SectionId,
        backend: &Rc<RefCell<dyn JournalBackend>>,
        out: &mut Vec<journal_core::PageId>,
    ) -> anyhow::Result<()> {
        // Pages in this section, sorted by position.
        let mut pages = backend
            .borrow_mut()
            .list_pages(section_id)
            .map_err(|e| anyhow::anyhow!("list_pages failed: {}", e))?;
        pages.sort_by_key(|p| p.position);
        for p in pages {
            out.push(p.id);
        }
        // Child sections, sorted by position.
        let mut children = backend
            .borrow_mut()
            .list_child_sections(section_id)
            .map_err(|e| anyhow::anyhow!("list_child_sections failed: {}", e))?;
        children.sort_by_key(|s| s.position);
        for child in children {
            collect_pages_for_section(child.id, backend, out)?;
        }
        Ok(())
    }

    // Walk root sections in position order.
    let mut root_sections = backend
        .borrow_mut()
        .list_root_sections(notebook_id)
        .map_err(|e| anyhow::anyhow!("failed to list root sections: {}", e))?;
    root_sections.sort_by_key(|s| s.position);

    for section in root_sections {
        collect_pages_for_section(section.id, &backend, &mut ordered_page_ids)?;
    }

    if ordered_page_ids.is_empty() {
        return Err(anyhow::anyhow!("notebook has no pages"));
    }

    let mut first_page = true;
    for page_id in ordered_page_ids {
        // Load the page to get its template_id and planner_address.
        let page = backend
            .borrow_mut()
            .get_page(page_id)
            .map_err(|e| anyhow::anyhow!("get_page failed: {}", e))?;

        let strokes = backend
            .borrow_mut()
            .list_strokes_for_page(page_id)
            .map_err(|e| anyhow::anyhow!("list_strokes failed: {}", e))?;

        // Resolve template → background + page_rect + widgets.
        let template_opt = page
            .template_id
            .and_then(|tid| templates.borrow().get(tid).cloned());

        let (background, page_rect, widgets) = if let Some(ref t) = template_opt {
            let bg = journal_templates::page_template_to_background_config(t);
            let pr = Rect {
                x: 0.0,
                y: 0.0,
                width: t.size_mm.0,
                height: t.size_mm.1,
            };
            let w = t.widgets.clone();
            (bg, pr, w)
        } else {
            (
                crate::state::default_background(),
                crate::state::default_page_rect(),
                Vec::new(),
            )
        };

        // Compute fit zoom with 5% margin.
        let pw = page_rect.width.max(1.0);
        let ph = page_rect.height.max(1.0);
        let fit_zoom = (A4_W_PT / pw).min(A4_H_PT / ph) * 0.95;

        let viewport = Viewport {
            center: journal_core::Point {
                x: page_rect.x + pw * 0.5,
                y: page_rect.y + ph * 0.5,
            },
            zoom: fit_zoom,
            rotation: 0.0,
        };
        let transform = ViewportTransform::new(viewport, A4_W_PT, A4_H_PT);

        // Resolve page date for TextBlock placeholder expansion.
        let page_date = match page.planner_address {
            Some(journal_core::CalendarPageAddress::Day { date, .. }) => Some(date),
            _ => None,
        };
        let render_ctx = WidgetRenderContext {
            date: page_date,
            overrides: page.widget_overrides.clone(),
        };

        if !first_page {
            // Emit the previous page and start a fresh A4 page.
            ctx.show_page()
                .map_err(|e| anyhow::anyhow!("show_page failed: {:?}", e))?;
        }
        first_page = false;

        paint_with_widgets_ctx(
            &ctx,
            &transform,
            &background,
            page_rect,
            &widgets,
            &strokes,
            &empty_selected,
            dark_mode,
            &render_ctx,
        );
    }

    surface.finish();
    tracing::info!("notebook PDF exported to {:?}", path);
    Ok(())
}
