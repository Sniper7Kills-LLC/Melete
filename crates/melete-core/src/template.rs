use chrono::Weekday;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::pen::Color;
use crate::viewport::Viewport;

/// Unique identifier for a template (page or notebook).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TemplateId(pub Uuid);

/// The background type for a page template.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BackgroundType {
    Blank,
    Dots {
        spacing: f64,
    },
    Lines {
        spacing: f64,
    },
    Grid {
        spacing: f64,
    },
    /// Three sets of parallel lines at 30°, 90°, 150° forming a triangle
    /// lattice. Tiles infinitely; great for technical drawing, board games,
    /// and 3D sketches.
    Isometric {
        spacing: f64,
    },
    /// Pointy-top hexagonal grid. Tiles infinitely; useful for tabletop
    /// games and hex-based note layouts.
    Hexagonal {
        spacing: f64,
    },
    Image {
        path: String,
    },
    Pdf {
        path: String,
        page: u32,
    },
}

/// How the template background tiles.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TilingMode {
    None,
    Repeat,
}

/// Position and size of a widget on the template canvas, in mm.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WidgetRect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// Stroke/fill style for a template widget.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WidgetStyle {
    pub stroke_color: Color,
    #[serde(default)]
    pub fill_color: Option<Color>,
    pub stroke_width_mm: f64,
}

impl Default for WidgetStyle {
    fn default() -> Self {
        Self {
            stroke_color: Color {
                r: 60,
                g: 60,
                b: 80,
                a: 200,
            },
            fill_color: None,
            stroke_width_mm: 0.3,
        }
    }
}

/// What kind of element a template widget represents.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum WidgetKind {
    TextBlock {
        text: String,
        font_size_mm: f64,
    },
    Rectangle,
    Ellipse,
    /// Arc of an ellipse — used by templates that need a partial
    /// circle / sector marker (range arcs on a fighting-position
    /// sector sketch, dial markers, etc). The widget's `rect` defines
    /// the arc's bounding box; `start_deg` and `sweep_deg` measure
    /// from the +X axis with positive sweep going counter-clockwise
    /// (math convention). 0/360 sweeps render as a full ellipse.
    Arc {
        start_deg: f64,
        sweep_deg: f64,
        thickness_mm: f64,
    },
    Line {
        thickness_mm: f64,
    },
    GridRegion {
        spacing_mm: f64,
    },
    LinesRegion {
        spacing_mm: f64,
    },
    DotsRegion {
        spacing_mm: f64,
    },
    CalendarMonth,
    Timeline {
        start_hour: u8,
        end_hour: u8,
        slot_minutes: u32,
    },
    Checklist {
        items: Vec<String>,
    },
    /// Three numbered priority boxes stacked vertically (Full Focus daily layout).
    BigThree,
    /// Columnar priority list with A/B/C priority letter, sequence number, and write-on line.
    PriorityList {
        count: u32,
    },
    /// Two-column hourly schedule with hour labels on the left and blank lines + half-hour ticks on the right.
    DailyAppointments {
        start_hour: u8,
        end_hour: u8,
    },
    /// Grid of labeled role/goal boxes for weekly planning (Franklin Covey Weekly Compass).
    WeeklyCompass,
    /// Habit-tracker grid: rows are habits (one per name) and columns
    /// are day numbers (1..=days). Each cell renders an empty checkbox
    /// for the user to fill in. Smart: when bound to a real date, the
    /// column matching the bound date's day-of-month is highlighted.
    HabitTracker {
        habits: Vec<String>,
        /// Number of day columns. 31 covers any month.
        days: u32,
    },
    /// Horizontal row of N empty circles — quick visual tally for
    /// daily counts (water glasses, reps, pomodoros, etc).
    Tally {
        label: String,
        count: u32,
    },
    /// Concentric range-arc fan from the bottom-center of the widget's
    /// rect (the "weapon position" for a fighting-position sector
    /// sketch). Renders `rings` arcs at evenly spaced radii out to the
    /// rect's height, with a label every ring at
    /// `interval_m * ring_index` meters. `sweep_deg` is how wide the
    /// arcs themselves extend (180 = half-circle, 90 = quarter, etc).
    /// `sector_deg` is the narrower "my sector" V drawn over the arcs
    /// from WP — typical fighting-position sector of fire is 60 deg.
    RangeArcs {
        rings: u32,
        interval_m: u32,
        sweep_deg: f64,
        sector_deg: f64,
    },

    // ---- Fetch-backed widgets -------------------------------------------
    //
    // Each variant below is a "background data" widget — the renderer
    // reads a typed `WidgetPayload` from the page's `widget_data` cache
    // (see `melete_core::widget_data`). The actual fetch is performed
    // by the app-layer fetcher; this enum only declares *what* to fetch
    // and the per-instance config (location, source, etc).
    //
    // None of these require auth: every endpoint is either a free
    // public API with no key (Open-Meteo, bible-api.com, Wikipedia REST,
    // sunrise-sunset.org, public RSS) or computed locally (moon phase).

    /// Open-Meteo forecast for the bound page-date. Caches current
    /// conditions and a few-day strip. `Freshness::UntilDate`.
    Weather {
        lat: f64,
        lon: f64,
        location_label: String,
        days: u32,
    },
    /// Quote of the day — pulled from `source` (zenquotes.io if
    /// `source == "zen"`, quotable.io if `source == "quotable"`, or a
    /// local rotation file if `source == "local"`). `Freshness::Once`.
    Quote {
        source: String,
    },
    /// Bible verse — `bible-api.com`. `reference` is a verse spec
    /// (e.g. `"John 3:16"`) or `"random"` for verse-of-day.
    /// `translation` is a translation slug (kjv, web, asv, …).
    /// `Freshness::Once`.
    BibleVerse {
        reference: String,
        translation: String,
    },
    /// Sunrise / sunset / daylight length for the bound date and
    /// (lat, lon). Uses sunrise-sunset.org. `Freshness::Once`.
    Sunrise {
        lat: f64,
        lon: f64,
    },
    /// Moon phase for the bound date — computed locally, no network.
    /// `Freshness::Once` (since a date's phase never changes).
    MoonPhase,
    /// Wikipedia "On this day" events for the bound date.
    /// `Freshness::Once`. `lang` is a Wikipedia language code (e.g.
    /// `"en"`).
    OnThisDay {
        lang: String,
        max_events: u32,
    },
    /// Wiktionary word-of-day pulled at page open. `Freshness::Once`.
    WordOfDay {
        lang: String,
    },
    /// Top N items from a public RSS / Atom feed. `Freshness::UntilDate`.
    RssHeadline {
        url: String,
        count: u32,
    },
    /// Open-Meteo astronomy: planetary visibility / meteor shower
    /// notes for the date. `Freshness::Once`.
    Astronomy {
        lat: f64,
        lon: f64,
    },
}

/// A widget placed on a template canvas.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TemplateWidget {
    pub id: Uuid,
    pub kind: WidgetKind,
    pub rect: WidgetRect,
    #[serde(default)]
    pub style: WidgetStyle,
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
    #[serde(default)]
    pub widgets: Vec<TemplateWidget>,
    /// Free-form category label for grouping in the template manager UI.
    /// Empty string means "Uncategorized".
    #[serde(default)]
    pub category: String,
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
            widgets: Vec::new(),
            category: String::new(),
        }
    }
}

/// How a planner notebook groups its days under each year section.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind")]
#[derive(Default)]
pub enum PlannerGrouping {
    #[default]
    Month,
    Week,
}

/// Title format strings for the section wrappers a planner generates.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SectionTitleFormats {
    #[serde(default = "default_year_format")]
    pub year: String,
    #[serde(default = "default_month_format")]
    pub month: String,
    #[serde(default = "default_week_format")]
    pub week: String,
}

fn default_year_format() -> String {
    "{year}".into()
}
fn default_month_format() -> String {
    "{month_name} {year}".into()
}
fn default_week_format() -> String {
    "Week {week} {year}".into()
}

impl Default for SectionTitleFormats {
    fn default() -> Self {
        Self {
            year: default_year_format(),
            month: default_month_format(),
            week: default_week_format(),
        }
    }
}

fn default_page_title_format() -> String {
    "{date}".into()
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
    /// Whether days are bucketed under month or week wrapper sections.
    #[serde(default)]
    pub grouping: PlannerGrouping,
    /// Format string for daily page titles. See `melete_templates::title_format`.
    #[serde(default = "default_page_title_format")]
    pub page_title_format: String,
    /// Title formats for the year and month/week wrapper sections.
    #[serde(default)]
    pub section_title_formats: SectionTitleFormats,
    /// Per-entry bridge flags. Keys are `"year_start:N"`, `"before_quarter:N"`,
    /// `"before_month:N"`, `"before_week:N"`, or `"daily:S:N"`.
    /// Planner runtime does not yet act on these — persisted for the editor,
    /// bridge-rendering is a future phase.
    #[serde(default)]
    pub entry_options: std::collections::HashMap<String, EntryFlags>,
}

/// Specifies which page templates to use on which days of the week.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DailySlot {
    /// Which days of the week this slot applies to.
    pub days: Vec<Weekday>,
    /// Templates to insert for each matching day.
    pub templates: Vec<TemplateId>,
}

/// Per-entry bridge flags persisted inside a `NotebookTemplate`.
///
/// Keys in `NotebookTemplate::entry_options` are formatted as:
///   `"year_start:N"` / `"before_quarter:N"` / `"before_month:N"` /
///   `"before_week:N"` / `"daily:S:N"` (S = daily-slot index, N = template
///   index inside that slot).
///
/// The planner runtime does not yet act on these flags — they are persisted
/// and surfaced in the editor; bridge-rendering will be wired in a later phase.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct EntryFlags {
    #[serde(default)]
    pub bridge_previous: bool,
    #[serde(default)]
    pub bridge_next: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entry_flags_serde_round_trip() {
        let flags = EntryFlags {
            bridge_previous: true,
            bridge_next: false,
        };
        let serialized = toml::to_string(&flags).expect("serialize EntryFlags");
        let deserialized: EntryFlags = toml::from_str(&serialized).expect("deserialize EntryFlags");
        assert_eq!(flags, deserialized);
    }

    #[test]
    fn entry_flags_default_is_all_false() {
        let flags = EntryFlags::default();
        assert!(!flags.bridge_previous);
        assert!(!flags.bridge_next);
    }

    #[test]
    fn entry_flags_serde_defaults_on_empty() {
        // An empty TOML table should deserialize to all-false defaults.
        let flags: EntryFlags = toml::from_str("").expect("deserialize empty EntryFlags");
        assert_eq!(flags, EntryFlags::default());
    }
}
