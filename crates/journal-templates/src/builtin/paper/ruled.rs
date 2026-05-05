//! Ruled-line page  -  horizontal lines at 7mm spacing.

use uuid::{uuid, Uuid};

use journal_core::{BackgroundType, PageTemplate, TemplateId, TilingMode};

use crate::builtin::US_LETTER;

pub const BUILTIN_RULED_ID: Uuid = uuid!("00000000-0000-0000-0000-000000000003");

pub fn builtin_ruled() -> PageTemplate {
    PageTemplate {
        id: TemplateId(BUILTIN_RULED_ID),
        name: "Ruled Lines".into(),
        description: "Ruled lines for prose (7mm spacing).".into(),
        background: BackgroundType::Lines { spacing: 7.0 },
        size_mm: US_LETTER,
        tiling: TilingMode::None,
        default_viewport: None,
        widgets: Vec::new(),
        category: "Paper".into(),
    }
}
