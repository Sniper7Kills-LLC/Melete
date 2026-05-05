//! SALUTE Report Card  -  A5 portrait, three stacked sighting blocks.
//!
//! Spot-report card useful as a tear-off for patrols. Each block: Size,
//! Activity, Location, Unit, Time, Equipment.

use uuid::{uuid, Uuid};

use journal_core::{BackgroundType, PageTemplate, TemplateId, TemplateWidget, TilingMode};

use crate::builtin::{hline, mw, rect, text, A5};

pub const BUILTIN_MILITARY_SALUTE_ID: Uuid = uuid!("00000000-0000-0000-0000-000000000015");

pub fn builtin_military_salute() -> PageTemplate {
    let t: u8 = 0x15;
    let margin = 6.0_f64;
    let (page_w, page_h) = A5;
    let mut widgets: Vec<TemplateWidget> = Vec::new();

    widgets.push(text(
        mw(t, 1),
        margin,
        margin,
        page_w - margin * 2.0,
        8.0,
        "SALUTE REPORT  (Size / Activity / Location / Unit / Time / Equipment)",
        4.0,
    ));

    let body_top = margin + 10.0;
    let body_h = page_h - body_top - margin;
    let block_h = body_h / 3.0;
    let labels: [&str; 6] = [
        "S  -  Size",
        "A  -  Activity",
        "L  -  Location (grid)",
        "U  -  Unit / dress / markings",
        "T  -  Time (DTG observed)",
        "E  -  Equipment",
    ];
    for b in 0..3 {
        let block_y = body_top + block_h * b as f64;
        widgets.push(rect(
            mw(t, (10 + b) as u16),
            margin,
            block_y,
            page_w - margin * 2.0,
            block_h - 1.5,
        ));
        widgets.push(text(
            mw(t, (20 + b) as u16),
            margin + 1.0,
            block_y + 0.5,
            page_w - margin * 2.0,
            4.0,
            &format!("Sighting #{}", b + 1),
            3.5,
        ));
        let row_h = (block_h - 6.0) / 6.0;
        for (i, label) in labels.iter().enumerate() {
            let y = block_y + 5.0 + row_h * i as f64;
            widgets.push(text(
                mw(t, (40 + b * 10 + i) as u16),
                margin + 1.5,
                y,
                page_w - margin * 2.0 - 3.0,
                row_h,
                label,
                3.0,
            ));
            widgets.push(hline(
                mw(t, (90 + b * 10 + i) as u16),
                margin + 25.0,
                y + row_h - 0.4,
                page_w - margin * 2.0 - 27.0,
                0.3,
            ));
        }
    }

    PageTemplate {
        id: TemplateId(BUILTIN_MILITARY_SALUTE_ID),
        name: "SALUTE Report".into(),
        description: "Three SALUTE spot-report blocks per A5 page (Size, Activity, Location, Unit, Time, Equipment). Useful as a tear-off card for patrols.".into(),
        background: BackgroundType::Blank,
        size_mm: A5,
        tiling: TilingMode::None,
        default_viewport: None,
        widgets,
        category: "Military".into(),
    }
}
