//! Square grid page — 10mm tiling grid.

use uuid::{uuid, Uuid};

use journal_core::{BackgroundType, PageTemplate, TemplateId, TilingMode};

use crate::builtin::US_LETTER;

pub const BUILTIN_GRID_ID: Uuid = uuid!("00000000-0000-0000-0000-000000000004");

pub fn builtin_grid() -> PageTemplate {
    PageTemplate {
        id: TemplateId(BUILTIN_GRID_ID),
        name: "Grid".into(),
        description: "Tiling grid that repeats infinitely (5mm base spacing).".into(),
        background: BackgroundType::Grid { spacing: 10.0 },
        size_mm: US_LETTER,
        tiling: TilingMode::Repeat,
        default_viewport: None,
        widgets: Vec::new(),
        category: "Grids".into(),
    }
}
