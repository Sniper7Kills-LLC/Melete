//! Composable brush model.
//!
//! Replaces the hardcoded `BrushStyle` enum + per-style render-fn
//! dispatch with an ordered list of *layers*. Each layer composes
//! a `Geometry` (how the path is emitted), a `WidthMode` (how each
//! emitted point is widened), a `TipShape` (what is stamped at each
//! point), a `ColorMod` (per-layer alpha / hue tweaks), and a
//! `BlendMode`.
//!
//! Built-in tools become named `Brush` compositions (see
//! `built_in.rs`); the Tool Editor lets users fork them. The renderer
//! lowers a `Brush` to GPU calls in
//! `vello_renderer::draw_brush_into_scene`.
//!
//! See `docs/brush-engine.md` for the full plan.

use journal_core::BlendMode;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Brush {
    pub id: Uuid,
    pub name: String,
    /// Ordered render passes — drawn first → drawn last (later layers
    /// land on top). Most built-in tools are one layer; Paintbrush is
    /// three (outer halo, mid, core); Pencil is two (sharp core,
    /// tilt-driven shading).
    pub layers: Vec<BrushLayer>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct BrushLayer {
    #[serde(default = "default_true")]
    pub enabled: bool,
    pub geometry: Geometry,
    pub width: WidthMode,
    pub tip: TipShape,
    #[serde(default)]
    pub color: ColorMod,
    #[serde(default = "default_blend_normal")]
    pub blend: BlendMode,
}

fn default_true() -> bool {
    true
}
fn default_blend_normal() -> BlendMode {
    BlendMode::Normal
}

/// How the layer's path is emitted. Width and tip shape are applied
/// on top — `Geometry` only decides where stamps land.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Geometry {
    /// One smooth quadratic-through-midpoints stroke. Tip is
    /// rendered via the GPU stroke style (one logical stamp per
    /// pixel along the path).
    Smooth { resample_step_mm: f64 },
    /// Variable-width filled polygon (offset left + right of the
    /// path). The polygon outline IS the tip shape — `tip` is
    /// informational for the editor preview only.
    Outline {
        resample_step_mm: f64,
        smooth_outline: bool,
    },
    /// Scatter cloud — N tip stamps at randomized offsets per input
    /// point.
    Scatter {
        density: u32,
        spread_mm: f64,
        falloff: f64,
        directional_bias_deg: Option<f64>,
    },
    /// Stamps the tip at fixed intervals along the path.
    DabStamp { step_mult: f64 },
}

/// How each emitted stamp is widened.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WidthMode {
    Constant {
        width_mult: f64,
    },
    /// Constant width clamped between `min_mm` and `max_mm` after
    /// the base width is computed. Pencil uses this for its sharp
    /// core layer (cap the line at "pencil-sharp" thickness).
    ClampedConstant {
        width_mult: f64,
        min_mm: f64,
        max_mm: f64,
    },
    Pressure {
        floor: f64,
        amp: f64,
    },
    DirectionAngled {
        nib_deg: f64,
        min_ratio: f64,
    },
    /// Per-segment tilt-band overlay. Emits *additional* paint only
    /// where stylus tilt exceeds `threshold`. Designed to layer on
    /// top of a constant-width core (Pencil-cylindrical pattern).
    TiltBand {
        threshold: f64,
        band_mult: f64,
        alpha_scale: f64,
    },
}

/// What is stamped at each emitted position.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TipShape {
    Round,
    Square,
    FlatNib { angle_deg: f64, aspect: f64 },
    Diamond,
    StarN { points: u8, inner_ratio: f64 },
}

/// Per-layer multiplier on the stroke's pen color. Lets multi-pass
/// brushes (paintbrush halo) emit the same color at different alphas.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
pub struct ColorMod {
    #[serde(default = "default_one")]
    pub alpha_mult: f64,
    #[serde(default)]
    pub hue_shift_deg: f64,
}
fn default_one() -> f64 {
    1.0
}
impl Default for ColorMod {
    fn default() -> Self {
        Self {
            alpha_mult: 1.0,
            hue_shift_deg: 0.0,
        }
    }
}

impl Brush {
    /// One-layer convenience constructor for the common case.
    pub fn one_layer(
        id: Uuid,
        name: impl Into<String>,
        geometry: Geometry,
        width: WidthMode,
        tip: TipShape,
    ) -> Self {
        Self {
            id,
            name: name.into(),
            layers: vec![BrushLayer {
                enabled: true,
                geometry,
                width,
                tip,
                color: ColorMod::default(),
                blend: BlendMode::Normal,
            }],
        }
    }
}
