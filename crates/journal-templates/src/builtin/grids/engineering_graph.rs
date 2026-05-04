//! Engineering graph paper — 5.08mm grid (5 squares per inch).

use uuid::{uuid, Uuid};

use journal_core::{BackgroundType, PageTemplate, TemplateId, TilingMode};

use crate::builtin::US_LETTER;

pub const BUILTIN_ENGINEERING_GRAPH_ID: Uuid = uuid!("00000000-0000-0000-0000-000000000010");

pub fn builtin_engineering_graph() -> PageTemplate {
    PageTemplate {
        id: TemplateId(BUILTIN_ENGINEERING_GRAPH_ID),
        name: "Engineering Graph".into(),
        description: "Engineering graph paper at 5.08mm (5 squares per inch). Tiles infinitely."
            .into(),
        background: BackgroundType::Grid { spacing: 5.08 },
        size_mm: US_LETTER,
        tiling: TilingMode::Repeat,
        default_viewport: None,
        widgets: Vec::new(),
        category: "Grids".into(),
    }
}
