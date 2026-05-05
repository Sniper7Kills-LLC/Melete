//! Built-in page templates.
//!
//! Organized into one folder per template `category`. Each individual
//! template lives in its own file inside the matching category folder so
//! it can be edited / versioned / tweaked independently. Shared
//! paper-size constants and the small widget-construction shorthands
//! the military forms use are declared here at the module root.

use uuid::Uuid;

use journal_core::{PageTemplate, TemplateWidget, WidgetKind, WidgetRect, WidgetStyle};

// Categorized template folders.
pub mod franklin;
pub mod fullfocus;
pub mod grids;
pub mod military;
pub mod note_taking;
pub mod paper;
pub mod planner;

pub use franklin::{
    builtin_franklin_daily, builtin_franklin_weekly, BUILTIN_FRANKLIN_DAILY_ID,
    BUILTIN_FRANKLIN_WEEKLY_ID,
};
pub use fullfocus::{builtin_fullfocus_daily, BUILTIN_FULLFOCUS_DAILY_ID};
pub use grids::{
    builtin_dotted, builtin_engineering_graph, builtin_grid, builtin_hexagonal, builtin_isometric,
    BUILTIN_DOTTED_ID, BUILTIN_ENGINEERING_GRAPH_ID, BUILTIN_GRID_ID, BUILTIN_HEX_ID,
    BUILTIN_ISOMETRIC_ID,
};
pub use military::{
    builtin_military_gotwa, builtin_military_medevac, builtin_military_opord,
    builtin_military_pace, builtin_military_pcc_pci, builtin_military_range_card,
    builtin_military_salute, builtin_military_uxo, BUILTIN_MILITARY_GOTWA_ID,
    BUILTIN_MILITARY_MEDEVAC_ID, BUILTIN_MILITARY_OPORD_ID, BUILTIN_MILITARY_PACE_ID,
    BUILTIN_MILITARY_PCC_PCI_ID, BUILTIN_MILITARY_RANGE_CARD_ID, BUILTIN_MILITARY_SALUTE_ID,
    BUILTIN_MILITARY_UXO_ID,
};
pub use note_taking::{
    builtin_cornell_notes, builtin_music_staff, BUILTIN_CORNELL_NOTES_ID, BUILTIN_MUSIC_STAFF_ID,
};
pub use paper::{
    builtin_blank, builtin_college_ruled, builtin_ruled, builtin_wide_ruled, BUILTIN_BLANK_ID,
    BUILTIN_COLLEGE_RULED_ID, BUILTIN_RULED_ID, BUILTIN_WIDE_RULED_ID,
};
pub use planner::{
    builtin_daily_planner, builtin_monthly_goals, builtin_quarterly_review,
    BUILTIN_DAILY_PLANNER_ID, BUILTIN_MONTHLY_GOALS_ID, BUILTIN_QUARTERLY_REVIEW_ID,
};

// ---------------------------------------------------------------------------
// Page sizes (mm)
// ---------------------------------------------------------------------------

pub(crate) const A6: (f64, f64) = (105.0, 148.0);
pub(crate) const A5: (f64, f64) = (148.0, 210.0);
pub(crate) const US_LETTER: (f64, f64) = (215.9, 279.4);
pub(crate) const US_LETTER_LANDSCAPE: (f64, f64) = (US_LETTER.1, US_LETTER.0);

// ---------------------------------------------------------------------------
// Widget shorthand helpers (used by military forms  -  kept generic so any
// future template file can borrow them).
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

pub(crate) fn text(
    id: Uuid,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    body: &str,
    font_mm: f64,
) -> TemplateWidget {
    TemplateWidget {
        id,
        kind: WidgetKind::TextBlock {
            text: body.into(),
            font_size_mm: font_mm,
        },
        rect: WidgetRect {
            x,
            y,
            width: w,
            height: h,
        },
        style: WidgetStyle::default(),
    }
}

pub(crate) fn rect(id: Uuid, x: f64, y: f64, w: f64, h: f64) -> TemplateWidget {
    TemplateWidget {
        id,
        kind: WidgetKind::Rectangle,
        rect: WidgetRect {
            x,
            y,
            width: w,
            height: h,
        },
        style: WidgetStyle::default(),
    }
}

pub(crate) fn hline(id: Uuid, x: f64, y: f64, w: f64, thickness: f64) -> TemplateWidget {
    TemplateWidget {
        id,
        kind: WidgetKind::Line {
            thickness_mm: thickness,
        },
        rect: WidgetRect {
            x,
            y,
            width: w,
            height: 0.0,
        },
        style: WidgetStyle::default(),
    }
}

pub(crate) fn vline(id: Uuid, x: f64, y: f64, h: f64, thickness: f64) -> TemplateWidget {
    TemplateWidget {
        id,
        kind: WidgetKind::Line {
            thickness_mm: thickness,
        },
        rect: WidgetRect {
            x,
            y,
            width: 0.0,
            height: h,
        },
        style: WidgetStyle::default(),
    }
}

pub(crate) fn lines_region(
    id: Uuid,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    spacing: f64,
) -> TemplateWidget {
    TemplateWidget {
        id,
        kind: WidgetKind::LinesRegion {
            spacing_mm: spacing,
        },
        rect: WidgetRect {
            x,
            y,
            width: w,
            height: h,
        },
        style: WidgetStyle::default(),
    }
}

/// Free-orientation line segment between two arbitrary points. Encodes
/// (x1, y1) -> (x2, y2) into the underlying `Line` widget by allowing
/// negative `height` on the bounding rect so the diagonal can run in
/// any of the four quadrants. The widget renderer's TL->BR
/// `move_to(x,y) -> line_to(x+w, y+h)` shape draws the segment
/// correctly with signed deltas.
/// Standard "school notebook paper" chrome — red side margin, header
/// rule, and three filled dots showing where a 3-hole punch would
/// land if the page were printed. Reused by every lined-paper
/// template in the Paper category.
///
/// 3-hole punch geometry (US standard): 1/2" (12.7mm) from the left
/// edge, 4.25" (107.95mm) between holes, centered vertically on the
/// page.
pub(crate) fn lined_paper_widgets(t: u8) -> Vec<TemplateWidget> {
    let red = journal_core::Color {
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

#[allow(dead_code)]
pub(crate) fn segment(
    id: Uuid,
    x1: f64,
    y1: f64,
    x2: f64,
    y2: f64,
    thickness: f64,
) -> TemplateWidget {
    TemplateWidget {
        id,
        kind: WidgetKind::Line {
            thickness_mm: thickness,
        },
        rect: WidgetRect {
            x: x1,
            y: y1,
            width: x2 - x1,
            height: y2 - y1,
        },
        style: WidgetStyle::default(),
    }
}

#[allow(dead_code)]
pub(crate) fn arc(
    id: Uuid,
    cx: f64,
    cy: f64,
    radius: f64,
    start_deg: f64,
    sweep_deg: f64,
    thickness: f64,
) -> TemplateWidget {
    TemplateWidget {
        id,
        kind: WidgetKind::Arc {
            start_deg,
            sweep_deg,
            thickness_mm: thickness,
        },
        rect: WidgetRect {
            x: cx - radius,
            y: cy - radius,
            width: radius * 2.0,
            height: radius * 2.0,
        },
        style: WidgetStyle::default(),
    }
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

pub(crate) fn checklist(
    id: Uuid,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    items: &[&str],
) -> TemplateWidget {
    TemplateWidget {
        id,
        kind: WidgetKind::Checklist {
            items: items.iter().map(|s| (*s).to_string()).collect(),
        },
        rect: WidgetRect {
            x,
            y,
            width: w,
            height: h,
        },
        style: WidgetStyle::default(),
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
        builtin_cornell_notes(),
        builtin_isometric(),
        builtin_hexagonal(),
        builtin_engineering_graph(),
        builtin_music_staff(),
        builtin_daily_planner(),
        builtin_fullfocus_daily(),
        builtin_franklin_daily(),
        builtin_franklin_weekly(),
        builtin_monthly_goals(),
        builtin_quarterly_review(),
        builtin_military_medevac(),
        builtin_military_uxo(),
        builtin_military_salute(),
        builtin_military_gotwa(),
        builtin_military_range_card(),
        builtin_military_pace(),
        builtin_military_opord(),
        builtin_military_pcc_pci(),
    ]
}
