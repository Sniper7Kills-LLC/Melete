use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use gtk4::cairo;
use journal_canvas::{BackgroundConfig, GridSettings, ViewportTransform};
use journal_core::{Color, PageId, PageTemplate, PenSettings, Point, Rect, Stroke, TilingMode, Viewport};
use journal_storage::{JournalBackend, StrokeStore};
use journal_templates::{NotebookTemplateRegistry, TemplateRegistry};
use uuid::Uuid;

use crate::history::History;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EraserMode {
    Stroke,
    Partial,
}

/// Which of the 8 resize handles the user grabbed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HandlePos {
    TL, T, TR, R, BR, B, BL, L,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tool {
    Pen,
    Eraser(EraserMode),
    Highlighter,
    Selection,
}

pub struct CanvasState {
    pub transform: ViewportTransform,
    pub strokes: Vec<Stroke>,
    pub current_stroke: Option<Stroke>,
    pub pen: PenSettings,
    pub background: BackgroundConfig,
    pub page_rect: Rect,
    pub backend: Rc<RefCell<dyn JournalBackend>>,
    pub templates: Rc<RefCell<TemplateRegistry>>,
    pub notebook_templates: Rc<RefCell<NotebookTemplateRegistry>>,
    pub current_page_id: Option<PageId>,
    pub current_template: Option<PageTemplate>,
    /// Calendar date bound to the current page, used by widget renderers to
    /// expand `{date}/{weekday}/...` placeholders in template `TextBlock`s.
    /// `None` when the current page is not a planner page.
    pub current_page_date: Option<chrono::NaiveDate>,
    /// Installed by the planner navigation strip; called from `load_page`
    /// when the user clicks a Day-addressed planner page so prev/next walk
    /// from that date instead of from "today".
    pub planner_nav_sync_date: Option<Rc<dyn Fn(chrono::NaiveDate)>>,

    pub tool: Tool,
    pub history: History,
    pub selected_stroke_ids: HashSet<Uuid>,
    pub lasso_points: Vec<(f64, f64)>,
    pub lasso_active: bool,
    pub selection_drag_start: Option<(f64, f64)>,
    pub selection_drag_total_canvas: (f64, f64),
    pub selection_resize_handle: Option<HandlePos>,
    pub selection_resize_start: Option<(f64, f64)>,
    pub selection_resize_bbox_orig: Option<journal_core::Rect>,
    pub selection_resize_cumulative: (f64, f64),
    pub selection_resize_anchor: (f64, f64),
    pub dark_mode: bool,
    pub thumbnail_cache: HashMap<PageId, cairo::ImageSurface>,

    pub saved_pen_color: Color,
    pub saved_pen_width: f64,

    pub placeholder_image: Option<cairo::ImageSurface>,
    pub placeholder_text: String,
    pub show_page_bounds: bool,
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

pub fn new_shared_state(
    backend: Rc<RefCell<dyn JournalBackend>>,
    templates: Rc<RefCell<TemplateRegistry>>,
    notebook_templates: Rc<RefCell<NotebookTemplateRegistry>>,
) -> SharedState {
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
        backend,
        templates,
        notebook_templates,
        current_page_id: None,
        current_template: None,
        current_page_date: None,
        planner_nav_sync_date: None,
        tool: Tool::Pen,
        history: History::new(),
        selected_stroke_ids: HashSet::new(),
        lasso_points: Vec::new(),
        lasso_active: false,
        selection_drag_start: None,
        selection_drag_total_canvas: (0.0, 0.0),
        selection_resize_handle: None,
        selection_resize_start: None,
        selection_resize_bbox_orig: None,
        selection_resize_cumulative: (1.0, 1.0),
        selection_resize_anchor: (0.0, 0.0),
        dark_mode: false,
        thumbnail_cache: HashMap::new(),
        saved_pen_color: Color { r: 20, g: 20, b: 20, a: 255 },
        saved_pen_width: 2.0,
        placeholder_image: None,
        placeholder_text: "Select a page to start drawing".into(),
        show_page_bounds: true,
    }))
}

/// Reload placeholder image + text from on-disk config into the state.
pub fn reload_placeholder(state: &SharedState) {
    let cfg = crate::config::load();
    let mut s = state.borrow_mut();
    s.placeholder_text = cfg
        .placeholder_text
        .filter(|t| !t.trim().is_empty())
        .unwrap_or_else(|| "Select a page to start drawing".into());
    s.placeholder_image = cfg.placeholder_image_path.and_then(load_image_surface);
    s.show_page_bounds = cfg.show_page_bounds;
}

fn load_image_surface(path: std::path::PathBuf) -> Option<cairo::ImageSurface> {
    let pixbuf = match gtk4::gdk_pixbuf::Pixbuf::from_file(&path) {
        Ok(p) => p,
        Err(e) => {
            tracing::warn!("placeholder image load failed {:?}: {}", path, e);
            return None;
        }
    };
    let w = pixbuf.width();
    let h = pixbuf.height();
    if w <= 0 || h <= 0 {
        return None;
    }
    let surface = cairo::ImageSurface::create(cairo::Format::ARgb32, w, h).ok()?;
    {
        let ctx = cairo::Context::new(&surface).ok()?;
        use gtk4::prelude::*;
        ctx.set_source_pixbuf(&pixbuf, 0.0, 0.0);
        let _ = ctx.paint();
    }
    Some(surface)
}

/// Load strokes for the given page and update current_page_id.
/// Caller is responsible for queue_draw on the canvas afterwards.
pub fn set_current_page(state: &SharedState, page_id: PageId) {
    let backend = state.borrow().backend.clone();
    let (strokes, page_date) = {
        let mut b = backend.borrow_mut();
        let strokes = match b.list_strokes_for_page(page_id) {
            Ok(v) => v,
            Err(e) => {
                tracing::error!("failed to load strokes for {:?}: {}", page_id, e);
                Vec::new()
            }
        };
        // Resolve the page's calendar date if it has a planner address.
        let page_date = match b.get_page(page_id) {
            Ok(p) => match p.planner_address {
                Some(journal_core::CalendarPageAddress::Day { date, .. }) => Some(date),
                _ => None,
            },
            Err(_) => None,
        };
        (strokes, page_date)
    };
    let mut s = state.borrow_mut();
    s.strokes = strokes;
    s.current_stroke = None;
    s.current_page_id = Some(page_id);
    s.current_page_date = page_date;
    s.history.clear();
    s.selected_stroke_ids.clear();
    s.lasso_points.clear();
    s.lasso_active = false;
    s.selection_resize_handle = None;
    s.selection_resize_start = None;
    s.selection_resize_bbox_orig = None;
    s.selection_resize_cumulative = (1.0, 1.0);
    s.selection_resize_anchor = (0.0, 0.0);
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

pub fn fit_viewport_to_page(transform: &mut ViewportTransform, page: Rect) {
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

pub fn fit_viewport_to_page_pub(transform: &mut ViewportTransform, page: Rect) {
    fit_viewport_to_page(transform, page);
}

pub fn toggle_tool_eraser(state: &SharedState) {
    let mut s = state.borrow_mut();
    s.tool = match s.tool {
        Tool::Eraser(_) => Tool::Pen,
        _ => Tool::Eraser(EraserMode::Stroke),
    };
}

pub fn set_tool_pen(state: &SharedState) {
    let mut s = state.borrow_mut();
    s.tool = Tool::Pen;
    s.pen.blend_mode = journal_core::BlendMode::Normal;
    s.pen.opacity = 1.0;
    s.pen.color = s.saved_pen_color;
    s.pen.base_width = s.saved_pen_width;
}

pub fn set_tool_highlighter(state: &SharedState) {
    let mut s = state.borrow_mut();
    s.saved_pen_color = s.pen.color;
    s.saved_pen_width = s.pen.base_width;
    s.tool = Tool::Highlighter;
    s.pen.opacity = 0.35;
    s.pen.blend_mode = journal_core::BlendMode::Multiply;
}

pub fn set_tool_selection(state: &SharedState) {
    let mut s = state.borrow_mut();
    s.tool = Tool::Selection;
}

pub fn clear_selection(state: &SharedState) {
    let mut s = state.borrow_mut();
    s.selected_stroke_ids.clear();
    s.lasso_points.clear();
    s.lasso_active = false;
    s.selection_drag_start = None;
    s.selection_resize_handle = None;
    s.selection_resize_start = None;
    s.selection_resize_bbox_orig = None;
    s.selection_resize_cumulative = (1.0, 1.0);
    s.selection_resize_anchor = (0.0, 0.0);
}
