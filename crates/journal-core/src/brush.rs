//! Composable brush model.
//!
//! Replaces the hardcoded `ToolStyle` enum + per-style render-fn
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

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::pen::BlendMode;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Brush {
    pub id: Uuid,
    pub name: String,
    /// Ordered render passes — drawn first → drawn last (later layers
    /// land on top). Most built-in tools are one layer; Paintbrush is
    /// three (outer halo, mid, core); Pencil is two (sharp core,
    /// tilt-driven shading).
    pub layers: Vec<BrushLayer>,
    /// Hover-cursor shape rendered on the canvas while this brush is
    /// active. `Auto` derives the cursor from the first layer's tip
    /// + width. Other variants override.
    #[serde(default)]
    pub cursor: CursorShape,
    /// Optional default ink color in RGBA8. When `Some`, applying
    /// this brush via the Tool Editor's "Use this brush" also sets
    /// the active pen color. `None` keeps the user's current toolbar
    /// color (default — most users picked the toolbar color first).
    #[serde(default)]
    pub default_color: Option<[u8; 4]>,
}

/// Hover-cursor shape for the brush. Drives the canvas overlay that
/// shows where the next stroke will land.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
#[derive(Default)]
pub enum CursorShape {
    /// Derive from the brush's first layer (tip shape + computed
    /// width at the current pen base width). Default.
    #[default]
    Auto,
    /// Fixed circle outline.
    Circle,
    /// Ellipse with a width:height aspect ratio.
    Oval { aspect: f64 },
    /// Mirror the first layer's `TipShape` exactly. Useful for
    /// calligraphy nibs where the cursor itself should hint at the
    /// nib angle.
    ExactTip,
    /// User-designed polygon (same unit-space convention as
    /// `TipShape::Custom`). Lets users draw a recognisable cursor
    /// (e.g. crosshair, brush silhouette) independent of what the
    /// brush actually paints.
    Custom { points: Vec<(f64, f64)> },
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct BrushLayer {
    #[serde(default = "default_true")]
    pub enabled: bool,
    pub geometry: Geometry,
    pub width: WidthMode,
    pub tip: TipShape,
    /// Multiplier on the computed tip stamp size, applied AFTER the
    /// width formula. Lets users decouple stamp size from stroke
    /// width — e.g. a thin pen line that paints big stars
    /// (`width.Constant{0.3}` + `tip_scale = 8.0`). 1.0 = no
    /// extra scaling. Affects every code path that stamps a
    /// `tip_polygon` (Smooth-stamp fallback, DabStamp, Scatter,
    /// single-point Smooth) but not GPU stroking (Round + Square
    /// on Smooth) — those follow `width` directly so the trace
    /// stays continuous.
    #[serde(default = "default_tip_scale")]
    pub tip_scale: f64,
    #[serde(default)]
    pub color: ColorMod,
    #[serde(default = "default_blend_normal")]
    pub blend: BlendMode,
}

fn default_true() -> bool {
    true
}
fn default_tip_scale() -> f64 {
    1.0
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
    /// Fan-bristle: emit `count` parallel offset Smooth strokes
    /// spread `spread_mult * width` perpendicular to the stroke
    /// direction. Each tine renders thin to read as bristle hair.
    /// Reproduces `PaintbrushShape::Fan` natively.
    FanOffset { count: u32, spread_mult: f64 },
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
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TipShape {
    Round,
    Square,
    FlatNib {
        angle_deg: f64,
        aspect: f64,
    },
    Diamond,
    StarN {
        points: u8,
        inner_ratio: f64,
    },
    /// User-designed polygon. Points are in unit space (-1..1 on
    /// each axis around the stamp centre); the renderer scales them
    /// by the layer's width. Minimum 3 points, edges connect in
    /// order, the polygon auto-closes.
    Custom {
        points: Vec<(f64, f64)>,
    },
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
                tip_scale: 1.0,
                color: ColorMod::default(),
                blend: BlendMode::Normal,
            }],
            cursor: CursorShape::default(),
            default_color: None,
        }
    }
}

/// Built-in nib-shape presets. Each preset is a `TipShape` with
/// pre-tuned parameters; the editor surfaces them in a dropdown so
/// the user can pick a calligraphy nib (Italic, Chisel, Brush, …)
/// without typing angle/aspect numbers by hand. Custom polygons
/// live alongside as `TipShape::Custom`.
pub fn nib_presets() -> Vec<(&'static str, TipShape)> {
    vec![
        ("Round point", TipShape::Round),
        ("Square block", TipShape::Square),
        ("Diamond", TipShape::Diamond),
        (
            "Italic 45°",
            TipShape::FlatNib {
                angle_deg: 45.0,
                aspect: 0.25,
            },
        ),
        (
            "Italic 30°",
            TipShape::FlatNib {
                angle_deg: 30.0,
                aspect: 0.20,
            },
        ),
        (
            "Chisel 0°",
            TipShape::FlatNib {
                angle_deg: 0.0,
                aspect: 0.18,
            },
        ),
        (
            "Broad-edge 60°",
            TipShape::FlatNib {
                angle_deg: 60.0,
                aspect: 0.30,
            },
        ),
        (
            "Star (5)",
            TipShape::StarN {
                points: 5,
                inner_ratio: 0.5,
            },
        ),
        (
            "Star (8)",
            TipShape::StarN {
                points: 8,
                inner_ratio: 0.4,
            },
        ),
        (
            "Leaf",
            TipShape::Custom {
                points: vec![
                    (0.0, -1.0),
                    (0.6, -0.4),
                    (0.4, 0.4),
                    (0.0, 1.0),
                    (-0.4, 0.4),
                    (-0.6, -0.4),
                ],
            },
        ),
        (
            "Arrow",
            TipShape::Custom {
                points: vec![
                    (0.0, -1.0),
                    (0.7, 0.4),
                    (0.25, 0.2),
                    (0.25, 1.0),
                    (-0.25, 1.0),
                    (-0.25, 0.2),
                    (-0.7, 0.4),
                ],
            },
        ),
    ]
}
