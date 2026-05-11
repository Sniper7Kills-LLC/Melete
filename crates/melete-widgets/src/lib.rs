#![allow(clippy::too_many_arguments)]
//! Vello-based template widget renderer.
//!
//! This crate is the Vello replacement for `melete_canvas::widget_renderer`
//! (Cairo). It depends only on `vello`, `parley`, and `journal-core`, so it
//! can be linked from native (`journal-app`) and from a future WASM web
//! viewer with no GTK / Cairo / SQLite in the dependency closure.
//!
//! Public surface: [`WidgetRenderer`] caches the parley font + layout
//! contexts (per-frame allocation is fine but reusing them is cheaper at
//! steady state) and exposes [`WidgetRenderer::draw_widgets`] which
//! mirrors the original Cairo entry point.

// Re-export the bits of vello that callers need to pre-build / cache
// scenes that get appended into ours. Keeps `journal-app` from having
// to declare a direct vello dependency.
pub use vello::kurbo::Affine as VelloAffine;
pub use vello::Scene as VelloScene;

use std::collections::HashMap;

use melete_core::{
    render_title, Color, Rect, TemplateWidget, TitleContext, WidgetData, WidgetKind,
    WidgetOverride, WidgetPayload, WidgetRect, WidgetStyle,
};
use parley::{
    Alignment, AlignmentOptions, FontContext, LayoutContext, PositionedLayoutItem, StyleProperty,
};
use uuid::Uuid;
use vello::kurbo::{
    Affine, BezPath, Cap, Circle, Ellipse, Join, Rect as KRect, Shape, Stroke as KStroke,
};
use vello::peniko::{Brush, Color as PColor, Fill};
use vello::Scene;

/// Per-frame context the canvas hands to every template widget's draw fn.
/// Mirrors the field shape of `melete_canvas::WidgetRenderContext` so
/// callers can use the same value with either renderer.
#[derive(Debug, Clone, Default)]
pub struct WidgetRenderContext {
    pub date: Option<chrono::NaiveDate>,
    pub overrides: HashMap<Uuid, WidgetOverride>,
    /// Cached fetch payloads for this page's fetch-backed widgets,
    /// keyed by `TemplateWidget.id`. Renderers read from here when
    /// drawing widgets like Weather / Quote / RssHeadline; the actual
    /// fetch is owned by the app-layer fetcher.
    pub widget_data: HashMap<Uuid, WidgetData>,
    /// Set when the host UI is in dark mode. Widgets use this to
    /// auto-invert dark stroke colors to a bright foreground so text
    /// stays legible against the dark page background.
    pub dark_mode: bool,
}

/// Resolve a widget's style for the current render context. In dark
/// mode, stroke / fill colors that are too dark to read against the
/// dim-teal page background are flipped to a bright foreground while
/// preserving alpha. Light-mode rendering returns the style unchanged.
fn effective_style(style: &WidgetStyle, dark_mode: bool) -> WidgetStyle {
    if !dark_mode {
        return style.clone();
    }
    WidgetStyle {
        stroke_color: brighten_for_dark(style.stroke_color),
        fill_color: style.fill_color.map(brighten_for_dark),
        stroke_width_mm: style.stroke_width_mm,
    }
}

fn brighten_for_dark(c: Color) -> Color {
    // Perceptual luminance (Rec. 601). If the color is dark, swap to
    // a near-white foreground so it reads against the dim-teal page;
    // otherwise leave it alone (amber accents stay amber).
    let lum = 0.299 * c.r as f32 + 0.587 * c.g as f32 + 0.114 * c.b as f32;
    if lum < 140.0 {
        Color {
            r: 234,
            g: 234,
            b: 240,
            a: c.a,
        }
    } else {
        c
    }
}

fn resolve_date(ctx: &WidgetRenderContext) -> chrono::NaiveDate {
    ctx.date
        .unwrap_or_else(|| chrono::Local::now().date_naive())
}

/// Returns `Some(minutes_since_midnight)` when the page the widget is
/// being rendered onto is bound to today's date — used by Timeline and
/// DailyAppointments to draw a "now" line. Returns `None` for past or
/// future-dated pages and for pages with no bound date (templates
/// previewed in the editor or non-planner pages).
fn now_marker_for_date(ctx: &WidgetRenderContext) -> Option<u32> {
    let bound = ctx.date?;
    let now = chrono::Local::now();
    let today = now.date_naive();
    if bound != today {
        return None;
    }
    let t = now.time();
    use chrono::Timelike;
    Some(t.hour() * 60 + t.minute())
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

    /// Register a fallback font directly with parley's `FontContext` so
    /// layouts can resolve glyphs even when the host has no system font
    /// store. Desktop builds rely on fontconfig and don't need this; the
    /// WASM viewer ships a bundled font and calls this at construction.
    /// Returns the registered family info (or empty if the data is not a
    /// recognized font format).
    pub fn register_fallback_font(&mut self, data: Vec<u8>) {
        let blob = vello::peniko::Blob::new(std::sync::Arc::new(data));
        self.font_ctx.collection.register_fonts(blob, None);
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
        // Page-colour fill — editorial fieldbook palette.
        let bg = if dark_mode {
            vello::peniko::Color::from_rgba8(28, 42, 48, 255)
        } else {
            vello::peniko::Color::from_rgba8(244, 239, 226, 255)
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
            Color {
                r: 234,
                g: 234,
                b: 240,
                a: 230,
            }
        } else {
            Color {
                r: 36,
                g: 38,
                b: 64,
                a: 240,
            }
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
            Color {
                r: 199,
                g: 199,
                b: 209,
                a: 184,
            }
        } else {
            Color {
                r: 76,
                g: 78,
                b: 102,
                a: 200,
            }
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
            scene.push_layer(
                Fill::NonZero,
                vello::peniko::Mix::Normal,
                1.0_f32,
                world_to_screen,
                &clip,
            );
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
        let local_style = effective_style(&widget.style, render_ctx.dark_mode);
        let style = &local_style;
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
            WidgetKind::Arc {
                start_deg,
                sweep_deg,
                thickness_mm,
            } => {
                let cx = r.x + r.width * 0.5;
                let cy = r.y + r.height * 0.5;
                let rx = r.width * 0.5;
                let ry = r.height * 0.5;
                // Walk the arc as a polyline at ~1deg per segment so
                // non-uniform rx/ry (range arcs are usually circular but
                // ellipses sit on rectangular bounds) come out smooth.
                let mut path = BezPath::new();
                let steps = (sweep_deg.abs() as usize).max(8);
                for i in 0..=steps {
                    let t = i as f64 / steps as f64;
                    // Math convention -> Vello Y-down: negate angle.
                    let theta = (-(start_deg + sweep_deg * t)).to_radians();
                    let x = cx + rx * theta.cos();
                    let y = cy + ry * theta.sin();
                    if i == 0 {
                        path.move_to((x, y));
                    } else {
                        path.line_to((x, y));
                    }
                }
                let style_stroke = stroke_style(*thickness_mm);
                let brush = solid(style.stroke_color);
                scene.stroke(&style_stroke, transform, &brush, None, &path);
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
            WidgetKind::Timeline {
                start_hour,
                end_hour,
                slot_minutes,
            } => {
                let (s, e, m) = match override_ {
                    Some(WidgetOverride::Timeline {
                        start_hour,
                        end_hour,
                        slot_minutes,
                    }) => (*start_hour, *end_hour, *slot_minutes),
                    _ => (*start_hour, *end_hour, *slot_minutes),
                };
                let now_marker = now_marker_for_date(render_ctx);
                self.draw_timeline_stub(scene, transform, r, style, s, e, m, now_marker);
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
            WidgetKind::DailyAppointments {
                start_hour,
                end_hour,
            } => {
                let (s, e) = match override_ {
                    Some(WidgetOverride::DailyAppointments {
                        start_hour,
                        end_hour,
                    }) => (*start_hour, *end_hour),
                    _ => (*start_hour, *end_hour),
                };
                let now_marker = now_marker_for_date(render_ctx);
                self.draw_daily_appointments(scene, transform, r, style, s, e, now_marker);
            }
            WidgetKind::WeeklyCompass => {
                self.draw_weekly_compass(scene, transform, r, style);
            }
            WidgetKind::HabitTracker { habits, days } => {
                let (habits, days) = match override_ {
                    Some(WidgetOverride::HabitTracker { habits, days }) => {
                        (habits.as_slice(), *days)
                    }
                    _ => (habits.as_slice(), *days),
                };
                let highlight_col = match render_ctx.date {
                    Some(d) if d == chrono::Local::now().date_naive() => {
                        use chrono::Datelike;
                        Some(d.day())
                    }
                    _ => None,
                };
                self.draw_habit_tracker(scene, transform, r, style, habits, days, highlight_col);
            }
            WidgetKind::Tally { label, count } => {
                let (label, count) = match override_ {
                    Some(WidgetOverride::Tally { label, count }) => (label.as_str(), *count),
                    _ => (label.as_str(), *count),
                };
                self.draw_tally(scene, transform, r, style, label, count);
            }
            WidgetKind::RangeArcs {
                rings,
                interval_m,
                sweep_deg,
                sector_deg,
            } => {
                let (rings, interval_m, sweep_deg, sector_deg) = match override_ {
                    Some(WidgetOverride::RangeArcs {
                        rings,
                        interval_m,
                        sweep_deg,
                        sector_deg,
                    }) => (*rings, *interval_m, *sweep_deg, *sector_deg),
                    _ => (*rings, *interval_m, *sweep_deg, *sector_deg),
                };
                self.draw_range_arcs(
                    scene,
                    transform,
                    r,
                    style,
                    rings,
                    interval_m,
                    sweep_deg,
                    sector_deg,
                );
            }

            // ---- Fetch widgets — read cached payload from
            // render_ctx.widget_data; show a "Loading…" placeholder
            // when the cache is empty (first open before fetcher
            // populates it) or an error string when it failed.
            WidgetKind::Weather { location_label, .. } => {
                let data = render_ctx.widget_data.get(&widget.id);
                self.draw_weather(scene, transform, r, style, location_label, data);
            }
            WidgetKind::Quote { .. } => {
                let data = render_ctx.widget_data.get(&widget.id);
                self.draw_quote(scene, transform, r, style, data);
            }
            WidgetKind::BibleVerse { reference, .. } => {
                let data = render_ctx.widget_data.get(&widget.id);
                self.draw_bible_verse(scene, transform, r, style, reference, data);
            }
            WidgetKind::Sunrise { .. } => {
                let data = render_ctx.widget_data.get(&widget.id);
                self.draw_sunrise(scene, transform, r, style, data);
            }
            WidgetKind::MoonPhase => {
                let data = render_ctx.widget_data.get(&widget.id);
                self.draw_moon_phase(scene, transform, r, style, data);
            }
            WidgetKind::OnThisDay { .. } => {
                let data = render_ctx.widget_data.get(&widget.id);
                self.draw_on_this_day(scene, transform, r, style, data);
            }
            WidgetKind::WordOfDay { .. } => {
                let data = render_ctx.widget_data.get(&widget.id);
                self.draw_word_of_day(scene, transform, r, style, data);
            }
            WidgetKind::RssHeadline { count, .. } => {
                let data = render_ctx.widget_data.get(&widget.id);
                self.draw_rss(scene, transform, r, style, *count, data);
            }
            WidgetKind::Astronomy { .. } => {
                let data = render_ctx.widget_data.get(&widget.id);
                self.draw_astronomy(scene, transform, r, style, data);
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

        // "Today" highlight: if the calendar's displayed month matches
        // the system's current month, ring today's cell in amber so
        // the user can see "this is today" at a glance. Skipped when
        // the calendar is bound to a different month via override or
        // page date.
        let today = chrono::Local::now().date_naive();
        let is_current_month = today.year() == year && today.month() == month;

        for day in 1..=total_days {
            let idx = first_dow + day - 1;
            let col = idx % 7;
            let row = idx / 7;
            if row >= 6 {
                break;
            }
            let cell_x = r.x + cw * col as f64;
            let cell_y = cells_y + rh * row as f64;

            if is_current_month && day as u32 == today.day() {
                let amber = vello::peniko::Color::from_rgba8(214, 168, 58, 255);
                let pad = (cw.min(rh) * 0.08).max(0.4);
                let cx = cell_x + cw * 0.5;
                let cy = cell_y + rh * 0.5;
                let rx = cw * 0.5 - pad;
                let ry = rh * 0.5 - pad;
                let ring_path = Ellipse::new((cx, cy), (rx, ry), 0.0).to_path(0.05);
                let ring_style = stroke_style(style.stroke_width_mm.max(0.5));
                scene.stroke(
                    &ring_style,
                    transform,
                    &Brush::Solid(amber),
                    None,
                    &ring_path,
                );
            }

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
        now_minutes: Option<u32>,
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
            if !m.is_multiple_of(60) {
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

        // "Now" line — only when the bound page-date is today and the
        // current time falls within the timeline's hour range.
        if let Some(now) = now_minutes {
            if now >= start_min && now < end_min {
                let frac = (now - start_min) as f64 / total_min as f64;
                let ny = r.y + r.height * frac;
                let amber = vello::peniko::Color::from_rgba8(214, 168, 58, 255);
                let mut line = BezPath::new();
                line.move_to((r.x, ny));
                line.line_to((r.x + r.width, ny));
                let now_stroke = stroke_style(style.stroke_width_mm.max(0.5));
                scene.stroke(&now_stroke, transform, &Brush::Solid(amber), None, &line);
                let dot = Circle::new((r.x + label_w, ny), (row_h * 0.18).clamp(0.6, 1.4));
                scene.fill(
                    Fill::NonZero,
                    transform,
                    &Brush::Solid(amber),
                    None,
                    &dot.path_elements(0.05).collect::<BezPath>(),
                );
            }
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
        now_minutes: Option<u32>,
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

        // Half-hour ticks — short marks across the writing area, makes
        // the page actually usable for appointment-time entries.
        let mut ticks = BezPath::new();
        for i in 0..(hours as u32) {
            let half_y = r.y + row_h * (i as f64 + 0.5);
            ticks.move_to((r.x + label_w, half_y));
            ticks.line_to((r.x + label_w + r.width * 0.04, half_y));
        }
        scene.stroke(&stroke_style_thin, transform, &brush, None, &ticks);

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

        // "Now" line — when the page is bound to today and current time
        // falls inside [start_hour, end_hour].
        if let Some(now) = now_minutes {
            let start_min = (start_hour as u32) * 60;
            let end_min = (end_hour as u32) * 60;
            if now >= start_min && now < end_min {
                let frac = (now - start_min) as f64 / (end_min - start_min) as f64;
                let ny = r.y + r.height * frac;
                let amber = vello::peniko::Color::from_rgba8(214, 168, 58, 255);
                let mut line = BezPath::new();
                line.move_to((r.x, ny));
                line.line_to((r.x + r.width, ny));
                let now_stroke = stroke_style(style.stroke_width_mm.max(0.5));
                scene.stroke(&now_stroke, transform, &Brush::Solid(amber), None, &line);
                let dot = Circle::new((r.x + label_w, ny), (row_h * 0.18).clamp(0.6, 1.4));
                scene.fill(
                    Fill::NonZero,
                    transform,
                    &Brush::Solid(amber),
                    None,
                    &dot.path_elements(0.05).collect::<BezPath>(),
                );
            }
        }
    }

    fn draw_habit_tracker(
        &mut self,
        scene: &mut Scene,
        transform: Affine,
        r: &WidgetRect,
        style: &WidgetStyle,
        habits: &[String],
        days: u32,
        highlight_col: Option<u32>,
    ) {
        let days = days.max(1);
        let row_count = habits.len().max(1);
        let label_col_w = (r.width * 0.28).clamp(20.0, 60.0);
        let header_h = (r.height * 0.07).clamp(4.0, 8.0);
        let body_w = r.width - label_col_w;
        let body_h = r.height - header_h;
        let col_w = body_w / days as f64;
        let row_h = body_h / row_count as f64;

        let stroke_style_thin = stroke_style(style.stroke_width_mm.max(0.18));
        let brush = solid(style.stroke_color);

        // Today's column highlight — drawn first so the grid lines and
        // checkboxes stack on top.
        if let Some(col) = highlight_col {
            if col >= 1 && col <= days {
                let amber = vello::peniko::Color::from_rgba8(214, 168, 58, 60);
                let cx = r.x + label_col_w + col_w * (col - 1) as f64;
                let cell = KRect::new(cx, r.y, cx + col_w, r.y + r.height);
                scene.fill(Fill::NonZero, transform, &Brush::Solid(amber), None, &cell);
            }
        }

        // Outer border + header underline + label divider.
        let mut grid = BezPath::new();
        grid.move_to((r.x, r.y));
        grid.line_to((r.x + r.width, r.y));
        grid.line_to((r.x + r.width, r.y + r.height));
        grid.line_to((r.x, r.y + r.height));
        grid.line_to((r.x, r.y));
        grid.move_to((r.x, r.y + header_h));
        grid.line_to((r.x + r.width, r.y + header_h));
        grid.move_to((r.x + label_col_w, r.y));
        grid.line_to((r.x + label_col_w, r.y + r.height));
        for c in 1..days {
            let x = r.x + label_col_w + col_w * c as f64;
            grid.move_to((x, r.y + header_h));
            grid.line_to((x, r.y + r.height));
        }
        for rr in 1..row_count {
            let y = r.y + header_h + row_h * rr as f64;
            grid.move_to((r.x, y));
            grid.line_to((r.x + r.width, y));
        }
        scene.stroke(&stroke_style_thin, transform, &brush, None, &grid);

        // Day-of-month numbers along the header row.
        let header_fs = (header_h * 0.6).clamp(2.5, 5.0) as f32;
        for d in 1..=days {
            let x = r.x + label_col_w + col_w * (d - 1) as f64;
            draw_text_runs(
                scene,
                &mut self.font_ctx,
                &mut self.layout_ctx,
                transform,
                &d.to_string(),
                header_fs,
                x,
                r.y + 0.5,
                col_w,
                style.stroke_color,
                Alignment::Center,
            );
        }

        // Habit names down the left column.
        let label_fs = (row_h * 0.55).clamp(3.0, 6.0) as f32;
        for (i, name) in habits.iter().enumerate() {
            let y = r.y + header_h + row_h * i as f64 + (row_h - label_fs as f64) * 0.5;
            draw_text_runs(
                scene,
                &mut self.font_ctx,
                &mut self.layout_ctx,
                transform,
                name,
                label_fs,
                r.x + 1.5,
                y,
                label_col_w - 3.0,
                style.stroke_color,
                Alignment::Start,
            );
        }
    }

    fn draw_tally(
        &mut self,
        scene: &mut Scene,
        transform: Affine,
        r: &WidgetRect,
        style: &WidgetStyle,
        label: &str,
        count: u32,
    ) {
        let count = count.max(1);
        let label_w = (r.width * 0.30).clamp(15.0, 60.0);
        let circles_w = r.width - label_w;
        let label_fs = (r.height * 0.55).clamp(3.0, 7.0) as f32;
        draw_text_runs(
            scene,
            &mut self.font_ctx,
            &mut self.layout_ctx,
            transform,
            label,
            label_fs,
            r.x + 1.0,
            r.y + (r.height - label_fs as f64) * 0.5,
            label_w - 2.0,
            style.stroke_color,
            Alignment::Start,
        );

        let max_d = (r.height * 0.85).max(2.0);
        let stride = circles_w / count as f64;
        let diameter = stride.min(max_d) * 0.8;
        let radius = diameter * 0.5;
        let stroke_style_thin = stroke_style(style.stroke_width_mm.max(0.25));
        let brush = solid(style.stroke_color);
        for i in 0..count {
            let cx = r.x + label_w + stride * (i as f64 + 0.5);
            let cy = r.y + r.height * 0.5;
            let path = Ellipse::new((cx, cy), (radius, radius), 0.0).to_path(0.05);
            scene.stroke(&stroke_style_thin, transform, &brush, None, &path);
        }
    }

    fn draw_range_arcs(
        &mut self,
        scene: &mut Scene,
        transform: Affine,
        r: &WidgetRect,
        style: &WidgetStyle,
        rings: u32,
        interval_m: u32,
        sweep_deg: f64,
        sector_deg: f64,
    ) {
        let rings = rings.max(1);
        let sweep = if sweep_deg <= 0.0 { 180.0 } else { sweep_deg };
        // Weapon position at the bottom-center of the rect; arcs fan
        // upward to the rect top edge. The radius of the outermost
        // ring is the rect's height (so widget rect height controls
        // how big the fan reaches).
        let wp_x = r.x + r.width * 0.5;
        let wp_y = r.y + r.height;
        let max_radius = r.height.min(r.width * 0.5).max(1.0);
        let stroke_style_arc = stroke_style(style.stroke_width_mm.max(0.25));
        let brush = solid(style.stroke_color);
        // Center the sweep on the +Y axis (90deg in math convention).
        let start_deg = 90.0 + sweep * 0.5;

        // "My sector" V — fighting-position sector of fire. Drawn at
        // a heavier weight than the range arcs so it reads as the
        // primary geometry. Default 90deg if the field is zero or
        // negative (older templates / overrides).
        let sector = if sector_deg <= 0.0 { 90.0 } else { sector_deg };
        let half_sector_rad = (sector * 0.5).to_radians();
        // Sector edges go from WP at +/- half-sector around vertical.
        // dy negative because Y is screen-down in canvas coords.
        let sector_dx = max_radius * half_sector_rad.sin();
        let sector_dy = -max_radius * half_sector_rad.cos();
        let amber = vello::peniko::Color::from_rgba8(214, 168, 58, 235);
        let sector_brush = vello::peniko::Brush::Solid(amber);
        let mut sector_v = BezPath::new();
        sector_v.move_to((wp_x - sector_dx, wp_y + sector_dy));
        sector_v.line_to((wp_x, wp_y));
        sector_v.line_to((wp_x + sector_dx, wp_y + sector_dy));
        scene.stroke(
            &stroke_style(style.stroke_width_mm.max(0.6)),
            transform,
            &sector_brush,
            None,
            &sector_v,
        );

        // Sector-limit lines (full sweep extents) drawn lighter so the
        // V remains the dominant marker.
        let half_sweep_rad = (sweep * 0.5).to_radians();
        let limit_dx = max_radius * half_sweep_rad.cos();
        let limit_dy = -max_radius * half_sweep_rad.sin();
        let mut limits = BezPath::new();
        limits.move_to((wp_x, wp_y));
        limits.line_to((wp_x - limit_dx, wp_y + limit_dy));
        limits.move_to((wp_x, wp_y));
        limits.line_to((wp_x + limit_dx, wp_y + limit_dy));
        scene.stroke(
            &stroke_style(style.stroke_width_mm.max(0.25)),
            transform,
            &brush,
            None,
            &limits,
        );

        for ring in 1..=rings {
            let radius = max_radius * (ring as f64 / rings as f64);
            let mut path = BezPath::new();
            let steps = (sweep.abs() as usize).max(8);
            for i in 0..=steps {
                let frac = i as f64 / steps as f64;
                let theta = (-(start_deg - sweep * frac)).to_radians();
                let x = wp_x + radius * theta.cos();
                let y = wp_y + radius * theta.sin();
                if i == 0 {
                    path.move_to((x, y));
                } else {
                    path.line_to((x, y));
                }
            }
            scene.stroke(&stroke_style_arc, transform, &brush, None, &path);

            // Distance label along the upper-right diagonal of the arc.
            let cos45 = std::f64::consts::FRAC_1_SQRT_2;
            let lx = wp_x + radius * cos45 + 1.0;
            let ly = wp_y - radius * cos45 - 1.5;
            let label = format!("{}m", interval_m * ring);
            let fs = (max_radius * 0.05).clamp(2.5, 5.0) as f32;
            draw_text_runs(
                scene,
                &mut self.font_ctx,
                &mut self.layout_ctx,
                transform,
                &label,
                fs,
                lx,
                ly,
                14.0,
                style.stroke_color,
                Alignment::Start,
            );
        }
    }

    // ---- Fetch-widget renderers ----------------------------------------
    //
    // All fetch widgets share the same surface: a 1px frame around the
    // rect, a small caption row at top, and a body block underneath
    // showing the cached payload (or a "Loading…" / error message
    // when the cache is empty). The fetcher in journal-app populates
    // `WidgetRenderContext::widget_data`; this code only reads.

    fn draw_fetch_frame(
        &mut self,
        scene: &mut Scene,
        transform: Affine,
        r: &WidgetRect,
        style: &WidgetStyle,
        title: &str,
    ) -> (f64, f64) {
        // Outer frame.
        let frame = stroke_style(style.stroke_width_mm.max(0.2));
        let brush = solid(style.stroke_color);
        let mut path = BezPath::new();
        path.move_to((r.x, r.y));
        path.line_to((r.x + r.width, r.y));
        path.line_to((r.x + r.width, r.y + r.height));
        path.line_to((r.x, r.y + r.height));
        path.line_to((r.x, r.y));
        scene.stroke(&frame, transform, &brush, None, &path);

        // Header band — title sits in the top-left in small caps.
        let header_h = (r.height * 0.18).clamp(4.0, 8.0);
        let mut underline = BezPath::new();
        underline.move_to((r.x, r.y + header_h));
        underline.line_to((r.x + r.width, r.y + header_h));
        scene.stroke(&frame, transform, &brush, None, &underline);

        let title_fs = (header_h * 0.55).clamp(2.4, 4.5) as f32;
        draw_text_runs(
            scene,
            &mut self.font_ctx,
            &mut self.layout_ctx,
            transform,
            &title.to_uppercase(),
            title_fs,
            r.x + 1.5,
            r.y + (header_h - title_fs as f64) * 0.5,
            r.width - 3.0,
            style.stroke_color,
            Alignment::Start,
        );

        let body_top = r.y + header_h + 1.0;
        let body_h = r.height - header_h - 1.0;
        (body_top, body_h)
    }

    fn draw_fetch_body_text(
        &mut self,
        scene: &mut Scene,
        transform: Affine,
        r: &WidgetRect,
        style: &WidgetStyle,
        body_top: f64,
        body_h: f64,
        text: &str,
        font_mm: f64,
    ) {
        let fs = font_mm.clamp(2.5, 6.0) as f32;
        draw_text_runs(
            scene,
            &mut self.font_ctx,
            &mut self.layout_ctx,
            transform,
            text,
            fs,
            r.x + 1.5,
            body_top + 0.5,
            r.width - 3.0,
            style.stroke_color,
            Alignment::Start,
        );
        let _ = body_h;
    }

    fn draw_loading(
        &mut self,
        scene: &mut Scene,
        transform: Affine,
        r: &WidgetRect,
        style: &WidgetStyle,
        body_top: f64,
        body_h: f64,
        msg: &str,
    ) {
        let dim_color = Color {
            r: style.stroke_color.r,
            g: style.stroke_color.g,
            b: style.stroke_color.b,
            a: style.stroke_color.a / 2,
        };
        let fs = (body_h * 0.35).clamp(3.0, 5.5) as f32;
        draw_text_runs(
            scene,
            &mut self.font_ctx,
            &mut self.layout_ctx,
            transform,
            msg,
            fs,
            r.x + 1.5,
            body_top + (body_h - fs as f64) * 0.5,
            r.width - 3.0,
            dim_color,
            Alignment::Center,
        );
    }

    fn draw_weather(
        &mut self,
        scene: &mut Scene,
        transform: Affine,
        r: &WidgetRect,
        style: &WidgetStyle,
        location_label: &str,
        data: Option<&WidgetData>,
    ) {
        let (body_top, body_h) =
            self.draw_fetch_frame(scene, transform, r, style, &format!("Weather — {}", location_label));
        match data.map(|d| &d.payload) {
            Some(WidgetPayload::Weather {
                current_c,
                current_code,
                days,
                ..
            }) => {
                let glyph = weather_glyph(*current_code);
                let header = format!("{} {}  {:.0}°C", glyph, weather_summary(*current_code), current_c);
                let header_fs = (body_h * 0.32).clamp(3.5, 7.0) as f32;
                draw_text_runs(
                    scene,
                    &mut self.font_ctx,
                    &mut self.layout_ctx,
                    transform,
                    &header,
                    header_fs,
                    r.x + 1.5,
                    body_top + 0.5,
                    r.width - 3.0,
                    style.stroke_color,
                    Alignment::Start,
                );
                // Forecast strip — N day cells across the bottom 50%.
                let strip_y = body_top + body_h * 0.5;
                let strip_h = body_h * 0.5 - 1.0;
                let n = days.len().max(1);
                let col_w = r.width / n as f64;
                let cell_fs = (strip_h * 0.3).clamp(2.5, 4.5) as f32;
                for (i, day) in days.iter().enumerate() {
                    let x = r.x + col_w * i as f64;
                    let label = format!(
                        "{}\n{}\n{:.0}°/{:.0}°",
                        day.date.split('-').next_back().unwrap_or(""),
                        weather_glyph(day.code),
                        day.hi_c,
                        day.lo_c,
                    );
                    draw_text_runs(
                        scene,
                        &mut self.font_ctx,
                        &mut self.layout_ctx,
                        transform,
                        &label,
                        cell_fs,
                        x + 0.5,
                        strip_y,
                        col_w - 1.0,
                        style.stroke_color,
                        Alignment::Center,
                    );
                }
            }
            Some(WidgetPayload::Error { message }) => {
                self.draw_loading(scene, transform, r, style, body_top, body_h, message);
            }
            _ => self.draw_loading(scene, transform, r, style, body_top, body_h, "Loading weather…"),
        }
    }

    fn draw_quote(
        &mut self,
        scene: &mut Scene,
        transform: Affine,
        r: &WidgetRect,
        style: &WidgetStyle,
        data: Option<&WidgetData>,
    ) {
        let (body_top, body_h) = self.draw_fetch_frame(scene, transform, r, style, "Quote of the day");
        match data.map(|d| &d.payload) {
            Some(WidgetPayload::Quote { text, author }) => {
                let body_fs = (body_h * 0.18).clamp(3.0, 5.5) as f32;
                draw_text_runs(
                    scene,
                    &mut self.font_ctx,
                    &mut self.layout_ctx,
                    transform,
                    &format!("\u{201C}{}\u{201D}", text),
                    body_fs,
                    r.x + 1.5,
                    body_top + 0.5,
                    r.width - 3.0,
                    style.stroke_color,
                    Alignment::Start,
                );
                let author_fs = (body_h * 0.14).clamp(2.5, 4.5) as f32;
                draw_text_runs(
                    scene,
                    &mut self.font_ctx,
                    &mut self.layout_ctx,
                    transform,
                    &format!("— {}", author),
                    author_fs,
                    r.x + 1.5,
                    body_top + body_h - author_fs as f64 - 0.5,
                    r.width - 3.0,
                    style.stroke_color,
                    Alignment::End,
                );
            }
            Some(WidgetPayload::Error { message }) => {
                self.draw_loading(scene, transform, r, style, body_top, body_h, message);
            }
            _ => self.draw_loading(scene, transform, r, style, body_top, body_h, "Loading quote…"),
        }
    }

    fn draw_bible_verse(
        &mut self,
        scene: &mut Scene,
        transform: Affine,
        r: &WidgetRect,
        style: &WidgetStyle,
        configured_ref: &str,
        data: Option<&WidgetData>,
    ) {
        let title = if configured_ref.eq_ignore_ascii_case("random") {
            "Verse of the day".to_string()
        } else {
            format!("Bible — {}", configured_ref)
        };
        let (body_top, body_h) = self.draw_fetch_frame(scene, transform, r, style, &title);
        match data.map(|d| &d.payload) {
            Some(WidgetPayload::BibleVerse {
                reference,
                text,
                translation,
            }) => {
                let body_fs = (body_h * 0.16).clamp(3.0, 5.5) as f32;
                draw_text_runs(
                    scene,
                    &mut self.font_ctx,
                    &mut self.layout_ctx,
                    transform,
                    text,
                    body_fs,
                    r.x + 1.5,
                    body_top + 0.5,
                    r.width - 3.0,
                    style.stroke_color,
                    Alignment::Start,
                );
                let attr_fs = (body_h * 0.13).clamp(2.5, 4.0) as f32;
                draw_text_runs(
                    scene,
                    &mut self.font_ctx,
                    &mut self.layout_ctx,
                    transform,
                    &format!("— {} ({})", reference, translation),
                    attr_fs,
                    r.x + 1.5,
                    body_top + body_h - attr_fs as f64 - 0.5,
                    r.width - 3.0,
                    style.stroke_color,
                    Alignment::End,
                );
            }
            Some(WidgetPayload::Error { message }) => {
                self.draw_loading(scene, transform, r, style, body_top, body_h, message);
            }
            _ => self.draw_loading(scene, transform, r, style, body_top, body_h, "Loading verse…"),
        }
    }

    fn draw_sunrise(
        &mut self,
        scene: &mut Scene,
        transform: Affine,
        r: &WidgetRect,
        style: &WidgetStyle,
        data: Option<&WidgetData>,
    ) {
        let (body_top, body_h) = self.draw_fetch_frame(scene, transform, r, style, "Sun");
        match data.map(|d| &d.payload) {
            Some(WidgetPayload::Sunrise {
                sunrise_local,
                sunset_local,
                daylight_hms,
            }) => {
                let lines = format!(
                    "\u{2600} {}\n\u{1F319} {}\nDaylight {}",
                    sunrise_local, sunset_local, daylight_hms
                );
                self.draw_fetch_body_text(scene, transform, r, style, body_top, body_h, &lines, body_h * 0.22);
            }
            Some(WidgetPayload::Error { message }) => {
                self.draw_loading(scene, transform, r, style, body_top, body_h, message);
            }
            _ => self.draw_loading(scene, transform, r, style, body_top, body_h, "Loading sun…"),
        }
    }

    fn draw_moon_phase(
        &mut self,
        scene: &mut Scene,
        transform: Affine,
        r: &WidgetRect,
        style: &WidgetStyle,
        data: Option<&WidgetData>,
    ) {
        let (body_top, body_h) = self.draw_fetch_frame(scene, transform, r, style, "Moon");
        match data.map(|d| &d.payload) {
            Some(WidgetPayload::MoonPhase {
                name,
                illumination_pct,
                emoji,
            }) => {
                let glyph_fs = (body_h * 0.55).clamp(6.0, 18.0) as f32;
                draw_text_runs(
                    scene,
                    &mut self.font_ctx,
                    &mut self.layout_ctx,
                    transform,
                    emoji,
                    glyph_fs,
                    r.x,
                    body_top + 0.5,
                    r.width,
                    style.stroke_color,
                    Alignment::Center,
                );
                let label_fs = (body_h * 0.18).clamp(2.5, 5.0) as f32;
                draw_text_runs(
                    scene,
                    &mut self.font_ctx,
                    &mut self.layout_ctx,
                    transform,
                    &format!("{}\n{:.0}% lit", name, illumination_pct),
                    label_fs,
                    r.x,
                    body_top + body_h - (label_fs as f64) * 2.2,
                    r.width,
                    style.stroke_color,
                    Alignment::Center,
                );
            }
            Some(WidgetPayload::Error { message }) => {
                self.draw_loading(scene, transform, r, style, body_top, body_h, message);
            }
            _ => self.draw_loading(scene, transform, r, style, body_top, body_h, "Computing moon…"),
        }
    }

    fn draw_on_this_day(
        &mut self,
        scene: &mut Scene,
        transform: Affine,
        r: &WidgetRect,
        style: &WidgetStyle,
        data: Option<&WidgetData>,
    ) {
        let (body_top, body_h) = self.draw_fetch_frame(scene, transform, r, style, "On this day");
        match data.map(|d| &d.payload) {
            Some(WidgetPayload::OnThisDay { events }) => {
                let lines: Vec<String> = events
                    .iter()
                    .map(|e| format!("{} — {}", e.year, e.text))
                    .collect();
                let joined = lines.join("\n");
                let fs = (body_h / (events.len().max(1) as f64) * 0.55).clamp(2.4, 4.5) as f32;
                draw_text_runs(
                    scene,
                    &mut self.font_ctx,
                    &mut self.layout_ctx,
                    transform,
                    &joined,
                    fs,
                    r.x + 1.5,
                    body_top + 0.5,
                    r.width - 3.0,
                    style.stroke_color,
                    Alignment::Start,
                );
            }
            Some(WidgetPayload::Error { message }) => {
                self.draw_loading(scene, transform, r, style, body_top, body_h, message);
            }
            _ => self.draw_loading(scene, transform, r, style, body_top, body_h, "Loading history…"),
        }
    }

    fn draw_word_of_day(
        &mut self,
        scene: &mut Scene,
        transform: Affine,
        r: &WidgetRect,
        style: &WidgetStyle,
        data: Option<&WidgetData>,
    ) {
        let (body_top, body_h) = self.draw_fetch_frame(scene, transform, r, style, "Word of the day");
        match data.map(|d| &d.payload) {
            Some(WidgetPayload::WordOfDay { word, definition }) => {
                let word_fs = (body_h * 0.30).clamp(4.0, 8.0) as f32;
                draw_text_runs(
                    scene,
                    &mut self.font_ctx,
                    &mut self.layout_ctx,
                    transform,
                    word,
                    word_fs,
                    r.x + 1.5,
                    body_top + 0.5,
                    r.width - 3.0,
                    style.stroke_color,
                    Alignment::Start,
                );
                let def_fs = (body_h * 0.16).clamp(2.5, 4.5) as f32;
                draw_text_runs(
                    scene,
                    &mut self.font_ctx,
                    &mut self.layout_ctx,
                    transform,
                    definition,
                    def_fs,
                    r.x + 1.5,
                    body_top + word_fs as f64 + 1.5,
                    r.width - 3.0,
                    style.stroke_color,
                    Alignment::Start,
                );
            }
            Some(WidgetPayload::Error { message }) => {
                self.draw_loading(scene, transform, r, style, body_top, body_h, message);
            }
            _ => self.draw_loading(scene, transform, r, style, body_top, body_h, "Loading word…"),
        }
    }

    fn draw_rss(
        &mut self,
        scene: &mut Scene,
        transform: Affine,
        r: &WidgetRect,
        style: &WidgetStyle,
        max_items: u32,
        data: Option<&WidgetData>,
    ) {
        let title = match data.map(|d| &d.payload) {
            Some(WidgetPayload::RssHeadline { feed_title, .. }) if !feed_title.is_empty() => {
                feed_title.clone()
            }
            _ => "Headlines".to_string(),
        };
        let (body_top, body_h) = self.draw_fetch_frame(scene, transform, r, style, &title);
        match data.map(|d| &d.payload) {
            Some(WidgetPayload::RssHeadline { items, .. }) => {
                let take = (max_items as usize).max(1).min(items.len().max(1));
                let row_h = body_h / take as f64;
                let fs = (row_h * 0.55).clamp(2.6, 4.5) as f32;
                for (i, item) in items.iter().take(take).enumerate() {
                    let y = body_top + row_h * i as f64;
                    draw_text_runs(
                        scene,
                        &mut self.font_ctx,
                        &mut self.layout_ctx,
                        transform,
                        &format!("\u{2022} {}", item.title),
                        fs,
                        r.x + 1.5,
                        y + 0.5,
                        r.width - 3.0,
                        style.stroke_color,
                        Alignment::Start,
                    );
                }
            }
            Some(WidgetPayload::Error { message }) => {
                self.draw_loading(scene, transform, r, style, body_top, body_h, message);
            }
            _ => self.draw_loading(scene, transform, r, style, body_top, body_h, "Loading feed…"),
        }
    }

    fn draw_astronomy(
        &mut self,
        scene: &mut Scene,
        transform: Affine,
        r: &WidgetRect,
        style: &WidgetStyle,
        data: Option<&WidgetData>,
    ) {
        let (body_top, body_h) = self.draw_fetch_frame(scene, transform, r, style, "Astronomy");
        match data.map(|d| &d.payload) {
            Some(WidgetPayload::Astronomy { lines }) => {
                let joined = lines.join("\n");
                let fs = (body_h * 0.18).clamp(2.6, 4.5) as f32;
                draw_text_runs(
                    scene,
                    &mut self.font_ctx,
                    &mut self.layout_ctx,
                    transform,
                    &joined,
                    fs,
                    r.x + 1.5,
                    body_top + 0.5,
                    r.width - 3.0,
                    style.stroke_color,
                    Alignment::Start,
                );
            }
            Some(WidgetPayload::Error { message }) => {
                self.draw_loading(scene, transform, r, style, body_top, body_h, message);
            }
            _ => self.draw_loading(scene, transform, r, style, body_top, body_h, "Loading sky…"),
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
        let positions = [(r.x, r.y), (cx, r.y), (r.x, cy), (cx, cy)];
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
        1 => "January",
        2 => "February",
        3 => "March",
        4 => "April",
        5 => "May",
        6 => "June",
        7 => "July",
        8 => "August",
        9 => "September",
        10 => "October",
        11 => "November",
        12 => "December",
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
// Weather code → glyph / short summary
// ---------------------------------------------------------------------------
//
// Mapping follows the WMO weather-code table that Open-Meteo returns in
// `current.weather_code` and `daily.weather_code`. Kept as plain ASCII /
// emoji glyphs so the renderer can draw them without a custom symbol
// font.

fn weather_glyph(code: u32) -> &'static str {
    match code {
        0 => "\u{2600}",
        1 | 2 => "\u{1F324}",
        3 => "\u{2601}",
        45 | 48 => "\u{1F32B}",
        51..=57 => "\u{1F327}",
        61..=67 => "\u{1F327}",
        71..=77 => "\u{2744}",
        80..=82 => "\u{1F326}",
        85 | 86 => "\u{1F328}",
        95 | 96 | 99 => "\u{26C8}",
        _ => "?",
    }
}

fn weather_summary(code: u32) -> &'static str {
    match code {
        0 => "Clear",
        1 => "Mostly clear",
        2 => "Partly cloudy",
        3 => "Overcast",
        45 | 48 => "Fog",
        51..=57 => "Drizzle",
        61..=67 => "Rain",
        71..=77 => "Snow",
        80..=82 => "Showers",
        85 | 86 => "Snow showers",
        95 => "Thunderstorm",
        96 | 99 => "Thunder + hail",
        _ => "Unknown",
    }
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
        scene,
        font_ctx,
        layout_ctx,
        transform,
        text,
        font_size,
        x,
        y,
        max_width,
        color,
        Alignment::Center,
        font_size * tracking_em,
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
    // Prefer Noto Sans (the bundled WASM fallback registers under this
    // family name) and fall back to the platform's `sans-serif`
    // generic so desktop builds still pick up Liberation / Inter / DejaVu
    // Sans via fontconfig. Without an explicit FontStack default,
    // parley queries `system-ui` which has no resolution on wasm and
    // returns empty glyph runs.
    builder.push_default(StyleProperty::FontFamily(parley::FontFamily::Source(
        std::borrow::Cow::Borrowed("\"Noto Sans\", sans-serif"),
    )));
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
                        id: g.id,
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
