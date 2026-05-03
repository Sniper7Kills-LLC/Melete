use uuid::{uuid, Uuid};

use journal_core::{BackgroundType, PageTemplate, TemplateId, TilingMode};

pub const BUILTIN_BLANK_ID: Uuid = uuid!("00000000-0000-0000-0000-000000000001");
pub const BUILTIN_DOTTED_ID: Uuid = uuid!("00000000-0000-0000-0000-000000000002");
pub const BUILTIN_RULED_ID: Uuid = uuid!("00000000-0000-0000-0000-000000000003");
pub const BUILTIN_GRID_ID: Uuid = uuid!("00000000-0000-0000-0000-000000000004");
pub const BUILTIN_DAILY_PLANNER_ID: Uuid = uuid!("00000000-0000-0000-0000-000000000005");

const US_LETTER: (f64, f64) = (215.9, 279.4);

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
    }
}

pub fn builtin_dotted() -> PageTemplate {
    PageTemplate {
        id: TemplateId(BUILTIN_DOTTED_ID),
        name: "Dotted Grid".into(),
        description: "Light dotted grid for general note-taking (5mm spacing).".into(),
        background: BackgroundType::Dots { spacing: 5.0 },
        size_mm: US_LETTER,
        tiling: TilingMode::None,
        default_viewport: None,
        widgets: Vec::new(),
    }
}

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
    }
}

pub fn builtin_grid() -> PageTemplate {
    PageTemplate {
        id: TemplateId(BUILTIN_GRID_ID),
        name: "Grid".into(),
        description: "Tiling grid that repeats infinitely (5mm base spacing).".into(),
        background: BackgroundType::Grid { spacing: 5.0 },
        size_mm: US_LETTER,
        tiling: TilingMode::Repeat,
        default_viewport: None,
        widgets: Vec::new(),
    }
}

pub fn builtin_daily_planner() -> PageTemplate {
    PageTemplate {
        id: TemplateId(BUILTIN_DAILY_PLANNER_ID),
        name: "Daily Planner".into(),
        description: "Daily planner placeholder — ruled lines at 8mm spacing.".into(),
        background: BackgroundType::Lines { spacing: 8.0 },
        size_mm: US_LETTER,
        tiling: TilingMode::None,
        default_viewport: None,
        widgets: Vec::new(),
    }
}

pub fn builtin_templates() -> Vec<PageTemplate> {
    vec![
        builtin_blank(),
        builtin_dotted(),
        builtin_ruled(),
        builtin_grid(),
        builtin_daily_planner(),
    ]
}
