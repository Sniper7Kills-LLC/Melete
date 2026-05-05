//! Pre-Combat Check / Pre-Combat Inspection  -  US Letter portrait.
//!
//! Two-column checklist of eight categories: weapons, ammo / pyro,
//! optics / lasers, radios / comms, medical, water / sustainment,
//! personal kit, mission items.

use uuid::{uuid, Uuid};

use journal_core::{BackgroundType, PageTemplate, TemplateId, TemplateWidget, TilingMode};

use crate::builtin::{checklist, hline, mw, text, US_LETTER};

pub const BUILTIN_MILITARY_PCC_PCI_ID: Uuid = uuid!("00000000-0000-0000-0000-000000000019");

pub fn builtin_military_pcc_pci() -> PageTemplate {
    let t: u8 = 0x19;
    let margin = 8.0_f64;
    let (page_w, page_h) = US_LETTER;
    let mut widgets: Vec<TemplateWidget> = Vec::new();

    widgets.push(text(
        mw(t, 1),
        margin,
        margin,
        page_w - margin * 2.0,
        9.0,
        "PRE-COMBAT CHECK / PRE-COMBAT INSPECTION",
        6.0,
    ));
    widgets.push(text(
        mw(t, 2),
        margin,
        margin + 10.0,
        page_w - margin * 2.0,
        4.5,
        "Unit: __________   Mission / DTG: __________   Inspector: __________",
        3.0,
    ));

    let body_top = margin + 17.0;
    let body_h = page_h - body_top - margin;
    let col_w = (page_w - margin * 2.0 - 4.0) / 2.0;

    let sections: [(&str, &[&str]); 8] = [
        (
            "Weapons",
            &[
                "Rifle / SBR  -  clean, function check",
                "Optic / iron sights zeroed",
                "Magazines (count and serviceable)",
                "Sling tight, attached, serviceable",
                "Cleaning kit",
            ],
        ),
        (
            "Ammunition / Pyro",
            &[
                "Basic combat load loaded + serial verified",
                "Tracer ratio per SOP",
                "Smoke / signal grenades",
                "Pyro stowed safe",
                "Charging handle / chamber clear",
            ],
        ),
        (
            "Optics & Lasers",
            &[
                "NVG / NODs functional, batteries fresh",
                "Headborne mount torqued",
                "IR laser zero verified",
                "Spare batteries (count)",
                "Bore-sight tool",
            ],
        ),
        (
            "Radios / Comms",
            &[
                "Primary radio loaded, fill verified",
                "Alternate freq programmed",
                "Headset / PTT, retention lanyard",
                "Antenna serviceable + spare",
                "Spare battery (count)",
            ],
        ),
        (
            "Medical",
            &[
                "IFAK complete, expiration current",
                "TQ (CAT) accessible, serviceable",
                "Pressure / gauze dressings",
                "Combat-pill pack",
                "9-line MEDEVAC card on person",
            ],
        ),
        (
            "Water / Sustainment",
            &[
                "Hydration (>=3 L) full",
                "Snivel kit (snacks, electrolytes)",
                "Per-mission MREs / chow",
                "Mission duration food count verified",
            ],
        ),
        (
            "Personal Kit",
            &[
                "Plate carrier  -  plates serial verified",
                "Helmet retention adjusted",
                "Knee / elbow pads",
                "Eye-pro + ear-pro",
                "Gloves, boots, weather layers",
            ],
        ),
        (
            "Mission Items",
            &[
                "Map / overlay (current edition)",
                "Compass + protractor",
                "Notebook + pen / DTG card",
                "GPS, fresh batteries",
                "Mission-specific gear (per OPORD)",
            ],
        ),
    ];

    let rows_per_col = 4_usize;
    let col_block_h = body_h / rows_per_col as f64;
    let mut idx: u16 = 0;
    for (i, (heading, items)) in sections.iter().enumerate() {
        let col = i / rows_per_col;
        let row = i % rows_per_col;
        let x = margin + (col_w + 4.0) * col as f64;
        let y = body_top + col_block_h * row as f64;
        widgets.push(text(mw(t, 10 + idx), x, y + 0.5, col_w, 5.0, heading, 4.0));
        idx += 1;
        widgets.push(hline(mw(t, 30 + idx), x, y + 5.0, col_w, 0.25));
        idx += 1;
        widgets.push(checklist(
            mw(t, 50 + idx),
            x + 1.0,
            y + 6.0,
            col_w - 1.0,
            col_block_h - 7.0,
            items,
        ));
        idx += 1;
    }

    PageTemplate {
        id: TemplateId(BUILTIN_MILITARY_PCC_PCI_ID),
        name: "PCC / PCI Checklist".into(),
        description: "Pre-combat check / inspection sheet  -  8 sections (weapons, ammo / pyro, optics / lasers, radios, medical, sustainment, personal kit, mission items) with checkbox rows.".into(),
        background: BackgroundType::Blank,
        size_mm: US_LETTER,
        tiling: TilingMode::None,
        default_viewport: None,
        widgets,
        category: "Military".into(),
    }
}
