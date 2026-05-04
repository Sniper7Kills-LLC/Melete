//! 9-Line MEDEVAC Request — A5 portrait.
//!
//! Standard medical evacuation request: location, frequency, patient
//! precedence, special equipment, patients by type, security at PZ,
//! method of marking, nationality, NBC.

use uuid::{uuid, Uuid};

use journal_core::{BackgroundType, PageTemplate, TemplateId, TemplateWidget, TilingMode};

use crate::builtin::{hline, lines_region, mw, text, A5};

pub const BUILTIN_MILITARY_MEDEVAC_ID: Uuid = uuid!("00000000-0000-0000-0000-000000000012");

pub fn builtin_military_medevac() -> PageTemplate {
    let t: u8 = 0x12;
    let margin = 6.0_f64;
    let (page_w, page_h) = A5;
    let mut widgets: Vec<TemplateWidget> = Vec::new();

    widgets.push(text(
        mw(t, 1),
        margin,
        margin,
        page_w - margin * 2.0,
        9.0,
        "9-LINE MEDEVAC REQUEST",
        6.5,
    ));
    let body_top = margin + 11.0;
    let body_h = page_h - body_top - margin;
    let row_h = body_h / 9.0;

    let lines: [&str; 9] = [
        "1. Location of pickup (grid):",
        "2. Frequency / call sign:",
        "3. Patient precedence — A urgent · B urgent-surgical · C priority · D routine · E convenience:",
        "4. Special equipment — A none · B hoist · C extraction · D ventilator:",
        "5. Patients by type — L# litter · A# ambulatory:",
        "6. Security at PZ — N none · P possible · E enemy · X escort required:",
        "7. Method of marking — A panels · B pyro · C smoke · D none · E other:",
        "8. Patient nationality / status — A US mil · B US civ · C non-US mil · D non-US civ · E EPW:",
        "9. NBC contamination (N/B/C) / terrain description:",
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
        id: TemplateId(BUILTIN_MILITARY_MEDEVAC_ID),
        name: "9-Line MEDEVAC".into(),
        description: "Standard 9-line medical evacuation request — location, frequency, patient precedence, special equipment, patients by type, security at PZ, method of marking, nationality, NBC.".into(),
        background: BackgroundType::Blank,
        size_mm: A5,
        tiling: TilingMode::None,
        default_viewport: None,
        widgets,
        category: "Military".into(),
    }
}
