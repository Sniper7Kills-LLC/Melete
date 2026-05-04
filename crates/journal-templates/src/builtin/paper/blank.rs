//! Blank page — no background pattern, no widgets.

use uuid::{uuid, Uuid};

use journal_core::{BackgroundType, PageTemplate, TemplateId, TilingMode};

use crate::builtin::US_LETTER;

pub const BUILTIN_BLANK_ID: Uuid = uuid!("00000000-0000-0000-0000-000000000001");

pub fn builtin_blank() -> PageTemplate {
    PageTemplate {
        id: TemplateId(BUILTIN_BLANK_ID),
        name: "Blank".into(),
        description: "Plain page with no background pattern.".into(),
        background: BackgroundType::Blank,
        size_mm: US_LETTER,
        tiling: TilingMode::None,
        default_viewport: None,
        widgets: Vec::new(),
        category: "Paper".into(),
    }
}
