pub mod brush;
pub mod calendar;
pub mod notebook;
pub mod page;
pub mod pen;
pub mod stroke;
pub mod template;
pub mod title_format;
pub mod viewport;

pub use brush::{Brush, BrushLayer, ColorMod, CursorShape, Geometry, TipShape, WidthMode};
pub use calendar::{CalendarPageAddress, PlannerPageAddress};
pub use notebook::{Notebook, NotebookId, NotebookKind, Section, SectionId};
pub use page::{Page, PageId, WidgetOverride};
pub use pen::{BlendMode, Color, PenSettings, ToolStyle};
pub use stroke::{Stroke, StrokePoint};
pub use template::{
    BackgroundType, DailySlot, EntryFlags, NotebookTemplate, PageTemplate, PlannerGrouping,
    SectionTitleFormats, TemplateId, TemplateWidget, TilingMode, WidgetKind, WidgetRect,
    WidgetStyle,
};
pub use title_format::{render as render_title, TitleContext};
pub use viewport::Viewport;

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
