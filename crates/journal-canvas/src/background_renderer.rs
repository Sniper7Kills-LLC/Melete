use std::path::PathBuf;

#[cfg(feature = "desktop")]
use std::cell::RefCell;
#[cfg(feature = "desktop")]
use std::collections::HashMap;
#[cfg(feature = "desktop")]
use std::path::Path;

#[cfg(feature = "desktop")]
use gtk4::cairo;
#[cfg(feature = "desktop")]
use gtk4::gdk_pixbuf::Pixbuf;
#[cfg(feature = "desktop")]
use gtk4::prelude::*;
#[cfg(feature = "desktop")]
use journal_core::Color;
#[cfg(feature = "desktop")]
use journal_core::Rect;

use crate::grid_renderer::GridSettings;
#[cfg(feature = "desktop")]
use crate::grid_renderer::draw_grid;
#[cfg(feature = "desktop")]
use crate::viewport_transform::ViewportTransform;

/// Apply a uniform scale to every spacing-bearing variant of `bg`. Used by
/// the canvas to honour a per-page "Reset Grid" adjustment that re-sizes
/// the grid to match the user's current zoom (see `CanvasState::bg_scale`).
pub fn scale_background(bg: &BackgroundConfig, scale: f64) -> BackgroundConfig {
    if !scale.is_finite() || scale <= 0.0 || (scale - 1.0).abs() < 1e-9 {
        return bg.clone();
    }
    match bg {
        BackgroundConfig::Blank => BackgroundConfig::Blank,
        BackgroundConfig::Dots { spacing, tiling } => BackgroundConfig::Dots {
            spacing: spacing * scale,
            tiling: *tiling,
        },
        BackgroundConfig::Lines { spacing, tiling } => BackgroundConfig::Lines {
            spacing: spacing * scale,
            tiling: *tiling,
        },
        BackgroundConfig::Grid(s) => BackgroundConfig::Grid(GridSettings {
            base_spacing: s.base_spacing * scale,
            subdivisions: s.subdivisions,
            color: s.color,
        }),
        BackgroundConfig::Isometric { spacing } => BackgroundConfig::Isometric {
            spacing: spacing * scale,
        },
        BackgroundConfig::Hexagonal { spacing } => BackgroundConfig::Hexagonal {
            spacing: spacing * scale,
        },
        // Image / PDF aren't grids — leave alone.
        other => other.clone(),
    }
}

#[derive(Debug, Clone)]
pub enum BackgroundConfig {
    Blank,
    /// `tiling = true` → dots cover entire visible canvas (infinite).
    /// `false` → dots clipped to the page rect.
    Dots {
        spacing: f64,
        tiling: bool,
    },
    /// `tiling = true` → lines span entire visible canvas (infinite).
    /// `false` → lines clipped to the page rect.
    Lines {
        spacing: f64,
        tiling: bool,
    },
    Grid(GridSettings),
    /// Three-direction triangular lattice (30°, 90°, 150°). Tiles
    /// infinitely across the visible canvas like Grid does.
    Isometric {
        spacing: f64,
    },
    /// Pointy-top hexagonal grid that tiles infinitely.
    Hexagonal {
        spacing: f64,
    },
    Image {
        path: PathBuf,
        size_canvas: (f64, f64),
    },
    Pdf {
        path: PathBuf,
        page: u32,
        size_canvas: (f64, f64),
    },
}

#[cfg(feature = "desktop")]
fn pattern_color() -> Color {
    Color {
        r: 90,
        g: 90,
        b: 100,
        a: 200,
    }
}

/// Draw a 1-px screen-space outline around `page_rect` when the visible canvas
/// extends beyond the page on any side and the config is not a tiling grid.
/// Must be called while the canvas transform is active (after `paint` sets it up).
#[cfg(feature = "desktop")]
pub fn draw_page_bounds_outline(
    ctx: &cairo::Context,
    transform: &ViewportTransform,
    config: &BackgroundConfig,
    page_rect: Rect,
    dark_mode: bool,
) {
    if matches!(
        config,
        BackgroundConfig::Grid(_)
            | BackgroundConfig::Isometric { .. }
            | BackgroundConfig::Hexagonal { .. }
            | BackgroundConfig::Dots { tiling: true, .. }
            | BackgroundConfig::Lines { tiling: true, .. }
    ) {
        return;
    }
    let visible = transform.visible_canvas_rect();
    let page_right = page_rect.x + page_rect.width;
    let page_bottom = page_rect.y + page_rect.height;
    let vis_right = visible.x + visible.width;
    let vis_bottom = visible.y + visible.height;
    let extends_beyond = visible.x < page_rect.x
        || visible.y < page_rect.y
        || vis_right > page_right
        || vis_bottom > page_bottom;
    if !extends_beyond {
        return;
    }
    let zoom = transform.zoom().max(1e-6);
    let line_w = 1.0 / zoom;
    ctx.save().ok();
    if dark_mode {
        ctx.set_source_rgba(0.5, 0.5, 0.5, 0.4);
    } else {
        ctx.set_source_rgba(0.6, 0.6, 0.6, 0.5);
    }
    ctx.set_line_width(line_w);
    ctx.rectangle(page_rect.x, page_rect.y, page_rect.width, page_rect.height);
    let _ = ctx.stroke();
    ctx.restore().ok();
}

#[cfg(feature = "desktop")]
pub fn draw_background(
    ctx: &cairo::Context,
    transform: &ViewportTransform,
    config: &BackgroundConfig,
    page_rect: Rect,
) {
    match config {
        BackgroundConfig::Blank => {}
        BackgroundConfig::Grid(settings) => {
            draw_grid(ctx, transform, settings);
        }
        BackgroundConfig::Dots { spacing, tiling } => {
            draw_dots(ctx, transform, page_rect, *spacing, *tiling);
        }
        BackgroundConfig::Lines { spacing, tiling } => {
            draw_lines(ctx, transform, page_rect, *spacing, *tiling);
        }
        BackgroundConfig::Isometric { spacing } => {
            draw_isometric(ctx, transform, *spacing);
        }
        BackgroundConfig::Hexagonal { spacing } => {
            draw_hexagonal(ctx, transform, *spacing);
        }
        BackgroundConfig::Image { path, size_canvas } => {
            draw_image(ctx, page_rect, path, *size_canvas);
        }
        BackgroundConfig::Pdf {
            path,
            page,
            size_canvas,
        } => {
            #[cfg(feature = "pdf")]
            draw_pdf(ctx, page_rect, path, *page, *size_canvas);
            #[cfg(not(feature = "pdf"))]
            {
                let _ = (path, page, size_canvas);
                tracing::warn!("PDF support disabled");
            }
        }
    }
}

#[cfg(feature = "desktop")]
fn draw_dots(
    ctx: &cairo::Context,
    transform: &ViewportTransform,
    page_rect: Rect,
    spacing: f64,
    tiling: bool,
) {
    if spacing <= 0.0 {
        return;
    }
    let zoom = transform.zoom().max(1e-6);
    let radius_canvas = (1.5 / zoom).clamp(0.05, 3.0);

    ctx.save().ok();
    let bounds = if tiling {
        transform.visible_canvas_rect()
    } else {
        ctx.rectangle(page_rect.x, page_rect.y, page_rect.width, page_rect.height);
        ctx.clip();
        page_rect
    };

    let base = pattern_color();
    let base_alpha = base.a as f64 / 255.0;
    let r = base.r as f64 / 255.0;
    let g = base.g as f64 / 255.0;
    let b = base.b as f64 / 255.0;

    ctx.set_source_rgba(r, g, b, base_alpha);
    let x_start = (bounds.x / spacing).floor() * spacing;
    let y_start = (bounds.y / spacing).floor() * spacing;
    let x_end = bounds.x + bounds.width;
    let y_end = bounds.y + bounds.height;
    let mut y = y_start;
    while y <= y_end {
        let mut x = x_start;
        while x <= x_end {
            ctx.arc(x, y, radius_canvas, 0.0, std::f64::consts::TAU);
            let _ = ctx.fill();
            x += spacing;
        }
        y += spacing;
    }

    ctx.restore().ok();
}

#[cfg(feature = "desktop")]
fn draw_lines(
    ctx: &cairo::Context,
    transform: &ViewportTransform,
    page_rect: Rect,
    spacing: f64,
    tiling: bool,
) {
    if spacing <= 0.0 {
        return;
    }
    let zoom = transform.zoom().max(1e-6);

    ctx.save().ok();
    let bounds = if tiling {
        transform.visible_canvas_rect()
    } else {
        ctx.rectangle(page_rect.x, page_rect.y, page_rect.width, page_rect.height);
        ctx.clip();
        page_rect
    };

    let base = pattern_color();
    let base_alpha = base.a as f64 / 255.0;
    let r = base.r as f64 / 255.0;
    let g = base.g as f64 / 255.0;
    let b = base.b as f64 / 255.0;
    ctx.set_line_width(1.0 / zoom);

    ctx.set_source_rgba(r, g, b, base_alpha);
    let y_start = (bounds.y / spacing).floor() * spacing;
    let y_end = bounds.y + bounds.height;
    let x_start = bounds.x;
    let x_end = bounds.x + bounds.width;
    let mut y = y_start;
    while y <= y_end {
        ctx.move_to(x_start, y);
        ctx.line_to(x_end, y);
        y += spacing;
    }
    let _ = ctx.stroke();

    ctx.restore().ok();
}

#[cfg(feature = "desktop")]
#[derive(Clone)]
struct CachedSurface {
    surface: cairo::ImageSurface,
    pixel_w: i32,
    pixel_h: i32,
}

#[cfg(feature = "desktop")]
thread_local! {
    static IMAGE_CACHE: RefCell<HashMap<PathBuf, CachedSurface>> = RefCell::new(HashMap::new());
}

#[cfg(feature = "desktop")]
fn load_surface(path: &Path) -> Option<CachedSurface> {
    let pixbuf = match Pixbuf::from_file(path) {
        Ok(p) => p,
        Err(e) => {
            tracing::warn!("pixbuf failed to load {:?}: {}", path, e);
            return None;
        }
    };
    let w = pixbuf.width();
    let h = pixbuf.height();
    if w <= 0 || h <= 0 {
        return None;
    }
    let surface = match cairo::ImageSurface::create(cairo::Format::ARgb32, w, h) {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!("cairo create surface failed for {:?}: {}", path, e);
            return None;
        }
    };
    {
        let ctx = match cairo::Context::new(&surface) {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!("cairo context failed for surface {:?}: {}", path, e);
                return None;
            }
        };
        ctx.set_source_pixbuf(&pixbuf, 0.0, 0.0);
        let _ = ctx.paint();
    }
    Some(CachedSurface {
        surface,
        pixel_w: w,
        pixel_h: h,
    })
}

#[cfg(feature = "desktop")]
fn with_cached_surface<F: FnOnce(&CachedSurface)>(path: &Path, f: F) {
    IMAGE_CACHE.with(|cache| {
        let mut map = cache.borrow_mut();
        if !map.contains_key(path) {
            if let Some(loaded) = load_surface(path) {
                map.insert(path.to_path_buf(), loaded);
            }
        }
        if let Some(entry) = map.get(path) {
            f(entry);
        }
    });
}

#[cfg(all(feature = "pdf", feature = "desktop"))]
thread_local! {
    static PDF_CACHE: RefCell<HashMap<(PathBuf, u32), CachedSurface>> = RefCell::new(HashMap::new());
}

#[cfg(all(feature = "pdf", feature = "desktop"))]
fn render_pdf_to_surface(path: &Path, page_idx: u32) -> Option<CachedSurface> {
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
    let page = match doc.page(page_idx as i32) {
        Some(p) => p,
        None => {
            tracing::warn!("pdf page {} missing in {}", page_idx, uri);
            return None;
        }
    };
    let (w_pts, h_pts) = page.size();
    // Rasterize at 2x for sharpness on zoom
    let scale = 2.0;
    let pw = (w_pts * scale).ceil().max(1.0) as i32;
    let ph = (h_pts * scale).ceil().max(1.0) as i32;
    let surface = cairo::ImageSurface::create(cairo::Format::ARgb32, pw, ph).ok()?;
    {
        let ctx = cairo::Context::new(&surface).ok()?;
        ctx.set_source_rgb(1.0, 1.0, 1.0);
        let _ = ctx.paint();
        ctx.scale(scale, scale);
        page.render(&ctx);
    }
    Some(CachedSurface {
        surface,
        pixel_w: pw,
        pixel_h: ph,
    })
}

#[cfg(all(feature = "pdf", feature = "desktop"))]
fn draw_pdf(
    ctx: &cairo::Context,
    page_rect: Rect,
    path: &Path,
    page_idx: u32,
    size_canvas: (f64, f64),
) {
    if size_canvas.0 <= 0.0 || size_canvas.1 <= 0.0 {
        return;
    }
    let key = (path.to_path_buf(), page_idx);
    PDF_CACHE.with(|cache| {
        let mut map = cache.borrow_mut();
        if !map.contains_key(&key) {
            if let Some(loaded) = render_pdf_to_surface(path, page_idx) {
                map.insert(key.clone(), loaded);
            }
        }
        if let Some(cached) = map.get(&key) {
            let target_w = page_rect.width.max(size_canvas.0);
            let target_h = page_rect.height.max(size_canvas.1);
            let sx = target_w / cached.pixel_w as f64;
            let sy = target_h / cached.pixel_h as f64;
            ctx.save().ok();
            ctx.rectangle(page_rect.x, page_rect.y, target_w, target_h);
            ctx.clip();
            ctx.translate(page_rect.x, page_rect.y);
            ctx.scale(sx, sy);
            let _ = ctx.set_source_surface(&cached.surface, 0.0, 0.0);
            let _ = ctx.paint();
            ctx.restore().ok();
        }
    });
}

#[cfg(feature = "desktop")]
fn draw_image(ctx: &cairo::Context, page_rect: Rect, path: &Path, size_canvas: (f64, f64)) {
    if size_canvas.0 <= 0.0 || size_canvas.1 <= 0.0 {
        return;
    }
    with_cached_surface(path, |cached| {
        if cached.pixel_w <= 0 || cached.pixel_h <= 0 {
            return;
        }
        let target_w = page_rect.width.max(size_canvas.0);
        let target_h = page_rect.height.max(size_canvas.1);
        let sx = target_w / cached.pixel_w as f64;
        let sy = target_h / cached.pixel_h as f64;

        ctx.save().ok();
        ctx.rectangle(page_rect.x, page_rect.y, target_w, target_h);
        ctx.clip();
        ctx.translate(page_rect.x, page_rect.y);
        ctx.scale(sx, sy);
        let _ = ctx.set_source_surface(&cached.surface, 0.0, 0.0);
        let _ = ctx.paint();
        ctx.restore().ok();
    });
}

/// Draw an equilateral-triangle (isometric) lattice across the visible
/// canvas. `spacing` is the triangle side length — every triangle drawn
/// is the same equilateral shape with that edge length. Tiles infinitely.
///
/// Lattice geometry (pointy-top triangles, vertical edges allowed):
///
///   vertices = (k * spacing/2, m * spacing*√3/2) where k+m is even
///
/// Three line families pass through these vertices:
///   - vertical (90°): x = k * spacing/2     → perpendicular spacing = spacing/2
///   - +30° from horizontal (slope +1/√3): y = x/√3 + c, c step = spacing/√3
///   - −30° from horizontal (slope −1/√3): y = −x/√3 + c, c step = spacing/√3
///
/// All three families have perpendicular distance = (spacing*√3)/4 between
/// adjacent parallels, so triangles formed are equilateral.
#[cfg(feature = "desktop")]
fn draw_isometric(ctx: &cairo::Context, transform: &ViewportTransform, spacing: f64) {
    if spacing <= 0.0 {
        return;
    }
    let visible = transform.visible_canvas_rect();
    let zoom = transform.zoom().max(1e-6);
    let line_w = (1.0 / zoom).clamp(0.04, 0.5);

    let base = pattern_color();
    let base_alpha = base.a as f64 / 255.0;
    let r = base.r as f64 / 255.0;
    let g = base.g as f64 / 255.0;
    let b = base.b as f64 / 255.0;
    ctx.set_line_width(line_w);

    let xa = visible.x;
    let xb = visible.x + visible.width;
    let ya = visible.y;
    let yb = visible.y + visible.height;
    let slope = 1.0 / 3.0_f64.sqrt();

    ctx.set_source_rgba(r, g, b, base_alpha);

    // Perpendicular distance between adjacent parallels in each of the three
    // families equals `spacing / 2`.
    let h = spacing * 0.5;

    let x0 = (xa / h).floor() * h;
    let mut x = x0;
    while x <= xb {
        ctx.move_to(x, ya);
        ctx.line_to(x, yb);
        x += h;
    }

    // ±30° lines: c-step `dc = 2h/√3`, perpendicular distance `dc * cos(30°) = h`.
    let dc = 2.0 * h / 3.0_f64.sqrt();

    let c_min_p = ya - slope * xb;
    let c_max_p = yb - slope * xa;
    let mut c = (c_min_p / dc).floor() * dc;
    while c <= c_max_p {
        ctx.move_to(xa, slope * xa + c);
        ctx.line_to(xb, slope * xb + c);
        c += dc;
    }

    let c_min_n = ya + slope * xa;
    let c_max_n = yb + slope * xb;
    let mut c = (c_min_n / dc).floor() * dc;
    while c <= c_max_n {
        ctx.move_to(xa, -slope * xa + c);
        ctx.line_to(xb, -slope * xb + c);
        c += dc;
    }

    let _ = ctx.stroke();
}

/// Draw a pointy-top hexagonal grid across the visible canvas. `spacing`
/// is the distance between adjacent hex centres along the X axis.
#[cfg(feature = "desktop")]
fn draw_hexagonal(ctx: &cairo::Context, transform: &ViewportTransform, spacing: f64) {
    if spacing <= 0.0 {
        return;
    }
    let visible = transform.visible_canvas_rect();
    let zoom = transform.zoom().max(1e-6);
    let line_w = (1.0 / zoom).clamp(0.04, 0.5);

    let base = pattern_color();
    let base_alpha = base.a as f64 / 255.0;
    let r_col = base.r as f64 / 255.0;
    let g_col = base.g as f64 / 255.0;
    let b_col = base.b as f64 / 255.0;
    ctx.set_line_width(line_w);

    ctx.set_source_rgba(r_col, g_col, b_col, base_alpha);

    // Hex edge length = spacing / √3.
    let r = spacing / 3.0_f64.sqrt();
    let row_h = r * 1.5;
    let col_w = r * 3.0_f64.sqrt();

    let row0 = ((visible.y / row_h).floor() as i64) - 1;
    let row1 = (((visible.y + visible.height) / row_h).ceil() as i64) + 1;
    let col0 = ((visible.x / col_w).floor() as i64) - 1;
    let col1 = (((visible.x + visible.width) / col_w).ceil() as i64) + 1;

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
                ctx.move_to(p0.0, p0.1);
                ctx.line_to(p1.0, p1.1);
            }
        }
    }
    let _ = ctx.stroke();
}
