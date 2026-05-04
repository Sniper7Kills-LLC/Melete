//! Vello renderer.
//!
//! Builds `vello::Scene` values from `journal_core::Stroke` data and
//! rasterizes them to an offscreen wgpu texture, copying the result back
//! to RAM as RGBA8. The caller (today: `journal-app`'s GLArea overlay)
//! uploads those pixels to a GL texture and presents them on-screen.
//!
//! Per docs/renderer-vello-migration.md §7.2 the renderer holds no GTK
//! references — only `wgpu::Device`/`Queue`/`Renderer` and a reusable
//! offscreen target.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use journal_core::{BlendMode, Color as JColor, Point, Rect, Stroke};
use uuid::Uuid;
use vello::kurbo::{Affine, BezPath, Cap, Circle, Join, Rect as KRect, Shape, Stroke as KStroke};
use vello::peniko::{
    BlendMode as PBlendMode, Blob, Brush, Color, Compose, Fill, ImageAlphaType, ImageBrush,
    ImageData, ImageFormat, Mix,
};

type PImage = ImageBrush;

use crate::background_renderer::BackgroundConfig;
use crate::grid_renderer::GridSettings;
use vello::wgpu;
use vello::{AaConfig, RenderParams, Renderer, RendererOptions, Scene};

use crate::viewport_transform::ViewportTransform;

// ---------------------------------------------------------------------------
// Per-brush-style tuning parameters
// ---------------------------------------------------------------------------
//
// Each brush style's renderer has a handful of magic numbers (width
// floors, halo alphas, dot densities, nib angles, resample density…)
// that visibly change how the brush feels. These structs expose them
// so the developer-mode tool-settings dialog can edit them at runtime.
// `ToolStyleParams::default()` reproduces the values that were hardcoded
// before this struct existed, so callers that don't pass an override
// see the same output as before.

/// Tip geometry for the Pen / Highlighter family. `Round` is the
/// default — single smooth path stroked with a round-cap circle. `Flat`
/// turns the stroke into a calligraphy-lite variable-width polygon
/// (always perpendicular to the path). `Marker` is a fixed
/// chunky-tipped bookbinder marker.
#[derive(Debug, Clone, Copy, PartialEq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PenShape {
    #[default]
    Round,
    Flat,
    Marker,
}

#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct PenParams {
    #[serde(default)]
    pub shape: PenShape,
    pub width_floor: f64,
    pub width_pressure_amplitude: f64,
    /// Used by `PenShape::Marker` — multiplier on `base_width` for the
    /// chunky marker tip.
    #[serde(default = "default_marker_mult")]
    pub marker_width_mult: f64,
}
fn default_marker_mult() -> f64 {
    1.8
}
impl Default for PenParams {
    fn default() -> Self {
        Self {
            shape: PenShape::Round,
            width_floor: 0.6,
            width_pressure_amplitude: 0.4,
            marker_width_mult: 1.8,
        }
    }
}

/// Pencil-tip shape. `Cylindrical` is a normal pencil point.
/// `Carpenter` simulates a flat carpenter pencil — wider, with width
/// modulated by stroke direction (similar to calligraphy's flat-cut).
/// `Mechanical` is a very thin precision tip with no tilt shading.
#[derive(Debug, Clone, Copy, PartialEq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PencilShape {
    #[default]
    Cylindrical,
    Carpenter,
    Mechanical,
}

#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct PencilParams {
    #[serde(default)]
    pub shape: PencilShape,
    pub core_clamp_min: f64,
    pub core_clamp_max: f64,
    pub tilt_threshold: f64,
    pub tilt_band_mult: f64,
    pub tilt_alpha_scale: f64,
    /// Carpenter-shape: maximum width as a multiple of `base_width`.
    #[serde(default = "default_carpenter_mult")]
    pub carpenter_width_mult: f64,
}
fn default_carpenter_mult() -> f64 {
    2.0
}
impl Default for PencilParams {
    fn default() -> Self {
        Self {
            shape: PencilShape::Cylindrical,
            core_clamp_min: 0.4,
            core_clamp_max: 0.9,
            tilt_threshold: 0.12,
            tilt_band_mult: 8.0,
            tilt_alpha_scale: 0.22,
            carpenter_width_mult: 2.0,
        }
    }
}

/// Paintbrush bristle shape. `Round` is the default 3-pass halo+core
/// (current). `Flat` is a single hard-edge wide stroke — like a flat
/// sumi brush. `Fan` lays down 3 parallel offset strokes spread
/// perpendicular to the stroke direction (fan-bristle effect).
#[derive(Debug, Clone, Copy, PartialEq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PaintbrushShape {
    #[default]
    Round,
    Flat,
    Fan,
}

#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct PaintbrushParams {
    #[serde(default)]
    pub shape: PaintbrushShape,
    pub halo_width_mult: f64,
    pub outer_halo_mult: f64,
    pub mid_halo_mult: f64,
    pub outer_alpha: f64,
    pub mid_alpha: f64,
    pub core_alpha: f64,
    /// Fan-shape: number of parallel tines.
    #[serde(default = "default_fan_count")]
    pub fan_count: u32,
    /// Fan-shape: spread perpendicular to stroke as a multiple of
    /// stroke width (1.0 = the full bristle spread equals the brush
    /// width).
    #[serde(default = "default_fan_spread")]
    pub fan_spread_mult: f64,
}
fn default_fan_count() -> u32 {
    3
}
fn default_fan_spread() -> f64 {
    1.4
}
impl Default for PaintbrushParams {
    fn default() -> Self {
        Self {
            shape: PaintbrushShape::Round,
            halo_width_mult: 1.6,
            outer_halo_mult: 1.4,
            mid_halo_mult: 0.95,
            outer_alpha: 0.07,
            mid_alpha: 0.20,
            core_alpha: 0.95,
            fan_count: 3,
            fan_spread_mult: 1.4,
        }
    }
}

/// Spray-can scatter shape. `Circle` (default) — uniform-radius
/// circular spread. `Square` stamps small squares instead of circles.
/// `Cone` biases scatter direction along the stylus tilt vector,
/// simulating an angled airbrush.
#[derive(Debug, Clone, Copy, PartialEq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SprayShape {
    #[default]
    Circle,
    Square,
    Cone,
}

#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SprayParams {
    #[serde(default)]
    pub shape: SprayShape,
    pub dots_per_point: u32,
    pub dot_radius_factor: f64,
    pub min_dot_radius: f64,
    /// Cone-shape: half-angle of the cone in degrees. Smaller =
    /// tighter directional spray.
    #[serde(default = "default_cone_spread")]
    pub cone_spread_deg: f64,
}
fn default_cone_spread() -> f64 {
    35.0
}
impl Default for SprayParams {
    fn default() -> Self {
        Self {
            shape: SprayShape::Circle,
            dots_per_point: 36,
            dot_radius_factor: 0.06,
            min_dot_radius: 0.35,
            cone_spread_deg: 35.0,
        }
    }
}

/// Calligraphy nib shape. `FlatCut` (default) is the angled italic
/// nib — width varies by direction relative to the nib axis. `Round`
/// is a constant-width round nib (no angle bias). `BrushNib` is a
/// soft brush-tip with width driven by pen pressure rather than
/// stroke direction.
#[derive(Debug, Clone, Copy, PartialEq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CalligraphyShape {
    #[default]
    FlatCut,
    Round,
    BrushNib,
}

#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CalligraphyParams {
    #[serde(default)]
    pub shape: CalligraphyShape,
    pub nib_angle_deg: f64,
    pub min_ratio: f64,
    pub resample_step_mult: f64,
    pub smooth_outline: bool,
}
impl Default for CalligraphyParams {
    fn default() -> Self {
        Self {
            shape: CalligraphyShape::FlatCut,
            nib_angle_deg: 45.0,
            min_ratio: 0.18,
            resample_step_mult: 0.5,
            smooth_outline: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default, serde::Serialize, serde::Deserialize)]
pub struct ToolStyleParams {
    pub pen: PenParams,
    pub pencil: PencilParams,
    pub paintbrush: PaintbrushParams,
    pub spray: SprayParams,
    pub calligraphy: CalligraphyParams,
}

/// Screen-space overlay state passed to the renderer for the on-canvas
/// overlays (selection handles, lasso, brush cursor, page-bounds outline).
/// The caller computes pointer-derived values (cursor radius, drawing
/// state) before passing in — this keeps tool/state lookups out of the
/// renderer crate.
#[derive(Clone)]
pub struct OverlayState {
    pub selection_bbox: Option<Rect>,
    pub lasso_screen_points: Vec<(f64, f64)>,
    pub pointer_screen: Option<(f64, f64)>,
    pub pointer_drawing: bool,
    pub cursor_radius: f64,
    pub cursor_color: JColor,
    pub cursor_opacity: f32,
    pub show_page_bounds: bool,
    pub dark_mode: bool,
    /// Cursor outline shape. `None` → default circle (legacy behaviour).
    /// `Some(CursorShape)` overrides; `ExactTip` reads from `cursor_tip`.
    pub cursor_shape: Option<journal_core::CursorShape>,
    /// Tip shape used when `cursor_shape == Some(ExactTip)` or when
    /// `Auto` resolves to the first layer's tip. Null → fallback to
    /// circle.
    pub cursor_tip: Option<journal_core::TipShape>,
    /// Multiplier on the entire scene's alpha — `1.0` = fully visible.
    /// Used by the canvas to fade-in a freshly-loaded page surface
    /// over a couple of frames after `current_page_id` changes
    /// (audit §9). Values < 1.0 wrap the page-fill / background /
    /// widgets / strokes / overlays in a single `push_layer` so the
    /// fade applies to everything as one composite.
    pub fade_alpha: f32,
}

impl Default for OverlayState {
    fn default() -> Self {
        Self {
            selection_bbox: None,
            lasso_screen_points: Vec::new(),
            pointer_screen: None,
            pointer_drawing: false,
            cursor_radius: 5.0,
            cursor_color: JColor {
                r: 0,
                g: 0,
                b: 0,
                a: 255,
            },
            cursor_opacity: 1.0,
            show_page_bounds: false,
            dark_mode: false,
            cursor_shape: None,
            cursor_tip: None,
            fade_alpha: 1.0,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum VelloError {
    #[error("vello init failed: {0}")]
    Init(String),
    #[error("vello render failed: {0}")]
    Render(String),
}

const TEXTURE_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
const COPY_ALIGN: u32 = 256;

pub struct VelloRenderer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    renderer: Renderer,
    target: Option<Target>,
    scene: Scene,
    /// Cache of decoded raster images keyed by absolute path. peniko::Image
    /// is itself ref-counted (via `Blob<u8>`'s Arc) so cloning is cheap.
    image_cache: HashMap<PathBuf, PImage>,
    /// Cache of rasterized PDF pages.
    #[cfg(feature = "pdf")]
    pdf_cache: HashMap<(PathBuf, u32), PImage>,
}

struct Target {
    w: u32,
    h: u32,
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    readback: wgpu::Buffer,
    bytes_per_row: u32,
}

impl VelloRenderer {
    pub fn new() -> Result<Self, VelloError> {
        // Force Vulkan-only on Linux. GLArea elsewhere in the process owns the
        // EGL/GLES context; if wgpu picks the GLES backend it fights GTK's
        // current-context state and triggers
        // `epoxy_get_proc_address` "no current context" assertions.
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::VULKAN,
            ..Default::default()
        });
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: None,
            force_fallback_adapter: false,
        }))
        .map_err(|e| VelloError::Init(format!("no adapter: {e}")))?;
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("vello-device"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::default(),
            ..Default::default()
        }))
        .map_err(|e| VelloError::Init(format!("device: {e}")))?;
        let renderer = Renderer::new(&device, RendererOptions::default())
            .map_err(|e| VelloError::Init(format!("renderer: {e:?}")))?;
        Ok(Self {
            device,
            queue,
            renderer,
            target: None,
            scene: Scene::new(),
            image_cache: HashMap::new(),
            #[cfg(feature = "pdf")]
            pdf_cache: HashMap::new(),
        })
    }

    fn ensure_image_for_bg(&mut self, path: &Path) -> Option<PImage> {
        if let Some(img) = self.image_cache.get(path) {
            return Some(img.clone());
        }
        let dyn_img = match image::ImageReader::open(path) {
            Ok(r) => match r.with_guessed_format().map(|r| r.decode()) {
                Ok(Ok(d)) => d,
                Ok(Err(e)) => {
                    tracing::warn!("decode {:?}: {}", path, e);
                    return None;
                }
                Err(e) => {
                    tracing::warn!("guess format {:?}: {}", path, e);
                    return None;
                }
            },
            Err(e) => {
                tracing::warn!("open image {:?}: {}", path, e);
                return None;
            }
        };
        let rgba = dyn_img.to_rgba8();
        let (w, h) = rgba.dimensions();
        let bytes = rgba.into_raw();
        let blob = Blob::new(Arc::new(bytes));
        let data = ImageData {
            data: blob,
            format: ImageFormat::Rgba8,
            alpha_type: ImageAlphaType::Alpha,
            width: w,
            height: h,
        };
        let image = PImage::new(data);
        self.image_cache.insert(path.to_path_buf(), image.clone());
        Some(image)
    }

    #[cfg(feature = "pdf")]
    fn ensure_pdf_for_bg(&mut self, path: &Path, page_idx: u32) -> Option<PImage> {
        let key = (path.to_path_buf(), page_idx);
        if let Some(img) = self.pdf_cache.get(&key) {
            return Some(img.clone());
        }
        let bytes = render_pdf_page_to_rgba8(path, page_idx)?;
        let blob = Blob::new(Arc::new(bytes.bytes));
        let data = ImageData {
            data: blob,
            format: ImageFormat::Rgba8,
            alpha_type: ImageAlphaType::Alpha,
            width: bytes.width,
            height: bytes.height,
        };
        let image = PImage::new(data);
        self.pdf_cache.insert(key, image.clone());
        Some(image)
    }

    /// Render a stand-alone placeholder scene (no canvas content). Caller
    /// supplies a closure to draw whatever it wants — typically a centered
    /// "select a page" message via `journal_widgets::WidgetRenderer::draw_placeholder`.
    pub fn render_placeholder<F>(
        &mut self,
        w: u32,
        h: u32,
        _dark_mode: bool,
        scene_fn: F,
    ) -> Result<Vec<u8>, VelloError>
    where
        F: FnOnce(&mut Scene, u32, u32),
    {
        if w == 0 || h == 0 {
            return Ok(Vec::new());
        }
        self.ensure_target(w, h);
        self.scene.reset();
        scene_fn(&mut self.scene, w, h);
        let target = self.target.as_ref().expect("target after ensure");
        self.renderer
            .render_to_texture(
                &self.device,
                &self.queue,
                &self.scene,
                &target.view,
                &RenderParams {
                    base_color: Color::from_rgba8(0, 0, 0, 0),
                    width: w,
                    height: h,
                    antialiasing_method: AaConfig::Area,
                },
            )
            .map_err(|e| VelloError::Render(format!("render_to_texture: {e:?}")))?;
        readback_to_rgba(&self.device, &self.queue, target, w, h)
    }

    pub fn render_rgba<F>(
        &mut self,
        transform: &ViewportTransform,
        background: &BackgroundConfig,
        page_rect: Rect,
        strokes: &[Stroke],
        selected_ids: &HashSet<Uuid>,
        overlays: &OverlayState,
        brush_params: &ToolStyleParams,
        w: u32,
        h: u32,
        widgets_draw: F,
    ) -> Result<Vec<u8>, VelloError>
    where
        F: FnOnce(&mut Scene, Affine, Rect),
    {
        if w == 0 || h == 0 {
            return Ok(Vec::new());
        }
        // Resolve any image-shaped background to a peniko::Image up front so
        // build_scene doesn't need a mutable self borrow (the cache lives on
        // self alongside the scene).
        let bg_image = match background {
            BackgroundConfig::Image { path, .. } => self.ensure_image_for_bg(path),
            #[cfg(feature = "pdf")]
            BackgroundConfig::Pdf { path, page, .. } => self.ensure_pdf_for_bg(path, *page),
            _ => None,
        };
        self.ensure_target(w, h);
        self.scene.reset();
        build_scene(
            &mut self.scene,
            transform,
            background,
            page_rect,
            strokes,
            selected_ids,
            overlays,
            brush_params,
            bg_image.as_ref(),
            widgets_draw,
        );

        let target = self.target.as_ref().expect("target after ensure");
        self.renderer
            .render_to_texture(
                &self.device,
                &self.queue,
                &self.scene,
                &target.view,
                &RenderParams {
                    base_color: Color::from_rgba8(0, 0, 0, 0),
                    width: w,
                    height: h,
                    antialiasing_method: AaConfig::Area,
                },
            )
            .map_err(|e| VelloError::Render(format!("render_to_texture: {e:?}")))?;

        readback_to_rgba(&self.device, &self.queue, target, w, h)
    }

    fn ensure_target(&mut self, w: u32, h: u32) {
        if let Some(t) = &self.target {
            if t.w == w && t.h == h {
                return;
            }
        }
        let bytes_per_row = align_up(w * 4, COPY_ALIGN);
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("vello-target"),
            size: wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: TEXTURE_FORMAT,
            usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let readback = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("vello-readback"),
            size: (bytes_per_row as u64) * (h as u64),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        self.target = Some(Target {
            w,
            h,
            texture,
            view,
            readback,
            bytes_per_row,
        });
    }
}

fn align_up(v: u32, a: u32) -> u32 {
    v.div_ceil(a) * a
}

#[cfg(feature = "pdf")]
struct PdfRgba8 {
    bytes: Vec<u8>,
    width: u32,
    height: u32,
}

#[cfg(feature = "pdf")]
fn render_pdf_page_to_rgba8(path: &Path, page_idx: u32) -> Option<PdfRgba8> {
    use gtk4::cairo;
    use poppler::Document;
    let abs = match path.canonicalize() {
        Ok(p) => p,
        Err(e) => {
            tracing::warn!("canonicalize {:?}: {}", path, e);
            return None;
        }
    };
    let uri = format!("file://{}", abs.display());
    let doc = match Document::from_file(&uri, None) {
        Ok(d) => d,
        Err(e) => {
            tracing::warn!("poppler open {}: {}", uri, e);
            return None;
        }
    };
    let page = doc.page(page_idx as i32)?;
    let (w_pts, h_pts) = page.size();
    let scale = 2.0;
    let pw = (w_pts * scale).ceil().max(1.0) as i32;
    let ph = (h_pts * scale).ceil().max(1.0) as i32;
    let mut surface = cairo::ImageSurface::create(cairo::Format::ARgb32, pw, ph).ok()?;
    {
        let ctx = cairo::Context::new(&surface).ok()?;
        ctx.set_source_rgb(1.0, 1.0, 1.0);
        let _ = ctx.paint();
        ctx.scale(scale, scale);
        page.render(&ctx);
    }
    // Cairo finished painting; extract bytes. Format::ARgb32 on little-endian
    // = native u32 0xAARRGGBB → bytes B,G,R,A premultiplied. Convert to
    // straight-alpha RGBA8 (peniko::ImageFormat::Rgba8 + AlphaType::Alpha).
    surface.flush();
    let stride = surface.stride() as usize;
    let data = surface.data().ok()?;
    let mut rgba = Vec::with_capacity((pw * ph * 4) as usize);
    let row_bytes = (pw as usize) * 4;
    for y in 0..(ph as usize) {
        let row = &data[y * stride..y * stride + row_bytes];
        for x in 0..(pw as usize) {
            let b = row[x * 4] as u32;
            let g = row[x * 4 + 1] as u32;
            let r = row[x * 4 + 2] as u32;
            let a = row[x * 4 + 3] as u32;
            let (r_un, g_un, b_un) = match a {
                0 => (0u8, 0u8, 0u8),
                _ => (
                    ((r * 255 + a / 2) / a).min(255) as u8,
                    ((g * 255 + a / 2) / a).min(255) as u8,
                    ((b * 255 + a / 2) / a).min(255) as u8,
                ),
            };
            rgba.push(r_un);
            rgba.push(g_un);
            rgba.push(b_un);
            rgba.push(a as u8);
        }
    }
    drop(data);
    Some(PdfRgba8 {
        bytes: rgba,
        width: pw as u32,
        height: ph as u32,
    })
}

fn readback_to_rgba(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    target: &Target,
    w: u32,
    h: u32,
) -> Result<Vec<u8>, VelloError> {
    let mut encoder =
        device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture: &target.texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &target.readback,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(target.bytes_per_row),
                rows_per_image: Some(h),
            },
        },
        wgpu::Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        },
    );
    queue.submit(std::iter::once(encoder.finish()));

    let buf_slice = target.readback.slice(..);
    let (sender, receiver) = std::sync::mpsc::channel();
    buf_slice.map_async(wgpu::MapMode::Read, move |r| {
        let _ = sender.send(r);
    });
    device
        .poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: None,
        })
        .map_err(|e| VelloError::Render(format!("poll: {e:?}")))?;
    receiver
        .recv()
        .map_err(|e| VelloError::Render(format!("map recv: {e}")))?
        .map_err(|e| VelloError::Render(format!("map: {e:?}")))?;

    let data = buf_slice.get_mapped_range();
    let bpr = target.bytes_per_row as usize;
    let row_bytes = (w as usize) * 4;
    let mut out = Vec::with_capacity(row_bytes * (h as usize));
    for row in 0..(h as usize) {
        let start = row * bpr;
        out.extend_from_slice(&data[start..start + row_bytes]);
    }
    drop(data);
    target.readback.unmap();
    Ok(out)
}

// ---------------------------------------------------------------------------
// Scene building
// ---------------------------------------------------------------------------

fn build_scene<F>(
    scene: &mut Scene,
    transform: &ViewportTransform,
    background: &BackgroundConfig,
    page_rect: Rect,
    strokes: &[Stroke],
    selected_ids: &HashSet<Uuid>,
    overlays: &OverlayState,
    brush_params: &ToolStyleParams,
    bg_image: Option<&PImage>,
    widgets_draw: F,
) where
    F: FnOnce(&mut Scene, Affine, Rect),
{
    let dark_mode = overlays.dark_mode;
    let world_to_screen = world_to_screen_affine(transform);

    // 1) Page colour fill — covers entire visible viewport (screen-space).
    let (sw, sh) = transform.screen_size();
    let bg_color = if dark_mode {
        // Editorial fieldbook dark — dim teal, holds amber accents.
        Color::from_rgba8(28, 42, 48, 255)
    } else {
        // Editorial fieldbook light — fieldbook-paper cream.
        Color::from_rgba8(244, 239, 226, 255)
    };
    scene.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        &Brush::Solid(bg_color),
        None,
        &KRect::new(0.0, 0.0, sw, sh),
    );

    // Page-change fade: when `fade_alpha < 1.0`, wrap the rest of the
    // scene (background pattern + widgets + strokes + overlays) in a
    // single `push_layer` so the new page surface fades in over a few
    // frames. Audit §9. The page-fill above sits *outside* the layer
    // — we want the cream/teal page colour fully opaque from frame 0
    // so the fade reads as "ink lands" not "page colour ramps in".
    let fade = overlays.fade_alpha.clamp(0.0, 1.0);
    let faded = fade < 0.999;
    if faded {
        scene.push_layer(
            Fill::NonZero,
            vello::peniko::Mix::Normal,
            fade,
            Affine::IDENTITY,
            &KRect::new(0.0, 0.0, sw, sh),
        );
    }

    // 2) Background pattern.
    draw_background_pattern(
        scene,
        transform,
        world_to_screen,
        background,
        page_rect,
        bg_image,
    );

    // 3) Widgets — caller-supplied draw callback. journal-widgets fills
    //    this in with parley-laid-out template content; tests / viewer
    //    callers can also no-op it.
    widgets_draw(scene, world_to_screen, page_rect);

    // Group by blend mode so each non-Normal pass gets a single layer
    // wrapping every stroke that uses it. Per-stroke push/pop_layer would
    // be O(strokes) layers — prohibitive on dense pages.
    let mut normal: Vec<&Stroke> = Vec::new();
    let mut multiply: Vec<&Stroke> = Vec::new();
    let mut screen_b: Vec<&Stroke> = Vec::new();
    let mut overlay: Vec<&Stroke> = Vec::new();
    let mut darken: Vec<&Stroke> = Vec::new();
    let mut lighten: Vec<&Stroke> = Vec::new();
    let mut erase: Vec<&Stroke> = Vec::new();

    for s in strokes {
        if s.points.is_empty() {
            continue;
        }
        match s.pen.blend_mode {
            BlendMode::Normal => normal.push(s),
            BlendMode::Multiply => multiply.push(s),
            BlendMode::Screen => screen_b.push(s),
            BlendMode::Overlay => overlay.push(s),
            BlendMode::Darken => darken.push(s),
            BlendMode::Lighten => lighten.push(s),
            BlendMode::Erase => erase.push(s),
        }
    }

    // Selection highlight underlay — wider light-blue trace under any
    // selected stroke. Drawn before the stroke itself so the brush mark
    // sits on top of the highlight.
    if !selected_ids.is_empty() {
        let zoom = transform.zoom().max(1e-6);
        for s in strokes {
            if !selected_ids.contains(&s.id) {
                continue;
            }
            draw_selection_underlay(scene, world_to_screen, s, zoom);
        }
    }

    for s in &normal {
        draw_stroke(scene, world_to_screen, s, brush_params);
    }

    let coverage = full_coverage_path();
    for (group, mode) in [
        (multiply, PBlendMode::new(Mix::Multiply, Compose::SrcOver)),
        (screen_b, PBlendMode::new(Mix::Screen, Compose::SrcOver)),
        (overlay, PBlendMode::new(Mix::Overlay, Compose::SrcOver)),
        (darken, PBlendMode::new(Mix::Darken, Compose::SrcOver)),
        (lighten, PBlendMode::new(Mix::Lighten, Compose::SrcOver)),
        (erase, PBlendMode::new(Mix::Normal, Compose::DestOut)),
    ] {
        if group.is_empty() {
            continue;
        }
        scene.push_layer(Fill::NonZero, mode, 1.0_f32, Affine::IDENTITY, &coverage);
        for s in &group {
            draw_stroke(scene, world_to_screen, s, brush_params);
        }
        scene.pop_layer();
    }

    // Overlays (last → on top): page-bounds outline (canvas-space), then
    // selection handles + lasso + brush cursor (screen-space).
    if overlays.show_page_bounds {
        draw_page_bounds_overlay(
            scene,
            transform,
            world_to_screen,
            background,
            page_rect,
            dark_mode,
        );
    }
    if let Some(bbox) = overlays.selection_bbox {
        draw_selection_handles_overlay(scene, transform, bbox);
    }
    if overlays.lasso_screen_points.len() >= 2 {
        draw_lasso_overlay_scene(scene, &overlays.lasso_screen_points);
    }
    if let Some((px, py)) = overlays.pointer_screen {
        draw_brush_cursor_overlay(scene, px, py, overlays);
    }

    if faded {
        scene.pop_layer();
    }
}

// ---------------------------------------------------------------------------
// Overlay rendering (screen-space and canvas-space)
// ---------------------------------------------------------------------------

const HANDLE_SIZE: f64 = 8.0;

fn draw_page_bounds_overlay(
    scene: &mut Scene,
    transform: &ViewportTransform,
    world_to_screen: Affine,
    background: &BackgroundConfig,
    page_rect: Rect,
    dark_mode: bool,
) {
    if matches!(
        background,
        BackgroundConfig::Grid(_)
            | BackgroundConfig::Isometric { .. }
            | BackgroundConfig::Hexagonal { .. }
            | BackgroundConfig::Dots { tiling: true, .. }
            | BackgroundConfig::Lines { tiling: true, .. }
    ) {
        return;
    }
    let visible = transform.visible_canvas_rect();
    let extends_beyond = visible.x < page_rect.x
        || visible.y < page_rect.y
        || visible.x + visible.width > page_rect.x + page_rect.width
        || visible.y + visible.height > page_rect.y + page_rect.height;
    if !extends_beyond {
        return;
    }
    let zoom = transform.zoom().max(1e-6);
    let line_w = 1.0 / zoom;
    let path = KRect::new(
        page_rect.x,
        page_rect.y,
        page_rect.x + page_rect.width,
        page_rect.y + page_rect.height,
    );
    let color = if dark_mode {
        Color::from_rgba8(128, 128, 128, 102) // 0.4 alpha
    } else {
        Color::from_rgba8(153, 153, 153, 128) // 0.5 alpha
    };
    let style = KStroke::new(line_w);
    scene.stroke(&style, world_to_screen, &Brush::Solid(color), None, &path);
}

fn draw_selection_handles_overlay(
    scene: &mut Scene,
    transform: &ViewportTransform,
    selection_bbox: Rect,
) {
    let to_screen =
        |cx: f64, cy: f64| -> (f64, f64) { transform.canvas_to_screen(Point { x: cx, y: cy }) };
    let bb = selection_bbox;
    let mx = bb.x + bb.width * 0.5;
    let my = bb.y + bb.height * 0.5;
    let anchors = [
        to_screen(bb.x, bb.y),
        to_screen(mx, bb.y),
        to_screen(bb.x + bb.width, bb.y),
        to_screen(bb.x + bb.width, my),
        to_screen(bb.x + bb.width, bb.y + bb.height),
        to_screen(mx, bb.y + bb.height),
        to_screen(bb.x, bb.y + bb.height),
        to_screen(bb.x, my),
    ];
    let h = HANDLE_SIZE * 0.5;
    let fill_brush = Brush::Solid(Color::from_rgba8(255, 255, 255, 230));
    let stroke_brush = Brush::Solid(Color::from_rgba8(51, 128, 255, 255));
    let style = KStroke::new(1.5);
    for &(sx, sy) in &anchors {
        let path = KRect::new(sx - h, sy - h, sx + h, sy + h);
        scene.fill(Fill::NonZero, Affine::IDENTITY, &fill_brush, None, &path);
        scene.stroke(&style, Affine::IDENTITY, &stroke_brush, None, &path);
    }
}

fn draw_lasso_overlay_scene(scene: &mut Scene, points: &[(f64, f64)]) {
    let mut path = BezPath::new();
    path.move_to(points[0]);
    for &p in &points[1..] {
        path.line_to(p);
    }
    path.close_path();
    let fill_brush = Brush::Solid(Color::from_rgba8(51, 128, 255, 38)); // 0.15
    let stroke_brush = Brush::Solid(Color::from_rgba8(51, 128, 255, 153)); // 0.6
    scene.fill(Fill::NonZero, Affine::IDENTITY, &fill_brush, None, &path);
    let style = KStroke::new(1.5);
    scene.stroke(&style, Affine::IDENTITY, &stroke_brush, None, &path);
}

fn draw_brush_cursor_overlay(scene: &mut Scene, px: f64, py: f64, ov: &OverlayState) {
    let radius = ov.cursor_radius.max(2.0);
    let outline = build_cursor_outline(px, py, radius, ov);

    if ov.pointer_drawing {
        let alpha = (ov.cursor_opacity.clamp(0.2, 1.0) * 255.0) as u8;
        let fill = Brush::Solid(Color::from_rgba8(
            ov.cursor_color.r,
            ov.cursor_color.g,
            ov.cursor_color.b,
            alpha,
        ));
        scene.fill(Fill::NonZero, Affine::IDENTITY, &fill, None, &outline);
    }
    let (ring_color, halo_color) = if ov.dark_mode {
        (
            Color::from_rgba8(255, 255, 255, 217),
            Color::from_rgba8(0, 0, 0, 102),
        )
    } else {
        (
            Color::from_rgba8(0, 0, 0, 191),
            Color::from_rgba8(255, 255, 255, 153),
        )
    };
    let ring_style = KStroke::new(1.25);
    scene.stroke(
        &ring_style,
        Affine::IDENTITY,
        &Brush::Solid(ring_color),
        None,
        &outline,
    );
    let halo_style = KStroke::new(0.5);
    // Halo is the same outline scaled up slightly. For non-circular
    // shapes we draw the same path again — simpler and visually fine.
    scene.stroke(
        &halo_style,
        Affine::IDENTITY,
        &Brush::Solid(halo_color),
        None,
        &outline,
    );
}

/// Build the cursor outline path based on `OverlayState.cursor_shape`.
/// Falls back to a circle when shape is `None` or unresolved.
fn build_cursor_outline(px: f64, py: f64, radius: f64, ov: &OverlayState) -> BezPath {
    use journal_core::CursorShape as CS;
    match ov.cursor_shape.as_ref() {
        None | Some(CS::Auto) | Some(CS::Circle) => Circle::new((px, py), radius).to_path(0.05),
        Some(CS::Oval { aspect }) => {
            let asp = aspect.max(0.05);
            let mut p = BezPath::new();
            // Approximate ellipse with 32 segments.
            let n = 32;
            for i in 0..=n {
                let t = (i as f64) * std::f64::consts::TAU / (n as f64);
                let x = px + radius * t.cos();
                let y = py + radius * asp * t.sin();
                if i == 0 {
                    p.move_to((x, y));
                } else {
                    p.line_to((x, y));
                }
            }
            p.close_path();
            p
        }
        Some(CS::ExactTip) => {
            if let Some(tip) = ov.cursor_tip.as_ref() {
                tip_polygon(tip, (px, py), radius)
            } else {
                Circle::new((px, py), radius).to_path(0.05)
            }
        }
        Some(CS::Custom { points }) => {
            if points.len() < 3 {
                return Circle::new((px, py), radius).to_path(0.05);
            }
            let mut p = BezPath::new();
            for (i, (ux, uy)) in points.iter().enumerate() {
                let x = px + ux * radius;
                let y = py + uy * radius;
                if i == 0 {
                    p.move_to((x, y));
                } else {
                    p.line_to((x, y));
                }
            }
            p.close_path();
            p
        }
    }
}

// ---------------------------------------------------------------------------
// Backgrounds
// ---------------------------------------------------------------------------

const PATTERN_COLOR: Color = Color::from_rgba8(90, 90, 100, 200);

fn draw_background_pattern(
    scene: &mut Scene,
    transform: &ViewportTransform,
    world_to_screen: Affine,
    config: &BackgroundConfig,
    page_rect: Rect,
    bg_image: Option<&PImage>,
) {
    match config {
        BackgroundConfig::Blank => {}
        BackgroundConfig::Grid(settings) => {
            draw_grid_lines(scene, transform, world_to_screen, settings)
        }
        BackgroundConfig::Lines { spacing, tiling } => {
            draw_horizontal_lines(scene, transform, world_to_screen, *spacing, *tiling)
        }
        BackgroundConfig::Dots { spacing, tiling } => {
            draw_dots(scene, transform, world_to_screen, *spacing, *tiling)
        }
        BackgroundConfig::Isometric { spacing } => {
            draw_isometric_lines(scene, transform, world_to_screen, *spacing)
        }
        BackgroundConfig::Hexagonal { spacing } => {
            draw_hexagonal_lines(scene, transform, world_to_screen, *spacing)
        }
        BackgroundConfig::Image { size_canvas, .. } => {
            if let Some(image) = bg_image {
                draw_image_bg(scene, world_to_screen, image, page_rect, *size_canvas);
            }
        }
        BackgroundConfig::Pdf { size_canvas, .. } => {
            if let Some(image) = bg_image {
                draw_image_bg(scene, world_to_screen, image, page_rect, *size_canvas);
            }
        }
    }
}

fn draw_image_bg(
    scene: &mut Scene,
    world_to_screen: Affine,
    image: &PImage,
    page_rect: Rect,
    size_canvas: (f64, f64),
) {
    let target_w = page_rect.width.max(size_canvas.0);
    let target_h = page_rect.height.max(size_canvas.1);
    let img_w = image.image.width as f64;
    let img_h = image.image.height as f64;
    if img_w <= 0.0 || img_h <= 0.0 || target_w <= 0.0 || target_h <= 0.0 {
        return;
    }
    let sx = target_w / img_w;
    let sy = target_h / img_h;
    let local = Affine::translate((page_rect.x, page_rect.y)) * Affine::scale_non_uniform(sx, sy);
    scene.draw_image(image, world_to_screen * local);
}

fn draw_grid_lines(
    scene: &mut Scene,
    transform: &ViewportTransform,
    world_to_screen: Affine,
    settings: &GridSettings,
) {
    if settings.base_spacing <= 0.0 {
        return;
    }
    let visible = transform.visible_canvas_rect();
    let zoom = transform.zoom().max(1e-6);
    let spacing = settings.base_spacing;
    let sub = settings.subdivisions.max(1);
    let line_w = 1.0 / zoom;
    let major_w = 2.0 / zoom;
    let color = Color::from_rgba8(
        settings.color.r,
        settings.color.g,
        settings.color.b,
        settings.color.a,
    );
    let brush = Brush::Solid(color);

    let start_x = (visible.x / spacing).floor() * spacing;
    let end_x = visible.x + visible.width;
    let start_y = (visible.y / spacing).floor() * spacing;
    let end_y = visible.y + visible.height;
    let start_x_index = (start_x / spacing).round() as i64;
    let start_y_index = (start_y / spacing).round() as i64;

    let mut minor = BezPath::new();
    let mut major = BezPath::new();
    let mut x = start_x;
    let mut i = start_x_index;
    while x <= end_x {
        let path = if sub > 1 && i.rem_euclid(sub as i64) == 0 {
            &mut major
        } else {
            &mut minor
        };
        path.move_to((x, visible.y));
        path.line_to((x, visible.y + visible.height));
        x += spacing;
        i += 1;
    }
    let mut y = start_y;
    let mut j = start_y_index;
    while y <= end_y {
        let path = if sub > 1 && j.rem_euclid(sub as i64) == 0 {
            &mut major
        } else {
            &mut minor
        };
        path.move_to((visible.x, y));
        path.line_to((visible.x + visible.width, y));
        y += spacing;
        j += 1;
    }

    let minor_style = KStroke::new(line_w);
    scene.stroke(&minor_style, world_to_screen, &brush, None, &minor);
    if sub > 1 {
        let major_style = KStroke::new(major_w);
        scene.stroke(&major_style, world_to_screen, &brush, None, &major);
    }
}

fn draw_horizontal_lines(
    scene: &mut Scene,
    transform: &ViewportTransform,
    world_to_screen: Affine,
    spacing: f64,
    tiling: bool,
) {
    if spacing <= 0.0 {
        return;
    }
    let zoom = transform.zoom().max(1e-6);
    let line_w = 1.0 / zoom;
    // `tiling` is plumbed through from BackgroundConfig::Lines but the
    // non-tiling clip-to-page path isn't implemented; both branches
    // used the visible-canvas rect. Kept on the signature so callers
    // can switch once the page-rect clip lands.
    let _ = tiling;
    let bounds = transform.visible_canvas_rect();
    let y_start = (bounds.y / spacing).floor() * spacing;
    let y_end = bounds.y + bounds.height;
    let x_start = bounds.x;
    let x_end = bounds.x + bounds.width;

    let mut path = BezPath::new();
    let mut y = y_start;
    while y <= y_end {
        path.move_to((x_start, y));
        path.line_to((x_end, y));
        y += spacing;
    }
    let style = KStroke::new(line_w);
    let brush = Brush::Solid(PATTERN_COLOR);
    scene.stroke(&style, world_to_screen, &brush, None, &path);
}

fn draw_dots(
    scene: &mut Scene,
    transform: &ViewportTransform,
    world_to_screen: Affine,
    spacing: f64,
    tiling: bool,
) {
    if spacing <= 0.0 {
        return;
    }
    let zoom = transform.zoom().max(1e-6);
    let radius_canvas = (1.5 / zoom).clamp(0.05, 3.0);
    // `tiling` is plumbed through from BackgroundConfig::Dots but the
    // non-tiling clip-to-page path isn't implemented yet — both
    // branches resolved to the visible-canvas bounds. Collapsed for
    // clippy; revisit if non-tiled dot grids land.
    let _ = tiling;
    let bounds = transform.visible_canvas_rect();
    let brush = Brush::Solid(PATTERN_COLOR);

    // Build all dots into a single BezPath, fill once. Far cheaper than
    // one fill call per dot at high density.
    let mut path = BezPath::new();
    let x_start = (bounds.x / spacing).floor() * spacing;
    let y_start = (bounds.y / spacing).floor() * spacing;
    let x_end = bounds.x + bounds.width;
    let y_end = bounds.y + bounds.height;
    let mut y = y_start;
    while y <= y_end {
        let mut x = x_start;
        while x <= x_end {
            path.extend(Circle::new((x, y), radius_canvas).path_elements(0.05));
            x += spacing;
        }
        y += spacing;
    }
    scene.fill(Fill::NonZero, world_to_screen, &brush, None, &path);
}

fn draw_isometric_lines(
    scene: &mut Scene,
    transform: &ViewportTransform,
    world_to_screen: Affine,
    spacing: f64,
) {
    if spacing <= 0.0 {
        return;
    }
    let visible = transform.visible_canvas_rect();
    let zoom = transform.zoom().max(1e-6);
    let line_w = (1.0 / zoom).clamp(0.04, 0.5);
    let brush = Brush::Solid(PATTERN_COLOR);

    let xa = visible.x;
    let xb = visible.x + visible.width;
    let ya = visible.y;
    let yb = visible.y + visible.height;
    let slope = 1.0 / 3.0_f64.sqrt();
    let h = spacing * 0.5;

    let mut path = BezPath::new();
    let x0 = (xa / h).floor() * h;
    let mut x = x0;
    while x <= xb {
        path.move_to((x, ya));
        path.line_to((x, yb));
        x += h;
    }

    let dc = 2.0 * h / 3.0_f64.sqrt();
    let c_min_p = ya - slope * xb;
    let c_max_p = yb - slope * xa;
    let mut c = (c_min_p / dc).floor() * dc;
    while c <= c_max_p {
        path.move_to((xa, slope * xa + c));
        path.line_to((xb, slope * xb + c));
        c += dc;
    }
    let c_min_n = ya + slope * xa;
    let c_max_n = yb + slope * xb;
    let mut c = (c_min_n / dc).floor() * dc;
    while c <= c_max_n {
        path.move_to((xa, -slope * xa + c));
        path.line_to((xb, -slope * xb + c));
        c += dc;
    }
    let style = KStroke::new(line_w);
    scene.stroke(&style, world_to_screen, &brush, None, &path);
}

fn draw_hexagonal_lines(
    scene: &mut Scene,
    transform: &ViewportTransform,
    world_to_screen: Affine,
    spacing: f64,
) {
    if spacing <= 0.0 {
        return;
    }
    let visible = transform.visible_canvas_rect();
    let zoom = transform.zoom().max(1e-6);
    let line_w = (1.0 / zoom).clamp(0.04, 0.5);
    let brush = Brush::Solid(PATTERN_COLOR);

    let r = spacing / 3.0_f64.sqrt();
    let row_h = r * 1.5;
    let col_w = r * 3.0_f64.sqrt();

    let row0 = ((visible.y / row_h).floor() as i64) - 1;
    let row1 = (((visible.y + visible.height) / row_h).ceil() as i64) + 1;
    let col0 = ((visible.x / col_w).floor() as i64) - 1;
    let col1 = (((visible.x + visible.width) / col_w).ceil() as i64) + 1;

    let mut path = BezPath::new();
    for row in row0..=row1 {
        let y_centre = row as f64 * row_h;
        let x_offset = if row.rem_euclid(2) == 1 {
            col_w * 0.5
        } else {
            0.0
        };
        for col in col0..=col1 {
            let x_centre = col as f64 * col_w + x_offset;
            for i in 0..6 {
                let a0 = (60.0 * i as f64 + 30.0).to_radians();
                let a1 = (60.0 * (i + 1) as f64 + 30.0).to_radians();
                let p0 = (x_centre + r * a0.cos(), y_centre + r * a0.sin());
                let p1 = (x_centre + r * a1.cos(), y_centre + r * a1.sin());
                path.move_to(p0);
                path.line_to(p1);
            }
        }
    }
    let style = KStroke::new(line_w);
    scene.stroke(&style, world_to_screen, &brush, None, &path);
}

fn world_to_screen_affine(transform: &ViewportTransform) -> Affine {
    let (sw, sh) = transform.screen_size();
    let z = transform.zoom();
    let c = transform.center();
    Affine::translate((sw * 0.5 - c.x * z, sh * 0.5 - c.y * z)) * Affine::scale(z)
}

/// A full-screen rectangle path used as the clip region for blend-mode
/// layers. Strokes don't actually need clipping — we just need *some* path
/// for `push_layer` to define the layer's bounds.
fn full_coverage_path() -> BezPath {
    // The screen-space coverage layer needs to be larger than any plausible
    // viewport. Hardcoding a generous rect avoids reading transform here.
    let big = 1.0e6_f64;
    let mut p = BezPath::new();
    p.move_to((-big, -big));
    p.line_to((big, -big));
    p.line_to((big, big));
    p.line_to((-big, big));
    p.close_path();
    p
}

fn draw_selection_underlay(scene: &mut Scene, transform: Affine, stroke: &Stroke, zoom: f64) {
    let zoc = stroke.zoom_at_creation.max(1e-6);
    let base_w = stroke.pen.base_width / zoc;
    let extra = 4.0 / zoom; // 4 screen-px halo regardless of zoom
    let pts = &stroke.points;
    if pts.is_empty() {
        return;
    }
    let highlight = Brush::Solid(Color::from_rgba8(51, 128, 255, 115));

    if pts.len() == 1 {
        let p = &pts[0];
        let r = base_w * (p.pressure.max(0.05) as f64) * 0.5 + extra * 0.5;
        let path = Circle::new((p.x, p.y), r).to_path(0.05);
        scene.fill(Fill::NonZero, transform, &highlight, None, &path);
        return;
    }

    let mut path = BezPath::new();
    path.move_to((pts[0].x, pts[0].y));
    for p in pts.iter().skip(1) {
        path.line_to((p.x, p.y));
    }
    let avg_pressure =
        (pts.iter().map(|p| p.pressure as f64).sum::<f64>() / pts.len() as f64).max(0.05);
    let mut style = KStroke::new(base_w * avg_pressure + extra);
    style.start_cap = Cap::Round;
    style.end_cap = Cap::Round;
    style.join = Join::Round;
    scene.stroke(&style, transform, &highlight, None, &path);
}

fn draw_stroke(scene: &mut Scene, transform: Affine, stroke: &Stroke, params: &ToolStyleParams) {
    // A stroke's own `brush_recipe` (set at creation time from the
    // active Tool Editor brush) wins. Custom brushes survive a
    // tool/style change because the recipe is captured into the
    // stroke itself.
    if let Some(brush) = stroke.brush_recipe.as_ref() {
        draw_brush_into_scene(scene, transform, stroke, brush);
        return;
    }
    // Legacy strokes (no recipe) — `legacy_brush_for` returns Some
    // for every (style, shape) combination after Phase 5, so the
    // composable engine handles every render path.
    if let Some(brush) = crate::built_in_brushes::legacy_brush_for(stroke.pen.brush_style, params) {
        draw_brush_into_scene(scene, transform, stroke, &brush);
    }
}

fn solid_color(color: journal_core::Color, alpha: f64) -> Color {
    let a = (alpha.clamp(0.0, 1.0) * 255.0) as u8;
    Color::from_rgba8(color.r, color.g, color.b, a)
}

// ---------------------------------------------------------------------------
// Path-building helpers (shared between the composable engine paths)
// ---------------------------------------------------------------------------

/// Build a quadratic-through-midpoints BezPath from a list of
/// `StrokePoint`s.
fn build_smooth_path(pts: &[journal_core::StrokePoint]) -> BezPath {
    let n = pts.len();
    let mut path = BezPath::new();
    path.move_to((pts[0].x, pts[0].y));
    if n == 2 {
        path.line_to((pts[1].x, pts[1].y));
    } else {
        for i in 0..n - 1 {
            let p0 = &pts[i];
            let p1 = &pts[i + 1];
            if i == 0 {
                let mid = ((p0.x + p1.x) * 0.5, (p0.y + p1.y) * 0.5);
                path.quad_to((p0.x, p0.y), mid);
            } else if i == n - 2 {
                path.quad_to((p0.x, p0.y), (p1.x, p1.y));
            } else {
                let next_mid = ((p0.x + p1.x) * 0.5, (p0.y + p1.y) * 0.5);
                path.quad_to((p0.x, p0.y), next_mid);
            }
        }
    }
    path
}

/// Resample a polyline so that consecutive samples are at most `step`
/// apart. Pressure is linearly interpolated. Used by the variable-width
/// outline shapes (PenFlat / Calligraphy).
fn resample_path(pts: &[journal_core::StrokePoint], step: f64) -> Vec<(f64, f64, f64)> {
    let mut out = Vec::with_capacity(pts.len() * 2);
    for i in 0..pts.len() {
        let p = &pts[i];
        let press = p.pressure as f64;
        if i == 0 {
            out.push((p.x, p.y, press));
            continue;
        }
        let prev = &pts[i - 1];
        let dx = p.x - prev.x;
        let dy = p.y - prev.y;
        let len = (dx * dx + dy * dy).sqrt();
        let n = ((len / step.max(1e-6)).ceil() as i32).max(1);
        for k in 1..=n {
            let t = k as f64 / n as f64;
            let x = prev.x + dx * t;
            let y = prev.y + dy * t;
            let pp = prev.pressure as f64 + (press - prev.pressure as f64) * t;
            out.push((x, y, pp));
        }
    }
    out
}

/// Compute a tangent vector at sample `i`. Uses neighbour difference
/// internally and clamps at endpoints to avoid zero-length tangents.
fn sample_tangent(samples: &[(f64, f64, f64)], i: usize) -> (f64, f64) {
    let n = samples.len();
    let (x, y, _) = samples[i];
    if i == 0 {
        let (nx, ny, _) = samples[1];
        (nx - x, ny - y)
    } else if i == n - 1 {
        let (px, py, _) = samples[i - 1];
        (x - px, y - py)
    } else {
        let (px, py, _) = samples[i - 1];
        let (nx, ny, _) = samples[i + 1];
        (nx - px, ny - py)
    }
}

/// Append `points` to `path` as a chain of quadratic-through-midpoints
/// curves. Each interior point becomes a control point; segments meet at
/// midpoints, giving a continuous smooth curve. The path's previous endpoint
/// must already be at the start of `points` (caller's responsibility).
fn smooth_polyline(path: &mut BezPath, points: &[(f64, f64)]) {
    let n = points.len();
    if n == 0 {
        return;
    }
    if n == 1 {
        path.line_to(points[0]);
        return;
    }
    for i in 0..n - 1 {
        let p0 = points[i];
        let p1 = points[i + 1];
        if i == n - 2 {
            path.quad_to(p0, p1);
        } else {
            let next_mid = ((p0.0 + p1.0) * 0.5, (p0.1 + p1.1) * 0.5);
            path.quad_to(p0, next_mid);
        }
    }
}

// ---------------------------------------------------------------------------
// Pseudo-noise (matches stroke_renderer::pseudo_noise)
// ---------------------------------------------------------------------------

fn pseudo_noise(x: f64, y: f64) -> f64 {
    let v = (x * 12.9898 + y * 78.233).sin() * 43758.5453;
    let f = v - v.floor();
    f.abs()
}

// ---------------------------------------------------------------------------
// Composable brush engine — Phase 0
// ---------------------------------------------------------------------------
//
// Lowers a `crate::brush::Brush` (ordered list of layers) into Vello
// scene calls. Reuses the existing path-building helpers
// (`build_smooth_path`, `resample_path`, `sample_tangent`,
// `smooth_polyline`) so the visual output matches the legacy per-style
// `draw_*` functions byte-for-byte for each composition emitted by
// `built_in_brushes::*`.
//
// Coverage Phase 0:
//   Smooth + (Constant | ClampedConstant | Pressure) + Round/Square
//   Smooth + TiltBand + Round   (Pencil shading layer)
//   Outline + (Constant | Pressure | DirectionAngled) + Round
//   Scatter + Constant + Round
// Other combinations are no-ops with a `tracing::debug` so users who
// build them in the Tool Editor see a hint to file a bug. Phase 5
// fills in the remainder.

use journal_core::{Brush as RBrush, BrushLayer, ColorMod, Geometry, TipShape, WidthMode};

/// Public entry point. Iterates the brush's layers in order and emits
/// each one onto the scene.
pub fn draw_brush_into_scene(
    scene: &mut Scene,
    transform: Affine,
    stroke: &Stroke,
    brush: &RBrush,
) {
    for layer in brush.layers.iter().filter(|l| l.enabled) {
        emit_layer(scene, transform, stroke, layer);
    }
}

fn emit_layer(scene: &mut Scene, transform: Affine, stroke: &Stroke, layer: &BrushLayer) {
    match &layer.geometry {
        Geometry::Smooth { .. } => emit_smooth(scene, transform, stroke, layer),
        Geometry::Outline {
            resample_step_mm,
            smooth_outline,
        } => emit_outline(
            scene,
            transform,
            stroke,
            layer,
            *resample_step_mm,
            *smooth_outline,
        ),
        Geometry::Scatter {
            density,
            spread_mm,
            falloff,
            directional_bias_deg,
        } => emit_scatter(
            scene,
            transform,
            stroke,
            layer,
            *density,
            *spread_mm,
            *falloff,
            *directional_bias_deg,
        ),
        Geometry::DabStamp { step_mult } => {
            emit_dab_stamp(scene, transform, stroke, layer, *step_mult)
        }
        Geometry::FanOffset { count, spread_mult } => {
            emit_fan_offset(scene, transform, stroke, layer, *count, *spread_mult)
        }
    }
}

/// FanOffset — emit `count` thin parallel offset Smooth strokes,
/// spread perpendicular to the path. Reproduces the legacy
/// `PaintbrushShape::Fan` look natively.
fn emit_fan_offset(
    scene: &mut Scene,
    transform: Affine,
    stroke: &Stroke,
    layer: &BrushLayer,
    count: u32,
    spread_mult: f64,
) {
    let pen = stroke.pen;
    let zoc = stroke.zoom_at_creation.max(1e-6);
    let canvas_w = pen.base_width / zoc;
    let pts = &stroke.points;
    if pts.len() < 2 {
        return;
    }
    let avg_pressure =
        (pts.iter().map(|p| p.pressure as f64).sum::<f64>() / pts.len() as f64).max(0.2);
    let total_width = canvas_w * avg_pressure * spread_mult;
    let count = count.max(2);
    let tine_w = (canvas_w * 0.18).max(0.4);
    let brush = layer_brush(pen.color, pen.opacity, layer.color);

    for i in 0..count {
        let t = i as f64 / (count - 1) as f64;
        let offset = (t - 0.5) * total_width;
        let mut path = BezPath::new();
        for (idx, p) in pts.iter().enumerate() {
            let (tx, ty) = if idx == 0 {
                let p1 = &pts[1];
                (p1.x - p.x, p1.y - p.y)
            } else if idx == pts.len() - 1 {
                let p0 = &pts[idx - 1];
                (p.x - p0.x, p.y - p0.y)
            } else {
                let p0 = &pts[idx - 1];
                let p1 = &pts[idx + 1];
                (p1.x - p0.x, p1.y - p0.y)
            };
            let tlen = (tx * tx + ty * ty).sqrt().max(1e-6);
            let nx = -ty / tlen;
            let ny = tx / tlen;
            let ox = p.x + nx * offset;
            let oy = p.y + ny * offset;
            if idx == 0 {
                path.move_to((ox, oy));
            } else {
                path.line_to((ox, oy));
            }
        }
        let mut style = KStroke::new(tine_w);
        style.start_cap = Cap::Round;
        style.end_cap = Cap::Round;
        style.join = Join::Round;
        scene.stroke(&style, transform, &brush, None, &path);
    }
}

/// Compute a peniko `Brush` from the stroke's pen color, the stroke's
/// opacity, and a layer's `ColorMod` (per-layer alpha and hue shift).
fn layer_brush(pen_color: JColor, pen_opacity: f32, mod_color: ColorMod) -> Brush {
    let alpha_mult = mod_color.alpha_mult.clamp(0.0, 1.0);
    let combined = (pen_opacity as f64 * alpha_mult).clamp(0.0, 1.0);
    let alpha_u8 = ((pen_color.a as f64 / 255.0) * combined * 255.0).clamp(0.0, 255.0) as u8;

    let (r, g, b) = if mod_color.hue_shift_deg.abs() > 0.05 {
        rotate_hue(
            pen_color.r,
            pen_color.g,
            pen_color.b,
            mod_color.hue_shift_deg,
        )
    } else {
        (pen_color.r, pen_color.g, pen_color.b)
    };
    Brush::Solid(Color::from_rgba8(r, g, b, alpha_u8))
}

/// Rotate an RGB color around HSL hue by `deg`. Saturation +
/// lightness preserved. Achromatic colors (saturation == 0) are
/// returned unchanged — hue is undefined for grays.
fn rotate_hue(r: u8, g: u8, b: u8, deg: f64) -> (u8, u8, u8) {
    let r_f = r as f64 / 255.0;
    let g_f = g as f64 / 255.0;
    let b_f = b as f64 / 255.0;
    let max = r_f.max(g_f).max(b_f);
    let min = r_f.min(g_f).min(b_f);
    let l = (max + min) * 0.5;
    let d = max - min;
    if d.abs() < 1e-6 {
        return (r, g, b);
    }
    let s = if l > 0.5 {
        d / (2.0 - max - min)
    } else {
        d / (max + min)
    };
    let mut h = if max == r_f {
        ((g_f - b_f) / d) + if g_f < b_f { 6.0 } else { 0.0 }
    } else if max == g_f {
        ((b_f - r_f) / d) + 2.0
    } else {
        ((r_f - g_f) / d) + 4.0
    };
    h *= 60.0;
    h = (h + deg).rem_euclid(360.0);
    let (r2, g2, b2) = hsl_to_rgb(h / 360.0, s, l);
    (
        (r2 * 255.0).clamp(0.0, 255.0) as u8,
        (g2 * 255.0).clamp(0.0, 255.0) as u8,
        (b2 * 255.0).clamp(0.0, 255.0) as u8,
    )
}

fn hsl_to_rgb(h: f64, s: f64, l: f64) -> (f64, f64, f64) {
    if s.abs() < 1e-6 {
        return (l, l, l);
    }
    let q = if l < 0.5 {
        l * (1.0 + s)
    } else {
        l + s - l * s
    };
    let p = 2.0 * l - q;
    (
        hue_to_rgb(p, q, h + 1.0 / 3.0),
        hue_to_rgb(p, q, h),
        hue_to_rgb(p, q, h - 1.0 / 3.0),
    )
}

fn hue_to_rgb(p: f64, q: f64, mut t: f64) -> f64 {
    if t < 0.0 {
        t += 1.0;
    }
    if t > 1.0 {
        t -= 1.0;
    }
    if t < 1.0 / 6.0 {
        return p + (q - p) * 6.0 * t;
    }
    if t < 1.0 / 2.0 {
        return q;
    }
    if t < 2.0 / 3.0 {
        return p + (q - p) * (2.0 / 3.0 - t) * 6.0;
    }
    p
}

/// Smooth — single quadratic-through-midpoints stroke. Width comes
/// from `WidthMode`, cap from `TipShape`.
fn emit_smooth(scene: &mut Scene, transform: Affine, stroke: &Stroke, layer: &BrushLayer) {
    let pen = stroke.pen;
    let zoc = stroke.zoom_at_creation.max(1e-6);
    let canvas_w = pen.base_width / zoc;
    let pts = &stroke.points;
    if pts.is_empty() {
        return;
    }
    let brush = layer_brush(pen.color, pen.opacity, layer.color);

    // Tilt-band is its own emission pattern (per-segment overlays).
    if let WidthMode::TiltBand {
        threshold,
        band_mult,
        alpha_scale,
    } = layer.width
    {
        emit_tilt_band(scene, transform, stroke, threshold, band_mult, alpha_scale);
        return;
    }

    // Non-strokeable tips (Diamond, StarN, FlatNib, Custom polygon)
    // can't be expressed as a GPU stroke style. When the user picks
    // them on a Smooth layer they expect a chain of *that shape*
    // along the path — stamp the tip at a tight step instead of
    // stroking. Round + Square stay on the stroke fast-path.
    let strokeable = matches!(layer.tip, TipShape::Round | TipShape::Square);
    if !strokeable && pts.len() >= 2 {
        emit_smooth_stamped(scene, transform, stroke, layer);
        return;
    }
    let (start_cap, end_cap, join) = caps_for_tip(&layer.tip);

    // Average pressure across the stroke — used by Pressure mode.
    let avg_pressure = if pts.is_empty() {
        0.0
    } else {
        (pts.iter().map(|p| p.pressure as f64).sum::<f64>() / pts.len() as f64).max(0.05)
    };

    let width = match layer.width {
        WidthMode::Constant { width_mult } => canvas_w * width_mult,
        WidthMode::ClampedConstant {
            width_mult,
            min_mm,
            max_mm,
        } => (canvas_w * width_mult).clamp(min_mm, max_mm),
        WidthMode::Pressure { floor, amp } => canvas_w * (floor + amp * avg_pressure),
        WidthMode::DirectionAngled { .. } => {
            // DirectionAngled only makes sense with Outline geometry;
            // on Smooth we degrade to a constant base width.
            canvas_w
        }
        WidthMode::TiltBand { .. } => unreachable!("handled above"),
    };

    if pts.len() == 1 {
        let p = &pts[0];
        let r = match layer.width {
            WidthMode::Pressure { floor, amp } => {
                canvas_w * (floor + amp * (p.pressure as f64).max(0.05)) * 0.5
            }
            _ => width * 0.5,
        } * layer.tip_scale.max(0.0);
        let path = tip_polygon(&layer.tip, (p.x, p.y), r);
        scene.fill(Fill::NonZero, transform, &brush, None, &path);
        return;
    }

    let path = build_smooth_path(pts);
    let mut style = KStroke::new(width);
    style.start_cap = start_cap;
    style.end_cap = end_cap;
    style.join = join;
    scene.stroke(&style, transform, &brush, None, &path);
}

/// Stamp a tip-shaped polygon at fixed intervals along a smooth
/// path. Used when the user picks a non-strokeable `TipShape`
/// (Diamond, StarN, FlatNib, Custom) on a Smooth-geometry layer —
/// they expect a chain of that shape along the path, not a wide
/// curved trace.
fn emit_smooth_stamped(scene: &mut Scene, transform: Affine, stroke: &Stroke, layer: &BrushLayer) {
    let pen = stroke.pen;
    let zoc = stroke.zoom_at_creation.max(1e-6);
    let canvas_w = pen.base_width / zoc;
    let pts = &stroke.points;
    let brush = layer_brush(pen.color, pen.opacity, layer.color);
    // Step ≈ half the canvas width keeps stamps overlapping enough
    // to read as a continuous trace at typical sizes.
    let step = (canvas_w * 0.4).max(0.5);
    let samples = resample_path(pts, step);
    for &(x, y, press) in &samples {
        let scale = match layer.width {
            WidthMode::Constant { width_mult } => canvas_w * width_mult * 0.5,
            WidthMode::ClampedConstant {
                width_mult,
                min_mm,
                max_mm,
            } => ((canvas_w * width_mult) * 0.5).clamp(min_mm * 0.5, max_mm * 0.5),
            WidthMode::Pressure { floor, amp } => canvas_w * (floor + amp * press) * 0.5,
            WidthMode::DirectionAngled { .. } => canvas_w * 0.5,
            WidthMode::TiltBand { .. } => canvas_w * 0.5,
        } * layer.tip_scale.max(0.0);
        let path = tip_polygon(&layer.tip, (x, y), scale);
        scene.fill(Fill::NonZero, transform, &brush, None, &path);
    }
}

/// Per-segment tilt-band overlay (Pencil shading layer).
fn emit_tilt_band(
    scene: &mut Scene,
    transform: Affine,
    stroke: &Stroke,
    threshold: f64,
    band_mult: f64,
    alpha_scale: f64,
) {
    let pen = stroke.pen;
    let zoc = stroke.zoom_at_creation.max(1e-6);
    let pts = &stroke.points;
    if pts.len() < 2 {
        return;
    }
    let core_w = (pen.base_width / zoc).clamp(0.4, 0.9);
    let inv_half_pi = 2.0 / std::f64::consts::PI;
    for i in 0..pts.len() - 1 {
        let a = &pts[i];
        let b = &pts[i + 1];
        let mag_a = ((a.tilt_x as f64).hypot(a.tilt_y as f64) * inv_half_pi).clamp(0.0, 1.0);
        let mag_b = ((b.tilt_x as f64).hypot(b.tilt_y as f64) * inv_half_pi).clamp(0.0, 1.0);
        let tilt = (mag_a + mag_b) * 0.5;
        if tilt < threshold {
            continue;
        }
        let avg_press = (((a.pressure + b.pressure) * 0.5) as f64).max(0.15);
        let band_w = core_w * (1.0 + band_mult * tilt * avg_press);
        let band_alpha = (pen.opacity as f64) * alpha_scale * tilt;
        let band_brush = Brush::Solid(solid_color(pen.color, band_alpha));
        let mut seg = BezPath::new();
        seg.move_to((a.x, a.y));
        seg.line_to((b.x, b.y));
        let mut style = KStroke::new(band_w);
        style.start_cap = Cap::Round;
        style.end_cap = Cap::Round;
        style.join = Join::Round;
        scene.stroke(&style, transform, &band_brush, None, &seg);
    }
}

/// Outline — variable-width filled polygon (offset perpendicular to
/// the path on both sides, then joined). Width per sample comes from
/// `WidthMode`; nib angle for `DirectionAngled` follows the stroke's
/// tangent direction.
fn emit_outline(
    scene: &mut Scene,
    transform: Affine,
    stroke: &Stroke,
    layer: &BrushLayer,
    resample_step_mult: f64,
    smooth_outline: bool,
) {
    let pen = stroke.pen;
    let zoc = stroke.zoom_at_creation.max(1e-6);
    let max_width = pen.base_width / zoc;
    let pts = &stroke.points;
    if pts.is_empty() {
        return;
    }
    let brush = layer_brush(pen.color, pen.opacity, layer.color);

    if pts.len() == 1 {
        let p = &pts[0];
        let r = match layer.width {
            WidthMode::DirectionAngled { min_ratio, .. } => max_width * 0.5 * min_ratio,
            _ => max_width * 0.5,
        };
        let path = Circle::new((p.x, p.y), r).to_path(0.05);
        scene.fill(Fill::NonZero, transform, &brush, None, &path);
        return;
    }

    let max_step = (max_width * resample_step_mult).max(0.25);
    let samples = resample_path(pts, max_step);
    if samples.len() < 2 {
        return;
    }

    let n = samples.len();
    let mut left: Vec<(f64, f64)> = Vec::with_capacity(n);
    let mut right: Vec<(f64, f64)> = Vec::with_capacity(n);
    for i in 0..n {
        let (x, y, press) = samples[i];
        let (tx, ty) = sample_tangent(&samples, i);
        let tlen = (tx * tx + ty * ty).sqrt().max(1e-6);
        let press_clamped = press.max(0.3);
        let w = match layer.width {
            WidthMode::DirectionAngled { nib_deg, min_ratio } => {
                let nib_angle = nib_deg.to_radians();
                let dir = ty.atan2(tx);
                let rel = (dir - nib_angle).sin().abs();
                max_width * (min_ratio + (1.0 - min_ratio) * rel) * press_clamped * 0.5
            }
            WidthMode::Pressure { floor, amp } => max_width * (floor + amp * press_clamped) * 0.5,
            WidthMode::Constant { width_mult } => max_width * width_mult * 0.5,
            WidthMode::ClampedConstant {
                width_mult,
                min_mm,
                max_mm,
            } => ((max_width * width_mult) * 0.5).clamp(min_mm * 0.5, max_mm * 0.5),
            WidthMode::TiltBand { .. } => max_width * 0.5,
        };
        let nxn = -ty / tlen;
        let nyn = tx / tlen;
        left.push((x + nxn * w, y + nyn * w));
        right.push((x - nxn * w, y - nyn * w));
    }
    let mut path = BezPath::new();
    path.move_to(left[0]);
    if smooth_outline {
        smooth_polyline(&mut path, &left[1..]);
        if let Some(&last_left) = left.last() {
            let first_right = right[right.len() - 1];
            let mid = (
                (last_left.0 + first_right.0) * 0.5,
                (last_left.1 + first_right.1) * 0.5,
            );
            path.quad_to(last_left, mid);
        }
        let right_rev: Vec<(f64, f64)> = right.iter().rev().copied().collect();
        smooth_polyline(&mut path, &right_rev);
    } else {
        for &p in left.iter().skip(1) {
            path.line_to(p);
        }
        for &p in right.iter().rev() {
            path.line_to(p);
        }
    }
    path.close_path();
    scene.fill(Fill::NonZero, transform, &brush, None, &path);
}

/// Scatter — N tip stamps per input point at randomized offsets.
/// `spread_mm == 0.0` means "use the stroke's base radius"; non-zero
/// values override.
fn emit_scatter(
    scene: &mut Scene,
    transform: Affine,
    stroke: &Stroke,
    layer: &BrushLayer,
    density: u32,
    spread_mm: f64,
    falloff: f64,
    directional_bias_deg: Option<f64>,
) {
    let pen = stroke.pen;
    let zoc = stroke.zoom_at_creation.max(1e-6);
    let radius = pen.base_width / zoc * 0.5;
    let scatter_radius = if spread_mm > 0.0 { spread_mm } else { radius };
    let dot_factor = match layer.width {
        WidthMode::Constant { width_mult } => width_mult,
        WidthMode::Pressure { floor, amp } => floor + amp * 0.5,
        _ => 0.06,
    };
    // Match legacy SprayParams::min_dot_radius default.
    let dot_radius = (radius * dot_factor).max(0.35) * layer.tip_scale.max(0.0);
    let brush = layer_brush(pen.color, pen.opacity, layer.color);
    let cone_half = directional_bias_deg.unwrap_or(0.0).to_radians();

    for (idx, p) in stroke.points.iter().enumerate() {
        let press = (p.pressure.max(0.2) as f64).min(1.0);
        let scatter = scatter_radius * press;
        for k in 0..density {
            let seed = (idx as f64) * 7.31 + k as f64 * 1.97 + p.x * 0.013 + p.y * 0.029;
            let r_unit = pseudo_noise(seed * 2.7, seed * 0.8);
            let r = scatter * r_unit.powf(falloff.max(1e-3));
            let theta = if directional_bias_deg.is_some() && cone_half > 0.0 {
                let local = (pseudo_noise(seed, seed * 1.3) - 0.5) * 2.0 * cone_half;
                -std::f64::consts::FRAC_PI_2 + local
            } else {
                pseudo_noise(seed, seed * 1.3) * std::f64::consts::TAU
            };
            let cx = p.x + theta.cos() * r;
            let cy = p.y + theta.sin() * r;
            let path = tip_polygon(&layer.tip, (cx, cy), dot_radius);
            scene.fill(Fill::NonZero, transform, &brush, None, &path);
        }
    }
}

/// DabStamp — fixed-interval stamping along the path. Each stamp is
/// a tip-shaped polygon scaled by the layer's width.
fn emit_dab_stamp(
    scene: &mut Scene,
    transform: Affine,
    stroke: &Stroke,
    layer: &BrushLayer,
    step_mult: f64,
) {
    let pen = stroke.pen;
    let zoc = stroke.zoom_at_creation.max(1e-6);
    let canvas_w = pen.base_width / zoc;
    let pts = &stroke.points;
    if pts.is_empty() {
        return;
    }
    let brush = layer_brush(pen.color, pen.opacity, layer.color);

    let avg_pressure =
        (pts.iter().map(|p| p.pressure as f64).sum::<f64>() / pts.len() as f64).max(0.05);
    let step = (canvas_w * step_mult).max(0.5);
    let samples = if pts.len() == 1 {
        vec![(pts[0].x, pts[0].y, pts[0].pressure as f64)]
    } else {
        resample_path(pts, step)
    };
    for &(x, y, press) in &samples {
        let scale = match layer.width {
            WidthMode::Constant { width_mult } => canvas_w * width_mult * 0.5,
            WidthMode::ClampedConstant {
                width_mult,
                min_mm,
                max_mm,
            } => ((canvas_w * width_mult) * 0.5).clamp(min_mm * 0.5, max_mm * 0.5),
            WidthMode::Pressure { floor, amp } => canvas_w * (floor + amp * press) * 0.5,
            WidthMode::DirectionAngled { .. } => canvas_w * 0.5,
            WidthMode::TiltBand { .. } => canvas_w * 0.5,
        } * layer.tip_scale.max(0.0);
        let _ = avg_pressure;
        let path = tip_polygon(&layer.tip, (x, y), scale);
        scene.fill(Fill::NonZero, transform, &brush, None, &path);
    }
}

fn caps_for_tip(tip: &TipShape) -> (Cap, Cap, Join) {
    match tip {
        TipShape::Round => (Cap::Round, Cap::Round, Join::Round),
        TipShape::Square => (Cap::Square, Cap::Square, Join::Miter),
        // Non-axis-aligned tips don't have GPU-stroke caps; stroking
        // a path with FlatNib/Diamond/StarN/Custom degrades to round.
        // For exact stamps use `Geometry::DabStamp`.
        _ => (Cap::Round, Cap::Round, Join::Round),
    }
}

fn square_path(center: (f64, f64), half: f64) -> BezPath {
    let (cx, cy) = center;
    let mut p = BezPath::new();
    p.move_to((cx - half, cy - half));
    p.line_to((cx + half, cy - half));
    p.line_to((cx + half, cy + half));
    p.line_to((cx - half, cy + half));
    p.close_path();
    p
}

/// Build a closed BezPath for a `TipShape` at `center`, scaled so
/// that the unit-space `(±1, ±1)` corners land at `±scale`.
fn tip_polygon(tip: &TipShape, center: (f64, f64), scale: f64) -> BezPath {
    let (cx, cy) = center;
    match tip {
        TipShape::Round => Circle::new(center, scale).to_path(0.05),
        TipShape::Square => square_path(center, scale),
        TipShape::Diamond => {
            let mut p = BezPath::new();
            p.move_to((cx, cy - scale));
            p.line_to((cx + scale, cy));
            p.line_to((cx, cy + scale));
            p.line_to((cx - scale, cy));
            p.close_path();
            p
        }
        TipShape::FlatNib { angle_deg, aspect } => {
            let a = angle_deg.to_radians();
            let cos = a.cos();
            let sin = a.sin();
            let half_long = scale;
            let half_short = scale * aspect.max(0.05);
            let pts: [(f64, f64); 4] = [
                (-half_long, -half_short),
                (half_long, -half_short),
                (half_long, half_short),
                (-half_long, half_short),
            ];
            let mut p = BezPath::new();
            for (i, (x, y)) in pts.iter().enumerate() {
                let rx = x * cos - y * sin;
                let ry = x * sin + y * cos;
                if i == 0 {
                    p.move_to((cx + rx, cy + ry));
                } else {
                    p.line_to((cx + rx, cy + ry));
                }
            }
            p.close_path();
            p
        }
        TipShape::StarN {
            points,
            inner_ratio,
        } => {
            let n = (*points as usize).max(3);
            let mut p = BezPath::new();
            for i in 0..(n * 2) {
                let theta = (i as f64) * std::f64::consts::PI / (n as f64);
                let r = if i % 2 == 0 {
                    scale
                } else {
                    scale * inner_ratio
                };
                let x = cx + r * theta.cos();
                let y = cy + r * theta.sin() - scale * 0.0;
                if i == 0 {
                    p.move_to((x, y));
                } else {
                    p.line_to((x, y));
                }
            }
            p.close_path();
            p
        }
        TipShape::Custom { points } => {
            if points.len() < 3 {
                return Circle::new(center, scale).to_path(0.05);
            }
            let mut p = BezPath::new();
            for (i, (ux, uy)) in points.iter().enumerate() {
                let x = cx + ux * scale;
                let y = cy + uy * scale;
                if i == 0 {
                    p.move_to((x, y));
                } else {
                    p.line_to((x, y));
                }
            }
            p.close_path();
            p
        }
    }
}
