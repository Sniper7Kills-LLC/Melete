//! `journal-web-viewer` — read-only canvas viewer for the Journal web POC.
//!
//! Wraps `vello::Renderer` + WebGPU (no WebGL2 fallback for the POC, per
//! `docs/web-portal.md` §5.4) and exposes a wasm-bindgen `Viewer` class
//! that the SPA's `web/src/wasm/index.ts` consumes. The viewer:
//!
//!   1. `init(canvas)` — async; creates a WebGPU `wgpu::Surface` over
//!      the supplied `<canvas>`, requests a device, builds a `Vello::Renderer`.
//!   2. `loadNotebook(bytes)` — decodes the JSON `NotebookBundle` envelope
//!      (`web/public/sample-notebook.json` shape; mirrored in
//!      `web/src/types/index.ts::NotebookBundle`), caches strokes /
//!      page-templates per page id, picks the first page as the default.
//!   3. `renderPage(index, w, h)` — resizes the surface if needed,
//!      builds a `vello::Scene` with page-color fill, the page template's
//!      background pattern, the widgets (via `journal-widgets`), and
//!      every cached stroke; then submits to wgpu.
//!   4. `pan(dx, dy)` / `zoomAt(sx, sy, factor)` — mutate the
//!      `ViewportTransform` and trigger a repaint at the last known
//!      `(w, h)`.
//!
//! Strokes are drawn via simple stroked Bezier paths (one path per
//! stroke, using the stored `pen.color` and `pen.base_width / zoom_at_creation`).
//! The desktop's full brush engine in `journal_canvas::vello_renderer` uses
//! private scene-build helpers we can't reach from outside the crate;
//! when those become reusable we can swap them in. The simple form
//! reproduces the mock viewer's visible behaviour and is enough for
//! the read-only web POC.
//!
//! Coordinate spaces match the desktop:
//!   - Canvas / world = template's mm space (template_widget rects, strokes).
//!   - Screen = physical px.
//!   - `Affine` from world → screen is `translate(w/2 + pan, h/2 + pan)`
//!     × `scale(zoom)` × `translate(-center)`. We feed this into Vello.

#![allow(clippy::needless_pass_by_value)]
#![allow(clippy::too_many_arguments)]

mod bundle;

use std::collections::HashMap;

use journal_canvas::{
    viewport_transform::ViewportTransform, BackgroundConfig, GridSettings,
};
use journal_core::{
    BackgroundType, BlendMode, PageTemplate, Point, Rect, Stroke, TemplateId, TilingMode, Viewport,
};
use journal_widgets::{WidgetRenderContext, WidgetRenderer};

use vello::kurbo::{Affine, BezPath, Cap, Join, Rect as KRect, Shape, Stroke as KStroke};
use vello::peniko::{Brush, Color as PColor, Fill};
use vello::{AaConfig, RenderParams, Renderer, Scene};
#[cfg(target_arch = "wasm32")]
use vello::RendererOptions;

use uuid::Uuid;
use wasm_bindgen::prelude::*;
use web_sys::HtmlCanvasElement;

use crate::bundle::NotebookBundle;

#[cfg(target_arch = "wasm32")]
const TEXTURE_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

fn js_err<E: std::fmt::Display>(e: E) -> JsValue {
    JsValue::from(js_sys::Error::new(&e.to_string()))
}

/// `wasm_bindgen` doesn't accept generic types in the public surface,
/// so we hand-roll a small struct over the wgpu / vello / app state
/// pieces. None of these survive a `&self` borrow alone (vello mutates
/// its scene buffer on every draw), so most methods take `&mut self`.
#[wasm_bindgen]
pub struct Viewer {
    inner: Option<ViewerState>,
    bundle: Option<NotebookBundle>,
    /// Per-page strokes keyed by `Page.id`. Populated by `load_notebook`.
    strokes_by_page: HashMap<Uuid, Vec<Stroke>>,
    /// Per-page templates, also keyed by `Page.id`. Pages without a
    /// template_id, or whose template_id doesn't resolve, fall back to
    /// `Blank`. We materialise the full template (not just the id) so
    /// `render_page` doesn't need to re-walk the bundle every frame.
    template_by_page: HashMap<Uuid, PageTemplate>,
    transform: Option<ViewportTransform>,
    last_size: (u32, u32),
    last_index: u32,
    widget_renderer: WidgetRenderer,
}

struct ViewerState {
    /// Held for lifetime — wgpu's `Surface` references the instance's
    /// adapter pool indirectly. Read by the host stub but not re-used
    /// elsewhere; kept so it doesn't drop until the viewer drops.
    #[allow(dead_code)]
    instance: wgpu::Instance,
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    renderer: Renderer,
    scene: Scene,
    /// Most recently used surface configuration. Re-applied (with new
    /// width/height) on `render_page` if the canvas resized.
    config: wgpu::SurfaceConfiguration,
}

#[wasm_bindgen]
impl Viewer {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Viewer {
        // Panic hook lives here so dev-mode panics (e.g. unwrap inside
        // load_notebook) show in the browser console with a stack
        // trace. Idempotent (`set_once` semantics).
        console_error_panic_hook::set_once();
        Viewer {
            inner: None,
            bundle: None,
            strokes_by_page: HashMap::new(),
            template_by_page: HashMap::new(),
            transform: None,
            last_size: (0, 0),
            last_index: 0,
            widget_renderer: WidgetRenderer::new(),
        }
    }

    /// Initialize the WebGPU surface bound to `canvas`. Async because
    /// `request_adapter` / `request_device` resolve via JS promises.
    ///
    /// Implementation is gated to `wasm32` — `wgpu::SurfaceTarget::Canvas`
    /// only exists when wgpu is built for the browser. On non-wasm
    /// targets the body is a no-op so `cargo build --workspace` keeps
    /// the crate green for tooling / clippy / future host-side tests.
    #[cfg(target_arch = "wasm32")]
    pub async fn init(&mut self, canvas: HtmlCanvasElement) -> Result<(), JsValue> {
        // Best-effort: call `set_once` again is a no-op.
        console_error_panic_hook::set_once();

        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::BROWSER_WEBGPU,
            ..Default::default()
        });

        // Build a surface from the canvas. wgpu 28's API takes a
        // `SurfaceTarget`; the `Canvas` variant carries an owned
        // `HtmlCanvasElement`. The lifetime is `'static` because wgpu
        // holds onto the JS handle for the duration of the surface.
        let surface = instance
            .create_surface(wgpu::SurfaceTarget::Canvas(canvas.clone()))
            .map_err(|e| js_err(format!("create_surface: {e}")))?;

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .map_err(|e| js_err(format!("request_adapter: {e}")))?;

        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("journal-web-viewer-device"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::downlevel_webgl2_defaults()
                    .using_resolution(adapter.limits()),
                ..Default::default()
            })
            .await
            .map_err(|e| js_err(format!("request_device: {e}")))?;

        let renderer = Renderer::new(&device, RendererOptions::default())
            .map_err(|e| js_err(format!("vello renderer init: {e:?}")))?;

        // Initial surface config — width/height are placeholders;
        // `render_page` will reconfigure on first paint with the real
        // viewport dimensions.
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::STORAGE_BINDING
                | wgpu::TextureUsages::COPY_DST,
            format: TEXTURE_FORMAT,
            width: 1.max(canvas.width()),
            height: 1.max(canvas.height()),
            present_mode: wgpu::PresentMode::AutoVsync,
            desired_maximum_frame_latency: 2,
            alpha_mode: wgpu::CompositeAlphaMode::Auto,
            view_formats: vec![],
        };
        surface.configure(&device, &config);

        self.inner = Some(ViewerState {
            instance,
            surface,
            device,
            queue,
            renderer,
            scene: Scene::new(),
            config,
        });
        Ok(())
    }

    /// Host-side stub. `wgpu::SurfaceTarget::Canvas` only exists on
    /// `wasm32`; on other targets we accept the canvas argument so the
    /// type signature stays compatible with the wasm-bindgen-exposed
    /// surface, then return Ok without setting up a surface. Calling
    /// any of the render fns afterwards is a no-op.
    #[cfg(not(target_arch = "wasm32"))]
    pub async fn init(&mut self, _canvas: HtmlCanvasElement) -> Result<(), JsValue> {
        Ok(())
    }

    /// Decode a `NotebookBundle` JSON envelope and cache the page →
    /// strokes / template lookup tables. Bytes are UTF-8 JSON; gunzip
    /// is a future concern (see `web-portal.md` §5.4 — POC streams raw).
    #[wasm_bindgen(js_name = loadNotebook)]
    pub fn load_notebook(&mut self, bytes: &[u8]) -> Result<(), JsValue> {
        let text = std::str::from_utf8(bytes)
            .map_err(|e| js_err(format!("notebook bytes are not UTF-8: {e}")))?;
        let bundle: NotebookBundle = serde_json::from_str(text)
            .map_err(|e| js_err(format!("notebook JSON parse: {e}")))?;

        self.strokes_by_page.clear();
        self.template_by_page.clear();

        for (page_id_str, strokes) in &bundle.strokes_by_page {
            if let Ok(page_id) = Uuid::parse_str(page_id_str) {
                self.strokes_by_page.insert(page_id, strokes.clone());
            }
        }

        // Convert envelope's JSON-schema templates into core PageTemplate.
        let resolved_templates = bundle.page_templates_resolved();
        let template_by_id: HashMap<Uuid, PageTemplate> = resolved_templates
            .iter()
            .map(|t| (t.id.0, t.clone()))
            .collect();

        for page in &bundle.pages {
            let template = page
                .template_id
                .as_ref()
                .and_then(|id| template_by_id.get(&id.0).cloned())
                .unwrap_or_else(blank_template);
            self.template_by_page.insert(page.id.0, template);
        }

        // Establish a default viewport from the first page's template
        // size so the first render paints something visible. Width /
        // height get reset by render_page.
        if let Some(first_page) = bundle.pages.first() {
            if let Some(template) = self.template_by_page.get(&first_page.id.0) {
                let (tw, th) = template.size_mm;
                let viewport = template.default_viewport.unwrap_or(Viewport {
                    center: Point {
                        x: tw * 0.5,
                        y: th * 0.5,
                    },
                    zoom: 1.0,
                    rotation: 0.0,
                });
                self.transform = Some(ViewportTransform::new(viewport, 1.0, 1.0));
            }
        }

        self.bundle = Some(bundle);
        self.last_index = 0;
        Ok(())
    }

    #[wasm_bindgen(js_name = renderPage)]
    pub fn render_page(&mut self, index: u32, w: u32, h: u32) {
        if w == 0 || h == 0 {
            return;
        }
        let Some(state) = self.inner.as_mut() else {
            return;
        };
        let Some(bundle) = self.bundle.as_ref() else {
            return;
        };
        let Some(page) = bundle.pages.get(index as usize) else {
            return;
        };

        self.last_index = index;
        self.last_size = (w, h);

        // Reconfigure surface if size changed.
        if state.config.width != w || state.config.height != h {
            state.config.width = w;
            state.config.height = h;
            state.surface.configure(&state.device, &state.config);
        }

        let template = self
            .template_by_page
            .get(&page.id.0)
            .cloned()
            .unwrap_or_else(blank_template);
        let strokes = self
            .strokes_by_page
            .get(&page.id.0)
            .cloned()
            .unwrap_or_default();

        // Materialize / refresh the viewport transform. If we don't
        // have one yet (e.g. init was called but load_notebook hasn't),
        // fit the template to the screen the same way the desktop's
        // template_preview does.
        let (tw_mm, th_mm) = template.size_mm;
        let transform = self.transform.get_or_insert_with(|| {
            let zoom = ((w as f64 / tw_mm).min(h as f64 / th_mm)) * 0.95;
            let viewport = template.default_viewport.unwrap_or(Viewport {
                center: Point {
                    x: tw_mm * 0.5,
                    y: th_mm * 0.5,
                },
                zoom,
                rotation: 0.0,
            });
            ViewportTransform::new(viewport, w as f64, h as f64)
        });
        transform.set_size(w as f64, h as f64);

        let bg = page_template_background(&template);
        let page_rect = Rect {
            x: 0.0,
            y: 0.0,
            width: tw_mm,
            height: th_mm,
        };

        state.scene.reset();
        build_scene(
            &mut state.scene,
            transform,
            &bg,
            page_rect,
            &template,
            &strokes,
            &mut self.widget_renderer,
            w,
            h,
        );

        let frame = match state.surface.get_current_texture() {
            Ok(f) => f,
            Err(e) => {
                web_sys::console::warn_1(
                    &format!("get_current_texture: {e:?}").into(),
                );
                return;
            }
        };
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        if let Err(e) = state.renderer.render_to_texture(
            &state.device,
            &state.queue,
            &state.scene,
            &view,
            &RenderParams {
                base_color: PColor::from_rgba8(0, 0, 0, 0),
                width: w,
                height: h,
                antialiasing_method: AaConfig::Area,
            },
        ) {
            web_sys::console::warn_1(&format!("vello render: {e:?}").into());
        }

        frame.present();
    }

    pub fn pan(&mut self, dx: f64, dy: f64) {
        if let Some(t) = self.transform.as_mut() {
            t.pan(dx, dy);
        }
        self.repaint_last();
    }

    #[wasm_bindgen(js_name = zoomAt)]
    pub fn zoom_at(&mut self, sx: f64, sy: f64, factor: f64) {
        if let Some(t) = self.transform.as_mut() {
            t.zoom_at((sx, sy), factor);
        }
        self.repaint_last();
    }

    fn repaint_last(&mut self) {
        let (w, h) = self.last_size;
        if w == 0 || h == 0 {
            return;
        }
        let idx = self.last_index;
        self.render_page(idx, w, h);
    }
}

impl Default for Viewer {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------
// Scene building
// ---------------------------------------------------------------------

fn build_scene(
    scene: &mut Scene,
    transform: &ViewportTransform,
    bg: &BackgroundConfig,
    page_rect: Rect,
    template: &PageTemplate,
    strokes: &[Stroke],
    widget_renderer: &mut WidgetRenderer,
    w: u32,
    h: u32,
) {
    let world_to_screen = world_to_screen_affine(transform);

    // 1) Page colour fill — covers entire viewport (light fieldbook cream).
    let bg_color = PColor::from_rgba8(244, 239, 226, 255);
    scene.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        &Brush::Solid(bg_color),
        None,
        &KRect::new(0.0, 0.0, w as f64, h as f64),
    );

    // 2) Background pattern — only the simple grid families (no
    //    Image/Pdf in the web POC; those need asset URLs which the
    //    viewer doesn't fetch yet). `Blank` is a no-op.
    draw_background_pattern(scene, transform, world_to_screen, bg, page_rect);

    // 3) Widgets — clipped per-widget by `WidgetRenderer::draw_widgets`.
    let widget_ctx = WidgetRenderContext {
        date: None,
        overrides: Default::default(),
        widget_data: Default::default(),
        dark_mode: false,
    };
    widget_renderer.draw_widgets(
        scene,
        world_to_screen,
        &template.widgets,
        page_rect,
        &widget_ctx,
    );

    // 4) Strokes — basic stroked beziers per stroke.
    for stroke in strokes {
        if stroke.points.len() < 2 {
            continue;
        }
        let blend = stroke.pen.blend_mode;
        if blend != BlendMode::Normal {
            // Group blend handling lives in the desktop's vello_renderer
            // private API; for the web POC we just paint Normal and the
            // others as Normal too. They still appear, just without the
            // multiply / screen / etc compositing flavour.
        }
        draw_stroke(scene, world_to_screen, stroke, transform.zoom());
    }
}

fn draw_stroke(scene: &mut Scene, world_to_screen: Affine, stroke: &Stroke, _zoom: f64) {
    let mut path = BezPath::new();
    let p0 = stroke.points[0];
    path.move_to((p0.x, p0.y));
    for p in &stroke.points[1..] {
        path.line_to((p.x, p.y));
    }
    let zoom_at_creation = stroke.zoom_at_creation.max(1e-3);
    // Re-canvas-relative width: `pen.base_width` was in screen px at
    // creation, so its canvas-space width is `base_width / zoom_at_creation`.
    let width_canvas = (stroke.pen.base_width / zoom_at_creation).max(0.05);
    let color = stroke.pen.color;
    let pcolor = PColor::from_rgba8(color.r, color.g, color.b, color.a);
    let stroke_style = KStroke::new(width_canvas)
        .with_caps(Cap::Round)
        .with_join(Join::Round);
    scene.stroke(
        &stroke_style,
        world_to_screen,
        &Brush::Solid(pcolor),
        None,
        &path,
    );
}

fn world_to_screen_affine(transform: &ViewportTransform) -> Affine {
    let (sw, sh) = transform.screen_size();
    let center = transform.center();
    let zoom = transform.zoom();
    Affine::translate((sw * 0.5, sh * 0.5))
        * Affine::scale(zoom)
        * Affine::translate((-center.x, -center.y))
}

fn draw_background_pattern(
    scene: &mut Scene,
    transform: &ViewportTransform,
    world_to_screen: Affine,
    bg: &BackgroundConfig,
    page_rect: Rect,
) {
    match bg {
        BackgroundConfig::Blank | BackgroundConfig::Image { .. } | BackgroundConfig::Pdf { .. } => {
            // POC: no image / pdf backgrounds (asset URLs aren't wired).
        }
        BackgroundConfig::Dots { spacing, tiling } => {
            draw_dots(scene, transform, world_to_screen, page_rect, *spacing, *tiling);
        }
        BackgroundConfig::Lines { spacing, tiling } => {
            draw_lines(scene, transform, world_to_screen, page_rect, *spacing, *tiling);
        }
        BackgroundConfig::Grid(g) => {
            draw_grid(scene, transform, world_to_screen, g);
        }
        BackgroundConfig::Isometric { spacing } | BackgroundConfig::Hexagonal { spacing } => {
            // Skip isometric / hexagonal for POC — not visually critical
            // for the demo notebook. Easy to add later.
            let _ = spacing;
        }
    }
}

fn draw_dots(
    scene: &mut Scene,
    transform: &ViewportTransform,
    world_to_screen: Affine,
    page_rect: Rect,
    spacing: f64,
    tiling: bool,
) {
    if spacing <= 0.0 {
        return;
    }
    let zoom = transform.zoom().max(1e-6);
    let r = (1.5 / zoom).clamp(0.05, 3.0);
    let bounds = if tiling {
        transform.visible_canvas_rect()
    } else {
        page_rect
    };
    let color = PColor::from_rgba8(90, 90, 100, 200);
    let mut path = BezPath::new();
    let x_start = (bounds.x / spacing).floor() * spacing;
    let y_start = (bounds.y / spacing).floor() * spacing;
    let x_end = bounds.x + bounds.width;
    let y_end = bounds.y + bounds.height;
    let mut y = y_start;
    while y <= y_end {
        let mut x = x_start;
        while x <= x_end {
            // Approximate each dot as a tiny rect in the path. Cheaper
            // than building an Ellipse per-point and visually
            // indistinguishable at typical zooms for a viewer POC.
            path.extend(
                vello::kurbo::Circle::new((x, y), r)
                    .to_path(0.05)
                    .into_iter(),
            );
            x += spacing;
        }
        y += spacing;
    }
    scene.fill(
        Fill::NonZero,
        world_to_screen,
        &Brush::Solid(color),
        None,
        &path,
    );
}

fn draw_lines(
    scene: &mut Scene,
    transform: &ViewportTransform,
    world_to_screen: Affine,
    page_rect: Rect,
    spacing: f64,
    tiling: bool,
) {
    if spacing <= 0.0 {
        return;
    }
    let zoom = transform.zoom().max(1e-6);
    let bounds = if tiling {
        transform.visible_canvas_rect()
    } else {
        page_rect
    };
    let mut path = BezPath::new();
    let y_start = (bounds.y / spacing).floor() * spacing;
    let y_end = bounds.y + bounds.height;
    let x_start = bounds.x;
    let x_end = bounds.x + bounds.width;
    let mut y = y_start;
    while y <= y_end {
        path.move_to((x_start, y));
        path.line_to((x_end, y));
        y += spacing;
    }
    let stroke = KStroke::new(1.0 / zoom);
    let color = PColor::from_rgba8(90, 90, 100, 200);
    scene.stroke(&stroke, world_to_screen, &Brush::Solid(color), None, &path);
}

fn draw_grid(
    scene: &mut Scene,
    transform: &ViewportTransform,
    world_to_screen: Affine,
    settings: &GridSettings,
) {
    let spacing = settings.base_spacing.max(0.0);
    if spacing <= 0.0 {
        return;
    }
    let zoom = transform.zoom().max(1e-6);
    let bounds = transform.visible_canvas_rect();
    let color = settings.color;
    let pcolor = PColor::from_rgba8(color.r, color.g, color.b, (color.a as f64 * 0.55) as u8);

    let mut path = BezPath::new();
    let x_start = (bounds.x / spacing).floor() * spacing;
    let y_start = (bounds.y / spacing).floor() * spacing;
    let x_end = bounds.x + bounds.width;
    let y_end = bounds.y + bounds.height;
    let mut x = x_start;
    while x <= x_end {
        path.move_to((x, bounds.y));
        path.line_to((x, y_end));
        x += spacing;
    }
    let mut y = y_start;
    while y <= y_end {
        path.move_to((bounds.x, y));
        path.line_to((x_end, y));
        y += spacing;
    }
    let stroke = KStroke::new(0.7 / zoom);
    scene.stroke(&stroke, world_to_screen, &Brush::Solid(pcolor), None, &path);
}

// ---------------------------------------------------------------------
// Background-config helper duplicated from `journal_templates::canvas_bridge`
// so we don't pull `journal-templates`'s `wasm` feature into the viewer
// just to call one function. The code is deliberately verbatim; if it
// drifts, prefer reaching for the canvas_bridge re-export.
// ---------------------------------------------------------------------

fn page_template_background(t: &PageTemplate) -> BackgroundConfig {
    let tiling = matches!(t.tiling, TilingMode::Repeat);
    match &t.background {
        BackgroundType::Blank => BackgroundConfig::Blank,
        BackgroundType::Dots { spacing } => BackgroundConfig::Dots {
            spacing: *spacing,
            tiling,
        },
        BackgroundType::Lines { spacing } => BackgroundConfig::Lines {
            spacing: *spacing,
            tiling,
        },
        BackgroundType::Grid { spacing } => BackgroundConfig::Grid(GridSettings {
            base_spacing: *spacing,
            subdivisions: 4,
            color: journal_core::Color {
                r: 80,
                g: 80,
                b: 90,
                a: 255,
            },
        }),
        BackgroundType::Isometric { spacing } => BackgroundConfig::Isometric { spacing: *spacing },
        BackgroundType::Hexagonal { spacing } => BackgroundConfig::Hexagonal { spacing: *spacing },
        BackgroundType::Image { path } => BackgroundConfig::Image {
            path: std::path::PathBuf::from(path),
            size_canvas: (t.size_mm.0, t.size_mm.1),
        },
        BackgroundType::Pdf { path, page } => BackgroundConfig::Pdf {
            path: std::path::PathBuf::from(path),
            page: *page,
            size_canvas: (t.size_mm.0, t.size_mm.1),
        },
    }
}

fn blank_template() -> PageTemplate {
    PageTemplate {
        id: TemplateId(Uuid::nil()),
        name: String::new(),
        description: String::new(),
        background: BackgroundType::Blank,
        size_mm: (215.9, 279.4),
        tiling: TilingMode::None,
        default_viewport: None,
        widgets: Vec::new(),
        category: String::new(),
    }
}
