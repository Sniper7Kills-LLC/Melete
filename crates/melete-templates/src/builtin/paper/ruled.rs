//! Ruled-line page  -  7mm lines + red margin + header strip
//! + 3-hole-punch indicator dots.

use uuid::{uuid, Uuid};

use melete_core::{BackgroundType, PageTemplate, TemplateId, TilingMode};

use crate::builtin::{lined_paper_widgets, US_LETTER};

pub const BUILTIN_RULED_ID: Uuid = uuid!("00000000-0000-0000-0000-000000000003");

pub fn builtin_ruled() -> PageTemplate {
    PageTemplate {
        id: TemplateId(BUILTIN_RULED_ID),
        name: "Ruled Lines".into(),
        description:
            "Ruled lines (7mm spacing) with red margin, header strip, and 3-hole-punch indicator \
             dots."
                .into(),
        background: BackgroundType::Lines { spacing: 7.0 },
        size_mm: US_LETTER,
        tiling: TilingMode::None,
        default_viewport: None,
        widgets: lined_paper_widgets(0x03),
        category: "Paper".into(),
    }
}
