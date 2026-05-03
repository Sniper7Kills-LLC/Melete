# Project Plan — Journal App

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
│ (collaps)│                                          │
│          │                                          │
│ ┌Section │        Infinite Canvas                   │
│ │ Page 1 │        (GLArea + Skia)                   │
│ │ Page 2 │                                          │
│ │ Page 3 │                                          │
│ └        │                                          │
│ ┌Section │                                          │
│ └(closed)│                                          │
│          │                                          │
├──────────┴──────────────────────────────────────────┤
│ (floating pen toolbar)                              │
└─────────────────────────────────────────────────────┘
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

## Phase 3: Page Templates ✅ (image/PDF import deferred)

**Goal:** Apply templates as page backgrounds, template management.

- [x] `journal-templates`: Template data model (background type, metadata)
- [x] `journal-templates`: Built-in templates (blank, dotted, ruled, grid, daily-planner placeholder)
- [x] `journal-templates`: TOML template definition format with schema_version + load_dir registry
- [x] `journal-canvas`: Render template backgrounds behind strokes (Cairo-based)
- [x] `journal-app`: Template picker when creating new page
- [x] `journal-app`: Auto-fit viewport to page on template load (when `tiling = None`)
- [ ] `journal-app`: Notebook settings — assign available templates (deferred)
- [ ] `journal-app`: Section settings — further limit templates (deferred)
- [ ] `journal-app`: Template management area (list, preview) (deferred)
- [ ] `journal-app`: Import image as template background (deferred — needs `image` crate + Cairo surface)
- [ ] `journal-app`: Import PDF page as template background (deferred — needs poppler bindings)
- [ ] `journal-app`: Basic template creator (deferred)

**Milestone:** Create pages with template backgrounds. ✅ (Custom background import deferred to Phase 3.5.)

---

## Phase 4: Notebook Templates (Planner Auto-Generation)

**Goal:** Calendar-based notebooks with automatic page structure.

- [ ] `journal-core`: NotebookTemplate struct (period → page template mappings)
- [ ] `journal-core`: Planner page address resolution (date → sequence of pages)
- [ ] `journal-core`: Creation date anchor + deterministic page ordering
- [ ] `journal-storage`: Lazy page creation from planner address
- [ ] `journal-app`: Create planner notebook (select notebook template)
- [ ] `journal-app`: Notebook template editor (define period structure)
- [ ] `journal-app`: Calendar navigation (today, prev/next day/week/month)
- [ ] `journal-app`: Auto-land on today's page when opening planner
- [ ] `journal-app`: Browse past/future dates (pages generate on demand)

**Milestone:** Create yearly planner. Navigate to any date. Pages auto-generate from template structure.

---

## Phase 5: Polish + Tools

**Goal:** Full drawing toolkit, quality of life.

- [ ] Undo/redo
- [ ] Eraser tool (stroke-level and partial)
- [ ] Selection tool (lasso, move, resize)
- [ ] Highlighter / different pen types
- [ ] Page thumbnails in sidebar
- [ ] PDF export
- [ ] Dark mode
- [ ] Keyboard shortcuts

---

## Future (Not Now)

- [ ] Calendar integration (Google Calendar, iCal) — display events on template areas
- [ ] Storage offloading — archive old notebooks to external storage
- [ ] Sync between devices
- [ ] Handwriting recognition / search
- [ ] Collaborative notebooks

---

## Workspace Structure

```
Journal/
├── Cargo.toml                    # Workspace root
├── crates/
│   ├── journal-app/              # GTK4 app shell, views, input
│   ├── journal-canvas/           # Skia rendering, viewport
│   ├── journal-core/             # Domain models, business logic
│   ├── journal-storage/          # SQLite persistence
│   └── journal-templates/        # Template definitions, import
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

(Cairo accessed via `gtk4::cairo` re-export — no separate dep.)

---

## Resolved Decisions

- **Renderer:** Cairo via `gtk4::DrawingArea`, not Skia. Phase 1 GPU Skia integration via GLArea hit Mesa/Wayland incompatibility (`direct_contexts::make_gl` returned None despite valid GL 4.6 context and resolved entry points). Cairo CPU rendering is fast enough for Phase 1 stroke counts; migrate to GSK paths (GTK 4.14+) if perf becomes a bottleneck.
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
