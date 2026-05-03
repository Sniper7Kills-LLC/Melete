# Change Document: Migrate Renderer from Cairo to Vello

**Status:** Draft
**Owner:** S7K
**Date:** 2026-05-03
**Scope:** Native canvas renderer migration with eye on future cloud sync (DynamoDB + AppSync) and view-only web viewer.

---

## 1. Goals

1. Replace `gtk4::cairo` rendering with Vello (`wgpu`-backed) in `journal-canvas`.
2. Preserve current visual fidelity for all six brush styles + backgrounds + widgets.
3. Keep the renderer **storage-agnostic** — same pipeline drives SQLite-backed local rendering today and DynamoDB/AppSync-backed remote rendering tomorrow.
4. Establish a cross-target render path (native + WASM/WebGPU) so the future web viewer reuses the renderer crate, not a JS reimplementation.
5. Zero data migration: existing `.journal` SQLite files render unchanged.

## 2. Non-Goals

- Remote sync implementation (separate doc, follows this).
- DynamoDB schema rollout (separate doc).
- QR-code share flow (separate doc).
- Web viewer hosting / Amplify wiring.
- Stylus input changes.
- GTK4 widget tree changes outside `DrawingArea` integration.

## 3. Current State

| Concern | Today |
|---|---|
| Renderer | `gtk4::cairo` on `DrawingArea` (`stroke_renderer.rs`, `paint_with_widgets_ctx`) |
| Backing surface | Direct draw into GTK4-managed Cairo context |
| Coordinate space | Canvas-space, viewport-transformed at render time |
| Stroke storage (local) | SQLite per-notebook: `points_blob` (bincode `Vec<StrokePoint>`) + `pen_json` (serde_json `PenSettings`) + bbox cols |
| Stroke storage (remote, planned) | DynamoDB row per stroke, structured attributes, UUID-keyed (NOT a JSON blob in S3) |
| Brush styles | Pen / Pencil / Highlighter / Paintbrush / SprayCan / Calligraphy. Renderer dispatches by `BrushStyle`. |
| Widgets | Templates declared in TOML, rendered between background and strokes via `paint_with_widgets_ctx` |
| Backgrounds | Grid / Lines / Dots / Isometric / Hexagonal / Image / PDF, rendered behind strokes |
| GPU | None — software raster only. Original Skia/GLArea attempt failed on Wayland (`direct_contexts::make_gl`) |

## 4. Target State

| Concern | Target |
|---|---|
| Renderer | `vello` building a `Scene`; rendered via `wgpu` to a GPU texture; presented inside GTK4 |
| Backing surface | GTK4 `GLArea` with a `wgpu::Surface` over the GLArea's GL context, OR readback to `cairo::ImageSurface` and blit (fallback path) |
| Coordinate space | Unchanged (canvas-space, viewport-applied as Vello affine transform) |
| Stroke storage | **Unchanged at the byte level.** Structured representation already lives in `journal-core` types; SQLite codec untouched. |
| Render path crate | `journal-canvas` becomes target-agnostic Rust (no `gtk4::cairo` deps). Native binary links it, future WASM viewer links the same crate. |
| Web viewer | Compiles `journal-canvas` to `wasm32-unknown-unknown`, drives Vello via `wgpu`'s WebGPU backend, renders into an HTML5 `<canvas>`. |

## 5. Storage Independence (Key Constraint)

The renderer must consume a **storage-shape-agnostic Stroke domain model**, because the same `Stroke` value will arrive from three sources over time:

1. **Today (local):** SQLite row → `points_blob` bincode-decoded → `Vec<StrokePoint>` + `PenSettings`
2. **Future (cloud sync):** AppSync GraphQL response → JSON → `Vec<StrokePoint>` + `PenSettings`
3. **Future (web viewer):** AppSync direct fetch → identical JSON → identical types

**Implication: the `journal-core::Stroke` type is the contract.** All three paths converge on it before the renderer sees it. Renderer never knows which backend produced the strokes.

This already holds today (`JournalBackend` trait yields `Stroke` regardless of impl). Migration **must not** introduce a Cairo-shaped intermediate that breaks this contract.

### Rendering data flow (post-migration)

```
SQLite/DynamoDB row
        │
        │  decode points_blob  /  GraphQL deserialize
        ▼
journal_core::Stroke { id, points: Vec<StrokePoint>, pen: PenSettings,
                       zoom_at_creation, bounding_box }
        │
        │  &[Stroke]  (owned by SharedState)
        ▼
journal_canvas::paint_scene(scene, transform, page_rect, background,
                            widgets, strokes, selection, …)
        │  builds vello::Scene
        ▼
vello::Renderer.render_to_texture(scene, render_target)
        │
        ├── Native: wgpu surface backed by GTK4 GLArea
        └── Web:    wgpu surface backed by HTMLCanvasElement
```

`Stroke` shape is the only thing that crosses the storage/renderer boundary. Migration touches only the boxed step (`paint_scene` and below).

## 6. Domain Model — Confirmed Stable

These types live in `journal-core` and are unchanged by this migration:

- `Stroke { id: Uuid, points: Vec<StrokePoint>, pen: PenSettings, zoom_at_creation: f64, bounding_box: Rect }`
- `StrokePoint { x, y, pressure, tilt_x, tilt_y, timestamp_ms }`
- `PenSettings { color, base_width, opacity, blend_mode, brush_style }`
- `BrushStyle { Pen, Pencil, Highlighter, Paintbrush, SprayCan, Calligraphy }`
- `PageTemplate`, `TemplateWidget`, `BackgroundConfig`, `WidgetKind`, …

DynamoDB schema (when defined) maps these 1:1:

| Type | DynamoDB table | PK | SK | Notes |
|---|---|---|---|---|
| Notebook | `notebooks` | `owner_id` | `notebook_id` | metadata only |
| Section | `sections` | `notebook_id` | `section_id` | parent_section_id attr |
| Page | `pages` | `section_id` | `page_id` | template_id attr |
| Stroke | `strokes` | `page_id` | `stroke_id` | `points` (B), `pen` (M), `bbox` (M), `zoom` (N) |
| Template | `templates` | `owner_id` | `template_id` | TOML body in attr |

**`points` attribute encoding for cloud:** MessagePack (compact, Lambda/JS decoders ubiquitous) or JSON (verbose, simple). Choose at cloud-doc time. **Bincode stays SQLite-local.** Native ↔ cloud sync re-encodes at the boundary; the renderer never sees either format.

## 7. Architecture

### 7.1 Crate boundaries

| Crate | Today | After |
|---|---|---|
| `journal-core` | Pure types, serde | **Unchanged** |
| `journal-canvas` | Cairo-dependent renderer + backgrounds + widgets | Target-agnostic Rust + Vello renderer; **no `gtk4` dep** |
| `journal-app` | GTK4 app, owns `DrawingArea`, calls into `journal-canvas` Cairo APIs | Owns `GLArea` (or hidden `GLArea` + readback), drives `journal-canvas` via Vello scene API |
| `journal-storage` | Backend trait + SQLite impl | **Unchanged** |
| `journal-templates` | Template loaders, TOML | **Unchanged** |
| (new) `journal-canvas-gtk` | — | Thin GTK4 ↔ wgpu surface adapter. Splits GTK-specific code out of `journal-canvas` so the renderer is portable to wasm. |
| (future) `journal-web` | — | WASM build of `journal-canvas` + JS wrapper |

### 7.2 New `journal-canvas` public API (post-migration)

```rust
pub struct CanvasRenderer { /* vello::Renderer + wgpu device */ }

impl CanvasRenderer {
    pub fn new(device: wgpu::Device, queue: wgpu::Queue) -> Result<Self>;

    /// Build a Vello scene for one frame. Pure scene construction —
    /// renders nothing yet. Caller schedules the draw against a wgpu
    /// surface.
    pub fn build_scene(
        &mut self,
        scene: &mut vello::Scene,
        transform: &ViewportTransform,
        page_rect: Rect,
        background: &BackgroundConfig,
        widgets: &[TemplateWidget],
        strokes: &[Stroke],
        selected_ids: &HashSet<Uuid>,
        dark_mode: bool,
        widget_ctx: &WidgetRenderContext,
    );

    pub fn render(
        &mut self,
        scene: &vello::Scene,
        target: &wgpu::TextureView,
        size: (u32, u32),
        clear: vello::peniko::Color,
    );
}
```

Renderer holds **no GTK references**. Construction takes a `wgpu::Device`/`Queue` from whoever owns the surface (GTK4 GLArea native, HTMLCanvasElement on web).

### 7.3 GTK4 integration

Two paths, ranked:

**Path A (preferred): direct GLArea surface**

```
GTK4 GLArea → glArea.attach_buffers() → wgpu::Surface::create_surface_unsafe(...)
            → wgpu::SurfaceTexture per frame
            → Vello renders into it
            → GLArea presents on swap_buffers
```

Risks the same `make_gl` Wayland pain as the original Skia attempt. **Mitigation:** route wgpu through Vulkan-on-Wayland instead of forcing GL. wgpu supports Vulkan natively on Linux; GTK4 can host a Vulkan surface via `gdk_wayland_*` bridges since 4.14, or via `gtk4::Native::wayland_surface()` for the underlying `wl_surface`.

**Path B (fallback): off-screen + readback**

```
Vello → wgpu offscreen texture → readback to RGBA buffer
      → cairo::ImageSurface from raw data
      → blit in DrawingArea::draw_func (current Cairo path)
```

Slower (CPU readback is not free) but unblocks shipping. Per-frame readback for a notebook canvas (~1280×800) is ~4 MB/frame, well within PCIe bandwidth. Acceptable as MVP; switch to Path A once stable.

### 7.4 Web target (forward-looking; not part of this migration)

`journal-canvas` builds with `wasm32-unknown-unknown` once GTK code is out:

```
HTMLCanvasElement → wgpu::Surface (WebGPU backend, WebGL2 fallback)
                  → vello::Scene built from same Rust code
                  → renders directly to canvas
```

JS layer = ~30 LOC: `init_renderer(canvas)`, `render_notebook(json)`. No JS port of stroke logic.

## 8. Phased Migration Plan

### Phase 0 — Spike (1–2 days)

- Stand up Vello in a throwaway example: render one stroke to a window via `gtk4::GLArea` + `wgpu` + `vello`.
- Validate Path A (Vulkan-on-Wayland through wgpu) works on Framework 12 without `make_gl` failure.
- If Path A blocks, lock in Path B (offscreen + readback) for MVP.

**Exit criterion:** one stroke visible on screen, identifiable as Vello-rendered (zoom in, see analytic AA quality vs Cairo's).

### Phase 1 — Core stroke port (3–5 days)

- New crate `journal-canvas-vello` (lives next to existing `journal-canvas`, both compile).
- Port `draw_stroke` dispatch for all six `BrushStyle` variants:
  - **Pen / Highlighter:** Vello `Stroke` with width-tapered `BezPath`. Pressure mapping → use Vello's `StrokeOpts` width profile.
  - **Pencil:** Vello stroke with constant-width `Path` + scattered `Path::circle` for grain. Use `peniko::BlendMode::Normal`.
  - **Paintbrush:** Vello `Path::circle` dabs at sub-radius spacing with `peniko::Gradient::new_radial` for soft falloff. Same algorithm as today, Cairo→Vello API translation.
  - **SprayCan:** Vello `Path::circle` for each scatter dot. Deterministic-noise unchanged.
  - **Calligraphy:** Replace polygon-fill math with Vello's `StrokeStyle` + tapered width function. Confirms whether Vello's variable-width stroking matches your current polygon construction; may keep polygon as fallback.
- Write a visual regression harness: render a fixed `Vec<Stroke>` to PNG via Cairo (current code) and Vello (new code), pixel-diff. Tolerance: <2% pixels different at threshold 8/255.
- Both renderers live side-by-side; behind a feature flag or runtime toggle.

**Exit criterion:** all six brush styles render correctly in a side-by-side test page.

### Phase 2 — Backgrounds + page bounds (2–3 days)

- Port `paint_with_widgets_ctx` background paths (Grid / Lines / Dots / Isometric / Hexagonal).
- Port `draw_page_bounds_outline`.
- Image / PDF backgrounds: Vello image rendering via `peniko::Image`. PDF backgrounds need a rasterizer — current Cairo path uses `poppler` or similar; check whether that's swappable for `pdfium-render` or stays Cairo with handoff.
- Selection handles + lasso overlay.

**Exit criterion:** a blank page with a grid template renders identically to current Cairo output.

### Phase 3 — Widgets (5–7 days)

- Port every `WidgetKind` renderer in `journal-templates` to emit Vello scene calls instead of Cairo calls.
- TextBlock + `title_format` engine — fonts via `parley` (Linebender's text crate, pairs with Vello). This is the largest unknown — text rendering parity is the main fidelity risk.
- Widget set: Calendar / Timeline / Checklist / Franklin priority list / Full Focus big-three / weekly compass / day-schedule.

**Exit criterion:** every notebook template in the test corpus renders pixel-comparable to Cairo output (allow 5% pixel diff for text anti-aliasing).

### Phase 4 — GTK4 integration swap (1–2 days)

- Replace `DrawingArea` + `cairo::Context` rendering in `canvas_widget.rs` with `GLArea` + Vello renderer (Path A) or hidden `GLArea`+readback into `DrawingArea` (Path B).
- Brush cursor overlay (`draw_brush_cursor`): keep on Cairo, drawn over the Vello surface — or port to Vello scene as last layer.
- Remove `gtk4` dep from `journal-canvas`. All GTK-specific code now in `journal-app`.

**Exit criterion:** running app shows live drawing, brush cursor visible, no crashes on resize / zoom / page swap.

### Phase 5 — Cleanup + flag flip (1 day)

- Delete Cairo renderer paths.
- Delete feature flag.
- Update `CLAUDE.md` (renderer line, GPU note).
- Remove the dual-renderer test harness; keep visual regression PNGs as golden snapshots against Vello.

**Exit criterion:** `cargo build -p journal-app` produces a binary with Vello as the only renderer.

### Phase 6 (later, separate doc) — Web viewer

- Strip `gtk4-rs` from `journal-canvas` dependency tree (already done in Phase 4).
- Add `wasm32-unknown-unknown` build target.
- Web viewer crate `journal-web` compiles `journal-canvas` + a small `wasm-bindgen` wrapper.

## 9. Schema & Data Model Changes

**None in this migration.** Existing SQLite + bincode `points_blob` + `pen_json` columns persist.

When cloud sync arrives later:

- DynamoDB `strokes` table: structured per the table in §6
- Wire format: MessagePack (proposed) for `points` attribute; JSON for `pen` attribute
- Sync layer in a new `journal-amplify` crate translates between local bincode and cloud MessagePack at sync boundary
- Renderer stays oblivious — both paths produce `Vec<Stroke>` for it

This document explicitly does not pre-commit to MessagePack vs JSON for cloud `points`; that decision belongs in the cloud-sync change doc.

## 10. Risk Register

| Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|
| Wayland GLArea + wgpu interaction fails like original Skia attempt | Medium | High | Phase 0 spike validates first; Path B fallback exists; wgpu's Vulkan backend bypasses GL entirely |
| Vello text rendering (parley) doesn't match Cairo's font output | High | Medium | Accept some pixel diff in text; lock font selection; render diff harness with high tolerance for glyph regions |
| Vello v0.x API churn breaks build between releases | High | Low | Pin Vello version; budget 0.5 day per upgrade; track Vello changelog |
| Variable-width Bezier stroking in Vello differs from current polygon math (calligraphy) | Medium | Low | Keep current polygon path as fallback inside `BrushStyle::Calligraphy` arm |
| Performance regression on huge pages | Low | Medium | Vello's design specifically targets dense scenes; benchmark before flag flip |
| WebGPU absent in target browsers (when web viewer ships) | Medium | Medium | wgpu's WebGL2 fallback covers older browsers with feature loss; document browser support matrix |
| GLArea swap introduces input-event regressions (stylus/touch routing changes between widgets) | Low | High | Phase 4 dedicates full day to input verification; keep `EventControllerStylus`/`EventControllerMotion` attached to GLArea same as current `DrawingArea` |
| PDF background rendering loses fidelity | Low | Medium | Keep Cairo+poppler PDF rasterization, hand resulting image to Vello as `peniko::Image` |

## 11. Test Strategy

1. **Visual regression harness** (`crates/journal-canvas/tests/visual.rs`):
   - Fixed test corpus: 30 reference scenes (each brush, each background, each widget kind, dense page).
   - Renders each via Cairo (during transition) and Vello, saves PNG, diffs against checked-in goldens.
   - Threshold: <2% pixel diff strokes/backgrounds; <8% for text-heavy widgets.
2. **Unit tests** for scene-construction helpers (curve math, polygon outline math) — pure functions, no GPU required.
3. **Integration test** binary that boots a hidden GLArea, renders one frame, dumps to PNG. Runs in CI on Linux x86_64.
4. **Manual checklist** at end of Phase 4: draw with each tool, zoom, pan, undo/redo, page switch, dark mode toggle, fullscreen.

## 12. Dependencies

New crate adds:

```toml
vello = "0.3"           # pin until 1.0
peniko = "0.2"          # color/brush types Vello uses
wgpu = "0.20"
parley = "0.2"          # text shaping/layout, Linebender
```

Removed eventually:

```toml
# from journal-canvas
gtk4 = "*"   # only the cairo re-export was used
```

Binary size delta: ~+8–12 MB (Vello + wgpu + parley + Vulkan loader). Acceptable for a desktop app.

## 13. Rollout

- Migration is **renderer-only**; storage and persistence untouched.
- Single-user app; no migration scripts needed.
- After Phase 5 lands, ship a tagged release (`v0.x renderer-vello`). Keep prior commit SHA tagged (`renderer-cairo-final`) for emergency revert.
- No feature-flag the user sees; renderer choice is internal.

## 14. Effort Estimate

| Phase | Days |
|---|---|
| 0 — Spike | 1–2 |
| 1 — Strokes | 3–5 |
| 2 — Backgrounds | 2–3 |
| 3 — Widgets | 5–7 |
| 4 — GTK swap | 1–2 |
| 5 — Cleanup | 1 |
| **Total** | **13–20 days** |

Phase 6 (web viewer) is separate work, ~3–4 days to bring up against the now-portable `journal-canvas` crate.

## 15. Open Questions

1. Path A (direct GLArea wgpu surface) vs Path B (offscreen readback) — answered by Phase 0 spike.
2. PDF background rendering: keep Cairo+poppler hand-off, or move to `pdfium-render` to drop poppler?
3. Text rendering: parley is required for Vello text; do we need it before Phase 3 widgets ship, or can we ship widgets-without-text first?
4. Brush cursor overlay: render via Vello scene (one renderer, consistent) or keep Cairo overlay (simpler, decoupled)?
5. When DynamoDB sync arrives, does the renderer touch any new code paths? **Expected: no.** Renderer sees `&[Stroke]` regardless of source.

## 16. Out of Scope (Listed Here So They're Not Forgotten)

- Cloud sync (DynamoDB + AppSync) — separate doc, builds on this migration.
- View-only web viewer + QR share — separate doc, depends on Phase 6.
- Server-side rendering for share links — separate doc; would reuse the same `journal-canvas` crate as a Lambda layer.
- Renderer-driven PDF export — current `pdf_export.rs` uses Cairo directly; revisit after Phase 5 to decide if it stays Cairo or moves to Vello-via-readback.

---

## Sign-off Checklist (before merging Phase 5)

- [ ] Visual regression harness green for full test corpus
- [ ] Manual checklist (§11.4) walked through on Framework 12 (touchscreen + stylus)
- [ ] No `gtk4::cairo` references in `journal-canvas` crate
- [ ] `journal-canvas` builds with `--target wasm32-unknown-unknown` (no GTK deps leaking)
- [ ] CLAUDE.md updated (renderer line, GPU note removed/changed)
- [ ] Cairo-final SHA tagged in git
- [ ] Tagged release pushed
