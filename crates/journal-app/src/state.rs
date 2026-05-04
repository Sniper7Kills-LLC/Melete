use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use gtk4::cairo;
use journal_canvas::{BackgroundConfig, GridSettings, ViewportTransform};
use journal_core::{Color, PageId, PageTemplate, PenSettings, Point, Rect, Stroke, TilingMode, Viewport};
use journal_storage::JournalBackend;
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
    Pencil,
    Highlighter,
    Paintbrush,
    SprayCan,
    Calligraphy,
    Eraser(EraserMode),
    Selection,
}

/// Per-tool brush parameters layered on top of the base `pen` at stroke
/// creation (see `input::begin_stroke`). Returns
/// `(opacity, base_width_multiplier, blend_mode, brush_style)`. Reads
/// from `state.tool_settings` so any per-tool overrides the user has
/// configured win over the built-in defaults; tools that don't draw
/// (Eraser, Selection) return neutral values + Pen style.
pub fn tool_brush_params(
    state: &CanvasState,
    tool: Tool,
) -> (f32, f64, journal_core::BlendMode, journal_core::BrushStyle) {
    // BrushStyle is tool-canonical: built-in tools always render with
    // their matching style. The `brush_style` field on ToolSettings is
    // reserved for the future custom-tool feature where the user can
    // build a tool that points at any brush style.
    if let Some(key) = crate::tool_settings::tool_key(tool) {
        let canonical = crate::tool_settings::default_settings_for(tool).brush_style;
        if let Some(s) = state.tool_settings.get(key) {
            return (s.opacity_mult, s.width_mult, s.blend_mode, canonical);
        }
        let d = crate::tool_settings::default_settings_for(tool);
        return (d.opacity_mult, d.width_mult, d.blend_mode, d.brush_style);
    }
    use journal_core::{BlendMode, BrushStyle};
    (1.0, 1.0, BlendMode::Normal, BrushStyle::Pen)
}

/// True when the tool draws strokes (vs. erase/select).
pub fn tool_is_drawing(tool: Tool) -> bool {
    matches!(
        tool,
        Tool::Pen
            | Tool::Pencil
            | Tool::Highlighter
            | Tool::Paintbrush
            | Tool::SprayCan
            | Tool::Calligraphy
    )
}

pub struct CanvasState {
    pub transform: ViewportTransform,
    pub strokes: Vec<Stroke>,
    pub current_stroke: Option<Stroke>,
    pub pen: PenSettings,
    pub background: BackgroundConfig,
    /// User-tunable grid spacing multiplier applied to `background` at paint
    /// time. `1.0` (default) renders the template's spacing as-is. The
    /// "Reset Grid" toolbar action sets this to `1.0 / current_zoom` so the
    /// grid's on-screen size matches what it looked like at zoom 1.0,
    /// letting the user lock the grid scale to whatever zoom level they're
    /// drawing at. Reset to 1.0 on page change.
    pub bg_scale: f64,
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
    /// Per-widget overrides loaded from `Page.widget_overrides` for the
    /// currently-loaded page. Empty when no page is loaded.
    pub current_page_overrides:
        std::collections::HashMap<uuid::Uuid, journal_core::WidgetOverride>,
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
    pub thumbnail_cache: HashMap<PageId, cairo::ImageSurface>,

    /// Per-app stroke clipboard for copy/paste between pages.
    /// Empty = nothing copied.
    pub stroke_clipboard: Vec<Stroke>,

    pub placeholder_image: Option<cairo::ImageSurface>,
    pub placeholder_text: String,
    pub show_page_bounds: bool,

    /// Last known pointer position in screen coordinates over the canvas.
    /// `None` when the pointer is not over the canvas. Used by the
    /// brush-cursor overlay to draw a circle showing the active brush size.
    pub pointer_screen: Option<(f64, f64)>,
    /// True while a stroke is being actively drawn — the brush cursor
    /// renders filled instead of outlined.
    pub pointer_drawing: bool,

    /// Active per-tool ToolSettings snapshot — derived from the active
    /// preset for each tool. The renderer reads from this; preset
    /// switching just copies the chosen preset's settings here.
    pub tool_settings: std::collections::HashMap<String, crate::tool_settings::ToolSettings>,

    /// Per-tool list of named presets. At least one preset per tool
    /// (`Default`) always exists.
    pub tool_presets:
        std::collections::HashMap<String, Vec<crate::tool_settings::NamedToolSettings>>,
    /// Per-tool currently-active preset name.
    pub active_tool_preset: std::collections::HashMap<String, String>,

    /// Per-brush-style internal tuning parameters (nib angle, halo
    /// alphas, dot density, …). Global — every stroke of a given brush
    /// style renders with these params.
    pub brush_params: journal_canvas::vello_renderer::BrushParams,

    /// Per-tool quick-pick color palettes. Each tool key maps to a
    /// list of RGBA swatches the user has saved for fast access from
    /// the Tool Options popup.
    pub tool_palettes: std::collections::HashMap<String, Vec<[u8; 4]>>,

    /// Optional composable-brush recipe stamped onto every new
    /// stroke. `None` means "use the built-in for the active tool's
    /// brush_style + BrushParams". `Some(brush)` overrides — used by
    /// the Tool Editor when the user is drawing with a custom brush.
    pub active_brush_recipe: Option<journal_core::Brush>,

    /// User-defined brush library. Loaded from
    /// `~/.config/journal/brushes.toml` at boot; written back when
    /// the Tool Editor saves. Built-in brushes are NOT in this list —
    /// the editor merges built-ins + this library at display time.
    pub brush_library: Vec<journal_core::Brush>,

    /// Per-tool assigned brush (full Brush, not just an id, since
    /// built-in compositions are constructed on demand and don't
    /// live anywhere persistent). Switching to a tool with an
    /// assignment loads the brush into `active_brush_recipe`;
    /// switching to one without snaps back to the legacy adapter.
    /// In-memory only for now — persistence lands with the
    /// per-tool brush picker UI in a follow-up commit.
    pub tool_brushes: std::collections::HashMap<String, journal_core::Brush>,
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
        brush_style: journal_core::BrushStyle::Pen,
    };

    Rc::new(RefCell::new(CanvasState {
        transform,
        strokes: Vec::new(),
        current_stroke: None,
        pen,
        background: default_background(),
        bg_scale: 1.0,
        page_rect: DEFAULT_PAGE_RECT,
        backend,
        templates,
        notebook_templates,
        current_page_id: None,
        current_template: None,
        current_page_date: None,
        current_page_overrides: std::collections::HashMap::new(),
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
        thumbnail_cache: HashMap::new(),
        stroke_clipboard: Vec::new(),
        placeholder_image: None,
        placeholder_text: "Select a page to start drawing".into(),
        show_page_bounds: true,
        pointer_screen: None,
        pointer_drawing: false,
        tool_settings: crate::tool_settings::default_settings_map(),
        tool_presets: crate::tool_settings::default_presets_map(),
        active_tool_preset: crate::tool_settings::default_active_preset_map(),
        brush_params: journal_canvas::vello_renderer::BrushParams::default(),
        tool_palettes: std::collections::HashMap::new(),
        active_brush_recipe: None,
        brush_library: crate::brush_library::load(),
        tool_brushes: std::collections::HashMap::new(),
    }))
}

/// Merge any per-tool overrides loaded from `~/.config/journal/config.toml`
/// on top of the built-in defaults. Migrates old single-snapshot
/// `tool_settings` entries into a one-preset list when the user hasn't
/// adopted the named-preset format yet.
pub fn load_tool_settings_from_config(state: &SharedState) {
    let cfg = crate::config::load();
    let mut s = state.borrow_mut();

    // Pull explicit named presets first (the richer source of truth).
    for (key, presets) in cfg.tool_presets {
        if !presets.is_empty() {
            s.tool_presets.insert(key, presets);
        }
    }
    for (key, name) in cfg.active_tool_preset {
        s.active_tool_preset.insert(key, name);
    }

    // Backfill: if the legacy flat `tool_settings` map has a value the
    // preset list doesn't, treat it as the user's "Default" preset.
    for (key, value) in cfg.tool_settings {
        let entry = s
            .tool_presets
            .entry(key.clone())
            .or_insert_with(|| {
                vec![crate::tool_settings::NamedToolSettings {
                    name: "Default".into(),
                    settings: value,
                }]
            });
        if entry.iter().any(|p| p.name == "Default") {
            for p in entry.iter_mut() {
                if p.name == "Default" {
                    p.settings = value;
                }
            }
        }
        s.tool_settings.insert(key, value);
    }

    // Sync `tool_settings` (the flat snapshot) to whatever the active
    // preset says — covers the case where presets were stored but the
    // legacy flat snapshot wasn't.
    let keys: Vec<String> = s.tool_presets.keys().cloned().collect();
    for key in keys {
        let active_name = s
            .active_tool_preset
            .get(&key)
            .cloned()
            .unwrap_or_else(|| "Default".to_string());
        let resolved = s.tool_presets.get(&key).and_then(|presets| {
            presets
                .iter()
                .find(|p| p.name == active_name)
                .or_else(|| presets.first())
                .map(|p| p.settings)
        });
        if let Some(settings) = resolved {
            s.tool_settings.insert(key, settings);
        }
    }

    if let Some(params) = cfg.brush_params {
        s.brush_params = params;
    }

    s.tool_palettes = cfg.tool_palettes;
}

/// Activate a named preset for a tool — copies the preset's settings
/// into `tool_settings` (where the renderer reads from) and updates
/// `active_tool_preset`. Persists to config.
pub fn activate_tool_preset(state: &SharedState, tool: Tool, preset_name: &str) {
    let key = match crate::tool_settings::tool_key(tool) {
        Some(k) => k.to_string(),
        None => return,
    };
    {
        let mut s = state.borrow_mut();
        let to_apply = s
            .tool_presets
            .get(&key)
            .and_then(|v| v.iter().find(|p| p.name == preset_name).cloned());
        if let Some(p) = to_apply {
            s.tool_settings.insert(key.clone(), p.settings);
            s.active_tool_preset.insert(key.clone(), p.name);
            s.pen.base_width = p.settings.default_base_width;
        }
    }
    persist_tool_state(state);
}

/// Persist the current preset list + active preset map + flat
/// tool_settings snapshot + per-tool palettes to disk. Called whenever
/// the user edits a preset or switches the active one.
pub fn persist_tool_state(state: &SharedState) {
    let mut cfg = crate::config::load();
    let s = state.borrow();
    cfg.tool_settings = s.tool_settings.clone();
    cfg.tool_presets = s.tool_presets.clone();
    cfg.active_tool_preset = s.active_tool_preset.clone();
    cfg.brush_params = Some(s.brush_params);
    cfg.tool_palettes = s.tool_palettes.clone();
    cfg.tool_brush_assignments = s
        .tool_brushes
        .iter()
        .map(|(k, b)| (k.clone(), b.id))
        .collect();
    drop(s);
    if let Err(e) = crate::config::save(&cfg) {
        tracing::warn!("persist tool state: {e}");
    }
}

/// Resolve `cfg.tool_brush_assignments` (key→Uuid) against built-in
/// + user-library brushes; populate `state.tool_brushes`. IDs that
/// don't resolve (e.g. a custom brush the user has since deleted)
/// are silently dropped.
pub fn load_tool_brush_assignments(state: &SharedState) {
    let cfg = crate::config::load();
    let mut s = state.borrow_mut();
    let library = s.brush_library.clone();
    for (key, id) in cfg.tool_brush_assignments {
        if let Some(brush) = crate::brush_library::resolve_id(id, &library) {
            s.tool_brushes.insert(key, brush);
        }
    }
    // After populating, snap the active tool's recipe to its slot.
    let tool = s.tool;
    let active = crate::tool_settings::tool_key(tool)
        .and_then(|k| s.tool_brushes.get(k).cloned());
    s.active_brush_recipe = active;
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
    let (strokes, page_date, overrides) = {
        let mut b = backend.borrow_mut();
        let strokes = match b.list_strokes_for_page(page_id) {
            Ok(v) => v,
            Err(e) => {
                tracing::error!("failed to load strokes for {:?}: {}", page_id, e);
                Vec::new()
            }
        };
        let (page_date, overrides) = match b.get_page(page_id) {
            Ok(p) => {
                let d = match p.planner_address {
                    Some(journal_core::CalendarPageAddress::Day { date, .. }) => Some(date),
                    _ => None,
                };
                (d, p.widget_overrides.clone())
            }
            Err(_) => (None, std::collections::HashMap::new()),
        };
        (strokes, page_date, overrides)
    };
    let mut s = state.borrow_mut();
    s.strokes = strokes;
    s.current_stroke = None;
    s.current_page_id = Some(page_id);
    s.current_page_date = page_date;
    s.current_page_overrides = overrides;
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
    s.bg_scale = 1.0;
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

/// Set the active tool. Per-tool stroke effects (opacity / width multiplier
/// / blend mode) are applied at stroke creation in `input::begin_stroke`,
/// not here, so `state.pen.color` remains the user-chosen base across
/// tool switches. `pen.base_width` IS updated from the tool's
/// `default_base_width` setting so e.g. picking the Highlighter snaps
/// the brush back to its 20mm default without the user touching the
/// width slider. The tool's active preset's settings are copied into
/// `tool_settings` so the renderer + cursor pick them up.
pub fn set_tool(state: &SharedState, tool: Tool) {
    let mut s = state.borrow_mut();
    let prev_tool = s.tool;
    s.tool = tool;
    // Each tool has an optional per-slot brush assignment. Switching
    // to a tool loads its assigned brush (or `None` → legacy adapter).
    if prev_tool != tool {
        let assigned = crate::tool_settings::tool_key(tool)
            .and_then(|k| s.tool_brushes.get(k).cloned());
        s.active_brush_recipe = assigned;
    }
    if let Some(key) = crate::tool_settings::tool_key(tool) {
        // Sync flat snapshot to the active preset for this tool, in
        // case the user edited a different tool's preset and switched
        // back to this one.
        let active_name = s
            .active_tool_preset
            .get(key)
            .cloned()
            .unwrap_or_else(|| "Default".to_string());
        if let Some(presets) = s.tool_presets.get(key) {
            if let Some(p) = presets.iter().find(|p| p.name == active_name).cloned() {
                s.tool_settings.insert(key.to_string(), p.settings);
                s.pen.base_width = p.settings.default_base_width;
                return;
            }
        }
        let bw = s
            .tool_settings
            .get(key)
            .map(|t| t.default_base_width)
            .unwrap_or_else(|| crate::tool_settings::default_settings_for(tool).default_base_width);
        s.pen.base_width = bw;
    }
}

pub fn set_tool_pen(state: &SharedState) {
    set_tool(state, Tool::Pen);
}

pub fn set_tool_highlighter(state: &SharedState) {
    set_tool(state, Tool::Highlighter);
}

pub fn set_tool_selection(state: &SharedState) {
    set_tool(state, Tool::Selection);
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
