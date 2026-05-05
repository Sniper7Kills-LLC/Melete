//! GOTWA Brief Card  -  A6 pocket card.
//!
//! Leader's leave-behind brief. Going / Others / Time / What if I don't
//! return / Actions on enemy contact.

use uuid::{uuid, Uuid};

use journal_core::{BackgroundType, PageTemplate, TemplateId, TemplateWidget, TilingMode};

use crate::builtin::{hline, lines_region, mw, text, A6};

pub const BUILTIN_MILITARY_GOTWA_ID: Uuid = uuid!("00000000-0000-0000-0000-000000000017");

pub fn builtin_military_gotwa() -> PageTemplate {
    let t: u8 = 0x17;
    let margin = 5.0_f64;
    let (page_w, page_h) = A6;
    let mut widgets: Vec<TemplateWidget> = Vec::new();

    widgets.push(text(
        mw(t, 1),
        margin,
        margin,
        page_w - margin * 2.0,
        7.0,
        "GOTWA BRIEF",
        5.5,
    ));
    widgets.push(text(
        mw(t, 2),
        margin,
        margin + 7.5,
        page_w - margin * 2.0,
        4.0,
        "Leader leave-behind brief  -  leave a copy with the second-in-charge.",
        2.7,
    ));

    let body_top = margin + 13.0;
    let body_h = page_h - body_top - margin;
    let row_h = body_h / 5.0;
    let labels: [&str; 5] = [
        "G  -  where I am Going",
        "O  -  Others I'm taking with me",
        "T  -  Time I'll be gone (return NLT)",
        "W  -  What if I don't return (escalation / next leader)",
        "A  -  Actions on enemy contact",
    ];
    for (i, label) in labels.iter().enumerate() {
        let y = body_top + row_h * i as f64;
        widgets.push(text(
            mw(t, (10 + i) as u16),
            margin,
            y,
            page_w - margin * 2.0,
            4.0,
            label,
            3.0,
        ));
        widgets.push(lines_region(
            mw(t, (20 + i) as u16),
            margin + 2.0,
            y + 4.5,
            page_w - margin * 2.0 - 2.0,
            row_h - 5.5,
            4.0,
        ));
        if i + 1 < labels.len() {
            widgets.push(hline(
                mw(t, (30 + i) as u16),
                margin,
                y + row_h - 0.4,
                page_w - margin * 2.0,
                0.3,
            ));
        }
    }

    PageTemplate {
        id: TemplateId(BUILTIN_MILITARY_GOTWA_ID),
        name: "GOTWA Brief".into(),
        description: "Pocket-card leader's leave-behind brief  -  Going, Others, Time, What-if-I-don't-return, Actions on contact.".into(),
        background: BackgroundType::Blank,
        size_mm: A6,
        tiling: TilingMode::None,
        default_viewport: None,
        widgets,
        category: "Military".into(),
    }
}
