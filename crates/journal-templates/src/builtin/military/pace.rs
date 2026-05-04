//! PACE Communications Plan — US Letter landscape.
//!
//! Four columns (Primary / Alternate / Contingency / Emergency) over six
//! rows (Net, Frequency, Call sign, Authentication, Encryption, Notes).

use uuid::{uuid, Uuid};

use journal_core::{BackgroundType, PageTemplate, TemplateId, TemplateWidget, TilingMode};

use crate::builtin::{hline, mw, rect, text, vline, US_LETTER_LANDSCAPE};

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
    let col_w = (page_w - margin * 2.0) / 4.0;
    let rows = ["Net", "Frequency", "Call sign", "Authentication", "Encryption / KEK", "Notes"];
    let header_h = 9.0_f64;
    let row_h = (body_h - header_h) / rows.len() as f64;

    for (i, h) in cols.iter().enumerate() {
        let x = margin + col_w * i as f64;
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
    widgets.push(rect(mw(t, 30), margin, body_top, page_w - margin * 2.0, body_h));
    widgets.push(hline(mw(t, 31), margin, body_top + header_h, page_w - margin * 2.0, 0.3));
    for i in 1..cols.len() {
        widgets.push(vline(
            mw(t, (40 + i) as u16),
            margin + col_w * i as f64,
            body_top,
            body_h,
            0.25,
        ));
    }
    for (r, rlabel) in rows.iter().enumerate() {
        let y = body_top + header_h + row_h * r as f64;
        widgets.push(hline(mw(t, (60 + r) as u16), margin, y, page_w - margin * 2.0, 0.2));
        widgets.push(text(
            mw(t, (70 + r) as u16),
            margin + 2.0,
            y + 1.5,
            col_w - 4.0,
            row_h - 3.0,
            rlabel,
            3.2,
        ));
    }

    PageTemplate {
        id: TemplateId(BUILTIN_MILITARY_PACE_ID),
        name: "PACE Plan".into(),
        description: "Communications plan — Primary / Alternate / Contingency / Emergency columns over Net, Frequency, Call sign, Authentication, Encryption, and Notes rows.".into(),
        background: BackgroundType::Blank,
        size_mm: US_LETTER_LANDSCAPE,
        tiling: TilingMode::None,
        default_viewport: None,
        widgets,
        category: "Military".into(),
    }
}
