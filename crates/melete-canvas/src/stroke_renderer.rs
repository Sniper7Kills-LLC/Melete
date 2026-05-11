use gtk4::cairo;
use melete_core::{BlendMode, PenSettings, Rect, Stroke, StrokePoint, ToolStyle};

use crate::viewport_transform::ViewportTransform;

fn rects_intersect(a: &Rect, b: &Rect) -> bool {
    a.x < b.x + b.width && b.x < a.x + a.width && a.y < b.y + b.height && b.y < a.y + a.height
}

fn set_color(ctx: &cairo::Context, c: melete_core::Color, opacity: f32) {
    let alpha = (c.a as f64 / 255.0) * opacity.clamp(0.0, 1.0) as f64;
    ctx.set_source_rgba(
        c.r as f64 / 255.0,
        c.g as f64 / 255.0,
        c.b as f64 / 255.0,
        alpha,
    );
}

fn blend_to_operator(mode: BlendMode) -> cairo::Operator {
    match mode {
        BlendMode::Normal => cairo::Operator::Over,
        BlendMode::Multiply => cairo::Operator::Multiply,
        BlendMode::Screen => cairo::Operator::Screen,
        BlendMode::Overlay => cairo::Operator::Overlay,
        BlendMode::Darken => cairo::Operator::Darken,
        BlendMode::Lighten => cairo::Operator::Lighten,
        BlendMode::Erase => cairo::Operator::DestOut,
    }
}

/// Draw a single stroke. Cairo context must already have viewport transform
/// applied (canvas-space drawing). Dispatches to a per-`brush_style`
/// renderer so each tool produces a visually distinct mark.
pub fn draw_stroke(ctx: &cairo::Context, transform: &ViewportTransform, stroke: &Stroke) -> bool {
    let visible = transform.visible_canvas_rect();
    if !rects_intersect(&visible, &stroke.bounding_box) {
        return false;
    }
    if stroke.points.is_empty() {
        return false;
    }

    let pen = stroke.pen;
    ctx.save().ok();
    ctx.set_operator(blend_to_operator(pen.blend_mode));

    match pen.brush_style {
        ToolStyle::Pen | ToolStyle::Highlighter => draw_smooth(ctx, stroke, &pen),
        ToolStyle::Pencil => draw_pencil(ctx, stroke, &pen),
        ToolStyle::Paintbrush => draw_paintbrush(ctx, stroke, &pen),
        ToolStyle::SprayCan => draw_spray(ctx, stroke, &pen),
        ToolStyle::Calligraphy => draw_calligraphy(ctx, stroke, &pen),
    }

    ctx.restore().ok();
    true
}

/// Smooth quadratic-through-midpoints path with pressure-tapered width.
/// Used for Pen and Highlighter (the only differences come from blend mode
/// and the per-tool opacity/width multipliers applied at stroke creation).
fn draw_smooth(ctx: &cairo::Context, stroke: &Stroke, pen: &PenSettings) {
    set_color(ctx, pen.color, pen.opacity);
    ctx.set_line_cap(cairo::LineCap::Round);
    ctx.set_line_join(cairo::LineJoin::Round);

    let zoc = stroke.zoom_at_creation.max(1e-6);
    let canvas_width_full = pen.base_width / zoc;

    let pts = &stroke.points;
    let n = pts.len();

    if n == 1 {
        let p = &pts[0];
        let r = canvas_width_full * (p.pressure.max(0.05) as f64) * 0.5;
        ctx.arc(p.x, p.y, r, 0.0, std::f64::consts::TAU);
        let _ = ctx.fill();
        return;
    }

    if n == 2 {
        let a = &pts[0];
        let b = &pts[1];
        let avg_pressure = ((a.pressure + b.pressure) * 0.5).max(0.05) as f64;
        ctx.set_line_width(canvas_width_full * avg_pressure);
        ctx.move_to(a.x, a.y);
        ctx.line_to(b.x, b.y);
        let _ = ctx.stroke();
        return;
    }

    for i in 0..n - 1 {
        let p0 = &pts[i];
        let p1 = &pts[i + 1];
        let avg_pressure = ((p0.pressure + p1.pressure) * 0.5).max(0.05) as f64;
        ctx.set_line_width(canvas_width_full * avg_pressure);

        if i == 0 {
            let mid_x = (p0.x + p1.x) * 0.5;
            let mid_y = (p0.y + p1.y) * 0.5;
            ctx.move_to(p0.x, p0.y);
            ctx.curve_to(p0.x, p0.y, p0.x, p0.y, mid_x, mid_y);
        } else if i == n - 2 {
            let prev = &pts[i - 1];
            let mid_x = (prev.x + p0.x) * 0.5;
            let mid_y = (prev.y + p0.y) * 0.5;
            ctx.move_to(mid_x, mid_y);
            ctx.curve_to(p0.x, p0.y, p0.x, p0.y, p1.x, p1.y);
        } else {
            let prev = &pts[i - 1];
            let prev_mid_x = (prev.x + p0.x) * 0.5;
            let prev_mid_y = (prev.y + p0.y) * 0.5;
            let next_mid_x = (p0.x + p1.x) * 0.5;
            let next_mid_y = (p0.y + p1.y) * 0.5;
            ctx.move_to(prev_mid_x, prev_mid_y);
            ctx.curve_to(p0.x, p0.y, p0.x, p0.y, next_mid_x, next_mid_y);
        }
        let _ = ctx.stroke();
    }
}

/// Pencil — graphite simulation. Photoshop "Pencil tool" hard-edge feel
/// plus a Krita/GIMP "pencil" speckled grain. The visible mark is a thin
/// constant-width hard line plus many tiny low-alpha specks scattered
/// perpendicular to the path; pressure modulates speck density and core
/// alpha rather than line width (real pencils don't get fatter under
/// pressure, they get darker and grainier).
fn draw_pencil(ctx: &cairo::Context, stroke: &Stroke, pen: &PenSettings) {
    let zoc = stroke.zoom_at_creation.max(1e-6);
    let core_w = (pen.base_width / zoc).clamp(0.6, 1.6);
    let speck_radius = core_w * 1.6;

    let pts = &stroke.points;
    if pts.is_empty() {
        return;
    }

    // 1) Hard, thin core line — anti-aliased thin enough that it reads as
    //    "graphite line", not as a brush stroke.
    set_color(ctx, pen.color, pen.opacity * 0.85);
    ctx.set_line_cap(cairo::LineCap::Round);
    ctx.set_line_join(cairo::LineJoin::Round);
    ctx.set_line_width(core_w);
    ctx.move_to(pts[0].x, pts[0].y);
    for p in pts.iter().skip(1) {
        ctx.line_to(p.x, p.y);
    }
    let _ = ctx.stroke();

    // 2) Graphite specks. For each segment scatter a number of tiny dots
    //    within a band perpendicular to the path. Density scales with
    //    pressure so harder presses → more grain, denser fill.
    for i in 0..pts.len() - 1 {
        let a = &pts[i];
        let b = &pts[i + 1];
        let dx = b.x - a.x;
        let dy = b.y - a.y;
        let len = (dx * dx + dy * dy).sqrt();
        if len < 1e-3 {
            continue;
        }
        let nx = -dy / len;
        let ny = dx / len;
        let press = ((a.pressure + b.pressure) * 0.5).clamp(0.1, 1.0) as f64;
        // Speck count per segment, roughly 1 per pixel of path length,
        // scaled by pressure.
        let count = ((len * 1.2).round() as i32 * (1 + (press * 4.0) as i32)).clamp(2, 60);
        for k in 0..count {
            let seed = (i as f64) * 11.7 + k as f64 * 0.91 + a.x * 0.017 + a.y * 0.031;
            let t = pseudo_noise(seed, seed * 0.7);
            let lateral = (pseudo_noise(seed * 1.9, seed * 2.3) - 0.5) * speck_radius * 2.0;
            let alpha = pen.opacity as f64
                * (0.20 + 0.55 * pseudo_noise(seed * 3.1, seed * 0.4))
                * (0.6 + 0.4 * press);
            let dot_r = (0.25 + 0.45 * pseudo_noise(seed * 4.2, seed * 5.5)) * core_w;
            let cx = a.x + dx * t + nx * lateral;
            let cy = a.y + dy * t + ny * lateral;
            set_color(ctx, pen.color, alpha as f32);
            ctx.arc(cx, cy, dot_r, 0.0, std::f64::consts::TAU);
            let _ = ctx.fill();
        }
    }
}

/// Paintbrush — soft round brush. Stamps radial-gradient dabs (full alpha
/// at center, 0 at the edge) along the path at sub-radius spacing so they
/// blend into a continuous soft-edged stroke. Photoshop's classic soft
/// round brush is built the same way.
fn draw_paintbrush(ctx: &cairo::Context, stroke: &Stroke, pen: &PenSettings) {
    let zoc = stroke.zoom_at_creation.max(1e-6);
    let radius_full = pen.base_width / zoc * 0.5;

    let pts = &stroke.points;
    if pts.is_empty() {
        return;
    }
    if pts.len() == 1 {
        let p = &pts[0];
        stamp_soft_dab(
            ctx,
            p.x,
            p.y,
            radius_full * (p.pressure.max(0.2) as f64),
            pen,
        );
        return;
    }

    // Resample the polyline at a fixed step (about 1/4 of the brush
    // radius) so dabs overlap heavily and the result reads as one
    // continuous stroke regardless of input sampling rate.
    let mut prev = &pts[0];
    let mut prev_press = prev.pressure as f64;
    stamp_soft_dab(ctx, prev.x, prev.y, radius_full * prev_press.max(0.2), pen);

    for cur in pts.iter().skip(1) {
        let dx = cur.x - prev.x;
        let dy = cur.y - prev.y;
        let len = (dx * dx + dy * dy).sqrt();
        let press = cur.pressure as f64;
        let step = (radius_full * 0.25).max(0.5);
        let n = ((len / step).ceil() as i32).max(1);
        for k in 1..=n {
            let t = k as f64 / n as f64;
            let x = prev.x + dx * t;
            let y = prev.y + dy * t;
            let p = (prev_press + (press - prev_press) * t).max(0.2);
            stamp_soft_dab(ctx, x, y, radius_full * p, pen);
        }
        prev = cur;
        prev_press = press;
    }
}

fn stamp_soft_dab(ctx: &cairo::Context, x: f64, y: f64, radius: f64, pen: &PenSettings) {
    if radius <= 0.0 {
        return;
    }
    let gradient = cairo::RadialGradient::new(x, y, 0.0, x, y, radius);
    let c = pen.color;
    let r = c.r as f64 / 255.0;
    let g = c.g as f64 / 255.0;
    let b = c.b as f64 / 255.0;
    let core_alpha = (c.a as f64 / 255.0) * pen.opacity.clamp(0.0, 1.0) as f64;
    // Quadratic falloff: full alpha at center, ~0.6 at half-radius, 0 at
    // edge. Cheap enough for many dabs per stroke.
    gradient.add_color_stop_rgba(0.0, r, g, b, core_alpha);
    gradient.add_color_stop_rgba(0.6, r, g, b, core_alpha * 0.55);
    gradient.add_color_stop_rgba(1.0, r, g, b, 0.0);
    ctx.set_source(&gradient).ok();
    ctx.arc(x, y, radius, 0.0, std::f64::consts::TAU);
    let _ = ctx.fill();
}

/// Spray-can — dense scatter with falloff toward the edge. Each path
/// point seeds many small dots whose distance from the center is biased
/// toward the middle (so the spray looks like a real airbrush cloud, not
/// a uniform disc).
fn draw_spray(ctx: &cairo::Context, stroke: &Stroke, pen: &PenSettings) {
    set_color(ctx, pen.color, pen.opacity);

    let zoc = stroke.zoom_at_creation.max(1e-6);
    let radius = pen.base_width / zoc * 0.5;
    let dot_radius = (radius * 0.06).max(0.35);
    let dots_per_point = 36;

    for (idx, p) in stroke.points.iter().enumerate() {
        let press = (p.pressure.max(0.2) as f64).min(1.0);
        let scatter = radius * press;
        for k in 0..dots_per_point {
            let seed = (idx as f64) * 7.31 + k as f64 * 1.97 + p.x * 0.013 + p.y * 0.029;
            let theta = pseudo_noise(seed, seed * 1.3) * std::f64::consts::TAU;
            // Square the noise → density biased toward center, like a
            // real airbrush hotspot.
            let r_unit = pseudo_noise(seed * 2.7, seed * 0.8);
            let r = scatter * (r_unit * r_unit);
            let dx = theta.cos() * r;
            let dy = theta.sin() * r;
            ctx.arc(p.x + dx, p.y + dy, dot_radius, 0.0, std::f64::consts::TAU);
            let _ = ctx.fill();
        }
    }
}

/// Calligraphy — flat-tip nib simulated as a variable-width filled
/// polygon. Width at every path vertex is computed from the angle of the
/// path tangent vs. a fixed nib axis (45°), so strokes parallel to the
/// nib are thin and perpendicular ones are at full width. Building the
/// outline as a single filled polygon (instead of stroking each segment
/// at its own line width) avoids the "blocky" look you get when adjacent
/// segments differ in width — adjacent quads don't share their offset
/// edges, so the boundary is continuous.
fn draw_calligraphy(ctx: &cairo::Context, stroke: &Stroke, pen: &PenSettings) {
    set_color(ctx, pen.color, pen.opacity);

    let zoc = stroke.zoom_at_creation.max(1e-6);
    let max_width = pen.base_width / zoc;
    let nib_angle: f64 = std::f64::consts::FRAC_PI_4;
    let min_ratio: f64 = 0.18;

    let pts = &stroke.points;
    if pts.is_empty() {
        return;
    }
    if pts.len() == 1 {
        let p = &pts[0];
        ctx.arc(
            p.x,
            p.y,
            max_width * 0.5 * min_ratio,
            0.0,
            std::f64::consts::TAU,
        );
        let _ = ctx.fill();
        return;
    }

    // Resample the path so width transitions are gradual: insert
    // interpolated samples whenever consecutive input points are far
    // apart relative to the nib width.
    let max_step = (max_width * 0.5).max(1.0);
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

    // Compute per-vertex tangent (avg of neighbouring segments) and the
    // width-at-this-vertex. Then offset perpendicular to the tangent by
    // ±width/2 to build the outline polygon.
    let n = samples.len();
    let mut left: Vec<(f64, f64)> = Vec::with_capacity(n);
    let mut right: Vec<(f64, f64)> = Vec::with_capacity(n);
    for i in 0..n {
        let (x, y, press) = samples[i];
        let (tx, ty) = if i == 0 {
            let (nx, ny, _) = samples[1];
            (nx - x, ny - y)
        } else if i == n - 1 {
            let (px, py, _) = samples[i - 1];
            (x - px, y - py)
        } else {
            let (px, py, _) = samples[i - 1];
            let (nx, ny, _) = samples[i + 1];
            (nx - px, ny - py)
        };
        let tlen = (tx * tx + ty * ty).sqrt().max(1e-6);
        let dir = ty.atan2(tx);
        let rel = (dir - nib_angle).sin().abs();
        let press_clamped = press.max(0.3);
        let w = max_width * (min_ratio + (1.0 - min_ratio) * rel) * press_clamped * 0.5;
        // Perpendicular = rotate tangent 90°.
        let nxn = -ty / tlen;
        let nyn = tx / tlen;
        left.push((x + nxn * w, y + nyn * w));
        right.push((x - nxn * w, y - nyn * w));
    }

    // Walk left-side forward, right-side backward → closed outline.
    ctx.move_to(left[0].0, left[0].1);
    for &(x, y) in left.iter().skip(1) {
        ctx.line_to(x, y);
    }
    for &(x, y) in right.iter().rev() {
        ctx.line_to(x, y);
    }
    ctx.close_path();
    let _ = ctx.fill();
}

/// Cheap deterministic pseudo-noise in [0, 1). Good enough for pencil
/// grain and spray scatter — not for cryptographic randomness. Uses the
/// fract-of-sin hash that's commonplace in shader-style rendering.
fn pseudo_noise(x: f64, y: f64) -> f64 {
    let v = (x * 12.9898 + y * 78.233).sin() * 43758.5453;
    let f = v - v.floor();
    f.abs()
}

#[allow(dead_code)]
fn _suppress_unused_strokepoint(_p: &StrokePoint) {}
