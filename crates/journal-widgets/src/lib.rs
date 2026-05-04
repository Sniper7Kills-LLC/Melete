//! Vello-based template widget renderer.
//!
//! This crate is the Vello replacement for `journal_canvas::widget_renderer`
//! (Cairo). It depends only on `vello`, `parley`, and `journal-core`, so it
//! can be linked from native (`journal-app`) and from a future WASM web
//! viewer with no GTK / Cairo / SQLite in the dependency closure.
//!
//! Public surface: [`WidgetRenderer`] caches the parley font + layout
//! contexts (per-frame allocation is fine but reusing them is cheaper at
//! steady state) and exposes [`WidgetRenderer::draw_widgets`] which
//! mirrors the original Cairo entry point.

use std::collections::HashMap;

use journal_core::{
    render_title, Color, Rect, TemplateWidget, TitleContext, WidgetKind, WidgetOverride,
    WidgetRect, WidgetStyle,
};
use parley::{Alignment, AlignmentOptions, FontContext, LayoutContext, PositionedLayoutItem, StyleProperty};
use uuid::Uuid;
use vello::kurbo::{Affine, BezPath, Cap, Circle, Ellipse, Join, Rect as KRect, Shape, Stroke as KStroke};
use vello::peniko::{Brush, Color as PColor, Fill};
use vello::Scene;

/// Per-frame context the canvas hands to every template widget's draw fn.
/// Mirrors the field shape of `journal_canvas::WidgetRenderContext` so
/// callers can use the same value with either renderer.
#[derive(Debug, Clone, Default)]
pub struct WidgetRenderContext {
    pub date: Option<chrono::NaiveDate>,
    pub overrides: HashMap<Uuid, WidgetOverride>,
}

fn resolve_date(ctx: &WidgetRenderContext) -> chrono::NaiveDate {
    ctx.date.unwrap_or_else(|| chrono::Local::now().date_naive())
}

pub struct WidgetRenderer {
    font_ctx: FontContext,
    layout_ctx: LayoutContext<Brush>,
}

impl Default for WidgetRenderer {
    fn default() -> Self {
        Self::new()
    }
}

impl WidgetRenderer {
    pub fn new() -> Self {
        Self {
            font_ctx: FontContext::new(),
            layout_ctx: LayoutContext::new(),
        }
    }

    /// Render the "no page selected" placeholder: page-colour fill with a
    /// soft dot-grid backdrop, a centered JOURNAL wordmark underlined in
    /// amber, and the caller-supplied prompt text below. Called by the
    /// canvas when `current_page_id` is None instead of the normal
    /// bg/widgets/strokes scene.
    pub fn draw_placeholder(
        &mut self,
        scene: &mut Scene,
        screen_w: f64,
        screen_h: f64,
        dark_mode: bool,
        text: &str,
    ) {
        // Page-colour fill.
        let bg = if dark_mode {
            vello::peniko::Color::from_rgba8(28, 28, 33, 255)
        } else {
            vello::peniko::Color::from_rgba8(247, 247, 250, 255)
        };
        scene.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            &Brush::Solid(bg),
            None,
            &KRect::new(0.0, 0.0, screen_w, screen_h),
        );

        // Soft dot grid — keeps the surface from reading as "broken / blank".
        let dot_color = if dark_mode {
            vello::peniko::Color::from_rgba8(255, 255, 255, 18)
        } else {
            vello::peniko::Color::from_rgba8(0, 0, 0, 18)
        };
        let spacing: f64 = 28.0;
        let radius: f64 = 1.1;
        let mut dots = BezPath::new();
        let mut y = spacing * 0.5;
        while y < screen_h {
            let mut x = spacing * 0.5;
            while x < screen_w {
                dots.extend(Circle::new((x, y), radius).path_elements(0.05));
                x += spacing;
            }
            y += spacing;
        }
        scene.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            &Brush::Solid(dot_color),
            None,
            &dots,
        );

        // Wordmark — branded "JOURNAL" with letter-spacing and amber rule.
        let wordmark = "JOURNAL";
        let wordmark_size: f32 = (screen_h * 0.10).clamp(34.0, 96.0) as f32;
        let wordmark_color = if dark_mode {
            Color { r: 234, g: 234, b: 240, a: 230 }
        } else {
            Color { r: 36, g: 38, b: 64, a: 240 }
        };
        let wm_band_w = screen_w.min(720.0);
        let wm_x = (screen_w - wm_band_w) * 0.5;
        let wm_y = screen_h * 0.5 - (wordmark_size as f64) * 1.05;
        draw_tracked_text(
            scene,
            &mut self.font_ctx,
            &mut self.layout_ctx,
            Affine::IDENTITY,
            wordmark,
            wordmark_size,
            wm_x,
            wm_y,
            wm_band_w,
            wordmark_color,
            0.18,
        );

        // Amber underline — same accent as `.page-row.current` (#d6a83a).
        let amber = vello::peniko::Color::from_rgba8(214, 168, 58, 235);
        let underline_w = wm_band_w * 0.32;
        let underline_h = (wordmark_size as f64 * 0.10).clamp(2.5, 6.0);
        let underline_x = (screen_w - underline_w) * 0.5;
        let underline_y = wm_y + (wordmark_size as f64) * 1.18;
        scene.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            &Brush::Solid(amber),
            None,
            &KRect::new(
                underline_x,
                underline_y,
                underline_x + underline_w,
                underline_y + underline_h,
            ),
        );

        // Subtitle prompt text — lifted alpha vs prior placeholder so it
        // clears WCAG AA against the dark page colour.
        let subtitle_color = if dark_mode {
            Color { r: 199, g: 199, b: 209, a: 184 }
        } else {
            Color { r: 76, g: 78, b: 102, a: 200 }
        };
        let subtitle_size: f32 = (screen_h * 0.026).clamp(13.0, 20.0) as f32;
        let subtitle_max_w = screen_w * 0.7;
        let subtitle_x = (screen_w - subtitle_max_w) * 0.5;
        let subtitle_y = underline_y + underline_h + (subtitle_size as f64) * 1.5;
        draw_text_runs(
            scene,
            &mut self.font_ctx,
            &mut self.layout_ctx,
            Affine::IDENTITY,
            text,
            subtitle_size,
            subtitle_x,
            subtitle_y,
            subtitle_max_w,
            subtitle_color,
            Alignment::Center,
        );
    }

    /// Append draws for every widget into `scene`. `world_to_screen` is
    /// the canvas → screen affine the caller has already computed for
    /// strokes / backgrounds; widgets share the same transform.
    pub fn draw_widgets(
        &mut self,
        scene: &mut Scene,
        world_to_screen: Affine,
        widgets: &[TemplateWidget],
        _page_rect: Rect,
        render_ctx: &WidgetRenderContext,
    ) {
        for widget in widgets {
            let clip = KRect::new(
                widget.rect.x,
                widget.rect.y,
                widget.rect.x + widget.rect.width,
                widget.rect.y + widget.rect.height,
            );
            // Per-widget clip layer keeps widget content from spilling
            // outside its declared rect — same semantics as the Cairo
            // renderer's `ctx.rectangle(...).clip()`.
            scene.push_layer(Fill::NonZero, vello::peniko::Mix::Normal, 1.0_f32, world_to_screen, &clip);
            self.draw_widget(scene, world_to_screen, widget, render_ctx);
            scene.pop_layer();
        }
    }

    fn draw_widget(
        &mut self,
        scene: &mut Scene,
        transform: Affine,
        widget: &TemplateWidget,
        render_ctx: &WidgetRenderContext,
    ) {
        let r = &widget.rect;
        let style = &widget.style;
        let override_ = render_ctx.overrides.get(&widget.id);

        match &widget.kind {
            WidgetKind::Rectangle => {
                let path = rect_path(r);
                fill_then_stroke(scene, transform, style, &path);
            }
            WidgetKind::Ellipse => {
                let cx = r.x + r.width * 0.5;
                let cy = r.y + r.height * 0.5;
                let rx = r.width * 0.5;
                let ry = r.height * 0.5;
                let path = Ellipse::new((cx, cy), (rx, ry), 0.0).to_path(0.05);
                fill_then_stroke(scene, transform, style, &path);
            }
            WidgetKind::Line { thickness_mm } => {
                let thickness = match override_ {
                    Some(WidgetOverride::Line { thickness_mm }) => *thickness_mm,
                    _ => *thickness_mm,
                };
                let mut path = BezPath::new();
                path.move_to((r.x, r.y));
                path.line_to((r.x + r.width, r.y + r.height));
                let style_stroke = stroke_style(thickness);
                let brush = solid(style.stroke_color);
                scene.stroke(&style_stroke, transform, &brush, None, &path);
            }
            WidgetKind::TextBlock { text, font_size_mm } => {
                let (text, font_size) = match override_ {
                    Some(WidgetOverride::TextBlock { text, font_size_mm }) => {
                        (text.as_str(), *font_size_mm)
                    }
                    _ => (text.as_str(), *font_size_mm),
                };
                let date = resolve_date(render_ctx);
                let expanded = render_title(text, &TitleContext::new(date));
                draw_text_runs(
                    scene,
                    &mut self.font_ctx,
                    &mut self.layout_ctx,
                    transform,
                    &expanded,
                    font_size as f32,
                    r.x,
                    r.y,
                    r.width,
                    style.stroke_color,
                    Alignment::Start,
                );
            }
            WidgetKind::GridRegion { spacing_mm } => {
                let s = match override_ {
                    Some(WidgetOverride::GridRegion { spacing_mm }) => *spacing_mm,
                    _ => *spacing_mm,
                };
                draw_grid_region(scene, transform, r, style, s);
            }
            WidgetKind::LinesRegion { spacing_mm } => {
                let s = match override_ {
                    Some(WidgetOverride::LinesRegion { spacing_mm }) => *spacing_mm,
                    _ => *spacing_mm,
                };
                draw_lines_region(scene, transform, r, style, s);
            }
            WidgetKind::DotsRegion { spacing_mm } => {
                let s = match override_ {
                    Some(WidgetOverride::DotsRegion { spacing_mm }) => *spacing_mm,
                    _ => *spacing_mm,
                };
                draw_dots_region(scene, transform, r, style, s);
            }
            WidgetKind::CalendarMonth => {
                let date = match override_ {
                    Some(WidgetOverride::CalendarMonth { year, month }) => {
                        chrono::NaiveDate::from_ymd_opt(*year, *month, 1)
                            .unwrap_or_else(|| resolve_date(render_ctx))
                    }
                    _ => resolve_date(render_ctx),
                };
                self.draw_calendar_month(scene, transform, r, style, date);
            }
            WidgetKind::Timeline { start_hour, end_hour, slot_minutes } => {
                let (s, e, m) = match override_ {
                    Some(WidgetOverride::Timeline { start_hour, end_hour, slot_minutes }) => {
                        (*start_hour, *end_hour, *slot_minutes)
                    }
                    _ => (*start_hour, *end_hour, *slot_minutes),
                };
                self.draw_timeline_stub(scene, transform, r, style, s, e, m);
            }
            WidgetKind::Checklist { items } => {
                let items = match override_ {
                    Some(WidgetOverride::Checklist { items }) => items.as_slice(),
                    _ => items.as_slice(),
                };
                self.draw_checklist(scene, transform, r, style, items);
            }
            WidgetKind::BigThree => {
                self.draw_big_three(scene, transform, r, style);
            }
            WidgetKind::PriorityList { count } => {
                let n = match override_ {
                    Some(WidgetOverride::PriorityList { count }) => *count,
                    _ => *count,
                };
                self.draw_priority_list(scene, transform, r, style, n);
            }
            WidgetKind::DailyAppointments { start_hour, end_hour } => {
                let (s, e) = match override_ {
                    Some(WidgetOverride::DailyAppointments { start_hour, end_hour }) => {
                        (*start_hour, *end_hour)
                    }
                    _ => (*start_hour, *end_hour),
                };
                self.draw_daily_appointments(scene, transform, r, style, s, e);
            }
            WidgetKind::WeeklyCompass => {
                self.draw_weekly_compass(scene, transform, r, style);
            }
        }
    }

    // ---- Calendar / Timeline / Lists --------------------------------------

    fn draw_calendar_month(
        &mut self,
        scene: &mut Scene,
        transform: Affine,
        r: &WidgetRect,
        style: &WidgetStyle,
        date: chrono::NaiveDate,
    ) {
        use chrono::Datelike;
        let year = date.year();
        let month = date.month();
        let first_dow = chrono::NaiveDate::from_ymd_opt(year, month, 1)
            .map(|d| d.weekday().num_days_from_sunday())
            .unwrap_or(0) as usize;
        let total_days = days_in_month(year, month) as usize;

        let header_h = (r.height * 0.10).max(8.0);
        let dow_h = (r.height * 0.07).max(6.0);
        let cells_y = r.y + header_h + dow_h;
        let cells_h = (r.y + r.height) - cells_y;

        let cols = 7.0;
        let rows = 6.0;
        let cw = r.width / cols;
        let rh = cells_h / rows;

        // Title: month name + year
        let title = format!("{} {}", month_name(month), year);
        draw_text_runs(
            scene,
            &mut self.font_ctx,
            &mut self.layout_ctx,
            transform,
            &title,
            (header_h * 0.7) as f32,
            r.x,
            r.y,
            r.width,
            style.stroke_color,
            Alignment::Center,
        );

        // Day-of-week labels
        let dows = ["S", "M", "T", "W", "T", "F", "S"];
        for (i, label) in dows.iter().enumerate() {
            draw_text_runs(
                scene,
                &mut self.font_ctx,
                &mut self.layout_ctx,
                transform,
                label,
                (dow_h * 0.6) as f32,
                r.x + cw * i as f64,
                r.y + header_h,
                cw,
                style.stroke_color,
                Alignment::Center,
            );
        }

        // Grid + day numbers
        let stroke_style_thin = stroke_style(style.stroke_width_mm.max(0.2));
        let brush = solid(style.stroke_color);
        let mut grid = BezPath::new();
        for c in 0..=7 {
            let x = r.x + cw * c as f64;
            grid.move_to((x, cells_y));
            grid.line_to((x, cells_y + cells_h));
        }
        for rr in 0..=6 {
            let y = cells_y + rh * rr as f64;
            grid.move_to((r.x, y));
            grid.line_to((r.x + r.width, y));
        }
        scene.stroke(&stroke_style_thin, transform, &brush, None, &grid);

        for day in 1..=total_days {
            let idx = first_dow + day - 1;
            let col = idx % 7;
            let row = idx / 7;
            if row >= 6 {
                break;
            }
            let cell_x = r.x + cw * col as f64;
            let cell_y = cells_y + rh * row as f64;
            draw_text_runs(
                scene,
                &mut self.font_ctx,
                &mut self.layout_ctx,
                transform,
                &day.to_string(),
                (rh * 0.45) as f32,
                cell_x + 1.0,
                cell_y + 1.0,
                cw - 2.0,
                style.stroke_color,
                Alignment::Start,
            );
        }
    }

    fn draw_timeline_stub(
        &mut self,
        scene: &mut Scene,
        transform: Affine,
        r: &WidgetRect,
        style: &WidgetStyle,
        start_hour: u8,
        end_hour: u8,
        slot_minutes: u32,
    ) {
        let start_min = (start_hour as u32) * 60;
        let end_min = (end_hour as u32) * 60;
        if end_min <= start_min || slot_minutes == 0 {
            return;
        }
        let total_min = end_min - start_min;
        let slots = total_min.div_ceil(slot_minutes) as f64;
        if slots <= 0.0 {
            return;
        }
        let row_h = r.height / slots;
        let label_w = (r.width * 0.22).clamp(8.0, 30.0);

        let stroke_style_thin = stroke_style(style.stroke_width_mm.max(0.2));
        let brush = solid(style.stroke_color);
        let mut grid = BezPath::new();
        // Vertical separator after labels
        grid.move_to((r.x + label_w, r.y));
        grid.line_to((r.x + label_w, r.y + r.height));
        // Outer frame
        grid.move_to((r.x, r.y));
        grid.line_to((r.x + r.width, r.y));
        grid.line_to((r.x + r.width, r.y + r.height));
        grid.line_to((r.x, r.y + r.height));
        grid.line_to((r.x, r.y));
        for i in 0..=(slots as u32) {
            let y = r.y + row_h * i as f64;
            grid.move_to((r.x, y));
            grid.line_to((r.x + r.width, y));
        }
        scene.stroke(&stroke_style_thin, transform, &brush, None, &grid);

        for i in 0..(slots as u32) {
            let m = start_min + i * slot_minutes;
            if m % 60 != 0 {
                continue;
            }
            let label = format!("{:02}:00", m / 60);
            let y = r.y + row_h * i as f64;
            draw_text_runs(
                scene,
                &mut self.font_ctx,
                &mut self.layout_ctx,
                transform,
                &label,
                (row_h * 0.5).clamp(4.0, 12.0) as f32,
                r.x + 1.0,
                y + 1.0,
                label_w - 2.0,
                style.stroke_color,
                Alignment::Start,
            );
        }
    }

    fn draw_checklist(
        &mut self,
        scene: &mut Scene,
        transform: Affine,
        r: &WidgetRect,
        style: &WidgetStyle,
        items: &[String],
    ) {
        let n = items.len().max(1);
        let row_h = r.height / n as f64;
        let box_size = (row_h * 0.6).clamp(2.0, 8.0);
        let stroke_style_thin = stroke_style(style.stroke_width_mm.max(0.2));
        let brush = solid(style.stroke_color);

        for (i, item) in items.iter().enumerate() {
            let row_y = r.y + row_h * i as f64;
            let cy = row_y + row_h * 0.5;
            let bx = r.x + 0.5;
            let by = cy - box_size * 0.5;
            let box_path = KRect::new(bx, by, bx + box_size, by + box_size);
            scene.stroke(&stroke_style_thin, transform, &brush, None, &box_path);

            draw_text_runs(
                scene,
                &mut self.font_ctx,
                &mut self.layout_ctx,
                transform,
                item,
                (row_h * 0.5).clamp(3.5, 12.0) as f32,
                bx + box_size + 1.5,
                row_y + (row_h - box_size) * 0.5,
                r.width - (box_size + 2.5),
                style.stroke_color,
                Alignment::Start,
            );
        }
    }

    fn draw_big_three(
        &mut self,
        scene: &mut Scene,
        transform: Affine,
        r: &WidgetRect,
        style: &WidgetStyle,
    ) {
        let labels = ["1.", "2.", "3."];
        let n = labels.len() as f64;
        let row_h = r.height / n;
        let stroke_style_thin = stroke_style(style.stroke_width_mm.max(0.2));
        let brush = solid(style.stroke_color);

        let mut grid = BezPath::new();
        for i in 1..3 {
            let y = r.y + row_h * i as f64;
            grid.move_to((r.x, y));
            grid.line_to((r.x + r.width, y));
        }
        scene.stroke(&stroke_style_thin, transform, &brush, None, &grid);

        for (i, label) in labels.iter().enumerate() {
            let row_y = r.y + row_h * i as f64;
            draw_text_runs(
                scene,
                &mut self.font_ctx,
                &mut self.layout_ctx,
                transform,
                label,
                (row_h * 0.4) as f32,
                r.x + 1.0,
                row_y + 1.0,
                r.width - 2.0,
                style.stroke_color,
                Alignment::Start,
            );
        }
    }

    fn draw_priority_list(
        &mut self,
        scene: &mut Scene,
        transform: Affine,
        r: &WidgetRect,
        style: &WidgetStyle,
        count: u32,
    ) {
        let n = count.max(1) as f64;
        let row_h = r.height / n;
        let stroke_style_thin = stroke_style(style.stroke_width_mm.max(0.2));
        let brush = solid(style.stroke_color);

        let mut grid = BezPath::new();
        for i in 1..(count as i32) {
            let y = r.y + row_h * i as f64;
            grid.move_to((r.x, y));
            grid.line_to((r.x + r.width, y));
        }
        scene.stroke(&stroke_style_thin, transform, &brush, None, &grid);

        for i in 0..count {
            let row_y = r.y + row_h * i as f64;
            draw_text_runs(
                scene,
                &mut self.font_ctx,
                &mut self.layout_ctx,
                transform,
                &format!("{}.", i + 1),
                (row_h * 0.45) as f32,
                r.x + 1.0,
                row_y + 1.0,
                r.width - 2.0,
                style.stroke_color,
                Alignment::Start,
            );
        }
    }

    fn draw_daily_appointments(
        &mut self,
        scene: &mut Scene,
        transform: Affine,
        r: &WidgetRect,
        style: &WidgetStyle,
        start_hour: u8,
        end_hour: u8,
    ) {
        let start = start_hour as i32;
        let end = end_hour as i32;
        if end <= start {
            return;
        }
        let hours = (end - start) as f64;
        let row_h = r.height / hours;
        let label_w = (r.width * 0.18).clamp(6.0, 24.0);

        let stroke_style_thin = stroke_style(style.stroke_width_mm.max(0.2));
        let brush = solid(style.stroke_color);
        let mut grid = BezPath::new();
        grid.move_to((r.x + label_w, r.y));
        grid.line_to((r.x + label_w, r.y + r.height));
        for i in 0..=(hours as u32) {
            let y = r.y + row_h * i as f64;
            grid.move_to((r.x, y));
            grid.line_to((r.x + r.width, y));
        }
        scene.stroke(&stroke_style_thin, transform, &brush, None, &grid);

        for i in 0..(hours as u32) {
            let label = format!("{}:00", start + i as i32);
            let row_y = r.y + row_h * i as f64;
            draw_text_runs(
                scene,
                &mut self.font_ctx,
                &mut self.layout_ctx,
                transform,
                &label,
                (row_h * 0.4) as f32,
                r.x + 1.0,
                row_y + 1.0,
                label_w - 2.0,
                style.stroke_color,
                Alignment::Start,
            );
        }
    }

    fn draw_weekly_compass(
        &mut self,
        scene: &mut Scene,
        transform: Affine,
        r: &WidgetRect,
        style: &WidgetStyle,
    ) {
        let cx = r.x + r.width * 0.5;
        let cy = r.y + r.height * 0.5;
        let stroke_style_thin = stroke_style(style.stroke_width_mm.max(0.3));
        let brush = solid(style.stroke_color);
        let mut grid = BezPath::new();
        grid.move_to((cx, r.y));
        grid.line_to((cx, r.y + r.height));
        grid.move_to((r.x, cy));
        grid.line_to((r.x + r.width, cy));
        scene.stroke(&stroke_style_thin, transform, &brush, None, &grid);

        let labels = ["Roles", "Goals", "Plans", "Reflect"];
        let positions = [
            (r.x, r.y),
            (cx, r.y),
            (r.x, cy),
            (cx, cy),
        ];
        let cell_w = r.width * 0.5;
        for (label, (qx, qy)) in labels.iter().zip(positions.iter()) {
            draw_text_runs(
                scene,
                &mut self.font_ctx,
                &mut self.layout_ctx,
                transform,
                label,
                (r.height * 0.07).clamp(3.0, 10.0) as f32,
                *qx + 1.0,
                *qy + 1.0,
                cell_w - 2.0,
                style.stroke_color,
                Alignment::Start,
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn solid(c: Color) -> Brush {
    Brush::Solid(PColor::from_rgba8(c.r, c.g, c.b, c.a))
}

fn rect_path(r: &WidgetRect) -> KRect {
    KRect::new(r.x, r.y, r.x + r.width, r.y + r.height)
}

fn stroke_style(width: f64) -> KStroke {
    let mut s = KStroke::new(width);
    s.start_cap = Cap::Butt;
    s.end_cap = Cap::Butt;
    s.join = Join::Miter;
    s
}

fn fill_then_stroke(scene: &mut Scene, transform: Affine, style: &WidgetStyle, path: &impl Shape) {
    if let Some(fill) = style.fill_color {
        let brush = solid(fill);
        scene.fill(Fill::NonZero, transform, &brush, None, path);
    }
    let brush = solid(style.stroke_color);
    let style_stroke = stroke_style(style.stroke_width_mm);
    scene.stroke(&style_stroke, transform, &brush, None, path);
}

fn days_in_month(year: i32, month: u32) -> u32 {
    let next = if month == 12 {
        chrono::NaiveDate::from_ymd_opt(year + 1, 1, 1)
    } else {
        chrono::NaiveDate::from_ymd_opt(year, month + 1, 1)
    };
    let first = chrono::NaiveDate::from_ymd_opt(year, month, 1);
    match (first, next) {
        (Some(a), Some(b)) => (b - a).num_days() as u32,
        _ => 30,
    }
}

fn month_name(m: u32) -> &'static str {
    match m {
        1 => "January", 2 => "February", 3 => "March", 4 => "April",
        5 => "May", 6 => "June", 7 => "July", 8 => "August",
        9 => "September", 10 => "October", 11 => "November", 12 => "December",
        _ => "",
    }
}

fn draw_grid_region(
    scene: &mut Scene,
    transform: Affine,
    r: &WidgetRect,
    style: &WidgetStyle,
    spacing: f64,
) {
    if spacing <= 0.0 {
        return;
    }
    let mut path = BezPath::new();
    let mut x = r.x;
    while x <= r.x + r.width {
        path.move_to((x, r.y));
        path.line_to((x, r.y + r.height));
        x += spacing;
    }
    let mut y = r.y;
    while y <= r.y + r.height {
        path.move_to((r.x, y));
        path.line_to((r.x + r.width, y));
        y += spacing;
    }
    let s = stroke_style(style.stroke_width_mm.max(0.2));
    let brush = solid(style.stroke_color);
    scene.stroke(&s, transform, &brush, None, &path);
}

fn draw_lines_region(
    scene: &mut Scene,
    transform: Affine,
    r: &WidgetRect,
    style: &WidgetStyle,
    spacing: f64,
) {
    if spacing <= 0.0 {
        return;
    }
    let mut path = BezPath::new();
    let mut y = r.y;
    while y <= r.y + r.height {
        path.move_to((r.x, y));
        path.line_to((r.x + r.width, y));
        y += spacing;
    }
    let s = stroke_style(style.stroke_width_mm.max(0.2));
    let brush = solid(style.stroke_color);
    scene.stroke(&s, transform, &brush, None, &path);
}

fn draw_dots_region(
    scene: &mut Scene,
    transform: Affine,
    r: &WidgetRect,
    style: &WidgetStyle,
    spacing: f64,
) {
    if spacing <= 0.0 {
        return;
    }
    let radius = (style.stroke_width_mm * 0.6).max(0.15);
    let mut path = BezPath::new();
    let mut y = r.y;
    while y <= r.y + r.height {
        let mut x = r.x;
        while x <= r.x + r.width {
            path.extend(Circle::new((x, y), radius).path_elements(0.05));
            x += spacing;
        }
        y += spacing;
    }
    let brush = solid(style.stroke_color);
    scene.fill(Fill::NonZero, transform, &brush, None, &path);
}

// ---------------------------------------------------------------------------
// Text via parley → vello glyphs
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
fn draw_text_runs(
    scene: &mut Scene,
    font_ctx: &mut FontContext,
    layout_ctx: &mut LayoutContext<Brush>,
    transform: Affine,
    text: &str,
    font_size: f32,
    x: f64,
    y: f64,
    max_width: f64,
    color: Color,
    alignment: Alignment,
) {
    draw_text_runs_inner(
        scene, font_ctx, layout_ctx, transform, text, font_size, x, y, max_width, color, alignment,
        0.0,
    );
}

#[allow(clippy::too_many_arguments)]
fn draw_tracked_text(
    scene: &mut Scene,
    font_ctx: &mut FontContext,
    layout_ctx: &mut LayoutContext<Brush>,
    transform: Affine,
    text: &str,
    font_size: f32,
    x: f64,
    y: f64,
    max_width: f64,
    color: Color,
    tracking_em: f32,
) {
    draw_text_runs_inner(
        scene, font_ctx, layout_ctx, transform, text, font_size, x, y, max_width, color,
        Alignment::Center, font_size * tracking_em,
    );
}

#[allow(clippy::too_many_arguments)]
fn draw_text_runs_inner(
    scene: &mut Scene,
    font_ctx: &mut FontContext,
    layout_ctx: &mut LayoutContext<Brush>,
    transform: Affine,
    text: &str,
    font_size: f32,
    x: f64,
    y: f64,
    max_width: f64,
    color: Color,
    alignment: Alignment,
    letter_spacing_px: f32,
) {
    if text.is_empty() || font_size <= 0.0 {
        return;
    }
    let brush: Brush = solid(color);
    let mut builder = layout_ctx.ranged_builder(font_ctx, text, 1.0, true);
    builder.push_default(StyleProperty::FontSize(font_size));
    builder.push_default(StyleProperty::Brush(brush.clone()));
    if letter_spacing_px != 0.0 {
        builder.push_default(StyleProperty::LetterSpacing(letter_spacing_px));
    }
    let mut layout = builder.build(text);
    layout.break_all_lines(Some(max_width as f32));
    layout.align(alignment, AlignmentOptions::default());

    for line in layout.lines() {
        for item in line.items() {
            let glyph_run = match item {
                PositionedLayoutItem::GlyphRun(g) => g,
                PositionedLayoutItem::InlineBox(_) => continue,
            };
            let mut run_x = glyph_run.offset() as f64 + x;
            let baseline = glyph_run.baseline() as f64 + y;
            let run = glyph_run.run();
            let font = run.font().clone();
            let font_size = run.font_size();
            let style_brush = glyph_run.style().brush.clone();
            let glyphs: Vec<vello::Glyph> = glyph_run
                .glyphs()
                .map(|g| {
                    let gx = run_x;
                    run_x += g.advance as f64;
                    vello::Glyph {
                        id: g.id as u32,
                        x: gx as f32,
                        y: (baseline - g.y as f64) as f32,
                    }
                })
                .collect();
            scene
                .draw_glyphs(&font)
                .font_size(font_size)
                .brush(&style_brush)
                .transform(transform)
                .draw(Fill::NonZero, glyphs.into_iter());
        }
    }
}
