pub mod stroke;
pub mod viewport;
pub mod pen;
pub mod page;
pub mod notebook;
pub mod template;
pub mod calendar;
pub mod title_format;

pub use stroke::{Stroke, StrokePoint};
pub use viewport::Viewport;
pub use pen::{PenSettings, Color, BlendMode, BrushStyle};
pub use page::{Page, PageId, WidgetOverride};
pub use notebook::{Notebook, NotebookId, NotebookKind, Section, SectionId};
pub use template::{
    PageTemplate, TemplateId, BackgroundType, TilingMode,
    NotebookTemplate, DailySlot, PlannerGrouping, SectionTitleFormats,
    TemplateWidget, WidgetKind, WidgetRect, WidgetStyle,
    EntryFlags,
};
pub use calendar::{CalendarPageAddress, PlannerPageAddress};
pub use title_format::{render as render_title, TitleContext};

/// A simple 2D point.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

/// An axis-aligned rectangle.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}
