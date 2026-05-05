//! 5-Paragraph OPORD  -  US Letter portrait.
//!
//! Operations-order skeleton: Situation, Mission, Execution, Sustainment,
//! Command & Signal. Each block has a heading row and a ruled fill area.

use uuid::{uuid, Uuid};

use journal_core::{BackgroundType, PageTemplate, TemplateId, TemplateWidget, TilingMode};

use crate::builtin::{lines_region, mw, rect, text, US_LETTER};

pub const BUILTIN_MILITARY_OPORD_ID: Uuid = uuid!("00000000-0000-0000-0000-000000000014");

pub fn builtin_military_opord() -> PageTemplate {
    let t: u8 = 0x14;
    let margin = 8.0_f64;
    let (page_w, page_h) = US_LETTER;
    let mut widgets: Vec<TemplateWidget> = Vec::new();

    widgets.push(text(
        mw(t, 1),
        margin,
        margin,
        page_w - margin * 2.0,
        9.0,
        "OPERATION ORDER (OPORD)",
        7.0,
    ));
    widgets.push(text(
        mw(t, 2),
        margin,
        margin + 9.0,
        page_w - margin * 2.0,
        5.0,
        "Unit: __________  OPORD #: ______  DTG: __________  Map / Sheet: __________",
        3.2,
    ));

    let body_top = margin + 18.0;
    let body_h = page_h - body_top - margin;
    let blocks: [(&str, f64); 5] = [
        ("1. SITUATION  (Enemy / Friendly / Civilian / Attachments-Detachments)", 0.20),
        ("2. MISSION", 0.10),
        ("3. EXECUTION  (Commander's intent / Concept / Tasks to subordinate units / Coordinating instructions)", 0.30),
        ("4. SUSTAINMENT  (Logistics / Personnel / Health Service Support)", 0.20),
        ("5. COMMAND & SIGNAL  (Command relationships / Succession / CP locations / Signal / PACE)", 0.20),
    ];
    let mut y = body_top;
    for (i, (label, frac)) in blocks.iter().enumerate() {
        let h = body_h * frac;
        widgets.push(rect(
            mw(t, (10 + i) as u16),
            margin,
            y,
            page_w - margin * 2.0,
            h,
        ));
        widgets.push(text(
            mw(t, (20 + i) as u16),
            margin + 1.5,
            y + 1.0,
            page_w - margin * 2.0 - 3.0,
            5.0,
            label,
            3.5,
        ));
        widgets.push(lines_region(
            mw(t, (30 + i) as u16),
            margin + 1.5,
            y + 7.0,
            page_w - margin * 2.0 - 3.0,
            h - 8.0,
            5.5,
        ));
        y += h;
    }

    PageTemplate {
        id: TemplateId(BUILTIN_MILITARY_OPORD_ID),
        name: "5-Paragraph OPORD".into(),
        description: "Operations order skeleton with the five standard paragraphs (Situation, Mission, Execution, Sustainment, Command & Signal) and ruled fill space inside each block.".into(),
        background: BackgroundType::Blank,
        size_mm: US_LETTER,
        tiling: TilingMode::None,
        default_viewport: None,
        widgets,
        category: "Military".into(),
    }
}
