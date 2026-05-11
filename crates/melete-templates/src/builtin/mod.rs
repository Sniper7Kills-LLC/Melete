//! Built-in page templates.
//!
//! Organized into one folder per template `category`. Each individual
//! template lives in its own file inside the matching category folder so
//! it can be edited / versioned / tweaked independently. Shared
//! paper-size constants and the small widget-construction shorthands
//! that the lined-paper templates use are declared here at the module
//! root.

use uuid::Uuid;

use melete_core::{PageTemplate, TemplateWidget, WidgetKind, WidgetRect, WidgetStyle};

// Categorized template folders.
pub mod grids;
pub mod paper;

pub use grids::{
    builtin_dotted, builtin_engineering_graph, builtin_grid, builtin_hexagonal, builtin_isometric,
    BUILTIN_DOTTED_ID, BUILTIN_ENGINEERING_GRAPH_ID, BUILTIN_GRID_ID, BUILTIN_HEX_ID,
    BUILTIN_ISOMETRIC_ID,
};
pub use paper::{
    builtin_blank, builtin_college_ruled, builtin_ruled, builtin_wide_ruled, BUILTIN_BLANK_ID,
    BUILTIN_COLLEGE_RULED_ID, BUILTIN_RULED_ID, BUILTIN_WIDE_RULED_ID,
};

// ---------------------------------------------------------------------------
// Page sizes (mm)
// ---------------------------------------------------------------------------

pub(crate) const US_LETTER: (f64, f64) = (215.9, 279.4);

// ---------------------------------------------------------------------------
// Widget shorthand helpers used by the lined-paper templates.
// ---------------------------------------------------------------------------

/// Deterministic per-widget UUID. `template` is the parent
/// `PageTemplate.id`'s last byte; `widget` is a stable index inside that
/// template. Avoids having to write each per-widget id as a `uuid!(...)`
/// literal.
pub(crate) fn mw(template: u8, widget: u16) -> Uuid {
    let mut bytes = [0u8; 16];
    bytes[0] = 0xc0;
    bytes[1] = template;
    bytes[2] = (widget >> 8) as u8;
    bytes[3] = widget as u8;
    Uuid::from_bytes(bytes)
}

/// Standard "school notebook paper" chrome — red side margin, header
/// rule, and three filled dots showing where a 3-hole punch would
/// land if the page were printed. Reused by every lined-paper
/// template in the Paper category.
///
/// 3-hole punch geometry (US standard): 1/2" (12.7mm) from the left
/// edge, 4.25" (107.95mm) between holes, centered vertically on the
/// page.
pub(crate) fn lined_paper_widgets(t: u8) -> Vec<TemplateWidget> {
    let red = melete_core::Color {
        r: 200,
        g: 60,
        b: 70,
        a: 230,
    };
    let red_style = WidgetStyle {
        stroke_color: red,
        fill_color: None,
        stroke_width_mm: 0.4,
    };
    let margin_x = 32.0_f64;
    let header_h = 12.0_f64;
    let (page_w, page_h) = US_LETTER;

    let mut widgets = Vec::new();

    // Red side margin.
    widgets.push(TemplateWidget {
        id: mw(t, 1),
        kind: WidgetKind::Line { thickness_mm: 0.4 },
        rect: WidgetRect {
            x: margin_x,
            y: 0.0,
            width: 0.0,
            height: page_h,
        },
        style: red_style,
    });

    // Header underline (sits above the first ruled line at the
    // template's background spacing — gives the user a place to write
    // a date / title without touching the body lines).
    widgets.push(TemplateWidget {
        id: mw(t, 2),
        kind: WidgetKind::Line { thickness_mm: 0.3 },
        rect: WidgetRect {
            x: margin_x + 4.0,
            y: header_h,
            width: page_w - margin_x - 8.0,
            height: 0.0,
        },
        style: WidgetStyle::default(),
    });

    // 3-hole punch dots — centered on the page vertically with
    // 4.25-inch spacing, 1/2 inch in from the left edge.
    let hole_x = 12.7_f64;
    let hole_radius = 1.8_f64;
    let center_y = page_h * 0.5;
    let spacing = 107.95_f64;
    for (i, dy) in [-spacing, 0.0, spacing].iter().enumerate() {
        widgets.push(dot(
            mw(t, (10 + i) as u16),
            hole_x,
            center_y + dy,
            hole_radius,
        ));
    }

    widgets
}

pub(crate) fn dot(id: Uuid, cx: f64, cy: f64, radius: f64) -> TemplateWidget {
    let mut style = WidgetStyle::default();
    style.fill_color = Some(style.stroke_color);
    style.stroke_width_mm = 0.0;
    TemplateWidget {
        id,
        kind: WidgetKind::Ellipse,
        rect: WidgetRect {
            x: cx - radius,
            y: cy - radius,
            width: radius * 2.0,
            height: radius * 2.0,
        },
        style,
    }
}

// ---------------------------------------------------------------------------
// Template registry  -  every built-in PageTemplate in display order.
// ---------------------------------------------------------------------------

pub fn builtin_templates() -> Vec<PageTemplate> {
    vec![
        builtin_blank(),
        builtin_dotted(),
        builtin_ruled(),
        builtin_grid(),
        builtin_wide_ruled(),
        builtin_college_ruled(),
        builtin_isometric(),
        builtin_hexagonal(),
        builtin_engineering_graph(),
    ]
}
