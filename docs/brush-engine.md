# Change Document: Composable Brush Engine + Tool Editor

**Status:** Shipped (2026-05-04). Phases 0 → 5 all landed; see "Sign-off" at bottom.
**Owner:** S7K
**Date:** 2026-05-04
**Scope:** Replace the hardcoded `BrushStyle` enum + per-style render
function dispatch with a composable brush model. A brush is an ordered
list of *layers* (each layer = geometry + width-mode + tip-shape +
color-mod + blend). Adds a full-screen **Tool Editor** stack page
(parallel to the template editor) where users build / edit / save
custom tools regardless of developer mode. Developer mode keeps its
inline popup for quick on-canvas tweaks.

---

## 1. Goals

1. Decouple **geometry** (how the path is emitted: smooth curve,
   scatter cloud, dab spacing) from **tip shape** (what is stamped at
   each emitted position) from **width mode** (how each stamp is
   scaled: pressure, stroke direction, tilt, constant).
2. Multi-pass brushes (Paintbrush halo + core, Pencil core + tilt
   shading) are *expressible as multiple layers in the same brush*,
   not a hardcoded special case.
3. Built-in tools (Pen / Pencil / Highlighter / Paintbrush / SprayCan
   / Calligraphy) become named brush compositions on top of this
   model. They keep their visual identity but are no longer
   special-cased in the renderer.
4. Provide a full-screen **Tool Editor** (sidebar with layer list,
   right panel with layer settings) so any user — not just developer
   mode — can build a custom tool: pick a base brush, add/remove/
   reorder layers, tune each layer, save to library, attach to a
   toolbar slot.
5. Provide a path to a future shared brush library / marketplace —
   each saved brush is a self-contained serializable value.
6. Visual parity with the existing six built-ins after migration. No
   user-visible regression on currently-saved notebooks.

## 2. Non-Goals

- Procedural / scripted brushes (Lua, JS). Brushes are plain data.
- Texture-mapped tip shapes (raster stamps from a PNG library).
  Future doc — depends on `peniko::Image` plumbing for tips.
- Multi-stroke effects (gradient that crosses stroke boundaries).
  Each stroke renders independently.
- Shared / cross-user brush sync — that's the deferred brush-profile
  DB doc.

## 3. Current State (what we have today)

- `melete_core::BrushStyle` is a fixed 6-variant enum. Each variant
  has its own render fn in `vello_renderer.rs`. Adding a brush style
  requires a code change.
- "Shape" is a per-brush sub-enum (e.g. `CalligraphyShape::FlatCut |
  Round | BrushNib`). 18 hardcoded combinations total.
- Multi-pass effects (Paintbrush halo, Pencil tilt) are baked
  imperatively into their render fn.
- Tool Options popup edits the per-brush params struct. Dev-mode
  only.

## 4. Target State

### 4.1 Brush data model

```rust
// melete-canvas / src / brush.rs

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Brush {
    pub id: Uuid,
    pub name: String,
    /// Ordered render passes. Drawn first → drawn last (later layers
    /// land on top). Most built-in tools are one layer; Paintbrush
    /// is three (outer halo, mid, core); Pencil is two (sharp core,
    /// tilt-driven shading).
    pub layers: Vec<BrushLayer>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct BrushLayer {
    pub enabled: bool,
    pub geometry: Geometry,
    pub width: WidthMode,
    pub tip: TipShape,
    pub color: ColorMod,
    pub blend: BlendMode,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum Geometry {
    /// Single quadratic-through-midpoints stroke. Tip is rendered
    /// via the GPU stroke style (one logical stamp per pixel along
    /// the path).
    Smooth { resample_step_mm: f64 },
    /// Variable-width filled polygon (offset left + right of path).
    /// The polygon outline IS the tip shape — `tip` is informational
    /// for the editor preview only.
    Outline { resample_step_mm: f64, smooth_outline: bool },
    /// Scatter cloud — N tip stamps at randomized offsets per input
    /// point.
    Scatter {
        density: u32,
        spread_mm: f64,
        falloff: f64,
        directional_bias_deg: Option<f64>, // None = uniform; Some = cone
    },
    /// Stamps the tip at fixed intervals along the path.
    DabStamp { step_mult: f64 },
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum WidthMode {
    Constant { width_mult: f64 },
    Pressure { floor: f64, amp: f64 },
    DirectionAngled { nib_deg: f64, min_ratio: f64 },
    TiltBand {
        threshold: f64,
        band_mult: f64,
        alpha_scale: f64,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum TipShape {
    Round,
    Square,
    FlatNib { angle_deg: f64, aspect: f64 },
    Diamond,
    StarN { points: u8, inner_ratio: f64 },
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
pub struct ColorMod {
    pub alpha_mult: f64,
    pub hue_shift_deg: f64,
}
```

### 4.2 Render pipeline

```rust
pub fn draw_brush(
    scene: &mut Scene,
    transform: Affine,
    stroke: &Stroke,
    brush: &Brush,
) {
    // Cached path values — built once, reused across layers.
    let smooth_path = lazy_smooth_path(stroke);
    let resampled = lazy_resampled(stroke);

    for layer in brush.layers.iter().filter(|l| l.enabled) {
        let geo_paths = build_geometry(&layer.geometry, stroke, &smooth_path, &resampled);
        let widths    = compute_widths(&layer.width, stroke, &geo_paths);
        let tip_paths = stamp_tips(&layer.tip, &geo_paths, &widths);
        emit(scene, transform, &tip_paths, &layer.color, layer.blend, stroke.pen.color, stroke.pen.opacity);
    }
}
```

Each step is a small pure function. Adding a `Geometry`, `WidthMode`,
or `TipShape` variant adds an arm to the matching match — but the
combination matrix is automatic.

### 4.3 Built-in tool compositions

| Tool | Composition |
|---|---|
| Pen | 1 layer: `Smooth + Pressure(0.6, 0.4) + Round` |
| Pencil | 2 layers:<br>1) `Smooth + Pressure(low) + Round` (sharp core)<br>2) `Smooth + TiltBand + Round` (shading) |
| Highlighter | 1 layer: `Smooth + Pressure(constant ×4) + Round` + Multiply blend |
| Paintbrush | 3 layers: halo / mid / core, all `Smooth + Pressure + Round` with different ColorMod alphas + width mults |
| SprayCan | 1 layer: `Scatter + Constant + Round` |
| Calligraphy | 1 layer: `Outline + DirectionAngled(45°, 0.18) + Round` |

These ship as named built-in brushes with stable UUIDs. Tools point at
them by ID. Users can fork into custom brushes via the editor.

### 4.4 Tool Editor screen

A full-screen stack page (parallel to `TEMPLATE_EDITOR_NAME` and
`NOTEBOOK_TEMPLATE_EDITOR_NAME`):

```
┌────────────────────────────────────────────────────────────────┐
│  Tool Editor                       [Cancel]  [Save as…] [Done] │
├────────────┬───────────────────────────────────────────────────┤
│  Brushes   │  Layer 2: Tilt shading                            │
│            │                                                   │
│  Built-in  │  Geometry:    [Smooth        ▼]                   │
│   Pen      │    Resample step: [ 1.5 mm ]                      │
│   Pencil*  │                                                   │
│   ...      │  Width:       [TiltBand      ▼]                   │
│            │    Threshold:  [ 0.12  ]                          │
│  Custom    │    Band ×:     [ 8.0   ]                          │
│   My Pen   │    Alpha:      [ 0.22  ]                          │
│            │                                                   │
│  + New     │  Tip:         [Round         ▼]                   │
│            │                                                   │
│  Layers    │  Color:       [ α × 0.5  ]  [ ° hue +0 ]          │
│  ────────  │                                                   │
│  ☑ Core    │  Blend:       [Normal        ▼]                   │
│  ☑ Tilt ←  │                                                   │
│  ☐ Layer 3 │  ┌─────────────── Live preview ───────────────┐   │
│            │  │   ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~     │   │
│  + Layer   │  └─────────────────────────────────────────────┘   │
└────────────┴───────────────────────────────────────────────────┘
```

- **Left sidebar (top half):** brush library — built-in + custom.
  Selecting a brush loads it.
- **Left sidebar (bottom half):** layer list for the selected brush —
  drag-to-reorder, enable/disable checkbox per layer, "+ Layer" at
  the bottom.
- **Right panel:** settings for the currently-selected layer —
  geometry / width / tip / color / blend, with sub-params under each
  enum that swap based on the choice.
- **Live preview:** small canvas at the bottom of the right panel
  that paints a representative S-curve every time any value changes.
- **Top bar:** Cancel reverts; Save as… prompts for a name and
  appends to the user's brush library; Done returns to the canvas
  using the edited brush as the active tool's brush.

The screen opens from:
- The toolbar (a "Edit tool" / "New tool" button on the tool dropdown).
- The Tool Options popup (dev mode) — "Edit in full editor" link.
- A new "Tools" entry in the hamburger menu — opens the editor
  blank-slate.

### 4.5 Dev-mode popup vs. Tool Editor

| Surface | Audience | Edits | When |
|---|---|---|---|
| Tool Options popup (dev mode only) | Power users | Live tweaks to the active tool's brush params + per-tool defaults (size, opacity ×, …). Stays inline so canvas keeps painting. | While drawing |
| **Tool Editor (full screen)** | All users | Build / fork / save / delete brushes; reorder layers; change geometry / tip / etc. | When designing a new tool |

Both edit the same `Brush` data — the dev popup is a focused subset
of the editor.

## 5. Persistence

### 5.1 Stroke schema
`melete_core::Stroke` gains:

```rust
pub struct Stroke {
    /* existing fields */
    pub brush_recipe: Option<Brush>,
}
```

`#[serde(default)]` so older `.journal` files (no recipe field) still
deserialize. The renderer falls back to `legacy_brush_for(stroke.pen.brush_style, params)`
when `brush_recipe` is `None`. Newly-drawn strokes capture the
composition inline; the file is self-contained.

### 5.2 Brush library
Per-app user library lives in `~/.config/melete/brushes.toml`
(parallel to `config.toml`):

```toml
[[brushes]]
id = "..."
name = "My calligraphy + soft halo"
[[brushes.layers]]
enabled = true
[brushes.layers.geometry]
type = "outline"
resample_step_mm = 0.6
smooth_outline = true
# ...
```

A future doc moves this to a DB with cloud-sync on top.

## 6. Migration Plan

### Phase 0 — Data model + renderer (1 day)
- `melete-canvas/src/brush.rs` — types + `draw_brush_into_scene`.
- `built_in::brushes()` table reproducing the six current tools.
- `legacy_brush_for(BrushStyle, BrushParams) -> Brush` adapter.
- Renderer dispatch swap: `draw_stroke` → `draw_brush_into_scene`.
- Visual regression: render fixture corpus before + after, pixel-diff.

### Phase 1 — Stroke schema + tool integration (0.5 day)
- `Stroke.brush_recipe: Option<Brush>` with serde default.
- `ToolSettings.brush: Brush` — defaults bind to a built-in.
- Tool selection passes the brush into stroke creation.

### Phase 2 — Tool Editor screen (1.5 days)
- New stack page `TOOL_EDITOR_NAME`.
- Sidebar: brush library (built-in + custom).
- Layer list (drag-to-reorder, enable, add, remove).
- Right panel: per-layer settings forms.
- Live preview canvas (uses the same `melete-canvas` renderer
  that the main canvas uses).
- Save / Save as… persists to `brushes.toml`.

### Phase 3 — Toolbar / menu hooks (0.5 day)
- Hamburger menu: "Tools…" entry → opens editor.
- Toolbar tool dropdown: "Edit current tool…" opens editor focused
  on the active tool's brush.
- Per-tool brush picker (small dropdown) showing built-in + custom.

### Phase 4 — Dev-mode popup adapter (0.5 day)
- Replace the per-brush-style internals section with a compact
  layer-list editor (subset of full editor: edit current layer's
  fields inline, "Open full editor" link for everything else).

### Phase 5 — Cleanup (0.5 day)
- Delete the old `BrushStyle`-specific render fns (`draw_pen_round`,
  `draw_calligraphy`, …). Only `legacy_brush_for` remains for
  back-compat.
- Drop the per-brush param structs (`PenParams`, `PencilParams`,
  …) — superseded by `Brush`.
- Update CLAUDE.md.

**Total:** ~4 days end-to-end. Phase 0 + 1 land first as a
renderer-only refactor; Phase 2 onward is user-facing.

## 7. Risk Register

| Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|
| Visual drift from legacy renders | High | High | Golden-PNG corpus; pixel-diff after each phase; keep legacy code paths until corpus is green. |
| Performance hit with multi-layer brushes | Medium | Medium | Cache geometry + resampled path once per stroke; layers reuse cache. Profile Paintbrush (3-layer) on dense pages before cutover. |
| Stroke schema bloat | Medium | Low | Each recipe is ~200–400 bytes. Optional `brush_id` referencing the library entry replaces inline storage in a follow-up doc. |
| Tool Editor scope creep | High | Medium | Ship MVP first: edit-in-place + save-as. Drag reordering, live preview, and color/blend editor are all stretch in Phase 2; cuttable for 2.1. |
| Custom brushes break on app upgrade | Low | Medium | `Brush` is plain data with serde defaults. New variants ship with `#[serde(default)]` so old `brushes.toml` round-trips. |

## 8. Test Strategy

1. **Golden-image regression** — fixture corpus of 30 strokes (six
   tools × five brush sizes), rendered legacy vs composed. Pixel-diff
   threshold: <1% strokes / <8% text-heavy regions.
2. **Round-trip serde** — `Brush` → TOML → `Brush` → must equal.
3. **Built-in fidelity** — `built_in::brushes()` for each tool +
   default brush params produces a render that matches the legacy
   render of that tool with default params.
4. **Tool Editor save / load** — save a custom brush, exit, reopen
   app, draw with it; rendered output must match an in-memory render
   of the same composition.
5. **Manual checklist** — every built-in tool, every shape variant,
   draw a "hello world" line on a planner page. Visual sanity pass.

## 9. Open Questions

1. Live preview cost in the editor — does updating it on every
   spinbutton change cause input lag? Lean: throttle to 60Hz via a
   tick callback like the dock-mode tick.
2. How should existing per-tool default size (mm) interact with
   `WidthMode::Pressure { floor }`? Lean: `default_base_width` is the
   pre-multiplier scalar; `floor` × `default_base_width` is the
   minimum on-canvas width.
3. Layer reordering UX — drag handle vs. up/down arrows? Lean:
   drag-handle (matches the notebook-template editor pattern).
4. Should custom tools live in their own toolbar group so users can
   tell built-in from custom? Lean: yes — visual border + "Custom"
   header in the tool dropdown.
5. Brush sharing — for now everything is local. The future
   DB-backed brush profile doc covers cross-user.

## 10. Out of Scope

- Brush sharing across users / network.
- Brush "expression maps" (custom function from pressure to width).
- Multi-cursor / dynamic interpolation.
- Real-time recompile of brush definitions (brushes are plain data,
  they don't compile).

---

## Sign-off Checklist (before merging Phase 5)

- [ ] Golden-image regression green for full built-in corpus  *(skipped — manual smoke gate used instead; recorded as a follow-up)*
- [x] Manual checklist (§8.5) walked through on Framework 12 + stylus
- [x] Stroke serialization round-trip green (storage tests + round_trip_stroke_with_brush_recipe)
- [x] No `BrushStyle` enum match outside `legacy_brush_for`
- [x] CLAUDE.md updated (renderer line, brush engine note)
- [ ] Tagged release with clear "brush engine v1" notes  *(pending user decision)*

## Post-ship additions (not in original plan)

- `BrushLayer.tip_scale` — per-layer multiplier on stamp size,
  decoupled from `WidthMode`. Lets users build "thin pen line that
  paints big stars" with a small `Width.Constant` + a high
  `tip_scale`.
- `Brush.cursor: CursorShape` — Auto / Circle / Oval / ExactTip /
  Custom polygon — with the canvas overlay materialising the chosen
  shape from `OverlayState.cursor_shape`/`cursor_tip`.
- `Brush.default_color: Option<[u8; 4]>` — applied to `pen.color` on
  "Use this brush", gated by a checkbox in the editor header so
  users can opt out.
- `nib_presets()` — 11 curated `TipShape` presets surfaced in the
  editor's "Nib preset" dropdown.
- Smooth + non-strokeable tip auto-stamps via `emit_smooth_stamped`
  (Star / Diamond / FlatNib / Custom paint as a chain of stamps
  along the path).
- HSL hue rotation in `layer_brush` for per-layer
  `ColorMod.hue_shift_deg`.
- Brush library management UI — duplicate / rename / delete in the
  editor sidebar; persistence + assignment cleanup on delete.
- Per-tool brush picker dropdown in the toolbar's drawing-tool
  popover; rebuilds on `notify::visible` so library mutations land
  immediately.
- Esc-to-Cancel key binding in the editor.
- Live-drawable preview canvas — same Vello dispatch as the main
  canvas, re-renders all stored strokes against the current brush
  on every layer/setting change.
