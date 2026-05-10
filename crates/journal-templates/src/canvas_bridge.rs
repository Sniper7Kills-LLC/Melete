use journal_canvas::{BackgroundConfig, GridSettings};
use journal_core::{BackgroundType, Color, PageTemplate, TilingMode};

/// Strip the `asset:` prefix when present, so renderers always see a
/// bare asset name (`foo.png`) regardless of whether the template was
/// migrated from the legacy fs layout or freshly authored.
pub fn asset_name(raw: &str) -> &str {
    raw.strip_prefix("asset:").unwrap_or(raw)
}

pub fn page_template_to_background_config(t: &PageTemplate) -> BackgroundConfig {
    let tiling = matches!(t.tiling, TilingMode::Repeat);
    match &t.background {
        BackgroundType::Blank => BackgroundConfig::Blank,
        BackgroundType::Dots { spacing } => BackgroundConfig::Dots {
            spacing: *spacing,
            tiling,
        },
        BackgroundType::Lines { spacing } => BackgroundConfig::Lines {
            spacing: *spacing,
            tiling,
        },
        BackgroundType::Grid { spacing } => BackgroundConfig::Grid(GridSettings {
            base_spacing: *spacing,
            subdivisions: 4,
            color: Color {
                r: 80,
                g: 80,
                b: 90,
                a: 255,
            },
        }),
        BackgroundType::Isometric { spacing } => BackgroundConfig::Isometric { spacing: *spacing },
        BackgroundType::Hexagonal { spacing } => BackgroundConfig::Hexagonal { spacing: *spacing },
        BackgroundType::Image { path } => BackgroundConfig::Image {
            asset: asset_name(path).to_string(),
            size_canvas: (t.size_mm.0, t.size_mm.1),
        },
        BackgroundType::Pdf { path, page } => BackgroundConfig::Pdf {
            asset: asset_name(path).to_string(),
            page: *page,
            size_canvas: (t.size_mm.0, t.size_mm.1),
        },
    }
}
