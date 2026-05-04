//! 9-Line UXO / IED Report — A5 portrait.
//!
//! Unexploded-ordnance / IED report: DTG, reporting unit, freq, location,
//! type / quantity / configuration, NBC, resources threatened, mission
//! impact, recommended priority + protective measures.

use uuid::{uuid, Uuid};

use journal_core::{BackgroundType, PageTemplate, TemplateId, TemplateWidget, TilingMode};

use crate::builtin::{hline, lines_region, mw, text, A5};

pub const BUILTIN_MILITARY_UXO_ID: Uuid = uuid!("00000000-0000-0000-0000-000000000016");

pub fn builtin_military_uxo() -> PageTemplate {
    let t: u8 = 0x16;
    let margin = 6.0_f64;
    let (page_w, page_h) = A5;
    let mut widgets: Vec<TemplateWidget> = Vec::new();

    widgets.push(text(
        mw(t, 1),
        margin,
        margin,
        page_w - margin * 2.0,
        9.0,
        "9-LINE UXO / IED REPORT",
        6.5,
    ));
    let body_top = margin + 11.0;
    let body_h = page_h - body_top - margin;
    let row_h = body_h / 9.0;

    let lines: [&str; 9] = [
        "1. DTG (date-time group):",
        "2. Reporting unit / activity:",
        "3. Contact frequency / call sign:",
        "4. Location (grid):",
        "5. Type / quantity / configuration (size, shape, markings, fuze):",
        "6. NBC contamination (Y/N + agent):",
        "7. Resources threatened (people / equipment / facility):",
        "8. Impact on mission:",
        "9. Recommended priority — A immediate · B indirect · C minor · D no threat / Protective measures:",
    ];
    for (i, label) in lines.iter().enumerate() {
        let y = body_top + row_h * i as f64;
        widgets.push(text(
            mw(t, (10 + i) as u16),
            margin,
            y,
            page_w - margin * 2.0,
            5.0,
            label,
            3.0,
        ));
        widgets.push(lines_region(
            mw(t, (30 + i) as u16),
            margin + 3.0,
            y + 5.5,
            page_w - margin * 2.0 - 3.0,
            row_h - 6.5,
            5.0,
        ));
        if i + 1 < lines.len() {
            widgets.push(hline(
                mw(t, (60 + i) as u16),
                margin,
                y + row_h - 0.4,
                page_w - margin * 2.0,
                0.15,
            ));
        }
    }

    PageTemplate {
        id: TemplateId(BUILTIN_MILITARY_UXO_ID),
        name: "9-Line UXO / IED".into(),
        description: "Standard 9-line unexploded-ordnance / IED report — DTG, unit, freq, location, type, NBC, resources, mission impact, recommended priority.".into(),
        background: BackgroundType::Blank,
        size_mm: A5,
        tiling: TilingMode::None,
        default_viewport: None,
        widgets,
        category: "Military".into(),
    }
}
