//! Range Card  -  US Letter portrait.
//!
//! Fighting-position sector sketch. Title, info row, sketch box with
//! weapon-position dot + magnetic-N marker + sector-limit labels, TRP
//! table (six numbered rows: description / distance / azimuth), remarks
//! / dead-space block. DA 5517-style layout.

use uuid::{uuid, Uuid};

use journal_core::{
    BackgroundType, PageTemplate, TemplateId, TemplateWidget, TilingMode, WidgetKind, WidgetRect,
    WidgetStyle,
};

use crate::builtin::{arc, hline, lines_region, mw, rect, segment, text, vline, US_LETTER};

pub const BUILTIN_MILITARY_RANGE_CARD_ID: Uuid = uuid!("00000000-0000-0000-0000-000000000013");

pub fn builtin_military_range_card() -> PageTemplate {
    let t: u8 = 0x13;
    let margin = 8.0_f64;
    let (page_w, page_h) = US_LETTER;
    let mut widgets: Vec<TemplateWidget> = Vec::new();

    widgets.push(text(
        mw(t, 1),
        margin,
        margin,
        page_w - margin * 2.0,
        9.0,
        "RANGE CARD",
        7.5,
    ));

    let info_y = margin + 11.0;
    let info_h = 12.0;
    let cell_w = (page_w - margin * 2.0) / 4.0;
    let info_labels = ["Unit:", "Position #:", "Date:", "DTG:"];
    for (i, label) in info_labels.iter().enumerate() {
        let x = margin + cell_w * i as f64;
        widgets.push(text(
            mw(t, (10 + i) as u16),
            x + 1.0,
            info_y + 0.5,
            cell_w - 2.0,
            4.0,
            label,
            3.0,
        ));
        widgets.push(hline(
            mw(t, (20 + i) as u16),
            x + 1.0,
            info_y + info_h - 1.0,
            cell_w - 2.0,
            0.2,
        ));
    }
    widgets.push(rect(
        mw(t, 30),
        margin,
        info_y,
        page_w - margin * 2.0,
        info_h,
    ));
    for i in 1..info_labels.len() {
        widgets.push(vline(
            mw(t, (40 + i) as u16),
            margin + cell_w * i as f64,
            info_y,
            info_h,
            0.2,
        ));
    }

    let sketch_y = info_y + info_h + 4.0;
    let sketch_h = 130.0;
    let sketch_w = page_w - margin * 2.0;
    widgets.push(rect(mw(t, 50), margin, sketch_y, sketch_w, sketch_h));

    let wp_x = margin + sketch_w * 0.5;
    let wp_y = sketch_y + sketch_h - 6.0;
    let mut wp_style = WidgetStyle::default();
    wp_style.fill_color = Some(wp_style.stroke_color);
    widgets.push(TemplateWidget {
        id: mw(t, 51),
        kind: WidgetKind::Rectangle,
        rect: WidgetRect {
            x: wp_x - 1.5,
            y: wp_y - 1.5,
            width: 3.0,
            height: 3.0,
        },
        style: wp_style,
    });
    widgets.push(text(
        mw(t, 52),
        wp_x - 18.0,
        wp_y - 4.5,
        16.0,
        4.0,
        "WP",
        3.0,
    ));

    let n_x = margin + sketch_w - 16.0;
    let n_top = sketch_y + 6.0;
    widgets.push(vline(mw(t, 60), n_x, n_top, 14.0, 0.4));
    widgets.push(text(mw(t, 61), n_x - 2.0, n_top - 5.0, 8.0, 4.0, "N", 4.0));

    // ── Range arcs + sector limit lines ─────────────────────────────────
    // DA 5517-style range card: concentric arcs at three range bands
    // fanning out from the weapon position, plus sector-limit lines
    // running from WP through the LL / RL labels at the sketch corners.
    // Math convention: 0deg = +x, 90deg = +y, sweep CCW. The widget
    // renderer flips Y at draw time so the arcs read "up" on screen.
    let max_radius = (sketch_h - 8.0).min(sketch_w * 0.5 - 4.0);
    let r1 = max_radius * 0.34;
    let r2 = max_radius * 0.67;
    let r3 = max_radius * 0.95;
    // 90-deg fan opening upward — start at 45deg right-up, sweep 90deg
    // CCW through 90deg straight-up to 135deg left-up.
    for (i, radius) in [r1, r2, r3].iter().enumerate() {
        widgets.push(arc(
            mw(t, (90 + i) as u16),
            wp_x,
            wp_y,
            *radius,
            45.0,
            90.0,
            0.25,
        ));
    }

    // Range labels along the upper-right diagonal.
    let cos45 = std::f64::consts::FRAC_1_SQRT_2;
    let label_dx = cos45;
    let label_dy = -cos45;
    for (i, (radius, name)) in
        [(r1, "100m"), (r2, "200m"), (r3, "300m")].iter().enumerate()
    {
        let lx = wp_x + radius * label_dx + 1.0;
        let ly = wp_y + radius * label_dy - 1.5;
        widgets.push(text(
            mw(t, (93 + i) as u16),
            lx,
            ly,
            14.0,
            3.5,
            name,
            2.8,
        ));
    }

    // Sector-limit lines: from WP along the 45/135deg fan edges to the
    // LL / RL label corners.
    let limit_len = max_radius;
    let ll_end_x = wp_x - limit_len * cos45;
    let ll_end_y = wp_y - limit_len * cos45;
    let rl_end_x = wp_x + limit_len * cos45;
    let rl_end_y = wp_y - limit_len * cos45;
    widgets.push(segment(mw(t, 96), wp_x, wp_y, ll_end_x, ll_end_y, 0.3));
    widgets.push(segment(mw(t, 97), wp_x, wp_y, rl_end_x, rl_end_y, 0.3));

    widgets.push(text(
        mw(t, 98),
        ll_end_x - 6.0,
        ll_end_y - 4.0,
        14.0,
        4.0,
        "LL",
        3.0,
    ));
    widgets.push(text(
        mw(t, 99),
        rl_end_x + 1.0,
        rl_end_y - 4.0,
        12.0,
        4.0,
        "RL",
        3.0,
    ));

    let trp_y = sketch_y + sketch_h + 6.0;
    let trp_rows = 7_usize;
    let row_h = 7.5_f64;
    let trp_h = row_h * trp_rows as f64;
    let cols: [f64; 4] = [0.10, 0.45, 0.20, 0.25];
    let col_x: Vec<f64> = std::iter::once(margin)
        .chain(cols.iter().scan(margin, |acc, c| {
            *acc += sketch_w * c;
            Some(*acc)
        }))
        .collect();
    let headers = ["TRP", "Description", "Distance (m)", "Azimuth (mils)"];
    for (i, h) in headers.iter().enumerate() {
        widgets.push(text(
            mw(t, (70 + i) as u16),
            col_x[i] + 1.5,
            trp_y + 1.5,
            sketch_w * cols[i] - 3.0,
            row_h - 2.0,
            h,
            3.5,
        ));
    }
    for r in 1..=6 {
        let y = trp_y + row_h * r as f64;
        widgets.push(text(
            mw(t, (80 + r) as u16),
            col_x[0] + 2.0,
            y + 1.5,
            sketch_w * cols[0] - 3.0,
            row_h - 2.0,
            &r.to_string(),
            4.0,
        ));
    }
    widgets.push(rect(mw(t, 100), margin, trp_y, sketch_w, trp_h));
    for (i, x) in col_x.iter().enumerate().take(col_x.len() - 1).skip(1) {
        widgets.push(vline(mw(t, (110 + i) as u16), *x, trp_y, trp_h, 0.2));
    }
    for r in 1..trp_rows {
        let y = trp_y + row_h * r as f64;
        widgets.push(hline(mw(t, (120 + r) as u16), margin, y, sketch_w, 0.2));
    }

    let rem_y = trp_y + trp_h + 6.0;
    let rem_h = page_h - rem_y - margin;
    if rem_h > 6.0 {
        widgets.push(text(
            mw(t, 200),
            margin,
            rem_y,
            page_w - margin * 2.0,
            5.0,
            "Remarks / dead space:",
            3.5,
        ));
        widgets.push(lines_region(
            mw(t, 201),
            margin,
            rem_y + 5.5,
            page_w - margin * 2.0,
            rem_h - 5.5,
            6.0,
        ));
    }

    PageTemplate {
        id: TemplateId(BUILTIN_MILITARY_RANGE_CARD_ID),
        name: "Range Card".into(),
        description: "DA 5517-style fighting-position sector sketch with weapon position, magnetic-N marker, sector limits, six numbered TRPs, and a remarks / dead-space block.".into(),
        background: BackgroundType::Blank,
        size_mm: US_LETTER,
        tiling: TilingMode::None,
        default_viewport: None,
        widgets,
        category: "Military".into(),
    }
}
