use gtk4::cairo;
use journal_core::{Color, Rect, TemplateWidget, WidgetKind, WidgetRect, WidgetStyle};

use crate::viewport_transform::ViewportTransform;

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
    _page_rect: Rect,
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
        draw_widget(ctx, transform, widget);
        ctx.restore().ok();
    }
}

fn draw_widget(ctx: &cairo::Context, transform: &ViewportTransform, widget: &TemplateWidget) {
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
            let _ = ctx.show_text(text);
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
