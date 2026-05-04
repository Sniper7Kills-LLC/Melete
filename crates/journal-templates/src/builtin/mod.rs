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
    builtin_military_gotwa, builtin_military_medevac, builtin_military_opord, builtin_military_pace,
    builtin_military_pcc_pci, builtin_military_range_card, builtin_military_salute,
    builtin_military_uxo, BUILTIN_MILITARY_GOTWA_ID, BUILTIN_MILITARY_MEDEVAC_ID,
    BUILTIN_MILITARY_OPORD_ID, BUILTIN_MILITARY_PACE_ID, BUILTIN_MILITARY_PCC_PCI_ID,
    BUILTIN_MILITARY_RANGE_CARD_ID, BUILTIN_MILITARY_SALUTE_ID, BUILTIN_MILITARY_UXO_ID,
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
// Widget shorthand helpers (used by military forms — kept generic so any
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
        rect: WidgetRect { x, y, width: w, height: h },
        style: WidgetStyle::default(),
    }
}

pub(crate) fn rect(id: Uuid, x: f64, y: f64, w: f64, h: f64) -> TemplateWidget {
    TemplateWidget {
        id,
        kind: WidgetKind::Rectangle,
        rect: WidgetRect { x, y, width: w, height: h },
        style: WidgetStyle::default(),
    }
}

pub(crate) fn hline(id: Uuid, x: f64, y: f64, w: f64, thickness: f64) -> TemplateWidget {
    TemplateWidget {
        id,
        kind: WidgetKind::Line { thickness_mm: thickness },
        rect: WidgetRect { x, y, width: w, height: 0.0 },
        style: WidgetStyle::default(),
    }
}

pub(crate) fn vline(id: Uuid, x: f64, y: f64, h: f64, thickness: f64) -> TemplateWidget {
    TemplateWidget {
        id,
        kind: WidgetKind::Line { thickness_mm: thickness },
        rect: WidgetRect { x, y, width: 0.0, height: h },
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
        kind: WidgetKind::LinesRegion { spacing_mm: spacing },
        rect: WidgetRect { x, y, width: w, height: h },
        style: WidgetStyle::default(),
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
        rect: WidgetRect { x, y, width: w, height: h },
        style: WidgetStyle::default(),
    }
}

// ---------------------------------------------------------------------------
// Template registry — every built-in PageTemplate in display order.
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
