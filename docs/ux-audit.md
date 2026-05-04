# UX / Frontend-Design Audit

**Status:** Shipped (2026-05-04). Every item below has a corresponding
commit; see "Sign-off" at the bottom.
**Date:** 2026-05-04
**Owner:** S7K
**Scope:** GTK4 + libadwaita desktop UI surface — visual polish, hierarchy,
discoverability, and aesthetic cohesion. Excludes performance and
correctness (covered by Vello migration doc and brush engine doc).

This is a snapshot of areas where the app currently felt "wired up but
rough" relative to the parts that already felt shipped. Listed roughly
high-impact → low-impact. Each item has a concrete next step so this doc
could be turned into discrete tasks.

---

## 1. What already feels shipped

Reference points so the rest of the doc has a calibration. These are
the bar to aim for in the rougher surfaces.

| Surface | File | Why it works |
|---|---|---|
| Floating pen toolbar | `crates/journal-app/src/toolbar.rs` | Drag handle with persisted position, OSD translucency, inline color picker auto-dismisses on stroke start, collapse chevron. Touch-first, single-thumb reachable. |
| Zoom corner badge | `window.rs:670–778` | OSD pill, live %, fit button, grid-reset toggle. Tight, unobtrusive. |
| Home view notebook cards | `views/home.rs` | FlowBox, drag/drop reordering, smooth transitions. Reads as a finished product. |
| Page-row sidebar styling | `main.rs` embedded CSS | `.page-row.current` uses amber inset shadow + alpha — distinctive, branded. |

Common thread: **opinionated colour, deliberate motion, persisted
state, OSD layering.** When a surface lacks all four it reads as
unfinished.

---

## 2. Tool Options popup — feels jittery on tool switch

**Files:** `crates/journal-app/src/tool_options_popup.rs:273–296` and
the per-brush `append_*_internals` functions further down.

**Problem:** Switching active tool clears the body GtkBox and rebuilds
the entire form (preset row + tool grid + palette + brush internals +
tool switch row). On Framework 12 the rebuild is visible — controls
flash, focus is lost, scroll position resets. The popup is the
power-user surface used most during long sessions; the rebuild flicker
adds up.

**Improvements:**
- Build all six brush-internal sections once and `set_visible(false)`
  on the inactive ones. Toggle visibility on tool change. Avoids
  widget destruction.
- Preserve scroll position across tool change (`ScrolledWindow.vadjustment`).
- Wrap the body in an `adw::Bin` with a `transition_type =
  Crossfade` so the rebuild we can't avoid (e.g. preset reload) at
  least doesn't pop.
- Once the brush-engine refactor lands, the per-brush param structs
  collapse into one layer-list editor — re-evaluate after that.

---

## 3. Notebook template editor — power-user dense, no narrative

**File:** `crates/journal-app/src/notebook_template_creator.rs` (1,714
lines).

**Problem:** Layout is a wall of spinners and dropdowns with terse or
absent labels. There is no left-to-right read order — the user is
expected to know where to start. Bottom preview chips are too small to
read at typical zoom on a Framework 12.

**Improvements:**
- Adopt an `adw::PreferencesPage` / `PreferencesGroup` skeleton so each
  group has a header + description. Free first-class hierarchy +
  consistent spacing without writing CSS.
- Add a one-line "What you're editing" status row at the top: e.g. *"Day
  page — 2 columns, 5 widgets, bound to date {date}"*.
- Bigger preview strip — at least 2× current chip height — and make
  chips clickable to open the matching page-template editor.
- Tooltip every spinner with units + range + an example value.

---

## 4. Page template editor — functional, no live result preview

**File:** `crates/journal-app/src/template_creator.rs` (1,949 lines).

**Problem:** While editing a widget, the user sees the widget's
property panel but the canvas behind it shows the template at the
moment the editor opened — not as it will render to a real page with
the user's pen + dark mode. Round-trip "save → close → open page → see
it" feels heavy.

**Improvements:**
- Right panel: add an "On a real page" mini preview that uses the
  exact `journal_canvas::vello_renderer::VelloRenderer` pipeline at
  thumbnail size. Renders the template + a few dummy strokes so the
  user can see how widgets sit relative to ink.
- Toggle between current edit-mode preview and live-render preview
  with a single button.
- Live-render preview must respect the system dark mode so the user
  sees what they will draw on.

---

## 5. Empty-state placeholder — bare

**File:** `crates/journal-app/src/canvas_widget.rs:202–252` (Cairo
fallback) and the equivalent Vello path in
`crates/journal-canvas/src/vello_renderer.rs` (search for the no-page
branch).

**Problem:** Centered text on a flat fill. No call-to-action, no
illustration, no branded character. Worst impression surface — first
launch lands here if the user has no notebooks.

**Improvements:**
- Render a vector wordmark + amber underline (same accent pair as
  `.page-row.current`) using the existing widget renderer — keeps the
  empty state on the GPU path.
- Below the wordmark: 2–3 tappable affordances ("New notebook", "Import
  PDF", "Open template gallery") rendered as Vello rounded rects with
  pen-style outlines, not GTK buttons. Consistent with the canvas
  aesthetic.
- Subtle background — soft dot grid at very low alpha so the surface
  doesn't read as "broken / blank".
- Different copy for "no notebooks at all" vs. "notebook open, no page
  selected".

---

## 6. Settings dialogs — generic GTK modal feel

**File:** `crates/journal-app/src/settings_dialogs.rs` (1,396 lines).

**Problem:** Standard `gtk4::Dialog` wrappers. The recent
`developer_mode` toggle was added as a raw `CheckButton` + Label row —
inconsistent with libadwaita patterns elsewhere in the app. Every new
setting we add piles on the same grid.

**Improvements:**
- Migrate to `adw::PreferencesWindow` + `adw::PreferencesPage` +
  `adw::SwitchRow` / `ComboRow` / `SpinRow`. Free animations, search,
  consistent spacing, accessible labels. Removes the bespoke row-builder
  helpers.
- Group settings semantically: *Drawing*, *Page templates*, *Storage*,
  *Developer*. Right now everything is one flat list.
- The brush-tuner section inside settings is a duplicate of the
  Tool Options popup body. Once the brush-engine doc lands, both
  should converge on one `BrushEditor` widget — settings dialog hosts
  the full editor; popup hosts the focused subset.

---

## 7. Toolbar — colour slot UX gap

**File:** `crates/journal-app/src/toolbar.rs:408–599`.

**Problem:** Inline RGB picker is fast for power users but there's no
discoverability for the *palette* feature (which lives in Tool
Options). New users won't know they can save a palette colour and
re-use it from the toolbar.

**Improvements:**
- Long-press on a colour slot opens a small popover with: current
  hex, "Save to palette", and the active palette swatches. Reading
  shortest-path to the existing palette feature.
- Empty colour slot: render a faint diagonal stripe (vector) instead
  of a transparent square — signals "drag a colour here" affordance.
- Slot drag-and-drop reorder (already supported in palette flowbox)
  should work on the toolbar slots too.

---

## 8. Typography — using GTK defaults everywhere

**File:** `crates/journal-app/src/main.rs` embedded CSS.

**Problem:** Only `.wordmark` deviates from system font. Sidebar
labels, headers, page-row text, and template editor headings all
inherit Cantarell / system. The app has a clear personality (deep
indigo + amber) but the type does not carry it.

**Improvements:**
- Bundle a single distinctive display font for headers — something like
  IBM Plex Serif or Recoleta for marks of identity (notebook titles,
  section headers, empty-state hero). Body copy stays system.
- Ship the font with the app (license-permitting) under
  `crates/journal-app/assets/fonts/` and load via `pango::FontMap` at
  startup. Don't depend on system install.
- Use the `parley` font stack already in `journal-widgets` for canvas
  text so on-canvas headings and chrome headings match.
- Define type tokens in CSS: `--display-font`, `--body-font`,
  `--mono-font`. Apply via `.title-1`/`.title-2`/etc. utility classes.

---

## 9. Motion — almost nothing

**Files:** stack-page transitions in `window.rs`, page-row hover in
embedded CSS.

**Problem:** Stack pages slide left-right (good), page-row hover
transitions background (good). Beyond that, nothing animates. Tool
selection, colour pick, zoom step, page change — all instant. Reads
correct but plastic.

**Improvements:**
- Tool toolbar selection: 120ms scale-up on the selected slot,
  scale-down on the previously-selected. Pure CSS via class swap.
- Zoom step: animate the badge from old → new value over 80ms.
- Page change in the canvas: 1-frame fade-in for the new page surface
  via the Vello pipeline (already double-buffered).
- New-notebook card creation: stagger reveal of FlowBox children with
  CSS `animation-delay`.
- Resist scattering micro-interactions — pick the four moments above
  and execute them tightly. Better than animating everything.

---

## 10. Dark mode — works, but lacks contrast in two surfaces

**Files:** `canvas_widget.rs:202–252` (placeholder text colours),
`tool_options_popup.rs` (rule lines between sections).

**Problem:** Canvas dark mode bg `#1f1f21` against placeholder text
`rgba(0.7, 0.7, 0.75, 0.6)` — 4.4:1, just under WCAG AA for body. The
amber accent does not appear in dark mode at all (only used in
`.page-row.current` hover gloss).

**Improvements:**
- Lift placeholder text to `rgba(0.78, 0.78, 0.83, 0.72)` in dark.
- Pull the amber accent into dark mode chrome — toolbar drag handle
  active state, current zoom value, current tool slot ring. The brand
  loses identity in dark mode without it.
- Use `adw::StyleManager::dark_notify` to swap CSS classes (`dark` /
  `light`) on the root rather than feeding `dark_mode: bool` into every
  renderer. Less wiring.

---

## 11. Discoverability — many features are keyboard-only or hidden

Examples gathered from the source:
- Cheatsheet button exists (`window.rs`) but the keyboard shortcuts
  feature is not surfaced anywhere else.
- Developer mode (now exposed via settings, but still no in-app hint
  that it unlocks the Tool Options popup + brush tuner).
- Template gallery vs. notebook templates vs. page templates — three
  related concepts, no breadcrumb explaining the relationship.

**Improvements:**
- First-launch guided tour: 4 cards, dismiss permanent. Cards: pick a
  notebook template → draw → switch tools → save palette.
- "What's new" pane after upgrade — driven off a crate-version constant.
  Lists the last 1–2 commits' user-visible changes (military templates,
  customizable tools, etc).
- Top-bar `?` button next to the menu — global keyboard shortcut +
  cheatsheet trigger; current placement is non-obvious.

---

## 12. Brush / tool naming consistency

The codebase mixes "brush" and "tool" almost interchangeably:

- `BrushStyle` enum in `journal_core` — also represents non-drawing
  modes? Look up.
- `BrushParams` struct in `vello_renderer.rs` — per-brush, per-style.
- "Tool Options" popup, "Tool settings", "Brush tuner" — three names,
  one surface.
- The pending brush-engine doc introduces "Tool Editor" as the
  full-screen, vs. Tool Options popup.

**Improvements:**
- Decide once: **Brush** = the recipe (geometry + tip + width). **Tool**
  = the toolbar slot that points at a brush. Update all UI copy +
  identifiers in one PR before the brush-engine refactor lands; doing
  it after means renaming public types twice.
- Reflect the split in the menu: "Tools" submenu lists toolbar slots,
  "Brushes" submenu lists the recipe library.

---

## 13. WIP coherence — uncommitted changes pull two ways

**Files:** `vello_renderer.rs` (+594 lines: PenShape / PencilShape /
PaintbrushShape / SprayShape / CalligraphyShape enums and params),
`tool_options_popup.rs` (+286 lines: shape dropdowns), and the new
`docs/brush-engine.md`.

**Problem:** The WIP extends the hardcoded per-brush-style render fns
with five more shape enums, while the brush-engine doc proposes
deleting all per-brush render fns in favour of a composable layer
model. If both land, we ship the shape-enum work, then immediately
throw most of it away in Phase 5 of the brush-engine plan.

**Improvements:**
- Decide before merging the WIP:
  - **A.** Land WIP as-is, treat the new shapes as the "legacy" set
    that `legacy_brush_for(BrushStyle, BrushParams) -> Brush` (Phase 0
    of brush-engine.md) must reproduce. WIP becomes the corpus the
    composable model is validated against.
  - **B.** Park WIP, jump straight to Phase 0 of brush-engine.md.
    Migrate shape variety into composable layers from the start.
- Recommendation: **A.** WIP has visible user-value (more tip
  variety today) and gives the regression corpus a richer surface
  than the current six tools. Wire the new shape enums through
  `legacy_brush_for` when the composable engine lands.

---

## 14. Aesthetic direction — pick a stronger one

The current visual identity is *libadwaita-default + indigo accent +
amber hover*. It's tasteful but forgettable. The app is a personal
notebook for a Framework 12 + stylus — that's a niche with strong
analogues (paper notebooks, fine pens, leather covers, deckle edges).
Lean into that.

**Direction proposal — "Editorial fieldbook":**
- Display font: serif with character (Recoleta, Lora, EB Garamond).
- Body: existing system sans.
- Page surface: cream `#f4efe2` light / dim teal `#1c2a30` dark, not
  the current near-white / near-black.
- Accent: existing deep indigo + amber, but apply them *more* — to
  selection rings, focus halos, scrollbar thumbs, switch tracks.
- Page corner micro-ornament: a tiny rendered dog-ear / fold on the
  current page sidebar entry, vector.
- Toolbar: slightly warmer OSD background tint, leather-ish shadow.

This is one direction. Alternatives: brutalist-monochrome,
retro-futuristic graph-paper, organic-handdrawn. Pick one and commit
— the cost of switching is low because the app already isolates
colour + accent through CSS variables.

---

## Priorities

| Rank | Item | Effort | Payoff | Status |
|---|---|---|---|---|
| 1 | Empty-state placeholder rebuild (§5) | 0.5d | First-impression gap | ✅ `0e9ea60` |
| 2 | Tool Options jitter fix (§2) | 0.5d | Daily-use friction | ✅ `74af3e3` |
| 3 | Settings → adw::PreferencesWindow (§6) | 1d | Lifts every future settings change | ✅ `0c52b28` |
| 4 | Resolve WIP vs. brush-engine (§13) | 0.5d decision | Unblocks brush engine | ✅ resolved pre-audit-pass |
| 5 | Pick aesthetic direction + display font (§14, §8) | 0.5d decision + 0.5d implementation | Identity | ✅ `6cc042b` + `e3f9500` (selectable display font) |
| 6 | Notebook template editor hierarchy (§3) | 1d | Power-user surface readability | ✅ `2a1fb87` + `1952c9c` + `c87c628` |
| 7 | Motion — pick four moments, animate (§9) | 0.5d | Cheap delight | ✅ `2209fd3` + `01018a7` (page-change fade) |
| 8 | Page template live preview (§4) | 1d | Reduces edit round-trip | ✅ `ac64cba` |
| 9 | First-launch tour + "What's new" (§11) | 1d | Surfaces hidden features | ✅ `3c46c14` |
| 10 | Dark mode amber pull-through (§10) | 0.5d | Brand cohesion | ✅ `9152bec` + `1acbdf5` (StyleManager class swap) |
| 11 | Toolbar palette long-press (§7) | 0.5d | Discoverability | ✅ `843a90d` + `575c5fa` (empty-stripe) + `33137cc` (DnD reorder) |
| 12 | Brush vs. Tool rename (§12) | 0.5d | Cheaper now than after brush engine | ✅ `358a29a` (UI copy) + `0b022ac` (public types) |

## Sign-off

Every audit item has shipped. Cleanup pass `a3f7969` ran clippy + fmt
across the workspace afterward; the build is clean and clippy is at
zero warnings with crate-level allows for `type_complexity` and
`too_many_arguments` (both stylistic for this codebase, not bugs).

Skipped variants documented per commit body:
- §3 PreferencesPage skeleton for the full notebook-template editor
  (drag-drop layout doesn't fit a `PreferencesPage`).
- §4 toggle button between edit-mode preview and live-render preview
  (live preview always-on is simpler).
- §7 the audit's "drag a colour to the toolbar from outside" path —
  current scope is reorder-within-toolbar plus an explicit "Save to
  palette" / "Clear slot" affordance.
- §10 `journal-canvas` and `journal-widgets` still take
  `dark_mode: bool` parameters at their leaf entry points — they
  can't link libadwaita without breaking the future WASM viewer
  plan; the abstraction lives in `journal_app::is_dark_mode()`.
