use std::cell::RefCell;
use std::rc::Rc;

use journal_canvas::{BackgroundConfig, GridSettings, ViewportTransform};
use journal_core::{Color, PageId, PageTemplate, PenSettings, Point, Rect, Stroke, TilingMode, Viewport};
use journal_storage::{stroke_store, Db};
use journal_templates::TemplateRegistry;

pub struct CanvasState {
    pub transform: ViewportTransform,
    pub strokes: Vec<Stroke>,
    pub current_stroke: Option<Stroke>,
    pub pen: PenSettings,
    pub background: BackgroundConfig,
    pub page_rect: Rect,
    pub db: Rc<RefCell<Db>>,
    pub templates: Rc<RefCell<TemplateRegistry>>,
    pub current_page_id: Option<PageId>,
    pub current_template: Option<PageTemplate>,
}

pub type SharedState = Rc<RefCell<CanvasState>>;

const DEFAULT_PAGE_RECT: Rect = Rect {
    x: 0.0,
    y: 0.0,
    width: 816.0,
    height: 1056.0,
};

pub fn default_background() -> BackgroundConfig {
    BackgroundConfig::Grid(GridSettings {
        base_spacing: 20.0,
        subdivisions: 4,
        color: Color { r: 200, g: 200, b: 220, a: 255 },
    })
}

pub fn default_page_rect() -> Rect {
    DEFAULT_PAGE_RECT
}

pub fn new_shared_state(db: Rc<RefCell<Db>>, templates: Rc<RefCell<TemplateRegistry>>) -> SharedState {
    let viewport = Viewport {
        center: journal_core::Point { x: 408.0, y: 528.0 },
        zoom: 1.0,
        rotation: 0.0,
    };
    let transform = ViewportTransform::new(viewport, 1280.0, 800.0);

    let pen = PenSettings {
        color: Color { r: 20, g: 20, b: 20, a: 255 },
        base_width: 2.0,
        opacity: 1.0,
        blend_mode: journal_core::BlendMode::Normal,
    };

    Rc::new(RefCell::new(CanvasState {
        transform,
        strokes: Vec::new(),
        current_stroke: None,
        pen,
        background: default_background(),
        page_rect: DEFAULT_PAGE_RECT,
        db,
        templates,
        current_page_id: None,
        current_template: None,
    }))
}

/// Load strokes for the given page and update current_page_id.
/// Caller is responsible for queue_draw on the canvas afterwards.
pub fn set_current_page(state: &SharedState, page_id: PageId) {
    let db = state.borrow().db.clone();
    let strokes = match stroke_store::list_strokes_for_page(db.borrow().conn(), page_id) {
        Ok(v) => v,
        Err(e) => {
            tracing::error!("failed to load strokes for {:?}: {}", page_id, e);
            Vec::new()
        }
    };
    let mut s = state.borrow_mut();
    s.strokes = strokes;
    s.current_stroke = None;
    s.current_page_id = Some(page_id);
}

/// Apply a template to current canvas state (or clear back to defaults if None).
pub fn set_current_template(state: &SharedState, template: Option<PageTemplate>) {
    let mut s = state.borrow_mut();
    match &template {
        Some(t) => {
            s.background = journal_templates::page_template_to_background_config(t);
            s.page_rect = Rect {
                x: 0.0,
                y: 0.0,
                width: t.size_mm.0,
                height: t.size_mm.1,
            };
            if t.tiling == TilingMode::None {
                let page_rect = s.page_rect;
                fit_viewport_to_page(&mut s.transform, page_rect);
            }
        }
        None => {
            s.background = default_background();
            s.page_rect = DEFAULT_PAGE_RECT;
        }
    }
    s.current_template = template;
}

fn fit_viewport_to_page(transform: &mut ViewportTransform, page: Rect) {
    let (sw, sh) = transform.screen_size();
    if sw <= 0.0 || sh <= 0.0 || page.width <= 0.0 || page.height <= 0.0 {
        return;
    }
    let margin = 0.9;
    let zoom = (sw / page.width).min(sh / page.height) * margin;
    let mut viewport = transform.viewport();
    viewport.zoom = zoom.max(1e-3);
    viewport.center = Point {
        x: page.x + page.width * 0.5,
        y: page.y + page.height * 0.5,
    };
    transform.set_viewport(viewport);
}
