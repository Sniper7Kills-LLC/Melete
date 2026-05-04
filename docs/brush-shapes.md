# Change Document: Editable Brush Shapes

**Status:** Future work — placeholder
**Owner:** S7K
**Date:** 2026-05-03
**Scope:** Add per-brush "shape" / "nib" selection on top of the existing
brush-internal tuning. Lets the user swap the underlying geometry the
renderer generates (round nib → flat nib → chisel) without writing a
custom tool.

---

## 1. Goals

- Each brush style exposes a `shape_kind` parameter (default = current
  hard-coded shape) that swaps which geometry generator the renderer
  uses.
- Shape selection is a new dropdown in the per-brush internals section
  of the Tool Options popup + full Brush Tuner dialog.
- Existing settings (opacity ×, width ×, blend, brush internals)
  continue to work — `shape_kind` is an additional discriminator inside
  each brush's render fn.
- Each shape ships with its own small param set (e.g. flat nib has an
  angle, fan brush has a spread). Sub-params live alongside the existing
  `Pen / Pencil / Paintbrush / Spray / Calligraphy` param structs.

## 2. Non-Goals

- Importable / shareable shape libraries — out of scope for v1.
- Texture-mapped stamps (PNG-as-stamp) — could land in a v2 once the
  enum is wired.
- Pressure-driven shape morphing — keep shape static for one stroke.

## 3. Candidates per brush

| Brush | Default shape | Future shapes |
|---|---|---|
| Pen | round-tip | flat-marker tip · dual-tip · arrow-tip |
| Pencil | cylindrical | carpenter-flat · mechanical-thin · charcoal-stub |
| Highlighter | flat-band | round-tip · chisel-tip |
| Paintbrush | round bristle | flat · filbert · fan · palette-knife |
| Spray Can | circle scatter | square stamp · soft-disc with falloff · directional cone (tilt-driven) · texture-mapped stamp |
| Calligraphy | flat-cut nib (45°) | round nib · slanted nib · italic chisel · broad-edge · brush nib |

## 4. Implementation sketch

```rust
// In journal-canvas::vello_renderer
#[derive(Copy, Clone, Serialize, Deserialize, PartialEq)]
pub enum CalligraphyShape {
    FlatCut,           // current
    Round,
    Slanted { angle_deg: f64 },
    ItalicChisel { width_ratio: f64 },
    BroadEdge,
    BrushNib { stiffness: f64 },
}

pub struct CalligraphyParams {
    pub shape: CalligraphyShape,
    pub nib_angle_deg: f64,    // applies to FlatCut + ItalicChisel
    pub min_ratio: f64,
    pub resample_step_mult: f64,
    pub smooth_outline: bool,
}

fn draw_calligraphy(scene, transform, stroke, params) {
    match params.shape {
        CalligraphyShape::FlatCut => draw_calligraphy_flatcut(...),
        CalligraphyShape::Round => draw_calligraphy_round(...),
        CalligraphyShape::Slanted { angle_deg } => draw_calligraphy_slanted(angle_deg, ...),
        ...
    }
}
```

Each shape gets its own private fn. The existing `draw_calligraphy`
becomes a dispatcher.

For SprayCan-style stamping shapes:

```rust
pub enum SprayShape {
    CircleScatter,                  // current
    SquareStamp { size: f64 },
    SoftDisc { falloff: f64 },
    Cone { spread_deg: f64 },       // direction from tilt_x/tilt_y
    TextureStamp { stamp_id: Uuid },// future — stamp library
}
```

## 5. UI changes

- Tool Options popup: when the relevant brush style is selected for the
  current tool, the brush-internals section gains a `Shape` dropdown
  above the existing knobs. Changing it swaps which sub-params are
  shown (similar pattern to `WidgetKind`-specific params in the
  template editor).
- Brush Tuner full dialog: same shape dropdown per brush section.

## 6. Persistence + back-compat

- New fields default to the legacy shape (`*::CircleScatter`, etc.) so
  existing config files keep current behaviour.
- Each shape stores its sub-params on the brush's param struct.
  `#[serde(default)]` on every new field so older configs round-trip.

## 7. Effort estimate

- ~1 day per brush style (5 brushes total) to design + render + UI.
- Shape libraries (texture stamps, etc.) deferred — separate doc.

## 8. Open questions

1. Do shapes need to mutate input (e.g. directional cone uses tilt) or
   only output geometry? Lean: input-aware.
2. How do shape-specific sub-params show in the popup without making
   the panel feel cluttered? Lean: collapse to one expander per shape,
   only the current shape's sub-params expanded by default.
3. Will the future custom-tool feature share the same shape enum or
   have its own? Lean: share — custom tools just pick brush style +
   shape + sub-params + tool-level multipliers.
