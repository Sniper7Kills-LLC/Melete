use gtk4::cairo;
use journal_core::{
    render_title, Color, Rect, TemplateWidget, TitleContext, WidgetKind, WidgetRect, WidgetStyle,
};

use crate::viewport_transform::ViewportTransform;

/// Date used to expand `{date}/{weekday}/{month}/...` placeholders inside
/// `WidgetKind::TextBlock` text. `None` means "today (local)".
#[derive(Debug, Clone, Copy, Default)]
pub struct WidgetRenderContext {
    pub date: Option<chrono::NaiveDate>,
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

fn apply_style_stroke(ctx: &cairo::Context, style: &WidgetStyle, transform: &ViewportTransform) {
    let zoom = transform.zoom().max(1e-6);
    set_color(ctx, style.stroke_color);
    ctx.set_line_width(style.stroke_width_mm / zoom.max(1.0));
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
            let zoom = transform.zoom().max(1e-6);
            set_color(ctx, style.stroke_color);
            ctx.set_line_width(thickness_mm / zoom.max(1.0));
            ctx.move_to(r.x, r.y + r.height * 0.5);
            ctx.line_to(r.x + r.width, r.y + r.height * 0.5);
            let _ = ctx.stroke();
        }
        WidgetKind::TextBlock { text, font_size_mm } => {
            let zoom = transform.zoom().max(1e-6);
            set_color(ctx, style.stroke_color);
            ctx.set_font_size(font_size_mm / zoom.max(1.0));
            ctx.move_to(r.x, r.y + font_size_mm / zoom.max(1.0));
            let date = resolve_date(render_ctx);
            let expanded = render_title(text, &TitleContext::new(date));
            let _ = ctx.show_text(&expanded);
        }
        WidgetKind::GridRegion { spacing_mm } => {
            draw_grid_region(ctx, transform, r, style, *spacing_mm);
        }
        WidgetKind::LinesRegion { spacing_mm } => {
            draw_lines_region(ctx, transform, r, style, *spacing_mm);
        }
        WidgetKind::DotsRegion { spacing_mm } => {
            draw_dots_region(ctx, transform, r, style, *spacing_mm);
        }
        WidgetKind::CalendarMonth => {
            draw_calendar_stub(ctx, transform, r, style);
        }
        WidgetKind::Timeline { start_hour, end_hour, slot_minutes } => {
            draw_timeline_stub(ctx, transform, r, style, *start_hour, *end_hour, *slot_minutes);
        }
        WidgetKind::Checklist { items } => {
            draw_checklist(ctx, transform, r, style, items);
        }
        WidgetKind::BigThree => {
            draw_big_three(ctx, transform, r, style);
        }
        WidgetKind::PriorityList { count } => {
            draw_priority_list(ctx, transform, r, style, *count);
        }
        WidgetKind::DailyAppointments { start_hour, end_hour } => {
            draw_daily_appointments(ctx, transform, r, style, *start_hour, *end_hour);
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
    let zoom = transform.zoom().max(1e-6);
    set_color(ctx, style.stroke_color);
    ctx.set_line_width(style.stroke_width_mm / zoom.max(1.0));

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
    let zoom = transform.zoom().max(1e-6);
    set_color(ctx, style.stroke_color);
    ctx.set_line_width(style.stroke_width_mm / zoom.max(1.0));

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
    let zoom = transform.zoom().max(1e-6);
    let radius = (1.5 / zoom).clamp(0.05, spacing_mm * 0.25);
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

fn draw_calendar_stub(
    ctx: &cairo::Context,
    transform: &ViewportTransform,
    r: &WidgetRect,
    style: &WidgetStyle,
) {
    let zoom = transform.zoom().max(1e-6);
    let lw = style.stroke_width_mm / zoom.max(1.0);
    set_color(ctx, style.stroke_color);
    ctx.set_line_width(lw);

    // Outer border
    rect_path(ctx, r);
    let _ = ctx.stroke();

    let cols = 7usize;
    let header_h = r.height * 0.12;
    let body_h = r.height - header_h;
    let rows = 6usize;
    let col_w = r.width / cols as f64;
    let row_h = body_h / rows as f64;

    // Header row
    ctx.move_to(r.x, r.y + header_h);
    ctx.line_to(r.x + r.width, r.y + header_h);
    let _ = ctx.stroke();

    // Day-of-week header text
    let day_names = ["S", "M", "T", "W", "T", "F", "S"];
    let fs = (header_h * 0.55).min(col_w * 0.6);
    ctx.set_font_size(fs);
    for (i, name) in day_names.iter().enumerate() {
        let cx = r.x + col_w * i as f64 + col_w * 0.5;
        let cy = r.y + header_h * 0.75;
        if let Ok(ext) = ctx.text_extents(name) {
            ctx.move_to(cx - ext.width() * 0.5, cy);
            let _ = ctx.show_text(name);
        }
    }

    // Vertical grid lines
    for c in 1..cols {
        let x = r.x + col_w * c as f64;
        ctx.move_to(x, r.y + header_h);
        ctx.line_to(x, r.y + r.height);
        let _ = ctx.stroke();
    }
    // Horizontal grid lines
    for rw in 1..rows {
        let y = r.y + header_h + row_h * rw as f64;
        ctx.move_to(r.x, y);
        ctx.line_to(r.x + r.width, y);
        let _ = ctx.stroke();
    }

    // Day numbers (1..28 sample)
    let num_fs = (row_h * 0.45).min(col_w * 0.5);
    ctx.set_font_size(num_fs);
    let mut day = 1u32;
    'outer: for row in 0..rows {
        for col in 0..cols {
            if day > 31 {
                break 'outer;
            }
            let x = r.x + col_w * col as f64 + lw * 2.0;
            let y = r.y + header_h + row_h * row as f64 + num_fs + lw;
            let label = format!("{}", day);
            ctx.move_to(x, y);
            let _ = ctx.show_text(&label);
            day += 1;
        }
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
    let zoom = transform.zoom().max(1e-6);
    let lw = style.stroke_width_mm / zoom.max(1.0);
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
    let zoom = transform.zoom().max(1e-6);
    let lw = style.stroke_width_mm / zoom.max(1.0);
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
    let zoom = transform.zoom().max(1e-6);
    let lw = style.stroke_width_mm / zoom.max(1.0);
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
    let zoom = transform.zoom().max(1e-6);
    let lw = style.stroke_width_mm / zoom.max(1.0);
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
    let zoom = transform.zoom().max(1e-6);
    let lw = style.stroke_width_mm / zoom.max(1.0);
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
    let zoom = transform.zoom().max(1e-6);
    let lw = style.stroke_width_mm / zoom.max(1.0);
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
