use std::path::PathBuf;

use journal_canvas::{BackgroundConfig, GridSettings};
use journal_core::{BackgroundType, Color, PageTemplate};

pub fn page_template_to_background_config(t: &PageTemplate) -> BackgroundConfig {
    match &t.background {
        BackgroundType::Blank => BackgroundConfig::Blank,
        BackgroundType::Dots { spacing } => BackgroundConfig::Dots { spacing: *spacing },
        BackgroundType::Lines { spacing } => BackgroundConfig::Lines { spacing: *spacing },
        BackgroundType::Grid { spacing } => BackgroundConfig::Grid(GridSettings {
            base_spacing: *spacing,
            subdivisions: 4,
            color: Color { r: 80, g: 80, b: 90, a: 255 },
        }),
        BackgroundType::Image { path } => BackgroundConfig::Image {
            path: PathBuf::from(path),
            size_canvas: (t.size_mm.0, t.size_mm.1),
        },
        BackgroundType::Pdf { path, page } => BackgroundConfig::Pdf {
            path: PathBuf::from(path),
            page: *page,
            size_canvas: (t.size_mm.0, t.size_mm.1),
        },
    }
}
