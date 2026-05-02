use chrono::Weekday;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::viewport::Viewport;

/// Unique identifier for a template (page or notebook).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TemplateId(pub Uuid);

/// The background type for a page template.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BackgroundType {
    Blank,
    Dots { spacing: f64 },
    Lines { spacing: f64 },
    Grid { spacing: f64 },
    Image { path: String },
    Pdf { path: String, page: u32 },
}

/// How the template background tiles.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TilingMode {
    None,
    Repeat,
}

/// A template that defines the layout and background of a page.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PageTemplate {
    pub id: TemplateId,
    pub name: String,
    pub description: String,
    pub background: BackgroundType,
    /// Page size in millimeters (width, height). Default is US Letter: 215.9 x 279.4 mm.
    pub size_mm: (f64, f64),
    pub tiling: TilingMode,
    pub default_viewport: Option<Viewport>,
}

impl Default for PageTemplate {
    fn default() -> Self {
        Self {
            id: TemplateId(Uuid::new_v4()),
            name: String::new(),
            description: String::new(),
            background: BackgroundType::Blank,
            size_mm: (215.9, 279.4),
            tiling: TilingMode::None,
            default_viewport: None,
        }
    }
}

/// A notebook template describing the structure of a planner.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NotebookTemplate {
    pub id: TemplateId,
    pub name: String,
    pub description: String,
    /// Templates inserted at the start of the year.
    pub year_start: Vec<TemplateId>,
    /// Templates inserted before each quarter.
    pub before_quarter: Vec<TemplateId>,
    /// Templates inserted before each month.
    pub before_month: Vec<TemplateId>,
    /// Templates inserted before each week.
    pub before_week: Vec<TemplateId>,
    /// Daily page slots with day-of-week selectors.
    pub daily_slots: Vec<DailySlot>,
}

/// Specifies which page templates to use on which days of the week.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DailySlot {
    /// Which days of the week this slot applies to.
    pub days: Vec<Weekday>,
    /// Templates to insert for each matching day.
    pub templates: Vec<TemplateId>,
}
