use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use gtk4::cairo;
use gtk4::gdk_pixbuf::Pixbuf;
use gtk4::prelude::*;
use journal_core::{Color, Rect};

use crate::grid_renderer::{draw_grid, GridSettings};
use crate::viewport_transform::ViewportTransform;

#[derive(Debug, Clone)]
pub enum BackgroundConfig {
    Blank,
    Dots { spacing: f64 },
    Lines { spacing: f64 },
    Grid(GridSettings),
    Image { path: PathBuf, size_canvas: (f64, f64) },
}

fn pattern_color() -> Color {
    Color { r: 90, g: 90, b: 100, a: 200 }
}

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
        BackgroundConfig::Dots { spacing } => {
            draw_dots(ctx, transform, page_rect, *spacing);
        }
        BackgroundConfig::Lines { spacing } => {
            draw_lines(ctx, transform, page_rect, *spacing);
        }
        BackgroundConfig::Image { path, size_canvas } => {
            draw_image(ctx, page_rect, path, *size_canvas);
        }
    }
}

fn set_pattern_source(ctx: &cairo::Context) {
    let c = pattern_color();
    ctx.set_source_rgba(
        c.r as f64 / 255.0,
        c.g as f64 / 255.0,
        c.b as f64 / 255.0,
        c.a as f64 / 255.0,
    );
}

fn draw_dots(ctx: &cairo::Context, transform: &ViewportTransform, page_rect: Rect, spacing: f64) {
    if spacing <= 0.0 {
        return;
    }
    let zoom = transform.zoom().max(1e-6);
    let radius_canvas = (1.5 / zoom).clamp(0.05, 3.0);

    ctx.save().ok();
    ctx.rectangle(page_rect.x, page_rect.y, page_rect.width, page_rect.height);
    ctx.clip();

    set_pattern_source(ctx);

    let mut y = page_rect.y;
    while y <= page_rect.y + page_rect.height {
        let mut x = page_rect.x;
        while x <= page_rect.x + page_rect.width {
            ctx.arc(x, y, radius_canvas, 0.0, std::f64::consts::TAU);
            let _ = ctx.fill();
            x += spacing;
        }
        y += spacing;
    }

    ctx.restore().ok();
}

fn draw_lines(ctx: &cairo::Context, transform: &ViewportTransform, page_rect: Rect, spacing: f64) {
    if spacing <= 0.0 {
        return;
    }
    let zoom = transform.zoom().max(1e-6);

    ctx.save().ok();
    ctx.rectangle(page_rect.x, page_rect.y, page_rect.width, page_rect.height);
    ctx.clip();

    set_pattern_source(ctx);
    ctx.set_line_width(1.0 / zoom);

    let mut y = page_rect.y;
    while y <= page_rect.y + page_rect.height {
        ctx.move_to(page_rect.x, y);
        ctx.line_to(page_rect.x + page_rect.width, y);
        y += spacing;
    }
    let _ = ctx.stroke();

    ctx.restore().ok();
}

#[derive(Clone)]
struct CachedSurface {
    surface: cairo::ImageSurface,
    pixel_w: i32,
    pixel_h: i32,
}

thread_local! {
    static IMAGE_CACHE: RefCell<HashMap<PathBuf, CachedSurface>> = RefCell::new(HashMap::new());
}

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
    let surface =
        match cairo::ImageSurface::create(cairo::Format::ARgb32, w, h) {
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
    Some(CachedSurface { surface, pixel_w: w, pixel_h: h })
}

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

fn draw_image(
    ctx: &cairo::Context,
    page_rect: Rect,
    path: &Path,
    size_canvas: (f64, f64),
) {
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
