//! Daily Planner — placeholder ruled page for general daily use.

use uuid::{uuid, Uuid};

use journal_core::{BackgroundType, PageTemplate, TemplateId, TilingMode};

use crate::builtin::US_LETTER;

pub const BUILTIN_DAILY_PLANNER_ID: Uuid = uuid!("00000000-0000-0000-0000-000000000005");

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
        category: "Daily Planner".into(),
    }
}
