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

use journal_core::{BlendMode, BrushStyle, Color as JColor, Point, Rect, Stroke};
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
// `BrushParams::default()` reproduces the values that were hardcoded
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
pub struct BrushParams {
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
}

impl Default for OverlayState {
    fn default() -> Self {
        Self {
            selection_bbox: None,
            lasso_screen_points: Vec::new(),
            pointer_screen: None,
            pointer_drawing: false,
            cursor_radius: 5.0,
            cursor_color: JColor { r: 0, g: 0, b: 0, a: 255 },
            cursor_opacity: 1.0,
            show_page_bounds: false,
            dark_mode: false,
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
        let (device, queue) = pollster::block_on(adapter.request_device(
            &wgpu::DeviceDescriptor {
                label: Some("vello-device"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                ..Default::default()
            },
        ))
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
            Ok(r) => match r.with_guessed_format().and_then(|r| Ok(r.decode())) {
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
        brush_params: &BrushParams,
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
            let (r_un, g_un, b_un) = if a == 0 {
                (0, 0, 0)
            } else {
                (
                    ((r * 255 + a / 2) / a).min(255) as u8,
                    ((g * 255 + a / 2) / a).min(255) as u8,
                    ((b * 255 + a / 2) / a).min(255) as u8,
                )
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
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
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
    brush_params: &BrushParams,
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
        Color::from_rgba8(28, 27, 23, 255) // warm near-black
    } else {
        Color::from_rgba8(250, 247, 242, 255) // warm cream
    };
    scene.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        &Brush::Solid(bg_color),
        None,
        &KRect::new(0.0, 0.0, sw, sh),
    );

    // 2) Background pattern.
    draw_background_pattern(scene, transform, world_to_screen, background, page_rect, bg_image);

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
    let to_screen = |cx: f64, cy: f64| -> (f64, f64) {
        transform.canvas_to_screen(Point { x: cx, y: cy })
    };
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
    if ov.pointer_drawing {
        let alpha = (ov.cursor_opacity.clamp(0.2, 1.0) * 255.0) as u8;
        let fill = Brush::Solid(Color::from_rgba8(
            ov.cursor_color.r,
            ov.cursor_color.g,
            ov.cursor_color.b,
            alpha,
        ));
        let path = Circle::new((px, py), radius).to_path(0.05);
        scene.fill(Fill::NonZero, Affine::IDENTITY, &fill, None, &path);
    }
    let ring = Circle::new((px, py), radius).to_path(0.05);
    let halo = Circle::new((px, py), radius + 0.9).to_path(0.05);
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
    scene.stroke(&ring_style, Affine::IDENTITY, &Brush::Solid(ring_color), None, &ring);
    let halo_style = KStroke::new(0.5);
    scene.stroke(&halo_style, Affine::IDENTITY, &Brush::Solid(halo_color), None, &halo);
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
        BackgroundConfig::Lines { spacing, tiling } => draw_horizontal_lines(
            scene,
            transform,
            world_to_screen,
            *spacing,
            *tiling,
        ),
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
    let bounds = if tiling {
        transform.visible_canvas_rect()
    } else {
        // For non-tiling lines, clip to the visible-canvas region anyway —
        // the Cairo path used to clip to page_rect; we'd need a clip layer
        // here for parity. For the spike, draw across the full visible rect
        // and rely on the page-fill rect (already painted) for the visual
        // boundary on cream/dark page colour.
        transform.visible_canvas_rect()
    };
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
    let bounds = if tiling {
        transform.visible_canvas_rect()
    } else {
        transform.visible_canvas_rect()
    };
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
        let x_offset = if row.rem_euclid(2) == 1 { col_w * 0.5 } else { 0.0 };
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

fn draw_selection_underlay(
    scene: &mut Scene,
    transform: Affine,
    stroke: &Stroke,
    zoom: f64,
) {
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
    let avg_pressure = (pts.iter().map(|p| p.pressure as f64).sum::<f64>()
        / pts.len() as f64)
        .max(0.05);
    let mut style = KStroke::new(base_w * avg_pressure + extra);
    style.start_cap = Cap::Round;
    style.end_cap = Cap::Round;
    style.join = Join::Round;
    scene.stroke(&style, transform, &highlight, None, &path);
}

fn draw_stroke(
    scene: &mut Scene,
    transform: Affine,
    stroke: &Stroke,
    params: &BrushParams,
) {
    match stroke.pen.brush_style {
        BrushStyle::Pen | BrushStyle::Highlighter => {
            draw_smooth(scene, transform, stroke, &params.pen)
        }
        BrushStyle::Pencil => draw_pencil(scene, transform, stroke, &params.pencil),
        BrushStyle::Paintbrush => draw_paintbrush(scene, transform, stroke, &params.paintbrush),
        BrushStyle::SprayCan => draw_spray(scene, transform, stroke, &params.spray),
        BrushStyle::Calligraphy => {
            draw_calligraphy(scene, transform, stroke, &params.calligraphy)
        }
    }
}

fn solid(color: journal_core::Color, opacity: f32) -> Brush {
    let alpha = ((color.a as f32 / 255.0) * opacity.clamp(0.0, 1.0) * 255.0)
        .clamp(0.0, 255.0) as u8;
    Brush::Solid(Color::from_rgba8(color.r, color.g, color.b, alpha))
}

fn solid_color(color: journal_core::Color, alpha: f64) -> Color {
    let a = (alpha.clamp(0.0, 1.0) * 255.0) as u8;
    Color::from_rgba8(color.r, color.g, color.b, a)
}

// ---------------------------------------------------------------------------
// Pen / Highlighter
// ---------------------------------------------------------------------------

fn draw_smooth(scene: &mut Scene, transform: Affine, stroke: &Stroke, params: &PenParams) {
    match params.shape {
        PenShape::Round => draw_pen_round(scene, transform, stroke, params),
        PenShape::Flat => draw_pen_flat(scene, transform, stroke, params),
        PenShape::Marker => draw_pen_marker(scene, transform, stroke, params),
    }
}

fn draw_pen_round(scene: &mut Scene, transform: Affine, stroke: &Stroke, params: &PenParams) {
    let pen = stroke.pen;
    let zoc = stroke.zoom_at_creation.max(1e-6);
    let canvas_width_full = pen.base_width / zoc;

    let pts = &stroke.points;
    let n = pts.len();
    let brush = solid(pen.color, pen.opacity);

    if n == 1 {
        let p = &pts[0];
        let r = canvas_width_full * (p.pressure.max(0.05) as f64) * 0.5;
        let path = Circle::new((p.x, p.y), r).to_path(0.05);
        scene.fill(Fill::NonZero, transform, &brush, None, &path);
        return;
    }

    let avg_pressure = (pts.iter().map(|p| p.pressure as f64).sum::<f64>()
        / pts.len() as f64)
        .max(0.05);
    let width =
        canvas_width_full * (params.width_floor + params.width_pressure_amplitude * avg_pressure);

    let path = build_smooth_path(pts);
    let mut style = KStroke::new(width);
    style.start_cap = Cap::Round;
    style.end_cap = Cap::Round;
    style.join = Join::Round;
    scene.stroke(&style, transform, &brush, None, &path);
}

/// `PenShape::Flat` — draws the path as a calligraphy-lite
/// variable-width filled polygon, but the offset is purely
/// perpendicular to the stroke direction (no nib-angle bias). Width
/// scales with pressure floor + amplitude.
fn draw_pen_flat(scene: &mut Scene, transform: Affine, stroke: &Stroke, params: &PenParams) {
    let pen = stroke.pen;
    let zoc = stroke.zoom_at_creation.max(1e-6);
    let max_width = pen.base_width / zoc;
    let pts = &stroke.points;
    if pts.len() < 2 {
        draw_pen_round(scene, transform, stroke, params);
        return;
    }
    let brush = solid(pen.color, pen.opacity);
    let resample_step = (max_width * 0.5).max(0.5);
    let samples = resample_path(pts, resample_step);
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
        let press_eff =
            (params.width_floor + params.width_pressure_amplitude * press.max(0.05)).max(0.1);
        let w = max_width * press_eff * 0.5;
        let nxn = -ty / tlen;
        let nyn = tx / tlen;
        left.push((x + nxn * w, y + nyn * w));
        right.push((x - nxn * w, y - nyn * w));
    }
    let mut path = BezPath::new();
    path.move_to(left[0]);
    smooth_polyline(&mut path, &left[1..]);
    let right_rev: Vec<(f64, f64)> = right.iter().rev().copied().collect();
    smooth_polyline(&mut path, &right_rev);
    path.close_path();
    scene.fill(Fill::NonZero, transform, &brush, None, &path);
}

/// `PenShape::Marker` — fixed wide tip with constant width across the
/// stroke (no pressure modulation), suitable for a bookbinder /
/// permanent-marker feel.
fn draw_pen_marker(scene: &mut Scene, transform: Affine, stroke: &Stroke, params: &PenParams) {
    let pen = stroke.pen;
    let zoc = stroke.zoom_at_creation.max(1e-6);
    let width = (pen.base_width / zoc) * params.marker_width_mult;
    let brush = solid(pen.color, pen.opacity);
    let pts = &stroke.points;
    if pts.is_empty() {
        return;
    }
    if pts.len() == 1 {
        let p = &pts[0];
        let path = Circle::new((p.x, p.y), width * 0.5).to_path(0.05);
        scene.fill(Fill::NonZero, transform, &brush, None, &path);
        return;
    }
    let path = build_smooth_path(pts);
    let mut style = KStroke::new(width);
    style.start_cap = Cap::Round;
    style.end_cap = Cap::Round;
    style.join = Join::Round;
    scene.stroke(&style, transform, &brush, None, &path);
}

/// Build a quadratic-through-midpoints BezPath from a list of
/// `StrokePoint`s. Shared between Pen-Round and Pen-Marker.
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
fn resample_path(
    pts: &[journal_core::StrokePoint],
    step: f64,
) -> Vec<(f64, f64, f64)> {
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

// ---------------------------------------------------------------------------
// Pencil — sharp core line + tilt-driven shading band
// ---------------------------------------------------------------------------
//
// Real pencil: held upright = thin sharp graphite point; tilted = broad
// semi-transparent shading from the side of the lead. Modeled here as a
// hard-edge core stroke (always drawn) plus a wider, lower-alpha overlay
// per segment when the stylus is tilted. Tilt magnitude is averaged across
// each segment so the band width tracks the user's wrist angle smoothly.

fn draw_pencil(scene: &mut Scene, transform: Affine, stroke: &Stroke, params: &PencilParams) {
    match params.shape {
        PencilShape::Cylindrical => draw_pencil_cylindrical(scene, transform, stroke, params),
        PencilShape::Carpenter => draw_pencil_carpenter(scene, transform, stroke, params),
        PencilShape::Mechanical => draw_pencil_mechanical(scene, transform, stroke, params),
    }
}

fn draw_pencil_cylindrical(
    scene: &mut Scene,
    transform: Affine,
    stroke: &Stroke,
    params: &PencilParams,
) {
    let pen = stroke.pen;
    let zoc = stroke.zoom_at_creation.max(1e-6);
    let core_w = (pen.base_width / zoc).clamp(params.core_clamp_min, params.core_clamp_max);

    let pts = &stroke.points;
    if pts.is_empty() {
        return;
    }

    let core_brush = Brush::Solid(solid_color(pen.color, pen.opacity as f64));
    let mut core = BezPath::new();
    core.move_to((pts[0].x, pts[0].y));
    for p in pts.iter().skip(1) {
        core.line_to((p.x, p.y));
    }
    let mut core_style = KStroke::new(core_w);
    core_style.start_cap = Cap::Round;
    core_style.end_cap = Cap::Round;
    core_style.join = Join::Round;
    scene.stroke(&core_style, transform, &core_brush, None, &core);

    if pts.len() < 2 {
        return;
    }

    let inv_half_pi = 2.0 / std::f64::consts::PI;
    for i in 0..pts.len() - 1 {
        let a = &pts[i];
        let b = &pts[i + 1];
        let mag_a = ((a.tilt_x as f64).hypot(a.tilt_y as f64) * inv_half_pi).clamp(0.0, 1.0);
        let mag_b = ((b.tilt_x as f64).hypot(b.tilt_y as f64) * inv_half_pi).clamp(0.0, 1.0);
        let tilt = (mag_a + mag_b) * 0.5;
        if tilt < params.tilt_threshold {
            continue;
        }
        let avg_press = (((a.pressure + b.pressure) * 0.5) as f64).max(0.15);
        let band_w = core_w * (1.0 + params.tilt_band_mult * tilt * avg_press);
        let band_alpha = (pen.opacity as f64) * params.tilt_alpha_scale * tilt;
        let band_brush = Brush::Solid(solid_color(pen.color, band_alpha));

        let mut seg = BezPath::new();
        seg.move_to((a.x, a.y));
        seg.line_to((b.x, b.y));
        let mut band_style = KStroke::new(band_w);
        band_style.start_cap = Cap::Round;
        band_style.end_cap = Cap::Round;
        band_style.join = Join::Round;
        scene.stroke(&band_style, transform, &band_brush, None, &seg);
    }
}

/// Carpenter pencil — wider flat lead, width modulated by stroke
/// direction (similar to a flat-cut nib). Tilt-shading reused at lower
/// intensity since the wide lead already implies "shaded".
fn draw_pencil_carpenter(
    scene: &mut Scene,
    transform: Affine,
    stroke: &Stroke,
    params: &PencilParams,
) {
    let pen = stroke.pen;
    let zoc = stroke.zoom_at_creation.max(1e-6);
    let max_width = (pen.base_width / zoc) * params.carpenter_width_mult;
    let pts = &stroke.points;
    if pts.len() < 2 {
        draw_pencil_cylindrical(scene, transform, stroke, params);
        return;
    }
    let brush = Brush::Solid(solid_color(pen.color, pen.opacity as f64 * 0.85));
    let resample_step = (max_width * 0.4).max(0.5);
    let samples = resample_path(pts, resample_step);
    if samples.len() < 2 {
        return;
    }
    let nib_angle: f64 = std::f64::consts::FRAC_PI_4;
    let min_ratio = 0.35;
    let n = samples.len();
    let mut left: Vec<(f64, f64)> = Vec::with_capacity(n);
    let mut right: Vec<(f64, f64)> = Vec::with_capacity(n);
    for i in 0..n {
        let (x, y, press) = samples[i];
        let (tx, ty) = sample_tangent(&samples, i);
        let tlen = (tx * tx + ty * ty).sqrt().max(1e-6);
        let dir = ty.atan2(tx);
        let rel = (dir - nib_angle).sin().abs();
        let press_clamped = press.max(0.4);
        let w = max_width * (min_ratio + (1.0 - min_ratio) * rel) * press_clamped * 0.5;
        let nxn = -ty / tlen;
        let nyn = tx / tlen;
        left.push((x + nxn * w, y + nyn * w));
        right.push((x - nxn * w, y - nyn * w));
    }
    let mut path = BezPath::new();
    path.move_to(left[0]);
    smooth_polyline(&mut path, &left[1..]);
    let right_rev: Vec<(f64, f64)> = right.iter().rev().copied().collect();
    smooth_polyline(&mut path, &right_rev);
    path.close_path();
    scene.fill(Fill::NonZero, transform, &brush, None, &path);
}

/// Mechanical pencil — extra-thin precision tip, no tilt shading.
fn draw_pencil_mechanical(
    scene: &mut Scene,
    transform: Affine,
    stroke: &Stroke,
    _params: &PencilParams,
) {
    let pen = stroke.pen;
    let zoc = stroke.zoom_at_creation.max(1e-6);
    let core_w = (pen.base_width / zoc * 0.5).clamp(0.2, 0.6);
    let pts = &stroke.points;
    if pts.is_empty() {
        return;
    }
    let core_brush = Brush::Solid(solid_color(pen.color, pen.opacity as f64));
    let mut core = BezPath::new();
    core.move_to((pts[0].x, pts[0].y));
    for p in pts.iter().skip(1) {
        core.line_to((p.x, p.y));
    }
    let mut core_style = KStroke::new(core_w);
    core_style.start_cap = Cap::Round;
    core_style.end_cap = Cap::Round;
    core_style.join = Join::Round;
    scene.stroke(&core_style, transform, &core_brush, None, &core);
}

// ---------------------------------------------------------------------------
// Paintbrush — two-pass stroke for soft-edge feathered look
// ---------------------------------------------------------------------------
//
// Distinct from Highlighter (single hard-edge wide stroke) by composing a
// wide low-alpha halo + a narrower core stroke. The halo's transparent
// fringe gives the "watercolor" softness real bristle brushes have when
// the canvas is a single Vello layer (no per-dab radial gradients →
// fewer paths, much faster, and overlaps don't darken to opacity).

fn draw_paintbrush(
    scene: &mut Scene,
    transform: Affine,
    stroke: &Stroke,
    params: &PaintbrushParams,
) {
    match params.shape {
        PaintbrushShape::Round => draw_paintbrush_round(scene, transform, stroke, params),
        PaintbrushShape::Flat => draw_paintbrush_flat(scene, transform, stroke, params),
        PaintbrushShape::Fan => draw_paintbrush_fan(scene, transform, stroke, params),
    }
}

fn draw_paintbrush_round(
    scene: &mut Scene,
    transform: Affine,
    stroke: &Stroke,
    params: &PaintbrushParams,
) {
    let pen = stroke.pen;
    let zoc = stroke.zoom_at_creation.max(1e-6);
    let canvas_width_full = pen.base_width / zoc;

    let pts = &stroke.points;
    let n = pts.len();
    if n == 0 {
        return;
    }

    let avg_pressure = (pts.iter().map(|p| p.pressure as f64).sum::<f64>()
        / pts.len() as f64)
        .max(0.2);
    let core_w = canvas_width_full * avg_pressure;
    let halo_w = core_w * params.halo_width_mult;

    if n == 1 {
        let p = &pts[0];
        let halo_brush =
            Brush::Solid(solid_color(pen.color, pen.opacity as f64 * params.mid_alpha));
        let core_brush = solid(pen.color, pen.opacity * params.core_alpha as f32);
        let halo = Circle::new((p.x, p.y), halo_w * 0.5).to_path(0.05);
        scene.fill(Fill::NonZero, transform, &halo_brush, None, &halo);
        let core = Circle::new((p.x, p.y), core_w * 0.5).to_path(0.05);
        scene.fill(Fill::NonZero, transform, &core_brush, None, &core);
        return;
    }

    let path = build_smooth_path(pts);
    let bands: [(f64, f64); 3] = [
        (halo_w * params.outer_halo_mult, params.outer_alpha),
        (halo_w * params.mid_halo_mult, params.mid_alpha),
        (core_w, params.core_alpha),
    ];
    for &(w, alpha_mult) in &bands {
        let brush = Brush::Solid(solid_color(pen.color, pen.opacity as f64 * alpha_mult));
        let mut style = KStroke::new(w);
        style.start_cap = Cap::Round;
        style.end_cap = Cap::Round;
        style.join = Join::Round;
        scene.stroke(&style, transform, &brush, None, &path);
    }
}

/// `PaintbrushShape::Flat` — single hard-edge wide stroke, no halo.
/// Crisper than Round; reads as a flat sumi brush.
fn draw_paintbrush_flat(
    scene: &mut Scene,
    transform: Affine,
    stroke: &Stroke,
    params: &PaintbrushParams,
) {
    let pen = stroke.pen;
    let zoc = stroke.zoom_at_creation.max(1e-6);
    let canvas_width_full = pen.base_width / zoc;
    let pts = &stroke.points;
    if pts.is_empty() {
        return;
    }
    let avg_pressure = (pts.iter().map(|p| p.pressure as f64).sum::<f64>()
        / pts.len() as f64)
        .max(0.2);
    let width = canvas_width_full * avg_pressure;
    let brush = Brush::Solid(solid_color(pen.color, pen.opacity as f64 * params.core_alpha));
    if pts.len() == 1 {
        let p = &pts[0];
        let path = Circle::new((p.x, p.y), width * 0.5).to_path(0.05);
        scene.fill(Fill::NonZero, transform, &brush, None, &path);
        return;
    }
    let path = build_smooth_path(pts);
    let mut style = KStroke::new(width);
    style.start_cap = Cap::Square;
    style.end_cap = Cap::Square;
    style.join = Join::Miter;
    scene.stroke(&style, transform, &brush, None, &path);
}

/// `PaintbrushShape::Fan` — multiple parallel offset strokes spread
/// perpendicular to the stroke direction. Each "tine" is a thin
/// stroke; together they look like fan-bristle hair.
fn draw_paintbrush_fan(
    scene: &mut Scene,
    transform: Affine,
    stroke: &Stroke,
    params: &PaintbrushParams,
) {
    let pen = stroke.pen;
    let zoc = stroke.zoom_at_creation.max(1e-6);
    let canvas_width_full = pen.base_width / zoc;
    let pts = &stroke.points;
    if pts.len() < 2 {
        draw_paintbrush_round(scene, transform, stroke, params);
        return;
    }
    let avg_pressure = (pts.iter().map(|p| p.pressure as f64).sum::<f64>()
        / pts.len() as f64)
        .max(0.2);
    let total_width = canvas_width_full * avg_pressure * params.fan_spread_mult;
    let count = params.fan_count.max(2);
    let tine_w = (canvas_width_full * 0.18).max(0.4);
    let brush = Brush::Solid(solid_color(pen.color, pen.opacity as f64 * params.core_alpha));

    for i in 0..count {
        let t = if count == 1 {
            0.5
        } else {
            i as f64 / (count - 1) as f64
        };
        let offset = (t - 0.5) * total_width;
        let mut path = BezPath::new();
        for (idx, p) in pts.iter().enumerate() {
            // Rough tangent at this point, used to offset perpendicular.
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

// ---------------------------------------------------------------------------
// SprayCan — dense scatter, density biased toward center
// ---------------------------------------------------------------------------

fn draw_spray(scene: &mut Scene, transform: Affine, stroke: &Stroke, params: &SprayParams) {
    let pen = stroke.pen;
    let zoc = stroke.zoom_at_creation.max(1e-6);
    let radius = pen.base_width / zoc * 0.5;
    let dot_radius = (radius * params.dot_radius_factor).max(params.min_dot_radius);
    let dots_per_point = params.dots_per_point;
    let brush = solid(pen.color, pen.opacity);
    let cone_half = params.cone_spread_deg.to_radians();

    for (idx, p) in stroke.points.iter().enumerate() {
        let press = (p.pressure.max(0.2) as f64).min(1.0);
        let scatter = radius * press;
        // Cone direction from stylus tilt — falls back to up (-Y) when
        // the stylus is upright.
        let tilt_x = p.tilt_x as f64;
        let tilt_y = p.tilt_y as f64;
        let tilt_mag = (tilt_x.hypot(tilt_y)).max(1e-6);
        let cone_dir = if matches!(params.shape, SprayShape::Cone) && tilt_mag > 0.05 {
            tilt_y.atan2(tilt_x)
        } else {
            -std::f64::consts::FRAC_PI_2
        };

        for k in 0..dots_per_point {
            let seed = (idx as f64) * 7.31 + k as f64 * 1.97 + p.x * 0.013 + p.y * 0.029;
            let r_unit = pseudo_noise(seed * 2.7, seed * 0.8);
            let r = scatter * (r_unit * r_unit);
            let theta = match params.shape {
                SprayShape::Cone => {
                    let local = (pseudo_noise(seed, seed * 1.3) - 0.5) * 2.0 * cone_half;
                    cone_dir + local
                }
                _ => pseudo_noise(seed, seed * 1.3) * std::f64::consts::TAU,
            };
            let dx = theta.cos() * r;
            let dy = theta.sin() * r;
            let cx = p.x + dx;
            let cy = p.y + dy;
            let path = match params.shape {
                SprayShape::Square => {
                    let half = dot_radius;
                    let mut sp = BezPath::new();
                    sp.move_to((cx - half, cy - half));
                    sp.line_to((cx + half, cy - half));
                    sp.line_to((cx + half, cy + half));
                    sp.line_to((cx - half, cy + half));
                    sp.close_path();
                    sp
                }
                _ => Circle::new((cx, cy), dot_radius).to_path(0.05),
            };
            scene.fill(Fill::NonZero, transform, &brush, None, &path);
        }
    }
}

// ---------------------------------------------------------------------------
// Calligraphy — variable-width filled polygon, nib angle 45°
// ---------------------------------------------------------------------------

fn draw_calligraphy(
    scene: &mut Scene,
    transform: Affine,
    stroke: &Stroke,
    params: &CalligraphyParams,
) {
    let pen = stroke.pen;
    let zoc = stroke.zoom_at_creation.max(1e-6);
    let max_width = pen.base_width / zoc;
    let nib_angle = params.nib_angle_deg.to_radians();
    let min_ratio = params.min_ratio;

    let pts = &stroke.points;
    if pts.is_empty() {
        return;
    }
    let brush = solid(pen.color, pen.opacity);
    if pts.len() == 1 {
        let p = &pts[0];
        let path = Circle::new((p.x, p.y), max_width * 0.5 * min_ratio).to_path(0.05);
        scene.fill(Fill::NonZero, transform, &brush, None, &path);
        return;
    }

    // Round nib short-circuit: constant-width round trace, no
    // angle/pressure-driven outline polygon.
    if matches!(params.shape, CalligraphyShape::Round) {
        let path = build_smooth_path(pts);
        let mut style = KStroke::new(max_width);
        style.start_cap = Cap::Round;
        style.end_cap = Cap::Round;
        style.join = Join::Round;
        scene.stroke(&style, transform, &brush, None, &path);
        return;
    }

    let max_step = (max_width * params.resample_step_mult).max(0.25);
    let mut samples: Vec<(f64, f64, f64)> = Vec::with_capacity(pts.len() * 2);
    for i in 0..pts.len() {
        let p = &pts[i];
        let press = p.pressure as f64;
        if i == 0 {
            samples.push((p.x, p.y, press));
            continue;
        }
        let prev = &pts[i - 1];
        let dx = p.x - prev.x;
        let dy = p.y - prev.y;
        let len = (dx * dx + dy * dy).sqrt();
        let n = ((len / max_step).ceil() as i32).max(1);
        for k in 1..=n {
            let t = k as f64 / n as f64;
            let x = prev.x + dx * t;
            let y = prev.y + dy * t;
            let pp = prev.pressure as f64 + (press - prev.pressure as f64) * t;
            samples.push((x, y, pp));
        }
    }
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
        // Width formula depends on shape:
        // - FlatCut: angle-modulated (the current behaviour)
        // - BrushNib: pressure-only, no angle bias
        let w = match params.shape {
            CalligraphyShape::FlatCut | CalligraphyShape::Round => {
                let dir = ty.atan2(tx);
                let rel = (dir - nib_angle).sin().abs();
                max_width * (min_ratio + (1.0 - min_ratio) * rel) * press_clamped * 0.5
            }
            CalligraphyShape::BrushNib => {
                // Soft brush: width tracks pressure with a generous
                // floor so unloaded touches still show.
                max_width * (0.4 + 0.6 * press_clamped) * 0.5
            }
        };
        let nxn = -ty / tlen;
        let nyn = tx / tlen;
        left.push((x + nxn * w, y + nyn * w));
        right.push((x - nxn * w, y - nyn * w));
    }

    // Build the outline. With `params.smooth_outline = true` (default)
    // both sides are connected with quadratic-through-midpoints curves so
    // the polygon reads as a continuous nib trace; `false` falls back to
    // straight `line_to` edges (rigid, polygonal — Cairo-faithful).
    let mut path = BezPath::new();
    path.move_to(left[0]);
    if params.smooth_outline {
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
