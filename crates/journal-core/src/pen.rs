use serde::{Deserialize, Serialize};

/// RGBA color with 8-bit components.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

/// How a stroke blends with content beneath it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BlendMode {
    Normal,
    Multiply,
    Screen,
    Overlay,
    Darken,
    Lighten,
    Erase,
}

/// Brush family used for rendering. Each style has its own draw routine
/// in `journal_canvas::stroke_renderer` — pen draws solid lines with
/// pressure-tapered width, pencil hard-edges with light texture,
/// highlighter is a wide multiply pass, paintbrush layers soft dabs at
/// reduced opacity to allow color-mixing, spray can scatters dots around
/// the path, calligraphy modulates width by stroke direction (nib
/// simulation).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum BrushStyle {
    #[default]
    Pen,
    Pencil,
    Highlighter,
    Paintbrush,
    SprayCan,
    Calligraphy,
}

/// Settings for a pen stroke.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PenSettings {
    pub color: Color,
    pub base_width: f64,
    pub opacity: f32,
    pub blend_mode: BlendMode,
    /// Brush family. Defaults to `Pen` for back-compat with older
    /// `pen_json` rows persisted before this field existed.
    #[serde(default)]
    pub brush_style: BrushStyle,
}
