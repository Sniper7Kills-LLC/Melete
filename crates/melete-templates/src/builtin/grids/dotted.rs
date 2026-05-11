//! Dotted-grid page  -  light dot pattern at 8mm spacing.

use uuid::{uuid, Uuid};

use melete_core::{BackgroundType, PageTemplate, TemplateId, TilingMode};

use crate::builtin::US_LETTER;

pub const BUILTIN_DOTTED_ID: Uuid = uuid!("00000000-0000-0000-0000-000000000002");

pub fn builtin_dotted() -> PageTemplate {
    PageTemplate {
        id: TemplateId(BUILTIN_DOTTED_ID),
        name: "Dotted Grid".into(),
        description: "Light dotted grid for general note-taking (5mm spacing).".into(),
        background: BackgroundType::Dots { spacing: 8.0 },
        size_mm: US_LETTER,
        tiling: TilingMode::Repeat,
        default_viewport: None,
        widgets: Vec::new(),
        category: "Grids".into(),
    }
}
