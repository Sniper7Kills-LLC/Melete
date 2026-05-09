use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::calendar::PlannerPageAddress;
use crate::notebook::SectionId;
use crate::template::TemplateId;
use crate::widget_data::WidgetData;

/// Unique identifier for a page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PageId(pub Uuid);

/// Per-page override for a single template widget. Each variant pins a
/// configurable knob of the widget kind it names; rendered values fall
/// through to the widget's own template defaults when no override is set.
///
/// Saved on the page (not the template) so users can drop the same
/// "Monthly Planner" template onto two pages and have one show October
/// and the other November.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum WidgetOverride {
    CalendarMonth {
        year: i32,
        month: u32,
    },
    Timeline {
        start_hour: u8,
        end_hour: u8,
        slot_minutes: u32,
    },
    DailyAppointments {
        start_hour: u8,
        end_hour: u8,
    },
    PriorityList {
        count: u32,
    },
    Checklist {
        items: Vec<String>,
    },
    TextBlock {
        text: String,
        font_size_mm: f64,
    },
    LinesRegion {
        spacing_mm: f64,
    },
    GridRegion {
        spacing_mm: f64,
    },
    DotsRegion {
        spacing_mm: f64,
    },
    Line {
        thickness_mm: f64,
    },
    HabitTracker {
        habits: Vec<String>,
        days: u32,
    },
    Tally {
        label: String,
        count: u32,
    },
    RangeArcs {
        rings: u32,
        interval_m: u32,
        sweep_deg: f64,
        sector_deg: f64,
    },
}

/// A single page within a notebook section.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Page {
    pub id: PageId,
    pub template_id: Option<TemplateId>,
    pub section_id: SectionId,
    pub position: u32,
    #[serde(default)]
    pub name: String,
    pub planner_address: Option<PlannerPageAddress>,
    pub created_at: DateTime<Utc>,
    pub modified_at: DateTime<Utc>,
    /// Per-widget overrides keyed by the `TemplateWidget.id` they target.
    /// `#[serde(default)]` keeps old TOML/JSON / DB rows readable.
    #[serde(default)]
    pub widget_overrides: HashMap<Uuid, WidgetOverride>,
    /// Cached fetch payload for fetch-backed widgets, keyed by
    /// `TemplateWidget.id`. Populated by the app-layer fetcher; the
    /// renderer only reads from here. `#[serde(default)]` keeps pages
    /// from before this field was added decoding cleanly.
    #[serde(default)]
    pub widget_data: HashMap<Uuid, WidgetData>,
    /// User-toggled bookmark flag. Surfaces this page in the notebook
    /// sidebar's "Bookmarks" panel. `#[serde(default)]` keeps old
    /// TOML/JSON / DB rows readable.
    #[serde(default)]
    pub flagged: bool,
    /// Sort position within the notebook's Bookmarks panel. Only meaningful
    /// when `flagged` is true; otherwise zero. Newly bookmarked pages get
    /// `max + 1`; drag-reorder rewrites positions densely.
    #[serde(default)]
    pub bookmark_position: u32,
}
