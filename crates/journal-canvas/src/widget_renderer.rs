use std::collections::HashMap;

use gtk4::cairo;
use journal_core::{
    render_title, Color, Rect, TemplateWidget, TitleContext, WidgetKind, WidgetOverride,
    WidgetRect, WidgetStyle,
};
use uuid::Uuid;

use crate::viewport_transform::ViewportTransform;

/// Per-frame context the canvas hands to every template widget's draw fn.
///
/// - `date`: bound calendar date (planner page → that page's day; freeform
///   page → `None`, falls back to today). Used for `{date}` expansions and
///   the smart calendar.
/// - `overrides`: per-widget overrides from the page's `widget_overrides`
///   map (keyed by `TemplateWidget.id`). Lets a freeform page pin a
///   specific month / hour range / row count / etc. without changing the
///   shared template.
#[derive(Debug, Clone, Default)]
pub struct WidgetRenderContext {
    pub date: Option<chrono::NaiveDate>,
    pub overrides: HashMap<Uuid, WidgetOverride>,
}

fn resolve_date(ctx: &WidgetRenderContext) -> chrono::NaiveDate {
    ctx.date.unwrap_or_else(|| chrono::Local::now().date_naive())
}

fn set_color(ctx: &cairo::Context, c: Color) {
    ctx.set_source_rgba(
        c.r as f64 / 255.0,
        c.g as f64 / 255.0,
        c.b as f64 / 255.0,
        c.a as f64 / 255.0,
    );
}

fn rect_path(ctx: &cairo::Context, r: &WidgetRect) {
    ctx.rectangle(r.x, r.y, r.width, r.height);
}

fn apply_style_stroke(ctx: &cairo::Context, style: &WidgetStyle, _transform: &ViewportTransform) {
    // Render in canvas space (mm). Lines + text grow with the page when the
    // user zooms in — that's a deliberate choice so writers can draw inside
    // letterforms, fill-in priority-list cells, etc.
    set_color(ctx, style.stroke_color);
    ctx.set_line_width(style.stroke_width_mm);
}

fn apply_fill_then_stroke(
    ctx: &cairo::Context,
    style: &WidgetStyle,
    transform: &ViewportTransform,
) {
    if let Some(fill) = style.fill_color {
        set_color(ctx, fill);
        let _ = ctx.fill_preserve();
    }
    apply_style_stroke(ctx, style, transform);
    let _ = ctx.stroke();
}

pub fn draw_widgets(
    ctx: &cairo::Context,
    transform: &ViewportTransform,
    widgets: &[TemplateWidget],
    page_rect: Rect,
) {
    draw_widgets_with_context(ctx, transform, widgets, page_rect, &WidgetRenderContext::default());
}

/// Like [`draw_widgets`] but allows the caller to bind a date for
/// `WidgetKind::TextBlock` placeholder substitution (e.g. the planner page's
/// own date in production, or today's date for the template editor preview).
pub fn draw_widgets_with_context(
    ctx: &cairo::Context,
    transform: &ViewportTransform,
    widgets: &[TemplateWidget],
    _page_rect: Rect,
    render_ctx: &WidgetRenderContext,
) {
    for widget in widgets {
        ctx.save().ok();
        ctx.rectangle(
            widget.rect.x,
            widget.rect.y,
            widget.rect.width,
            widget.rect.height,
        );
        ctx.clip();
        draw_widget(ctx, transform, widget, render_ctx);
        ctx.restore().ok();
    }
}

fn draw_widget(
    ctx: &cairo::Context,
    transform: &ViewportTransform,
    widget: &TemplateWidget,
    render_ctx: &WidgetRenderContext,
) {
    let r = &widget.rect;
    let style = &widget.style;
    let override_ = render_ctx.overrides.get(&widget.id);

    match &widget.kind {
        WidgetKind::Rectangle => {
            rect_path(ctx, r);
            apply_fill_then_stroke(ctx, style, transform);
        }
        WidgetKind::Ellipse => {
            let cx = r.x + r.width * 0.5;
            let cy = r.y + r.height * 0.5;
            let rx = r.width * 0.5;
            let ry = r.height * 0.5;
            ctx.save().ok();
            ctx.translate(cx, cy);
            ctx.scale(rx, ry);
            ctx.arc(0.0, 0.0, 1.0, 0.0, std::f64::consts::TAU);
            ctx.restore().ok();
            apply_fill_then_stroke(ctx, style, transform);
        }
        WidgetKind::Line { thickness_mm } => {
            let thickness = match override_ {
                Some(WidgetOverride::Line { thickness_mm }) => *thickness_mm,
                _ => *thickness_mm,
            };
            set_color(ctx, style.stroke_color);
            ctx.set_line_width(thickness);
            ctx.move_to(r.x, r.y + r.height * 0.5);
            ctx.line_to(r.x + r.width, r.y + r.height * 0.5);
            let _ = ctx.stroke();
        }
        WidgetKind::TextBlock { text, font_size_mm } => {
            let (text, font_size) = match override_ {
                Some(WidgetOverride::TextBlock { text, font_size_mm }) => {
                    (text.as_str(), *font_size_mm)
                }
                _ => (text.as_str(), *font_size_mm),
            };
            set_color(ctx, style.stroke_color);
            ctx.set_font_size(font_size);
            ctx.move_to(r.x, r.y + font_size);
            let date = resolve_date(render_ctx);
            let expanded = render_title(text, &TitleContext::new(date));
            let _ = ctx.show_text(&expanded);
        }
        WidgetKind::GridRegion { spacing_mm } => {
            let s = match override_ {
                Some(WidgetOverride::GridRegion { spacing_mm }) => *spacing_mm,
                _ => *spacing_mm,
            };
            draw_grid_region(ctx, transform, r, style, s);
        }
        WidgetKind::LinesRegion { spacing_mm } => {
            let s = match override_ {
                Some(WidgetOverride::LinesRegion { spacing_mm }) => *spacing_mm,
                _ => *spacing_mm,
            };
            draw_lines_region(ctx, transform, r, style, s);
        }
        WidgetKind::DotsRegion { spacing_mm } => {
            let s = match override_ {
                Some(WidgetOverride::DotsRegion { spacing_mm }) => *spacing_mm,
                _ => *spacing_mm,
            };
            draw_dots_region(ctx, transform, r, style, s);
        }
        WidgetKind::CalendarMonth => {
            // Override pins the displayed month/year explicitly. Falls
            // through to the page's bound date (planner) or today.
            let date = match override_ {
                Some(WidgetOverride::CalendarMonth { year, month }) => {
                    chrono::NaiveDate::from_ymd_opt(*year, *month, 1)
                        .unwrap_or_else(|| resolve_date(render_ctx))
                }
                _ => resolve_date(render_ctx),
            };
            draw_calendar_month(ctx, transform, r, style, date);
        }
        WidgetKind::Timeline { start_hour, end_hour, slot_minutes } => {
            let (s, e, m) = match override_ {
                Some(WidgetOverride::Timeline { start_hour, end_hour, slot_minutes }) => {
                    (*start_hour, *end_hour, *slot_minutes)
                }
                _ => (*start_hour, *end_hour, *slot_minutes),
            };
            draw_timeline_stub(ctx, transform, r, style, s, e, m);
        }
        WidgetKind::Checklist { items } => {
            let items = match override_ {
                Some(WidgetOverride::Checklist { items }) => items.as_slice(),
                _ => items.as_slice(),
            };
            draw_checklist(ctx, transform, r, style, items);
        }
        WidgetKind::BigThree => {
            draw_big_three(ctx, transform, r, style);
        }
        WidgetKind::PriorityList { count } => {
            let n = match override_ {
                Some(WidgetOverride::PriorityList { count }) => *count,
                _ => *count,
            };
            draw_priority_list(ctx, transform, r, style, n);
        }
        WidgetKind::DailyAppointments { start_hour, end_hour } => {
            let (s, e) = match override_ {
                Some(WidgetOverride::DailyAppointments { start_hour, end_hour }) => {
                    (*start_hour, *end_hour)
                }
                _ => (*start_hour, *end_hour),
            };
            draw_daily_appointments(ctx, transform, r, style, s, e);
        }
        WidgetKind::WeeklyCompass => {
            draw_weekly_compass(ctx, transform, r, style);
        }
    }
}

fn draw_grid_region(
    ctx: &cairo::Context,
    transform: &ViewportTransform,
    r: &WidgetRect,
    style: &WidgetStyle,
    spacing_mm: f64,
) {
    if spacing_mm <= 0.0 {
        return;
    }
    let _ = transform;
    set_color(ctx, style.stroke_color);
    ctx.set_line_width(style.stroke_width_mm);

    let mut x = r.x;
    while x <= r.x + r.width {
        ctx.move_to(x, r.y);
        ctx.line_to(x, r.y + r.height);
        x += spacing_mm;
    }
    let mut y = r.y;
    while y <= r.y + r.height {
        ctx.move_to(r.x, y);
        ctx.line_to(r.x + r.width, y);
        y += spacing_mm;
    }
    let _ = ctx.stroke();
}

fn draw_lines_region(
    ctx: &cairo::Context,
    transform: &ViewportTransform,
    r: &WidgetRect,
    style: &WidgetStyle,
    spacing_mm: f64,
) {
    if spacing_mm <= 0.0 {
        return;
    }
    let _ = transform;
    set_color(ctx, style.stroke_color);
    ctx.set_line_width(style.stroke_width_mm);

    let mut y = r.y;
    while y <= r.y + r.height {
        ctx.move_to(r.x, y);
        ctx.line_to(r.x + r.width, y);
        y += spacing_mm;
    }
    let _ = ctx.stroke();
}

fn draw_dots_region(
    ctx: &cairo::Context,
    transform: &ViewportTransform,
    r: &WidgetRect,
    style: &WidgetStyle,
    spacing_mm: f64,
) {
    if spacing_mm <= 0.0 {
        return;
    }
    let _ = transform;
    let radius = spacing_mm * 0.15;
    set_color(ctx, style.stroke_color);

    let mut y = r.y;
    while y <= r.y + r.height {
        let mut x = r.x;
        while x <= r.x + r.width {
            ctx.arc(x, y, radius, 0.0, std::f64::consts::TAU);
            let _ = ctx.fill();
            x += spacing_mm;
        }
        y += spacing_mm;
    }
}

fn draw_calendar_month(
    ctx: &cairo::Context,
    transform: &ViewportTransform,
    r: &WidgetRect,
    style: &WidgetStyle,
    target_date: chrono::NaiveDate,
) {
    let _ = transform;
    let lw = style.stroke_width_mm;
    set_color(ctx, style.stroke_color);
    ctx.set_line_width(lw);

    rect_path(ctx, r);
    let _ = ctx.stroke();

    use chrono::Datelike;
    let year = target_date.year();
    let month = target_date.month();
    let first_of_month = chrono::NaiveDate::from_ymd_opt(year, month, 1)
        .unwrap_or(target_date);
    let first_weekday = first_of_month.weekday().num_days_from_sunday() as usize; // 0=Sun
    let days_in_month = days_in_month(year, month);

    let cols = 7usize;
    let title_h = r.height * 0.10; // "September 2026" band
    let dow_h = r.height * 0.06;   // S M T W T F S
    let body_h = r.height - title_h - dow_h;
    let rows = 6usize;
    let col_w = r.width / cols as f64;
    let row_h = body_h / rows as f64;

    // Title: "{Month} {year}"
    let title = format!("{} {}", month_name(month), year);
    let title_fs = (title_h * 0.65).min(col_w * 0.75);
    ctx.set_font_size(title_fs);
    if let Ok(ext) = ctx.text_extents(&title) {
        let cx = r.x + r.width * 0.5 - ext.width() * 0.5;
        let cy = r.y + title_h * 0.8;
        ctx.move_to(cx, cy);
        let _ = ctx.show_text(&title);
    }

    // Title underline
    ctx.move_to(r.x, r.y + title_h);
    ctx.line_to(r.x + r.width, r.y + title_h);
    let _ = ctx.stroke();

    // Day-of-week header
    let day_names = ["S", "M", "T", "W", "T", "F", "S"];
    let dow_fs = (dow_h * 0.7).min(col_w * 0.5);
    ctx.set_font_size(dow_fs);
    for (i, name) in day_names.iter().enumerate() {
        let cx = r.x + col_w * i as f64 + col_w * 0.5;
        let cy = r.y + title_h + dow_h * 0.75;
        if let Ok(ext) = ctx.text_extents(name) {
            ctx.move_to(cx - ext.width() * 0.5, cy);
            let _ = ctx.show_text(name);
        }
    }
    // DOW underline
    ctx.move_to(r.x, r.y + title_h + dow_h);
    ctx.line_to(r.x + r.width, r.y + title_h + dow_h);
    let _ = ctx.stroke();

    let body_top = r.y + title_h + dow_h;

    // Vertical grid lines (under DOW header)
    for c in 1..cols {
        let x = r.x + col_w * c as f64;
        ctx.move_to(x, body_top);
        ctx.line_to(x, r.y + r.height);
        let _ = ctx.stroke();
    }
    // Horizontal grid lines
    for rw in 1..rows {
        let y = body_top + row_h * rw as f64;
        ctx.move_to(r.x, y);
        ctx.line_to(r.x + r.width, y);
        let _ = ctx.stroke();
    }

    // Day numbers — laid out starting at `first_weekday` column.
    let num_fs = (row_h * 0.40).min(col_w * 0.45);
    ctx.set_font_size(num_fs);
    let today = chrono::Local::now().date_naive();
    let today_in_view = today.year() == year && today.month() == month;
    for day in 1..=days_in_month {
        let cell = first_weekday + (day as usize - 1);
        let row = cell / cols;
        let col = cell % cols;
        if row >= rows {
            break;
        }
        let x = r.x + col_w * col as f64 + lw * 2.0 + col_w * 0.06;
        let y = body_top + row_h * row as f64 + num_fs + lw;
        let label = format!("{}", day);
        // Highlight today's cell with a small filled circle behind the number.
        if today_in_view && today.day() == day {
            ctx.save().ok();
            let cx = r.x + col_w * (col as f64 + 0.5);
            let cy = body_top + row_h * (row as f64 + 0.35);
            let radius = num_fs * 0.85;
            ctx.arc(cx, cy, radius, 0.0, std::f64::consts::TAU);
            if let Some(fill) = style.fill_color {
                set_color(ctx, fill);
            } else {
                ctx.set_source_rgba(
                    style.stroke_color.r as f64 / 255.0,
                    style.stroke_color.g as f64 / 255.0,
                    style.stroke_color.b as f64 / 255.0,
                    0.18,
                );
            }
            let _ = ctx.fill();
            ctx.restore().ok();
            // Re-apply stroke colour for the number text.
            set_color(ctx, style.stroke_color);
            ctx.set_line_width(lw);
        }
        ctx.move_to(x, y);
        let _ = ctx.show_text(&label);
    }
}

fn days_in_month(year: i32, month: u32) -> u32 {
    let next = if month == 12 {
        chrono::NaiveDate::from_ymd_opt(year + 1, 1, 1)
    } else {
        chrono::NaiveDate::from_ymd_opt(year, month + 1, 1)
    };
    match next {
        Some(d) => d.pred_opt().map(|p| chrono::Datelike::day(&p)).unwrap_or(28),
        None => 28,
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

fn draw_timeline_stub(
    ctx: &cairo::Context,
    transform: &ViewportTransform,
    r: &WidgetRect,
    style: &WidgetStyle,
    start_hour: u8,
    end_hour: u8,
    slot_minutes: u32,
) {
    let _ = transform;
    let lw = style.stroke_width_mm;
    set_color(ctx, style.stroke_color);
    ctx.set_line_width(lw);

    let total_hours = (end_hour as i32 - start_hour as i32).max(1) as f64;
    let slots_per_hour = if slot_minutes == 0 { 1 } else { 60 / slot_minutes.max(1) };
    let total_slots = (total_hours * slots_per_hour as f64) as usize;
    let slot_h = r.height / total_slots.max(1) as f64;
    let label_w = r.width * 0.15;

    // Outer border
    rect_path(ctx, r);
    let _ = ctx.stroke();

    let fs = (slot_h * 0.7).min(label_w * 0.6);
    ctx.set_font_size(fs);

    for i in 0..=total_slots {
        let y = r.y + slot_h * i as f64;
        ctx.move_to(r.x, y);
        ctx.line_to(r.x + r.width, y);
        let _ = ctx.stroke();

        if slot_minutes > 0 && 60 % slot_minutes == 0 && i % slots_per_hour as usize == 0 {
            let hour = start_hour as usize + i / slots_per_hour as usize;
            if hour <= end_hour as usize {
                let label = format!("{:02}:00", hour);
                let text_y = y + fs * 0.9;
                ctx.move_to(r.x + lw * 2.0, text_y);
                let _ = ctx.show_text(&label);
            }
        }
    }
}

fn draw_checklist(
    ctx: &cairo::Context,
    transform: &ViewportTransform,
    r: &WidgetRect,
    style: &WidgetStyle,
    items: &[String],
) {
    let _ = transform;
    let lw = style.stroke_width_mm;
    set_color(ctx, style.stroke_color);
    ctx.set_line_width(lw);

    let n = items.len().max(1);
    let row_h = r.height / n as f64;
    let box_sz = row_h * 0.55;
    let fs = box_sz * 0.85;
    ctx.set_font_size(fs);

    for (i, item) in items.iter().enumerate() {
        let y = r.y + row_h * i as f64 + (row_h - box_sz) * 0.5;
        let bx = r.x + lw;
        // Checkbox
        ctx.rectangle(bx, y, box_sz, box_sz);
        let _ = ctx.stroke();
        // Item text
        ctx.move_to(bx + box_sz + lw * 3.0, y + fs);
        let _ = ctx.show_text(item);
    }
}

/// Draw three numbered priority boxes stacked vertically (Full Focus Big Three).
fn draw_big_three(
    ctx: &cairo::Context,
    transform: &ViewportTransform,
    r: &WidgetRect,
    style: &WidgetStyle,
) {
    let _ = transform;
    let lw = style.stroke_width_mm;
    set_color(ctx, style.stroke_color);
    ctx.set_line_width(lw);

    // Outer border
    rect_path(ctx, r);
    let _ = ctx.stroke();

    let box_h = r.height / 3.0;
    let label_fs = (box_h * 0.25).min(r.width * 0.1);
    ctx.set_font_size(label_fs);

    for i in 0..3usize {
        let bx = r.x;
        let by = r.y + box_h * i as f64;

        // Divider line between boxes (skip first)
        if i > 0 {
            ctx.move_to(bx, by);
            ctx.line_to(bx + r.width, by);
            let _ = ctx.stroke();
        }

        // Number label in top-left of each box
        let label = format!("{}", i + 1);
        let pad = lw * 2.0 + label_fs * 0.2;
        ctx.move_to(bx + pad, by + label_fs + pad);
        let _ = ctx.show_text(&label);
    }
}

/// Draw a Franklin-style priority list with A/B/C letter column, sequence number column,
/// checkbox + write-on-line right column.
fn draw_priority_list(
    ctx: &cairo::Context,
    transform: &ViewportTransform,
    r: &WidgetRect,
    style: &WidgetStyle,
    count: u32,
) {
    let _ = transform;
    let lw = style.stroke_width_mm;
    set_color(ctx, style.stroke_color);
    ctx.set_line_width(lw);

    let n = count.max(1) as usize;
    let row_h = r.height / n as f64;

    // Column widths: priority letter | sequence number | checkbox + write line
    let pri_col_w = r.width * 0.10;
    let seq_col_w = r.width * 0.08;
    let x_pri = r.x;
    let x_seq = r.x + pri_col_w;
    let x_body = r.x + pri_col_w + seq_col_w;
    let _body_w = r.width - pri_col_w - seq_col_w;

    // Outer border
    rect_path(ctx, r);
    let _ = ctx.stroke();

    // Vertical column dividers
    ctx.move_to(x_seq, r.y);
    ctx.line_to(x_seq, r.y + r.height);
    let _ = ctx.stroke();

    ctx.move_to(x_body, r.y);
    ctx.line_to(x_body, r.y + r.height);
    let _ = ctx.stroke();

    let fs = (row_h * 0.45).min(pri_col_w * 0.7);
    let box_sz = row_h * 0.50;
    ctx.set_font_size(fs);

    // Header glyphs "A", "B", "C" for first 3 rows in priority column
    let priority_labels = ["A", "B", "C"];

    for i in 0..n {
        let row_y = r.y + row_h * i as f64;

        // Horizontal row divider (skip first)
        if i > 0 {
            ctx.move_to(r.x, row_y);
            ctx.line_to(r.x + r.width, row_y);
            let _ = ctx.stroke();
        }

        let center_y = row_y + (row_h - box_sz) * 0.5;
        let text_y = row_y + (row_h + fs) * 0.5 - fs * 0.1;

        // Priority letter column — draw "A", "B", "C" for first 3 rows
        if i < priority_labels.len() {
            let label = priority_labels[i];
            if let Ok(ext) = ctx.text_extents(label) {
                let tx = x_pri + (pri_col_w - ext.width()) * 0.5;
                ctx.move_to(tx, text_y);
                let _ = ctx.show_text(label);
            }
        }

        // Sequence number column — draw row number
        let seq_label = format!("{}", i + 1);
        if let Ok(ext) = ctx.text_extents(&seq_label) {
            let tx = x_seq + (seq_col_w - ext.width()) * 0.5;
            ctx.move_to(tx, text_y);
            let _ = ctx.show_text(&seq_label);
        }

        // Checkbox in body column
        let bx = x_body + lw * 2.0;
        ctx.rectangle(bx, center_y, box_sz, box_sz);
        let _ = ctx.stroke();

        // Write-on baseline line
        let line_x_start = bx + box_sz + lw * 3.0;
        let line_x_end = r.x + r.width - lw;
        let line_y = row_y + row_h - lw * 2.0;
        ctx.move_to(line_x_start, line_y);
        ctx.line_to(line_x_end, line_y);
        let _ = ctx.stroke();
    }
}

/// Draw a two-column hourly appointment schedule (Franklin / Full Focus style).
fn draw_daily_appointments(
    ctx: &cairo::Context,
    transform: &ViewportTransform,
    r: &WidgetRect,
    style: &WidgetStyle,
    start_hour: u8,
    end_hour: u8,
) {
    let _ = transform;
    let lw = style.stroke_width_mm;
    set_color(ctx, style.stroke_color);
    ctx.set_line_width(lw);

    let total_hours = (end_hour as i32 - start_hour as i32).max(1) as usize;
    // Each hour has two rows: the hour row and a half-hour row
    let total_rows = total_hours * 2;
    let row_h = r.height / total_rows as f64;
    let label_col_w = r.width * 0.18;

    // Outer border
    rect_path(ctx, r);
    let _ = ctx.stroke();

    // Vertical divider between label and body columns
    ctx.move_to(r.x + label_col_w, r.y);
    ctx.line_to(r.x + label_col_w, r.y + r.height);
    let _ = ctx.stroke();

    let fs = (row_h * 0.65).min(label_col_w * 0.55);
    ctx.set_font_size(fs);

    for i in 0..=total_rows {
        let y = r.y + row_h * i as f64;
        let is_hour = i % 2 == 0;

        // Row divider — full width for hour boundaries, partial tick for half-hour
        if is_hour {
            ctx.move_to(r.x, y);
            ctx.line_to(r.x + r.width, y);
            let _ = ctx.stroke();

            // Hour label
            let hour = start_hour as usize + i / 2;
            if hour <= end_hour as usize && i < total_rows {
                let label = format!("{:2}:00", hour);
                let text_y = y + fs * 0.9 + lw;
                ctx.move_to(r.x + lw * 2.0, text_y);
                let _ = ctx.show_text(&label);
            }
        } else {
            // Half-hour tick: short line from the divider into the body column
            let tick_end = r.x + label_col_w + r.width * 0.06;
            ctx.move_to(r.x + label_col_w, y);
            ctx.line_to(tick_end, y);
            let _ = ctx.stroke();
        }
    }
}

/// Draw a 4×2 grid of role/goal boxes (Franklin Covey Weekly Compass).
fn draw_weekly_compass(
    ctx: &cairo::Context,
    transform: &ViewportTransform,
    r: &WidgetRect,
    style: &WidgetStyle,
) {
    let _ = transform;
    let lw = style.stroke_width_mm;
    set_color(ctx, style.stroke_color);
    ctx.set_line_width(lw);

    let cols = 2usize;
    let rows = 4usize;
    let cell_w = r.width / cols as f64;
    let cell_h = r.height / rows as f64;

    // Outer border
    rect_path(ctx, r);
    let _ = ctx.stroke();

    // Vertical divider
    ctx.move_to(r.x + cell_w, r.y);
    ctx.line_to(r.x + cell_w, r.y + r.height);
    let _ = ctx.stroke();

    // Horizontal dividers
    for row in 1..rows {
        let y = r.y + cell_h * row as f64;
        ctx.move_to(r.x, y);
        ctx.line_to(r.x + r.width, y);
        let _ = ctx.stroke();
    }

    let fs = (cell_h * 0.14).min(cell_w * 0.12);
    ctx.set_font_size(fs);

    // Caption labels in top-left of each cell
    for row in 0..rows {
        for col in 0..cols {
            let cell_index = row * cols + col;
            let label = format!("Role {}", cell_index + 1);
            let cx = r.x + cell_w * col as f64 + lw * 2.0;
            let cy = r.y + cell_h * row as f64 + fs + lw * 2.0;
            ctx.move_to(cx, cy);
            let _ = ctx.show_text(&label);
        }
    }
}
