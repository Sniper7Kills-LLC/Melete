//! Music staff paper — twelve 5-line staves stacked down the page.

use uuid::{uuid, Uuid};

use journal_core::{
    BackgroundType, PageTemplate, TemplateId, TemplateWidget, TilingMode, WidgetKind, WidgetRect,
    WidgetStyle,
};

use crate::builtin::US_LETTER;

pub const BUILTIN_MUSIC_STAFF_ID: Uuid = uuid!("00000000-0000-0000-0000-000000000011");

pub fn builtin_music_staff() -> PageTemplate {
    let margin_x = 12.0_f64;
    let top_margin = 16.0_f64;
    let staff_count = 12_u32;
    let staff_line_spacing = 1.8_f64;
    let staff_height = staff_line_spacing * 4.0;
    let avail_h = US_LETTER.1 - top_margin - 16.0;
    let gap =
        (avail_h - staff_height * staff_count as f64) / (staff_count as f64 - 1.0).max(1.0);
    let mut widgets = Vec::with_capacity(staff_count as usize);
    for i in 0..staff_count {
        let y = top_margin + (staff_height + gap) * i as f64;
        widgets.push(TemplateWidget {
            id: Uuid::parse_str(&format!("a0000011-1{:03}-0000-0000-000000000000", i)).unwrap(),
            kind: WidgetKind::LinesRegion { spacing_mm: staff_line_spacing },
            rect: WidgetRect {
                x: margin_x,
                y,
                width: US_LETTER.0 - margin_x * 2.0,
                height: staff_height,
            },
            style: WidgetStyle::default(),
        });
    }
    PageTemplate {
        id: TemplateId(BUILTIN_MUSIC_STAFF_ID),
        name: "Music Staff".into(),
        description: "Twelve blank 5-line staves for music notation.".into(),
        background: BackgroundType::Blank,
        size_mm: US_LETTER,
        tiling: TilingMode::None,
        default_viewport: None,
        widgets,
        category: "Note-taking".into(),
    }
}
