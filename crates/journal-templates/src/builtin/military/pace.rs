//! PACE Communications Plan  -  US Letter landscape.
//!
//! Four columns (Primary / Alternate / Contingency / Emergency) over six
//! rows (Net, Frequency, Call sign, Authentication, Encryption, Notes).

use uuid::{uuid, Uuid};

use journal_core::{
    BackgroundType, PageTemplate, TemplateId, TemplateWidget, TilingMode, WidgetKind, WidgetRect,
    WidgetStyle,
};

use crate::builtin::{mw, text, US_LETTER_LANDSCAPE};

pub const BUILTIN_MILITARY_PACE_ID: Uuid = uuid!("00000000-0000-0000-0000-000000000018");

pub fn builtin_military_pace() -> PageTemplate {
    let t: u8 = 0x18;
    let margin = 8.0_f64;
    let (page_w, page_h) = US_LETTER_LANDSCAPE;
    let mut widgets: Vec<TemplateWidget> = Vec::new();

    widgets.push(text(
        mw(t, 1),
        margin,
        margin,
        page_w - margin * 2.0,
        9.0,
        "PACE COMMUNICATIONS PLAN",
        7.0,
    ));
    widgets.push(text(
        mw(t, 2),
        margin,
        margin + 10.0,
        page_w - margin * 2.0,
        4.5,
        "Unit: __________   Op / Mission: __________   DTG effective: __________   Authentication table #: __________",
        3.0,
    ));

    let body_top = margin + 18.0;
    let body_h = page_h - body_top - margin;
    let cols = ["Primary", "Alternate", "Contingency", "Emergency"];
    let label_col_w = 36.0_f64;
    let table_w = page_w - margin * 2.0;
    let col_w = (table_w - label_col_w) / 4.0;
    let rows = [
        "Net",
        "Frequency",
        "Call sign",
        "Authentication",
        "Encryption / KEK",
        "Notes",
    ];
    let header_h = 9.0_f64;
    let row_h = (body_h - header_h) / rows.len() as f64;

    // ── Table grid ──────────────────────────────────────────────────────
    // Build the table out of one rect per cell — guarantees every
    // cell has its own visible border instead of relying on shared
    // hairlines that can get lost at typical zoom levels. Header row
    // and label column rects use a heavier stroke than the data
    // cells so the structure reads at a glance.
    let header_style = WidgetStyle {
        stroke_width_mm: 0.6,
        ..WidgetStyle::default()
    };
    let cell_style = WidgetStyle {
        stroke_width_mm: 0.4,
        ..WidgetStyle::default()
    };
    let mut cell_id: u16 = 200;

    // Header-row rects (one per column, including the label column).
    let total_cols = 1 + cols.len();
    for c in 0..total_cols {
        let cx = if c == 0 {
            margin
        } else {
            margin + label_col_w + col_w * (c - 1) as f64
        };
        let cw = if c == 0 { label_col_w } else { col_w };
        widgets.push(TemplateWidget {
            id: mw(t, cell_id),
            kind: WidgetKind::Rectangle,
            rect: WidgetRect {
                x: cx,
                y: body_top,
                width: cw,
                height: header_h,
            },
            style: header_style.clone(),
        });
        cell_id += 1;
    }
    // Data-row rects.
    for r in 0..rows.len() {
        let y = body_top + header_h + row_h * r as f64;
        for c in 0..total_cols {
            let cx = if c == 0 {
                margin
            } else {
                margin + label_col_w + col_w * (c - 1) as f64
            };
            let cw = if c == 0 { label_col_w } else { col_w };
            // Label-column data cells use the heavier style so the
            // entire left column reads as a header.
            let style = if c == 0 {
                header_style.clone()
            } else {
                cell_style.clone()
            };
            widgets.push(TemplateWidget {
                id: mw(t, cell_id),
                kind: WidgetKind::Rectangle,
                rect: WidgetRect {
                    x: cx,
                    y,
                    width: cw,
                    height: row_h,
                },
                style,
            });
            cell_id += 1;
        }
    }

    // Column headers (Type / Primary / Alternate / Contingency / Emergency).
    widgets.push(text(
        mw(t, 9),
        margin + 2.0,
        body_top + 2.0,
        label_col_w - 4.0,
        header_h - 4.0,
        "Type",
        4.5,
    ));
    for (i, h) in cols.iter().enumerate() {
        let x = margin + label_col_w + col_w * i as f64;
        widgets.push(text(
            mw(t, (10 + i) as u16),
            x + 2.0,
            body_top + 2.0,
            col_w - 4.0,
            header_h - 4.0,
            h,
            4.5,
        ));
    }
    // Row labels in the leftmost column.
    for (r, rlabel) in rows.iter().enumerate() {
        let y = body_top + header_h + row_h * r as f64;
        widgets.push(text(
            mw(t, (70 + r) as u16),
            margin + 2.0,
            y + 1.5,
            label_col_w - 4.0,
            row_h - 3.0,
            rlabel,
            3.6,
        ));
    }

    PageTemplate {
        id: TemplateId(BUILTIN_MILITARY_PACE_ID),
        name: "PACE Plan".into(),
        description: "Communications plan  -  Primary / Alternate / Contingency / Emergency columns over Net, Frequency, Call sign, Authentication, Encryption, and Notes rows.".into(),
        background: BackgroundType::Blank,
        size_mm: US_LETTER_LANDSCAPE,
        tiling: TilingMode::None,
        default_viewport: None,
        widgets,
        category: "Military".into(),
    }
}
