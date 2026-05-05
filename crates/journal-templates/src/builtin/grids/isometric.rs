//! Isometric grid paper  -  equilateral triangle lattice, 15mm side length.

use uuid::{uuid, Uuid};

use journal_core::{BackgroundType, PageTemplate, TemplateId, TilingMode};

use crate::builtin::US_LETTER;

pub const BUILTIN_ISOMETRIC_ID: Uuid = uuid!("00000000-0000-0000-0000-00000000000e");

pub fn builtin_isometric() -> PageTemplate {
    PageTemplate {
        id: TemplateId(BUILTIN_ISOMETRIC_ID),
        name: "Isometric Grid".into(),
        description: "Equilateral triangle isometric grid (15mm side length). Tiles infinitely."
            .into(),
        background: BackgroundType::Isometric { spacing: 15.0 },
        size_mm: US_LETTER,
        tiling: TilingMode::Repeat,
        default_viewport: None,
        widgets: Vec::new(),
        category: "Grids".into(),
    }
}
