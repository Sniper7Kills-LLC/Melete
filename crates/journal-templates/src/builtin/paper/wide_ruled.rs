//! Wide-ruled notebook paper  -  8.7mm lines + red margin + header strip
//! + 3-hole-punch indicator dots.

use uuid::{uuid, Uuid};

use journal_core::{BackgroundType, PageTemplate, TemplateId, TilingMode};

use crate::builtin::{lined_paper_widgets, US_LETTER};

pub const BUILTIN_WIDE_RULED_ID: Uuid = uuid!("00000000-0000-0000-0000-00000000000b");

pub fn builtin_wide_ruled() -> PageTemplate {
    PageTemplate {
        id: TemplateId(BUILTIN_WIDE_RULED_ID),
        name: "Wide Ruled Paper".into(),
        description: "Wide-ruled notebook paper (8.7mm lines) with red margin, header strip, and \
             3-hole-punch indicator dots."
            .into(),
        background: BackgroundType::Lines { spacing: 8.7 },
        size_mm: US_LETTER,
        tiling: TilingMode::None,
        default_viewport: None,
        widgets: lined_paper_widgets(0x0b),
        category: "Paper".into(),
    }
}
