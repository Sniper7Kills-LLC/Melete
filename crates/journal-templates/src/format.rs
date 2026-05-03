use serde::{Deserialize, Serialize};
use uuid::Uuid;

use journal_core::{
    BackgroundType, PageTemplate, Point, TemplateId, TilingMode, Viewport,
};

use crate::error::TemplateError;

pub const CURRENT_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemplateFile {
    pub schema_version: u32,
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    /// (width_mm, height_mm). Defaults to US Letter when omitted.
    #[serde(default = "default_size_mm")]
    pub size_mm: [f64; 2],
    #[serde(default)]
    pub tiling: TilingModeFile,
    pub background: BackgroundFile,
    #[serde(default)]
    pub default_viewport: Option<ViewportFile>,
}

fn default_size_mm() -> [f64; 2] {
    [215.9, 279.4]
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TilingModeFile {
    #[default]
    None,
    Repeat,
}

impl From<TilingModeFile> for TilingMode {
    fn from(value: TilingModeFile) -> Self {
        match value {
            TilingModeFile::None => TilingMode::None,
            TilingModeFile::Repeat => TilingMode::Repeat,
        }
    }
}

impl From<TilingMode> for TilingModeFile {
    fn from(value: TilingMode) -> Self {
        match value {
            TilingMode::None => TilingModeFile::None,
            TilingMode::Repeat => TilingModeFile::Repeat,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum BackgroundFile {
    Blank,
    Dots { spacing: f64 },
    Lines { spacing: f64 },
    Grid { spacing: f64 },
    Image { path: String },
    Pdf { path: String, page: u32 },
}

impl From<BackgroundFile> for BackgroundType {
    fn from(value: BackgroundFile) -> Self {
        match value {
            BackgroundFile::Blank => BackgroundType::Blank,
            BackgroundFile::Dots { spacing } => BackgroundType::Dots { spacing },
            BackgroundFile::Lines { spacing } => BackgroundType::Lines { spacing },
            BackgroundFile::Grid { spacing } => BackgroundType::Grid { spacing },
            BackgroundFile::Image { path } => BackgroundType::Image { path },
            BackgroundFile::Pdf { path, page } => BackgroundType::Pdf { path, page },
        }
    }
}

impl From<&BackgroundType> for BackgroundFile {
    fn from(value: &BackgroundType) -> Self {
        match value {
            BackgroundType::Blank => BackgroundFile::Blank,
            BackgroundType::Dots { spacing } => BackgroundFile::Dots { spacing: *spacing },
            BackgroundType::Lines { spacing } => BackgroundFile::Lines { spacing: *spacing },
            BackgroundType::Grid { spacing } => BackgroundFile::Grid { spacing: *spacing },
            BackgroundType::Image { path } => BackgroundFile::Image { path: path.clone() },
            BackgroundType::Pdf { path, page } => BackgroundFile::Pdf {
                path: path.clone(),
                page: *page,
            },
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ViewportFile {
    pub center_x: f64,
    pub center_y: f64,
    pub zoom: f64,
    #[serde(default)]
    pub rotation: f64,
}

impl From<ViewportFile> for Viewport {
    fn from(value: ViewportFile) -> Self {
        Viewport {
            center: Point {
                x: value.center_x,
                y: value.center_y,
            },
            zoom: value.zoom,
            rotation: value.rotation,
        }
    }
}

impl From<Viewport> for ViewportFile {
    fn from(value: Viewport) -> Self {
        ViewportFile {
            center_x: value.center.x,
            center_y: value.center.y,
            zoom: value.zoom,
            rotation: value.rotation,
        }
    }
}

pub fn parse_template_toml(s: &str) -> Result<TemplateFile, TemplateError> {
    let file: TemplateFile = toml::from_str(s)?;
    if file.schema_version != CURRENT_SCHEMA_VERSION {
        return Err(TemplateError::SchemaVersion(file.schema_version));
    }
    Uuid::parse_str(&file.id).map_err(|_| TemplateError::InvalidUuid(file.id.clone()))?;
    Ok(file)
}

pub fn template_file_to_page_template(f: TemplateFile) -> PageTemplate {
    let id = Uuid::parse_str(&f.id).unwrap_or_else(|_| Uuid::nil());
    PageTemplate {
        id: TemplateId(id),
        name: f.name,
        description: f.description,
        background: f.background.into(),
        size_mm: (f.size_mm[0], f.size_mm[1]),
        tiling: f.tiling.into(),
        default_viewport: f.default_viewport.map(Into::into),
    }
}

pub fn template_file_from_page_template(t: &PageTemplate) -> TemplateFile {
    TemplateFile {
        schema_version: CURRENT_SCHEMA_VERSION,
        id: t.id.0.to_string(),
        name: t.name.clone(),
        description: t.description.clone(),
        size_mm: [t.size_mm.0, t.size_mm.1],
        tiling: t.tiling.into(),
        background: (&t.background).into(),
        default_viewport: t.default_viewport.map(Into::into),
    }
}

pub fn serialize_template_toml(f: &TemplateFile) -> Result<String, TemplateError> {
    Ok(toml::to_string_pretty(f)?)
}
