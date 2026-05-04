//! Built-in brush compositions for the six default tools.
//!
//! Each entry has a stable UUID so saved brush-recipes that reference
//! a built-in by id keep working across upgrades. New built-ins should
//! mint a fresh UUID and append below — never reuse one.
//!
//! These compositions reproduce the *default-shape* visual of each
//! tool (Pen-Round, Pencil-Cylindrical, Paintbrush-Round, etc.).
//! Non-default WIP shape variants (PenShape::Marker, Carpenter, Fan,
//! …) currently fall through to the legacy per-style render fns in
//! `vello_renderer.rs`; Phase-5 cleanup retires them once every shape
//! has a native composition here.
//!
//! See `docs/brush-engine.md` §4.3 for the design table.

use journal_core::{
    BlendMode, Brush, BrushLayer, ColorMod, CursorShape, Geometry, TipShape, WidthMode,
};
use uuid::{uuid, Uuid};

#[cfg(feature = "vello")]
use crate::vello_renderer::{
    CalligraphyShape, PaintbrushShape, PenShape, PencilShape, SprayShape, ToolStyleParams,
};
#[cfg(feature = "vello")]
use journal_core::ToolStyle;

// --- Stable IDs ------------------------------------------------------------

pub const ID_PEN: Uuid = uuid!("9f8e3c10-0001-4a00-8000-000000000001");
pub const ID_PENCIL: Uuid = uuid!("9f8e3c10-0001-4a00-8000-000000000002");
pub const ID_HIGHLIGHTER: Uuid = uuid!("9f8e3c10-0001-4a00-8000-000000000003");
pub const ID_PAINTBRUSH: Uuid = uuid!("9f8e3c10-0001-4a00-8000-000000000004");
pub const ID_SPRAYCAN: Uuid = uuid!("9f8e3c10-0001-4a00-8000-000000000005");
pub const ID_CALLIGRAPHY: Uuid = uuid!("9f8e3c10-0001-4a00-8000-000000000006");

/// Constructs a `Pen` composition. `floor` and `amp` come from
/// `PenParams` so per-user tuning still flows through.
pub fn pen(floor: f64, amp: f64) -> Brush {
    Brush::one_layer(
        ID_PEN,
        "Pen",
        Geometry::Smooth {
            resample_step_mm: 1.0,
        },
        WidthMode::Pressure { floor, amp },
        TipShape::Round,
    )
}

/// Highlighter is structurally identical to Pen. The visual
/// difference comes from the stroke's `BlendMode::Multiply` set on
/// the user's pen settings, not from the brush composition itself.
pub fn highlighter(floor: f64, amp: f64) -> Brush {
    let mut b = pen(floor, amp);
    b.id = ID_HIGHLIGHTER;
    b.name = "Highlighter".into();
    b
}

/// Pencil — sharp clamped core + tilt-driven shading band layer.
pub fn pencil(
    core_clamp_min: f64,
    core_clamp_max: f64,
    tilt_threshold: f64,
    tilt_band_mult: f64,
    tilt_alpha_scale: f64,
) -> Brush {
    Brush {
        id: ID_PENCIL,
        name: "Pencil".into(),
        layers: vec![
            // Layer 1 — sharp core. `ClampedConstant` caps the
            // computed width between `core_clamp_min` and
            // `core_clamp_max`, matching the legacy
            // `(pen.base_width / zoc).clamp(min, max)` formula.
            BrushLayer {
                enabled: true,
                geometry: Geometry::Smooth {
                    resample_step_mm: 1.0,
                },
                width: WidthMode::ClampedConstant {
                    width_mult: 1.0,
                    min_mm: core_clamp_min,
                    max_mm: core_clamp_max,
                },
                tip: TipShape::Round,
                tip_scale: 1.0,
                color: ColorMod::default(),
                blend: BlendMode::Normal,
            },
            // Layer 2 — tilt shading overlay. Emits per-segment
            // bands only where stylus tilt exceeds threshold.
            BrushLayer {
                enabled: true,
                geometry: Geometry::Smooth {
                    resample_step_mm: 1.0,
                },
                width: WidthMode::TiltBand {
                    threshold: tilt_threshold,
                    band_mult: tilt_band_mult,
                    alpha_scale: tilt_alpha_scale,
                },
                tip: TipShape::Round,
                tip_scale: 1.0,
                color: ColorMod::default(),
                blend: BlendMode::Normal,
            },
        ],
        cursor: CursorShape::Auto,
        default_color: None,
    }
}

/// Paintbrush — three-layer halo+core composition. Outer halo +
/// mid-halo + core all stroke the same Smooth path with a
/// pressure-driven core width and decreasing alpha.
pub fn paintbrush(
    halo_width_mult: f64,
    outer_halo_mult: f64,
    mid_halo_mult: f64,
    outer_alpha: f64,
    mid_alpha: f64,
    core_alpha: f64,
) -> Brush {
    let core_width_mult = 1.0;
    let outer_w = core_width_mult * halo_width_mult * outer_halo_mult;
    let mid_w = core_width_mult * halo_width_mult * mid_halo_mult;
    Brush {
        id: ID_PAINTBRUSH,
        name: "Paintbrush".into(),
        layers: vec![
            BrushLayer {
                enabled: true,
                geometry: Geometry::Smooth {
                    resample_step_mm: 1.0,
                },
                // Outer halo: width mult * core, alpha = outer_alpha.
                // Pressure floor pinned high so the halo is always
                // visible regardless of light pressure.
                width: WidthMode::Pressure {
                    floor: 0.0,
                    amp: outer_w,
                },
                tip: TipShape::Round,
                tip_scale: 1.0,
                color: ColorMod {
                    alpha_mult: outer_alpha,
                    hue_shift_deg: 0.0,
                },
                blend: BlendMode::Normal,
            },
            BrushLayer {
                enabled: true,
                geometry: Geometry::Smooth {
                    resample_step_mm: 1.0,
                },
                width: WidthMode::Pressure {
                    floor: 0.0,
                    amp: mid_w,
                },
                tip: TipShape::Round,
                tip_scale: 1.0,
                color: ColorMod {
                    alpha_mult: mid_alpha,
                    hue_shift_deg: 0.0,
                },
                blend: BlendMode::Normal,
            },
            BrushLayer {
                enabled: true,
                geometry: Geometry::Smooth {
                    resample_step_mm: 1.0,
                },
                width: WidthMode::Pressure {
                    floor: 0.0,
                    amp: core_width_mult,
                },
                tip: TipShape::Round,
                tip_scale: 1.0,
                color: ColorMod {
                    alpha_mult: core_alpha,
                    hue_shift_deg: 0.0,
                },
                blend: BlendMode::Normal,
            },
        ],
        cursor: CursorShape::Auto,
        default_color: None,
    }
}

/// Spray can — single Scatter layer. `dot_radius_factor` becomes the
/// stamp width multiplier (constant per dot); `dots_per_point`
/// becomes density.
pub fn spray(dots_per_point: u32, dot_radius_factor: f64, min_dot_radius_mm: f64) -> Brush {
    let _ = min_dot_radius_mm; // floor enforced in renderer for now
    Brush::one_layer(
        ID_SPRAYCAN,
        "Spray Can",
        Geometry::Scatter {
            density: dots_per_point,
            // spread_mm = 0 means renderer derives spread from the
            // stroke's base_width (matches legacy behaviour).
            spread_mm: 0.0,
            // r_unit² distribution → centre-biased scatter.
            falloff: 2.0,
            directional_bias_deg: None,
        },
        WidthMode::Constant {
            width_mult: dot_radius_factor,
        },
        TipShape::Round,
    )
}

/// Calligraphy — variable-width Outline polygon, width modulated by
/// stroke direction relative to the nib axis.
pub fn calligraphy(
    nib_angle_deg: f64,
    min_ratio: f64,
    resample_step_mult: f64,
    smooth_outline: bool,
) -> Brush {
    Brush::one_layer(
        ID_CALLIGRAPHY,
        "Calligraphy",
        Geometry::Outline {
            // Renderer multiplies by the stroke's max_width to get
            // the actual mm step — encode the multiplier here.
            resample_step_mm: resample_step_mult,
            smooth_outline,
        },
        WidthMode::DirectionAngled {
            nib_deg: nib_angle_deg,
            min_ratio,
        },
        TipShape::Round,
    )
}

// --- Legacy adapter --------------------------------------------------------
//
// Maps every (ToolStyle, ToolStyleParams) combination — including all
// WIP shape variants — to a fully-native composable Brush. After
// Phase-5 every shape resolves through the composable engine; the
// per-style legacy `draw_*` fns in `vello_renderer.rs` are no longer
// reachable.

/// Returns a freshly-constructed `Brush` for the given legacy tool +
/// params. Always `Some` after Phase-5 — every shape has a native
/// composition. The `Option` return shape is preserved for
/// back-compat with callers that still treat it as fallible.
#[cfg(feature = "vello")]
pub fn legacy_brush_for(style: ToolStyle, params: &ToolStyleParams) -> Option<Brush> {
    match style {
        ToolStyle::Pen => Some(pen_for_shape(params, false)),
        ToolStyle::Highlighter => Some(pen_for_shape(params, true)),
        ToolStyle::Pencil => Some(pencil_for_shape(params)),
        ToolStyle::Paintbrush => Some(paintbrush_for_shape(params)),
        ToolStyle::SprayCan => Some(spray_for_shape(params)),
        ToolStyle::Calligraphy => Some(calligraphy_for_shape(params)),
    }
}

#[cfg(feature = "vello")]
fn pen_for_shape(params: &ToolStyleParams, highlighter_id: bool) -> Brush {
    let id = if highlighter_id {
        ID_HIGHLIGHTER
    } else {
        ID_PEN
    };
    let name = if highlighter_id { "Highlighter" } else { "Pen" };
    match params.pen.shape {
        PenShape::Round => {
            let mut b = pen(params.pen.width_floor, params.pen.width_pressure_amplitude);
            b.id = id;
            b.name = name.into();
            b
        }
        PenShape::Flat => Brush::one_layer(
            id,
            name,
            Geometry::Outline {
                resample_step_mm: 0.5,
                smooth_outline: true,
            },
            WidthMode::Pressure {
                floor: params.pen.width_floor,
                amp: params.pen.width_pressure_amplitude,
            },
            TipShape::Round,
        ),
        PenShape::Marker => Brush::one_layer(
            id,
            name,
            Geometry::Smooth {
                resample_step_mm: 1.0,
            },
            WidthMode::Constant {
                width_mult: params.pen.marker_width_mult,
            },
            TipShape::Round,
        ),
    }
}

#[cfg(feature = "vello")]
fn pencil_for_shape(params: &ToolStyleParams) -> Brush {
    match params.pencil.shape {
        PencilShape::Cylindrical => pencil(
            params.pencil.core_clamp_min,
            params.pencil.core_clamp_max,
            params.pencil.tilt_threshold,
            params.pencil.tilt_band_mult,
            params.pencil.tilt_alpha_scale,
        ),
        PencilShape::Carpenter => Brush::one_layer(
            ID_PENCIL,
            "Pencil — carpenter",
            Geometry::Outline {
                resample_step_mm: 0.4,
                smooth_outline: true,
            },
            WidthMode::DirectionAngled {
                nib_deg: 45.0,
                min_ratio: 0.35,
            },
            TipShape::Round,
        ),
        PencilShape::Mechanical => Brush::one_layer(
            ID_PENCIL,
            "Pencil — mechanical",
            Geometry::Smooth {
                resample_step_mm: 1.0,
            },
            WidthMode::ClampedConstant {
                width_mult: 0.5,
                min_mm: 0.2,
                max_mm: 0.6,
            },
            TipShape::Round,
        ),
    }
}

#[cfg(feature = "vello")]
fn paintbrush_for_shape(params: &ToolStyleParams) -> Brush {
    match params.paintbrush.shape {
        PaintbrushShape::Round => paintbrush(
            params.paintbrush.halo_width_mult,
            params.paintbrush.outer_halo_mult,
            params.paintbrush.mid_halo_mult,
            params.paintbrush.outer_alpha,
            params.paintbrush.mid_alpha,
            params.paintbrush.core_alpha,
        ),
        PaintbrushShape::Flat => Brush::one_layer(
            ID_PAINTBRUSH,
            "Paintbrush — flat",
            Geometry::Smooth {
                resample_step_mm: 1.0,
            },
            WidthMode::Pressure {
                floor: 0.2,
                amp: 0.8,
            },
            TipShape::Square,
        ),
        PaintbrushShape::Fan => Brush::one_layer(
            ID_PAINTBRUSH,
            "Paintbrush — fan",
            Geometry::FanOffset {
                count: params.paintbrush.fan_count,
                spread_mult: params.paintbrush.fan_spread_mult,
            },
            WidthMode::Pressure {
                floor: 0.2,
                amp: 0.8,
            },
            TipShape::Round,
        ),
    }
}

#[cfg(feature = "vello")]
fn spray_for_shape(params: &ToolStyleParams) -> Brush {
    let directional_bias_deg = match params.spray.shape {
        SprayShape::Cone => Some(params.spray.cone_spread_deg),
        _ => None,
    };
    let tip = match params.spray.shape {
        SprayShape::Square => TipShape::Square,
        _ => TipShape::Round,
    };
    Brush::one_layer(
        ID_SPRAYCAN,
        "Spray Can",
        Geometry::Scatter {
            density: params.spray.dots_per_point,
            spread_mm: 0.0,
            falloff: 2.0,
            directional_bias_deg,
        },
        WidthMode::Constant {
            width_mult: params.spray.dot_radius_factor,
        },
        tip,
    )
}

#[cfg(feature = "vello")]
fn calligraphy_for_shape(params: &ToolStyleParams) -> Brush {
    match params.calligraphy.shape {
        CalligraphyShape::FlatCut => calligraphy(
            params.calligraphy.nib_angle_deg,
            params.calligraphy.min_ratio,
            params.calligraphy.resample_step_mult,
            params.calligraphy.smooth_outline,
        ),
        CalligraphyShape::Round => Brush::one_layer(
            ID_CALLIGRAPHY,
            "Calligraphy — round",
            Geometry::Smooth {
                resample_step_mm: 1.0,
            },
            WidthMode::Constant { width_mult: 1.0 },
            TipShape::Round,
        ),
        CalligraphyShape::BrushNib => Brush::one_layer(
            ID_CALLIGRAPHY,
            "Calligraphy — brush nib",
            Geometry::Outline {
                resample_step_mm: params.calligraphy.resample_step_mult,
                smooth_outline: params.calligraphy.smooth_outline,
            },
            WidthMode::Pressure {
                floor: 0.4,
                amp: 0.6,
            },
            TipShape::Round,
        ),
    }
}

#[cfg(all(test, feature = "vello"))]
mod tests {
    use super::*;

    #[test]
    fn default_shapes_route_through_composable_engine() {
        let params = ToolStyleParams::default();
        for style in [
            ToolStyle::Pen,
            ToolStyle::Pencil,
            ToolStyle::Highlighter,
            ToolStyle::Paintbrush,
            ToolStyle::SprayCan,
            ToolStyle::Calligraphy,
        ] {
            assert!(
                legacy_brush_for(style, &params).is_some(),
                "default-shape composition missing for {:?}",
                style,
            );
        }
    }

    #[test]
    fn pencil_has_two_layers() {
        let params = ToolStyleParams::default();
        let brush = legacy_brush_for(ToolStyle::Pencil, &params).unwrap();
        assert_eq!(brush.layers.len(), 2, "Pencil = sharp core + tilt band");
    }

    #[test]
    fn paintbrush_has_three_layers() {
        let params = ToolStyleParams::default();
        let brush = legacy_brush_for(ToolStyle::Paintbrush, &params).unwrap();
        assert_eq!(
            brush.layers.len(),
            3,
            "Paintbrush = outer halo + mid + core",
        );
    }

    #[test]
    fn every_shape_resolves_natively_post_phase5() {
        // After Phase 5 the legacy adapter has a composable Brush
        // for every shape variant — no None fall-throughs left.
        let combos: &[(ToolStyle, fn(&mut ToolStyleParams))] = &[
            (ToolStyle::Pen, |p| p.pen.shape = PenShape::Round),
            (ToolStyle::Pen, |p| p.pen.shape = PenShape::Flat),
            (ToolStyle::Pen, |p| p.pen.shape = PenShape::Marker),
            (ToolStyle::Pencil, |p| {
                p.pencil.shape = PencilShape::Cylindrical
            }),
            (ToolStyle::Pencil, |p| {
                p.pencil.shape = PencilShape::Carpenter
            }),
            (ToolStyle::Pencil, |p| {
                p.pencil.shape = PencilShape::Mechanical
            }),
            (ToolStyle::Paintbrush, |p| {
                p.paintbrush.shape = PaintbrushShape::Round
            }),
            (ToolStyle::Paintbrush, |p| {
                p.paintbrush.shape = PaintbrushShape::Flat
            }),
            (ToolStyle::Paintbrush, |p| {
                p.paintbrush.shape = PaintbrushShape::Fan
            }),
            (ToolStyle::SprayCan, |p| p.spray.shape = SprayShape::Circle),
            (ToolStyle::SprayCan, |p| p.spray.shape = SprayShape::Square),
            (ToolStyle::SprayCan, |p| p.spray.shape = SprayShape::Cone),
            (ToolStyle::Calligraphy, |p| {
                p.calligraphy.shape = CalligraphyShape::FlatCut
            }),
            (ToolStyle::Calligraphy, |p| {
                p.calligraphy.shape = CalligraphyShape::Round
            }),
            (ToolStyle::Calligraphy, |p| {
                p.calligraphy.shape = CalligraphyShape::BrushNib
            }),
        ];
        for (style, mutate) in combos {
            let mut p = ToolStyleParams::default();
            mutate(&mut p);
            assert!(
                legacy_brush_for(*style, &p).is_some(),
                "{:?} + variant must resolve natively",
                style,
            );
        }
    }

    #[test]
    fn brush_serde_round_trip() {
        let brush = pen(0.6, 0.4);
        let toml_str = toml::to_string(&brush).expect("encode");
        let decoded: Brush = toml::from_str(&toml_str).expect("decode");
        assert_eq!(brush, decoded);
    }

    #[test]
    fn built_in_ids_are_stable_and_distinct() {
        let ids = [
            ID_PEN,
            ID_PENCIL,
            ID_HIGHLIGHTER,
            ID_PAINTBRUSH,
            ID_SPRAYCAN,
            ID_CALLIGRAPHY,
        ];
        for i in 0..ids.len() {
            for j in (i + 1)..ids.len() {
                assert_ne!(ids[i], ids[j], "built-in IDs must be distinct");
            }
        }
    }
}
