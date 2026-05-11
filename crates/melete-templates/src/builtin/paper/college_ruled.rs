//! College-ruled notebook paper  -  7.1mm lines + red margin + header strip
//! + 3-hole-punch indicator dots.

use uuid::{uuid, Uuid};

use melete_core::{BackgroundType, PageTemplate, TemplateId, TilingMode};

use crate::builtin::{lined_paper_widgets, US_LETTER};

pub const BUILTIN_COLLEGE_RULED_ID: Uuid = uuid!("00000000-0000-0000-0000-00000000000c");

pub fn builtin_college_ruled() -> PageTemplate {
    PageTemplate {
        id: TemplateId(BUILTIN_COLLEGE_RULED_ID),
        name: "College Ruled Paper".into(),
        description:
            "College-ruled notebook paper (7.1mm lines) with red margin, header strip, and \
             3-hole-punch indicator dots."
                .into(),
        background: BackgroundType::Lines { spacing: 7.1 },
        size_mm: US_LETTER,
        tiling: TilingMode::None,
        default_viewport: None,
        widgets: lined_paper_widgets(0x0c),
        category: "Paper".into(),
    }
}
