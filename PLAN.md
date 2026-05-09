# Project Plan — Journal App

> **Issue tracking has moved to GitHub.** Open work for Phase 6 and the
> "Future (Not Now)" backlog lives at
> <https://github.com/Sniper7Kills-LLC/Journal/issues>. This document
> remains the architectural reference and historical record of completed
> phases. Unchecked items below are annotated with their GH issue number.

## Vision

Personal OneNote/rnote alternative for Linux. Two key differentiators:
1. **Infinite scroll/zoom canvas** per page (not fixed page boundaries)
2. **Template system** — page templates (backgrounds/layouts) + notebook templates (planner auto-generation)

---

## Core Structure

```
App
├── Notebook (has: assigned page templates)
│   ├── Section (can further limit available templates, sortable)
│   │   ├── Page (infinite canvas, reorderable)
│   │   ├── Page
│   │   └── ...
│   ├── Section
│   └── ...
└── Notebook
```

---

## Concepts

### Page Template
A background/layout applied when creating a page. Examples:
- Blank
- Dotted grid
- Ruled lines
- Daily planner (time slots drawn as background)
- Custom image/PDF background

Templates are **fixed-size backgrounds** — user draws on top. Infinite canvas extends beyond template bounds as blank space. User can zoom infinitely into any area (draw pictures inside pictures inside pictures).

**Grid templates** are special — they can optionally tile/repeat as user scrolls ("autoscroll safe"). User can also request a grid "reload" at current zoom level. All other templates remain fixed size.

### Notebook Template (Planner)
Defines automatic page generation structure for calendar-based notebooks. Specifies:
- What page templates appear at year start
- What page templates go before each month
- What page templates go before each week
- What page templates for each day/group of days

Has a **creation date** as anchor. All dates after creation date have deterministic page structure. User can navigate to any future/past date and find the correct pages.

**Day-of-week selector:** Each daily-level template specifies which days it covers. Examples:
- "daily template" → Mon, Tue, Wed, Thu, Fri (one page per day)
- "weekend spread" → Sat+Sun (single page, both dates navigate here)

Navigating to Saturday OR Sunday → lands on same weekend spread page.

Example planner structure:
```
Year Start: [yearly goals template, yearly overview template]
Before Each Month: [month cover template, month calendar template]
Before Each Week: [week review template, week planning template]
Weekdays (Mon-Fri): [daily template]
Weekend (Sat+Sun): [weekend spread template]
```

### Template Creator
Built-in tool to:
- Design templates visually (place grid/lines/regions)
- Import image as template background
- Import PDF page as template background
- Define template metadata (name, description, default zoom)

---

## UI Layout

```
┌─────────────────────────────────────────────────────┐
│ Header Bar (notebook name, tools, navigation)       │
├──────────┬──────────────────────────────────────────┤
│ Sidebar  │                                          │
│ (collaps)│                ╔══════════════════╗      │
│          │                ║ ≡ | B H E e V |● Width ║ │
│ ┌Section │                ╚══════════════════╝      │
│ │ Page 1 │        Infinite Canvas                   │
│ │ Page 2 │        (DrawingArea + Cairo)             │
│ │ Page 3 │                                          │
│ └        │   ← Floating pen toolbar: drag the ≡    │
│ ┌Section │     grip handle to reposition anywhere. │
│ └(closed)│     Position persisted across restarts. │
│          │                                          │
└──────────┴──────────────────────────────────────────┘
```

Sections in sidebar are collapsible — pages only shown when section expanded.
Home screen (no notebook open) shows notebook grid/list.

---

## User Flows

### Launch App
1. See list of notebooks (home screen)
2. Select notebook → sidebar shows sections (collapsed)
3. Expand section → see pages
4. Select page → open infinite canvas

### Create Page
1. In a section, click "New Page"
2. Prompted: choose template (filtered by notebook + section settings) or blank
3. Page created, opens in canvas

### Navigate Planner Notebook
1. Open planner notebook → lands on today's page
2. Can browse forward/back through generated pages
3. Past dates accessible even if never opened (pages generated on-demand)
4. Pages within each period follow defined template order

### Manage Templates
1. App has template management area
2. Create new template: visual editor or import image/PDF
3. Templates available globally, assigned to notebooks

---

## Phase 1: Foundation — Canvas + Drawing ✅

**Goal:** Single infinite canvas with stylus drawing, pan/zoom.

- [x] Cargo workspace setup (5 crates)
- [x] `journal-core`: Stroke, StrokePoint, Viewport, PenSettings types
- [x] `journal-canvas`: GTK4 DrawingArea + Cairo (Skia/GLArea attempted, dropped — see CLAUDE.md)
- [x] `journal-canvas`: Stroke rendering (per-segment pressure-variable width via line_width)
- [x] `journal-canvas`: Infinite pan/zoom viewport (`ViewportTransform`)
- [x] `journal-app`: Window with DrawingArea canvas
- [x] `journal-app`: Stylus input (GestureStylus — pressure, tilt)
- [x] `journal-app`: Touch pan + pinch zoom (mode-locked GestureZoom: 12px drift → pan, 8% scale → zoom)
- [x] `journal-app`: Basic pen toolbar (ColorDialogButton, width Scale)
- [x] Mouse + middle-drag pan + ctrl-scroll zoom (desktop fallbacks)

**Milestone:** Draw with stylus, pan/zoom, infinite canvas. No save yet. ✅

---

## Phase 2: Persistence + Notebook Structure ✅

**Goal:** Save/load, notebook → section → page hierarchy.

- [x] `journal-storage`: SQLite schema (notebooks, sections, pages, strokes, page_templates) with `PRAGMA user_version` migration
- [x] `journal-storage`: Binary stroke point packing (versioned bincode blob)
- [x] `journal-storage`: R-tree spatial index for viewport culling (strokes_rtree virtual table)
- [x] `journal-storage`: Page ordering within sections + cross-section move
- [x] `journal-app`: Notebook list view (home screen)
- [x] `journal-app`: Section sidebar within notebook
- [x] `journal-app`: Page list within section
- [x] `journal-app`: Create notebook / section / page dialogs
- [x] `journal-app`: Drag-reorder pages and sections (dedicated drag-handle icons; cross-section page move appends to destination)
- [x] `journal-app`: Page + section rename (long-press OR double-click on label)
- [x] `journal-app`: Auto-save on stroke completion

**Milestone:** Create notebooks with sections and pages. Data persists. ✅

---

## Phase 3: Page Templates ✅

- [x] `journal-templates`: Template data model (background type, metadata)
- [x] `journal-templates`: Built-in templates (blank, dotted, ruled, grid, daily-planner placeholder)
- [x] `journal-templates`: TOML template definition format with schema_version + load_dir registry
- [x] `journal-canvas`: Render template backgrounds behind strokes (Cairo-based)
- [x] `journal-app`: Template picker when creating new page
- [x] `journal-app`: Auto-fit viewport to page on template load (when `tiling = None`)

## Phase 3.5: Template polish ✅

- [x] `journal-app`: Notebook settings — assign available templates (gear button in header)
- [x] `journal-app`: Section settings — limit templates (gear button per section, "inherit notebook" toggle)
- [x] `journal-app`: Template management area (list + delete user templates; built-ins protected)
- [x] `journal-app`: Import image as template background (via `gdk_pixbuf` → Cairo `ImageSurface` cache)
- [ ] `journal-app`: Import PDF page as template background (deferred — needs poppler bindings) — **GH [#1](https://github.com/Sniper7Kills-LLC/Journal/issues/1)**
- [x] ~~`journal-app`: Basic template creator (deferred)~~ — superseded by Phase 3.7 full-screen template editor

---

## Phase 3.6: Planner Widgets ✅

- [x] `journal-core`: Added 4 new `WidgetKind` variants:
  - `BigThree` — three numbered priority boxes stacked vertically (Full Focus daily layout)
  - `PriorityList { count: u32 }` — A/B/C priority letter column + sequence number column + checkbox/write-line rows (Franklin Planner style)
  - `DailyAppointments { start_hour, end_hour }` — two-column hourly schedule with hour labels and half-hour tick marks (Franklin/Full Focus standard)
  - `WeeklyCompass` — 4×2 grid of labeled role/goal boxes for weekly planning (Franklin Covey concept)
- [x] `journal-canvas`: Cairo renderers for all 4 new widget kinds (`draw_big_three`, `draw_priority_list`, `draw_daily_appointments`, `draw_weekly_compass`)
- [x] `journal-app`: Template creator palette entries: "Big Three", "Priority List", "Day Schedule", "Weekly Compass"; defaults: PriorityList{count:12}, DailyAppointments{7–19}, BigThree, WeeklyCompass
- [x] `journal-templates`: Two new built-in page templates (IDs `…000006` and `…000007`):
  - **Full Focus Daily**: BigThree top 30%, DailyAppointments 7–19 bottom-left 60%, Checklist (after-action review) bottom-right
  - **Franklin Daily**: Date TextBlock header, PriorityList×14 left half, DailyAppointments 7–21 right half

---

## Phase 4: Notebook Templates (Planner Auto-Generation) ✅

- [x] `journal-core`: NotebookTemplate with `grouping` (Month|Week), `page_title_format`, `section_title_formats`
- [x] `journal-core`: Section gains `parent_section_id` for hierarchy
- [x] `journal-storage`: schema migrations v3 (parent col) + v4 (idempotent re-ALTER)
- [x] `journal-storage`: `find_page_by_address`, `ensure_section`, `list_root_sections`, `list_child_sections`
- [x] `journal-templates`: NotebookTemplateRegistry with builtins + `load_dir`; title-format engine (`{year}/{month}/{month_name}/{week}/{day}/{weekday}/{date}`)
- [x] `journal-app`: Create planner notebook dialog (name + template + grouping dropdown + creation_date Calendar)
- [x] `journal-app`: Cloned NotebookTemplate persisted to TOML at `XDG_DATA_HOME/journal/notebook_templates/`
- [x] `journal-app`: Calendar navigation strip (Prev/Today/[date popover Calendar]/Next)
- [x] `journal-app`: Auto-land on today's page when opening planner; sidebar refresh after every date nav
- [x] `journal-app`: Hierarchical sidebar — Year section → Month-or-Week wrapper → daily pages, recursive expanders
- [x] `journal-app`: No-page placeholder canvas (drawing disabled until a page is selected)
- [x] ~~`journal-app`: Notebook template editor — full version (deferred; minimal stub exists)~~ — superseded by Phase 5.7 drag-drop editor

**Milestone:** Yearly planner. Navigate any date. Pages auto-generate. ✅

---

## Phase 5: Polish + Tools ✅

- [x] Undo/redo (Ctrl+Z, Ctrl+Shift+Z)
- [x] Eraser tool — stroke-level (partial-mode deferred)
- [x] Selection tool — lasso polygon, bbox-containment selection, move via drag (resize deferred)
- [x] Highlighter — opacity 0.35, base_width × 4, BlendMode::Multiply
- [x] Page thumbnails in sidebar (40×52 cached ImageSurface per page)
- [x] PDF export of active page (Cairo PdfSurface)
- [x] Dark mode — follows system preference automatically via `adw::StyleManager` (libadwaita portal-based detection, works across GNOME/KDE/Hyprland)
- [x] Keyboard shortcuts (B/H/V/Ctrl+E/Ctrl+Z/Delete/Esc/Ctrl+0/+/-/F11)
- [x] Floating, draggable pen toolbar (position persisted across restarts)
- [x] Visual identity refresh — deep indigo + amber accent theme, warm cream/dark canvas background, wordmark headerbar title, hover/active hit-feedback CSS on page rows + drag handles
- [x] Touch-target enlargement — sidebar drag handles 36×44, page rows 48px min height, toolbar grip 36×44
- [x] Toolbar tool buttons get tiny mnemonic letter labels (B/H/E/e/V) so users learn the shortcuts without hunting tooltips
- [x] Home screen card grid (`FlowBox`) with kind badge + subtitle; centred empty state with icon + CTA when zero notebooks
- [x] Sidebar visual hierarchy — `.section-header-label` heavier/larger, nested sections get a left accent border via `.section-nested`
- [x] Sidebar page thumbnails bumped 40×52 → 60×78 (`crate::thumbnail::THUMB_W/THUMB_H` reused as the single source of truth)
- [x] Header `?` button → keyboard cheat-sheet popover with `<kbd>`-styled shortcuts pulled from `shortcuts.rs`
- [x] Template editor: Ctrl+S to save, "Saved ✓" inline indicator, friendlier palette labels ("Grid Area", "Ruled Lines", "Dot Grid", "Calendar Month"), variable popover gains live "Today → …" preview row + grouped header
- [x] Floating toolbar shrunk to a single ~36px-tall row — mnemonic letter labels removed (tooltips still carry the shortcut), tool buttons compact (28px), width slider trimmed (120px), grip handle compact (20×32 vertical-dots icon)
- [x] Sidebar page rows: dropped the dedicated drag handle (entire row is now a drag source), reduced row height (36px min), thumbnails shrunk to 36×48; current page indicated by an amber left-edge accent + tinted background
- [x] Inline rename for sections AND pages — double-click the label, an `Entry` swaps in via a `Stack`, Enter commits, Esc cancels, focus-leave commits. No more modal `prompt_rename` popup
- [x] Section row: dropped its drag handle too; the entire header is the drag source; double-click the section label to rename inline
- [x] Home screen has one "New notebook" button — clicking it pops a small chooser dialog with two cards (Notebook / Planner) instead of two separate header buttons
- [x] Template manager rebuilt: 2-tab `Stack` + `StackSwitcher` (Page Templates / Notebook Templates). Page rows now show a real Cairo-rendered preview (`build_template_preview`) instead of a generic icon, and rows are grouped by category via `ListBox::set_header_func` (built-ins ship with `Basics` / `Daily Planner` categories; imports go to `Imported`)
- [x] `PageTemplate` gains a `category: String` field (`#[serde(default)]`); the template editor's metadata row gains a Category Entry next to Name/Description
- [x] Notebook template manager: list user + built-in notebook templates with delete (built-ins protected); `NotebookTemplateRegistry::remove` + `is_builtin_notebook_template` added; "New notebook template…" button moved out of the home header and into the manager
- [x] Planner notebooks lock down free-form structure — `+ New Section` and per-section `+ New Page` buttons hidden when the open notebook is a `NotebookKind::Planner`; pages are auto-generated by date navigation only. Empty-state copy adjusts to "Pages appear here as you navigate to dates above."
- [x] Floating pen toolbar collapse/expand — chevron button at the right end hides all widgets except the drag handle and a single active-tool icon; collapsed state persisted to `toolbar_collapsed` in `config.toml`; clicking the collapsed icon or chevron re-expands; a tick callback keeps the active-tool icon in sync with the current tool.

## Phase 4.5 finish ✅ (PDF import deferred)

- [x] Configurable no-page placeholder image + text via `~/.config/journal/config.toml`; settings dialog on home
- [x] Full notebook template editor (name, description, grouping, page title format, year/month/week section formats, daily slots with day-of-week toggles + page template picker, add/remove slots; persisted to disk)
- [ ] PDF template background import (deferred — poppler-rs crate compatibility with current gtk4-rs/glib generation needs verification; libpoppler-glib is available system-side) — **GH [#1](https://github.com/Sniper7Kills-LLC/Journal/issues/1)**

## Phase 5.5: Nav + Editing polish

- [x] Planner nav year-progress bar — 6px Cairo `DrawingArea` below the Prev/Today/Date/Next row; shows fraction of year elapsed with indigo fill, month tick lines, and click-to-jump (clicking a position in the bar navigates to that day). Re-draws on every date change.
- [x] Right-click context menu on page rows (non-planner notebooks only) — `PopoverMenu` with Duplicate page (clones page + strokes with fresh UUIDs, positions immediately after the original, auto-loads the copy) and Delete page (confirmation modal, then `page_store::delete_page`).
- [x] Notebook template editor now supports editing existing templates — `prompt_notebook_template_editor(parent, state, edit: Option<NotebookTemplate>, on_save)` pre-populates all fields when `edit = Some(...)`, rewrites the TOML file and updates the in-memory registry with the same id. `prompt_new_notebook_template` becomes a thin wrapper with `edit = None`. Edit button (`document-edit-symbolic`) added to user notebook template rows in the template manager.

## Phase 3.7: Template Editor Polish ✅

- [x] Template editor is now a full-screen stack page (`TEMPLATE_EDITOR_NAME`) — no longer a modal `Window`. Opens from "Templates" → "New template…"/"Edit"; back/save returns to wherever the user came from (home or notebook canvas).
- [x] Properties side panel rebuilds dynamically when the selection changes (driven by `add_tick_callback` watching `selected_idx`):
  - Stroke colour picker (`ColorDialogButton`)
  - Fill colour picker + on/off `Switch`
  - Stroke width spinner (mm)
  - Per-kind editors: text + font size + variable popover for `TextBlock`; thickness for `Line`; spacing for grid/lines/dots regions; start/end hour for `Timeline` and `DailyAppointments`; row count for `PriorityList`; pipe-separated items for `Checklist`
- [x] `WidgetKind::TextBlock` text now runs through `journal_core::title_format::render` so `{date}/{weekday}/{month_name}/{year}/{week}/{day}/{month}` expand at draw time. The template editor preview binds today's date; the planner canvas binds the page's calendar date.
- [x] `title_format` engine moved from `journal_templates` to `journal_core` (re-exported from `journal_templates` for back-compat) so `journal_canvas` can call it without a circular dep.
- [x] Variable insertion popover in the editor: pick `{date}`, `{year}`, `{month}`, `{month_name}`, `{week}`, `{day}`, `{weekday}` and it inserts at the entry caret.
- [x] Template editor undo/redo (Ctrl+Z / Ctrl+Shift+Z) — `EditorHistory` with Insert / Remove / Move / Resize / **Modify** ops. Modify captures `before` + `after` widget snapshots on every property edit; consecutive Modify ops on the same widget coalesce so a slider drag is one undo, not 50.
- [x] Template editor snap-to-grid — `snap_grid_mm: Option<f64>` in `CreatorState`; `Switch` + `SpinButton` in the editor top row; all drag-place, drag-move, and drag-resize endpoints are grid-snapped when enabled.
- [x] Template editor smart guides — while dragging a widget, amber guide lines are rendered in `draw_creator_canvas` wherever the dragged widget's left/right/top/bottom/center aligns within 1.5 mm of another widget's edges or the page edges; `apply_smart_snap` also adjusts the widget's position to that edge. Toggle via "Smart guides" Switch in the top row.
- [x] Template editor selection refresh via observer signal — replaced `add_tick_callback` with a `SelectionObserverFn` (`Rc<dyn Fn(Option<usize>)>`) stored in a separate `Rc<RefCell<Option<...>>>` outside `CreatorState`. Registered in `build_editor_view`; fired from all call sites that change `selected_indices`.
- [x] Template editor **multi-select** — `selected_indices: HashSet<usize>` replaces the old `selected_idx: Option<usize>`. Plain click replaces; Ctrl-click toggles; Shift-click extends. Drag-move applies to every selected widget. Resize handles and the props panel restrict to single-select. Delete removes the entire set in one undo op.

---

## Phase 5.6: Quick wins — multi-select, exports, presets, more builtins ✅

- [x] **Notebook → PDF export** (`pdf_export::export_notebook_to_pdf`) walks every section + child section depth-first, in `position` order, rendering each page (background + widgets + strokes) into a multi-page Cairo `PdfSurface`. Triggered from a new "Export notebook as PDF…" entry in the header menu, sensitive only when a notebook is open.
- [x] **Stroke copy/paste** — `state::CanvasState::stroke_clipboard: Vec<Stroke>`. Ctrl+C snapshots the selected strokes; Ctrl+V mints fresh UUIDs, offsets each by ~10 canvas units, persists via `backend.insert_stroke`, pushes Add ops to the per-page history, and selects the new ids so the user can drag them.
- [x] **Custom pen presets** — `config::PenPreset { name, color_rgba, width_mm }` persisted in `~/.config/journal/config.toml`. The floating toolbar renders a row of 28×28 colored chips between the tool buttons and the colour picker; clicking a chip switches the pen to that preset. App settings → "Pen presets" → "Manage presets…" opens a dialog with rename / re-color / re-width / reorder / delete / "Add current pen" actions.
- [x] **More built-in templates** (Phase 5.6 builtins): `Franklin Weekly` (weekly-compass left + 7 day-blocks right), `Monthly Goals` (calendar + 12-row priority list + reflection lines), `Quarterly Review` (3 month-strips + 9-row wins/lessons/next list). Categories `Weekly Planner` / `Monthly Planner` / `Quarterly Planner`.

---

## Phase 5.7: Drag-drop notebook-template editor ✅

- [x] **`EntryFlags` data model** — `journal_core::EntryFlags { bridge_previous: bool, bridge_next: bool }` added to `journal_core::template`. New `entry_options: HashMap<String, EntryFlags>` field on `NotebookTemplate` with `#[serde(default)]` so existing TOML files load unchanged. Keys are `"year_start:N"`, `"before_quarter:N"`, `"before_month:N"`, `"before_week:N"`, `"daily:S:N"`. Planner runtime does not yet act on these flags — they are persisted and surfaced in the editor; bridge-rendering is a future phase.
- [x] **Full-screen stack-page editor** (`crates/journal-app/src/notebook_template_creator.rs`) — mirrors `template_creator::build_editor_view`. Outer `GtkBox` (vertical) with: top action row (Back + title + Saved ✓ indicator + Save), meta row (Name / Description / Grouping / Page title format / Year-Month-Week section formats), three-pane `Paned`.
- [x] **Palette pane** (left, ~200px) — `ScrolledWindow` listing every available `PageTemplate` as a chip with a coloured swatch and template name. Each chip is a `gtk4::DragSource` (COPY) whose payload is `"page-template:{uuid}"`.
- [x] **Slots pane** (middle, flex) — `ScrolledWindow` with sections for Year start / Before each quarter / Before each month / Before each week (each a `FlowBox` drop target), then a "Daily slots" section with an "+ Add daily slot" button. Each daily slot has weekday `ToggleButton` chips (Mon–Sun), a `FlowBox` drop target for page-template chips, and a "Remove slot" button.
- [x] **Options panel** (right, ~260px) — when a chip is clicked, shows slot label, "Bridge to previous period" `Switch`, "Bridge to next period" `Switch`, and a hint that bridge-rendering is deferred. Shows placeholder text when no chip is selected.
- [x] **Drag-and-drop wiring** — `DropTarget::new(Type::STRING, DragAction::COPY)`; `connect_drop` parses `"page-template:{uuid}"`, pushes `TemplateId` into the slot vec, rebuilds the `FlowBox`. `connect_enter`/`connect_leave` toggle `.drag-target` CSS class for visual feedback. `DragSource::connect_prepare` returns `ContentProvider::for_value`.
- [x] **Per-entry options** — clicking a slot chip sets `EditorState::selected_key` and rebuilds the options panel. Bridge switches write to `template.entry_options` via `HashMap::entry(...).or_default()`. On chip removal, `renumber_flat_keys` / `renumber_daily_keys` keep keys aligned with Vec indices.
- [x] **Save / Back** — Save calls `dialogs::persist_notebook_template` (made `pub(crate)`) + `notebook_templates.borrow_mut().insert(...)`, shows "Saved ✓" for 450 ms, then returns via `on_done`. Back returns immediately.
- [x] **Window integration** — `window.rs` gains `NOTEBOOK_TEMPLATE_EDITOR_NAME` constant, `notebook_template_editor_container: GtkBox`, and `show_notebook_template_editor(win, edit)` function (mirrors `show_template_editor`). `build_home_into` registers `show_notebook_template_editor` as the opener via `template_manager::set_nb_editor_opener` (thread-local slot). The template manager's notebook-template Edit and New buttons route through the stack-page editor if an opener is registered, falling back to the modal (`prompt_notebook_template_editor`) otherwise.
- [x] **Back-compat** — `dialogs::prompt_new_notebook_template` and `dialogs::prompt_notebook_template_editor` are kept unchanged and still working.
- [x] **Unit tests** — 3 new tests for `EntryFlags` serde round-trip, default-all-false, and empty-TOML default deserialization.

---

## Future (Not Now)

- [ ] Calendar integration (Google Calendar, iCal) — display events on template areas — **GH [#15](https://github.com/Sniper7Kills-LLC/Journal/issues/15)**
- [ ] Storage offloading — archive old notebooks to external storage — **GH [#16](https://github.com/Sniper7Kills-LLC/Journal/issues/16)**
- [ ] Handwriting recognition / search — **GH [#17](https://github.com/Sniper7Kills-LLC/Journal/issues/17)**

---

## Phase 6: Storage Abstraction + Optional Server Backend

The Linux client stays native (GTK4) — this phase is about making the
**storage layer pluggable** so a hosted server can back templates first, and
notebooks/strokes later, without touching the canvas or UI code.

Existing rejected scope: the **client itself stays native**. Web is only for
the optional template-sharing portal, not for the journal app.

### 6.1 Trait-based storage abstraction ✅

- [x] `journal-storage::backend`: traits per store — `NotebookStore`, `SectionStore`, `PageStore`, `StrokeStore`, plus the aggregator `JournalBackend: NotebookStore + SectionStore + PageStore + StrokeStore`. `PlannerQueries` adds default-impl `pages_in_date_range` for future remote pushdown. No `Connection` in any signature.
- [x] `journal-storage::sqlite_backend::SqliteBackend` wraps `Db` and delegates each method to the existing free functions in the `*_store` modules (back-compat retained).
- [x] `journal-app` holds `Rc<RefCell<dyn JournalBackend>>` instead of `Rc<RefCell<Db>>` (`state::CanvasState.backend`); ~50 call sites migrated from `db.borrow().conn(); store::fn(conn, ...)` to `backend.borrow_mut().fn(...)`. Planner helpers (`ensure_planner_pages`, `min_day_date_in_section`, `reorder_sections_chronologically`, `chronological_target_position`) take `&mut dyn JournalBackend`.
- [x] `StorageError` gains `Network` / `Auth` / `Conflict` variants reserved for the future remote backend.

### 6.2 Local backends ✅

- [x] **SQLite (single file)** — original layout, kept as the `SqliteBackend` impl in case anyone wants a one-file backup.
- [x] **File-per-notebook `.journal`** — `MultiFileSqliteBackend`. Each notebook lives in its own self-contained SQLite file (`journals/{id}.journal`); a small `index.db` catalogues them for fast listing. Pre-existing single-file dbs migrate automatically on first boot (renamed `journal.db.legacy`). Per-process caches (`section_to_notebook`, `page_to_notebook`, `stroke_to_notebook`) route id-only operations without scanning every file. Cross-notebook page moves are explicitly rejected.
- [x] ~~**File-per-notebook `.journal`** — revisit the original PLAN.md design~~ — duplicate of the bullet above; shipped as `MultiFileSqliteBackend`.

### 6.3 Remote template backend (first network feature) — AWS Amplify

Templates are the lowest-risk thing to host: small TOML blobs, no per-stroke
write traffic, valuable to share. We do **not** roll our own server — the
backend is **AWS Amplify** (Cognito + AppSync/REST + DynamoDB + S3).

- [ ] AWS infra (Amplify project alongside the repo, e.g. `amplify/`) — **GH [#4](https://github.com/Sniper7Kills-LLC/Journal/issues/4) (CDK stack), [#5](https://github.com/Sniper7Kills-LLC/Journal/issues/5) (AppSync schema)**
- [ ] `journal-storage`: add `RemoteTemplateStore` impl — **GH [#6](https://github.com/Sniper7Kills-LLC/Journal/issues/6)**
- [ ] `journal-app`: settings pane to log in / log out / pick "sync templates" toggle. Template manager grows tabs for "Local", "My (synced)", "Public". — **GH [#7](https://github.com/Sniper7Kills-LLC/Journal/issues/7)**

### 6.4 Web template portal — Amplify Hosting

A browser UI for users to **design, manage, share, and browse** both page
templates and notebook templates without launching the desktop app. Hosted
on **Amplify Hosting** (static SPA against the same AppSync API + Cognito).
React/Vue is acceptable here because it's a **separate web property** for
template authoring/sharing, not the journal app itself. The "no web client
for the journal" rule still holds — **drawing on a page** stays native;
**designing the empty layout** is fair game in the browser.

- [ ] **Browse/share** — list public templates with Lambda-rendered PNG
  previews, fork to "my templates". Authenticated users can rename /
  set visibility / delete their own. — **GH [#8](https://github.com/Sniper7Kills-LLC/Journal/issues/8), [#9](https://github.com/Sniper7Kills-LLC/Journal/issues/9)**
- [ ] **Page template designer** — drag-and-drop editor mirroring the
  native template editor: a widget palette (TextBlock, Rectangle, Ellipse,
  Line, Grid/Lines/Dots Region, CalendarMonth, Timeline, Checklist,
  BigThree, PriorityList, DailyAppointments, WeeklyCompass), canvas with
  page outline + drag-place / drag-move / drag-resize, properties panel
  (stroke/fill colour, width, per-kind controls, text-variable insertion).
  Output is the **same TOML schema** consumed by the native client
  (`schema_version = 1`, `widgets = [...]`) so a template designed on the
  web loads unchanged on the desktop. — **GH [#10](https://github.com/Sniper7Kills-LLC/Journal/issues/10)**
- [ ] **Notebook template designer** — drag-and-drop editor for planner
  structure: define `year_start`, `before_quarter`, `before_month`,
  `before_week` slots (each takes an ordered list of page templates,
  picked from the user's library by drag-and-drop); define `daily_slots`
  (day-of-week multi-select chips + ordered page-template list); pick
  `grouping = Month | Week`; edit `page_title_format` +
  `section_title_formats` with a live preview that uses tomorrow's date.
  Output matches the native notebook-template TOML at
  `~/.local/share/journal/notebook_templates/`. — **GH [#11](https://github.com/Sniper7Kills-LLC/Journal/issues/11)**
- [ ] **Schema parity guarantee** — page-template + notebook-template
  schemas live in `journal-core` (already true for page templates via
  `journal_core::template`). The web SPA fetches a versioned JSON schema
  from a Lambda endpoint to render its forms, so adding a new
  `WidgetKind` variant on the desktop automatically becomes available
  in the web designer's palette without a separate web release. — **GH [#13](https://github.com/Sniper7Kills-LLC/Journal/issues/13)**
- [ ] **Render preview** — the web designer renders previews client-side
  via a small TypeScript port of `widget_renderer` against an HTML5
  `<canvas>`. Same coord system, same default sizes, same `title_format`
  expansion (port of `journal_core::title_format`). Lambda-rendered PNG
  remains the source of truth for thumbnails (server side, headless
  Cairo) so browse-list previews match the native client byte-for-byte. — **GH [#3](https://github.com/Sniper7Kills-LLC/Journal/issues/3) (WASM build), [#12](https://github.com/Sniper7Kills-LLC/Journal/issues/12) (viewer + QR share)**
- [ ] **Out of scope:** drawing on a page (strokes, stylus input, ink) —
  that stays on the native client, full stop.

### 6.5 Remote notebook/stroke backend (later, gated on 6.3)

Same Amplify stack scaled to notebooks/strokes — tracked as a single
umbrella issue **GH [#14](https://github.com/Sniper7Kills-LLC/Journal/issues/14)**:

- [ ] DynamoDB tables: `Notebook`, `Section`, `Page`, `Stroke`
- [ ] Sync engine with conflict resolution (last-writer-wins on append-only strokes; CRDT for page reorder)
- [ ] End-to-end encryption option (server stores ciphertext)
- [ ] Multi-device sync
- [ ] Collaborative notebooks via AppSync subscriptions

---

## Phase 7: Productization

After the web POC + desktop bookmarks land, the project shifts from
"working in isolation" to "shippable product". Seven gating steps in
roughly the order they should happen — each tracked as a top-level GH
issue so the work splits cleanly across sessions.

### 7.1 Final product name

- [ ] Decide on a single brand. Today the project lives under
  "Journal" + the `dev.s7k.journal` reverse-DNS id; that's a working
  title. Inputs: domain availability, conflict with existing apps
  (rnote / Joplin / Logseq / Obsidian / Notability / OneNote),
  trademark search, social-handle availability, .desktop AppId
  rename cost, GitHub repo rename cost. Once chosen: rename the repo,
  update Cargo metadata, freedesktop AppId, README, marketing doc.
  Tracked as **GH [#39](https://github.com/Sniper7Kills-LLC/Journal/issues/39)**.

### 7.2 Web ↔ Desktop integration

The web POC at `web/` and the desktop binary share `journal-core` /
`journal-templates` / `journal-canvas` / `journal-widgets` /
`journal-web-shim` / `journal-web-viewer` — but the round-trip
(designer ↔ desktop) has only been validated on a handful of
templates. This phase exhaustively integrates and tests them:

- [ ] Schema-parity CI for page-template + notebook-template + brush
  TOML round-trip (both desktop ↔ web SPA build artefacts).
- [ ] Visual regression harness for the web viewer against the
  desktop's rendered PNG (golden corpus).
- [ ] End-to-end: create template in Templeter → download TOML →
  drop in `~/.local/share/journal/templates/` → desktop loads,
  renders, edits, saves → re-export → diff against original.
- [ ] Same for brushes (Tooler → `brushes.toml`).
- [ ] Same for notebook templates (needs `serialize_notebook_template_toml`
  in `journal-web-shim` first — currently the Gallery emits JSON).

Tracked as **GH [#40](https://github.com/Sniper7Kills-LLC/Journal/issues/40)**.

### 7.3 Polish + publish

- [ ] UX audit pass on every web route (Viewer / Designer / Templeter
  / Tooler / Gallery). Smart-guides, undo/redo coverage, keyboard
  shortcuts, accessibility, dark-mode parity.
- [ ] Desktop polish backlog (bookmarks panel position, sidebar
  chrome, dialog modality, error toasts).
- [ ] Hosting decision for the web SPA: Amplify Hosting vs. static
  S3 + CloudFront vs. Vercel/Netlify (Amplify ties to the eventual
  backend, see #4).
- [ ] First public deploy of the SPA at the chosen domain (post-7.1).

Tracked as **GH [#41](https://github.com/Sniper7Kills-LLC/Journal/issues/41)**.

### 7.4 Feedback collection

- [ ] In-app feedback widget — "Send feedback" header item with text
  + optional screenshot, posts to a Lambda that lands in a
  DynamoDB / SES inbox.
- [ ] Public feedback channel: GitHub issues template + a triage
  rotation, plus an optional Discord / Discussions destination
  for non-bug ideation.
- [ ] Telemetry opt-in (anonymous): session counts, route hits,
  brush + template downloads. Strictly opt-in, off by default,
  documented in privacy doc.

Tracked as **GH [#42](https://github.com/Sniper7Kills-LLC/Journal/issues/42)**.

### 7.5 Multi-OS packaging

Originally out of scope (Linux-first); now in scope.

- [ ] Linux: AppImage, Flatpak, .deb, .rpm. Existing `Makefile` /
  `install.sh` handles `.desktop` + binary install — extend to
  one-shot Flatpak/AppImage builds.
- [ ] macOS: bundle (`.app` + DMG), code-signing, notarization. wgpu
  on macOS uses the Metal backend — already supported by Vello, but
  CI needs a macOS runner.
- [ ] Windows: MSI installer, code-signing certificate.
- [ ] CI matrix: every push builds artefacts for all three OSes;
  release tags publish to a downloads page.
- [ ] Auto-update: `cargo-dist` covers macOS + Windows; Linux
  packages handle their own updaters via the distro repo.

Tracked as **GH [#43](https://github.com/Sniper7Kills-LLC/Journal/issues/43)**.

### 7.6 Paid plans

Needs a design pass *before* implementation. The product ships as
"works-locally-free, pay-for-cloud-extras" — the desktop + offline
notebooks stay free forever; paid tier unlocks the Amplify-backed
features behind a Cognito user.

Provisional tier sketch (subject to design, not committed):

- **Free** — local desktop, local notebook storage, browse public
  Gallery content, fork / download to local.
- **Plus** — sync own templates + brushes across devices, publish
  templates / brushes to Gallery (with rate limit), 5 GB notebook
  storage on the cloud backend.
- **Pro** — increased notebook storage (50 GB), live sharing of
  notebooks (read-only links), longer share-link expiry.
- **Team** — collaborative editing, shared notebooks across
  multiple Cognito accounts, SSO.

Open questions to design before any code:

- [ ] Pricing strategy (one-off vs. monthly, USD anchor, regional
  pricing).
- [ ] Storage accounting (per-byte vs. per-stroke vs. per-page).
- [ ] Live-sharing model (read-only viewer first; collaborative
  edit needs CRDT work — gates on #14).
- [ ] Payment processor (Stripe is the obvious pick; tax + invoicing
  scope creep matters).
- [ ] Plan switching, downgrade pathway (data ownership when a user
  drops below a paid tier).
- [ ] EU/UK/US regulatory surface (VAT, sales tax, GDPR data
  retention).

Tracked as **GH [#44](https://github.com/Sniper7Kills-LLC/Journal/issues/44)**.

### 7.7 Informational / marketing site

- [ ] Marketing landing page on the chosen domain (post-7.1) — clean
  hero, "what is it" copy, screenshots, downloads link, link to the
  web Gallery.
- [ ] Documentation: getting-started, daily workflow, planner setup,
  brush authoring, troubleshooting. Likely a static site
  (Astro / Vitepress / Docusaurus) sharing the same domain.
- [ ] Pricing page (gates on 7.6 design).
- [ ] Privacy + Terms pages (gate on 7.4 telemetry decisions and
  7.6 payments).
- [ ] Press kit (logo PNG/SVG, screenshots, contact link).

Tracked as **GH [#45](https://github.com/Sniper7Kills-LLC/Journal/issues/45)**.

---

## Workspace Structure

```
Journal/
├── Cargo.toml                    # Workspace root
├── crates/
│   ├── journal-app/              # GTK4 app shell, views, input
│   ├── journal-canvas/           # Cairo rendering, viewport
│   ├── journal-core/             # Domain models, business logic
│   ├── journal-storage/          # Storage traits + SQLite impl (Phase 6: + RemoteAmplifyStore impl)
│   └── journal-templates/        # Template definitions, import
├── amplify/                      # (Phase 6.3+) AWS Amplify project — Cognito auth, AppSync API, DynamoDB, S3
├── resources/                    # GTK resources, icons, UI
└── templates/                    # Built-in template files
```

---

## Data Models

### Notebook
- id, name, created_at
- kind: Standard | Planner { notebook_template_id, creation_date }
- assigned_page_templates: Vec<TemplateId>

### Section
- id, notebook_id, name, position (for ordering)
- allowed_templates: Option<Vec<TemplateId>> (None = inherit from notebook)

### Page
- id, section_id, position (for ordering)
- template_id: Option<TemplateId>
- planner_address: Option<PlannerPageAddress> (for auto-generated pages)
- created_at, modified_at

### Stroke
- id, page_id
- points: binary packed (x, y, pressure, tilt_x, tilt_y, timestamp)
- pen_settings: color, width, opacity, blend_mode
- zoom_at_creation: f64
- bounding_box: Rect

### PageTemplate
- id, name, description
- background: Blank | Dots | Lines | Grid | Image(path) | PDF(path, page)
- size: (width_mm, height_mm) — physical dimensions, default 215.9 × 279.4 (US Letter)
- tiling: None | Repeat (grids tile infinitely with hierarchical subdivision)
- default_viewport: FitPage (page fills screen on open)

### NotebookTemplate (Planner)
- id, name, description
- year_start: Vec<TemplateId>
- before_quarter: Vec<TemplateId>
- before_month: Vec<TemplateId>
- before_week: Vec<TemplateId>
- daily_slots: Vec<DailySlot>

### DailySlot
- days: Vec<DayOfWeek> (e.g., [Mon] or [Sat, Sun])
- templates: Vec<TemplateId> (pages for that slot)
- Note: Both Sat and Sun navigate to same page when grouped

---

## Dependencies

```toml
gtk4 = { version = "0.9", features = ["v4_12"] }
rusqlite = { version = "0.32", features = ["bundled", "vtab"] }
uuid = { version = "1", features = ["v4", "serde"] }
chrono = { version = "0.4", features = ["serde"] }
serde = { version = "1", features = ["derive"] }
toml = "0.8"
bincode = "1"
thiserror = "2"
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
anyhow = "1"
```

(Vello / wgpu / parley pulled in by the `vello` feature on `journal-canvas` and `journal-app`. Cairo accessed via `gtk4::cairo` re-export — used now only by `pdf_export` for vector PDF output.)

---

## Resolved Decisions

- **System color-scheme detection:** Uses `adw::StyleManager` (libadwaita 0.7) instead of `gtk4::Settings::is_gtk_application_prefer_dark_theme`. The `StyleManager` queries the XDG desktop portal so it reflects the user's OS-level dark/light preference and fires `notify::dark` when it changes, regardless of DE (GNOME, KDE, Hyprland, etc.).
- **Renderer:** Vello (GPU compute via wgpu Vulkan) drawn into a `gtk4::GLArea` — see `docs/renderer-vello-migration.md` for the migration record. Strokes / backgrounds / widgets / overlays / image / PDF / placeholder all render through `journal_canvas::vello_renderer::VelloRenderer`. `pdf_export` retains Cairo to keep PDF output vector. The original Phase 1 Cairo-on-DrawingArea path remains as a fallback when `JOURNAL_VELLO=0` is set.
- **Touch gesture mode-lock:** A two-finger gesture is locked to either pan or zoom on the first frame that crosses a threshold (12px centroid drift → pan; 8% scale change → zoom). Avoids GestureZoom's tendency to interpret minor finger-distance jitter as zoom during a pure pan.
- **Template backgrounds:** Fixed size. Canvas extends as blank beyond. Grid templates tile infinitely.
- **Template size:** Physical units (mm). Default: US Letter (215.9mm × 279.4mm). Viewport fits full page on screen by default.
- **Grid zoom behavior:** Grids overlay at zoom levels — inner grids align with outer grids. Deeper zoom = finer grid lines (lighter). Coarser grid lines stay visible (darker). Hierarchical grid that maintains alignment. **Subdivision levels customizable per-page in-app.**
- **Section tabs:** Sidebar, collapsible. Pages hidden until section expanded.
- **Multi-day spreads:** Single page with day-of-week selector. Both dates navigate to same page.
- **Infinite zoom purpose:** Draw pictures inside pictures inside pictures. Zoom-relative strokes.
- **Planner navigation:** Both linear scroll through pages AND jump-to-date picker.
- **Default viewport:** Page fits full screen width/height on open.
- **Page bounds indicator:** Non-grid templates show faint US Letter outline when zoomed in. Disableable per-page.

## Open Questions

(None currently — all resolved)
