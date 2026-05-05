//! Hex grid paper  -  pointy-top hexes, 18mm centre spacing.

use uuid::{uuid, Uuid};

use journal_core::{BackgroundType, PageTemplate, TemplateId, TilingMode};

use crate::builtin::US_LETTER;

pub const BUILTIN_HEX_ID: Uuid = uuid!("00000000-0000-0000-0000-00000000000f");

pub fn builtin_hexagonal() -> PageTemplate {
    PageTemplate {
        id: TemplateId(BUILTIN_HEX_ID),
        name: "Hex Grid".into(),
        description: "Pointy-top hexagonal grid (18mm centre spacing). Tiles infinitely.".into(),
        background: BackgroundType::Hexagonal { spacing: 18.0 },
        size_mm: US_LETTER,
        tiling: TilingMode::Repeat,
        default_viewport: None,
        widgets: Vec::new(),
        category: "Grids".into(),
    }
}
