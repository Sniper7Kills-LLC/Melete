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
        // TODO: image/PDF rendering deferred — fall back to blank for now.
        BackgroundType::Image { .. } | BackgroundType::Pdf { .. } => BackgroundConfig::Blank,
    }
}
