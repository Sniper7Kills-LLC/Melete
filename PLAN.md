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
- [ ] `journal-app`: Import PDF page as template background (deferred — needs poppler bindings)
- [ ] `journal-app`: Basic template creator (deferred)

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
- [ ] `journal-app`: Notebook template editor — full version (deferred; minimal stub exists)

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

## Phase 4.5 finish ✅ (PDF import deferred)

- [x] Configurable no-page placeholder image + text via `~/.config/journal/config.toml`; settings dialog on home
- [x] Full notebook template editor (name, description, grouping, page title format, year/month/week section formats, daily slots with day-of-week toggles + page template picker, add/remove slots; persisted to disk)
- [ ] PDF template background import (deferred — poppler-rs crate compatibility with current gtk4-rs/glib generation needs verification; libpoppler-glib is available system-side)

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

---

## Future (Not Now)

- [ ] Calendar integration (Google Calendar, iCal) — display events on template areas
- [ ] Storage offloading — archive old notebooks to external storage
- [ ] Handwriting recognition / search

---

## Phase 6: Storage Abstraction + Optional Server Backend (Planned)

The Linux client stays native (GTK4) — this phase is about making the
**storage layer pluggable** so a hosted server can back templates first, and
notebooks/strokes later, without touching the canvas or UI code.

Existing rejected scope: the **client itself stays native**. Web is only for
the optional template-sharing portal, not for the journal app.

### 6.1 Trait-based storage abstraction

Today every store module exposes free functions taking `&rusqlite::Connection`.
The `Connection` type leaks into ~49 call sites across `journal-app`, blocking
any non-SQLite backend.

- [ ] `journal-storage`: introduce traits per store —
  `trait NotebookStore`, `trait SectionStore`, `trait PageStore`,
  `trait StrokeStore`, `trait TemplateStore`, `trait NotebookTemplateStore`.
  Each method returns `Result<T, StorageError>` (no `Connection` exposed).
- [ ] `journal-storage`: split current SQLite implementation into
  `sqlite/` submodules implementing each trait against `rusqlite`.
- [ ] `journal-storage`: define `pub struct Backend { notebooks: Box<dyn NotebookStore>, ... }`
  or a single `trait JournalBackend` aggregator the app holds via
  `Rc<RefCell<dyn JournalBackend>>`.
- [ ] `journal-app`: replace direct `db.borrow().conn()` calls with trait
  calls. Drop the `rusqlite` re-export; `Db` becomes private to the SQLite
  backend impl.
- [ ] Errors stay typed via `StorageError` (already `thiserror`) — add
  variants for `Network`, `Auth`, `Conflict` for the future remote backend.

### 6.2 Local backends

- [ ] **SQLite (current)** — keep as the default offline backend.
- [ ] **File-per-notebook `.journal`** — revisit the original PLAN.md design;
  may layer over SQLite by giving each notebook its own DB file (so users can
  copy/share/back-up a single file).

### 6.3 Remote template backend (first network feature) — AWS Amplify

Templates are the lowest-risk thing to host: small TOML blobs, no per-stroke
write traffic, valuable to share. We do **not** roll our own server — the
backend is **AWS Amplify** (Cognito + AppSync/REST + DynamoDB + S3).

- [ ] AWS infra (Amplify project alongside the repo, e.g. `amplify/`):
  - **Auth:** Cognito user pool — email + password, optional federated
    (Google/Apple) later. Hosted UI not used; client opens the OAuth flow
    in the system browser via `webbrowser` + a localhost loopback redirect.
  - **API:** AppSync (GraphQL) preferred over REST — schema models
    `Template`, `User`, `Visibility { Private | Unlisted | Public }`,
    `Fork` mutation. Authorization rules per-field via Cognito groups.
  - **Storage:** DynamoDB for metadata (id, owner, name, description,
    visibility, fork_of, created_at). S3 bucket for the TOML body
    (key = `templates/{id}.toml`); lets large/binary attachments grow
    without DynamoDB row-size pain.
  - **Render service (later):** Lambda triggered on template upload that
    runs a headless `journal-canvas` Cairo pass to render a PNG preview
    into `templates/{id}.png`.
- [ ] `journal-storage`: add `RemoteTemplateStore` impl. No `reqwest` — use
  the **AWS Rust SDK** (`aws-sdk-cognitoidentityprovider`,
  `aws-sdk-dynamodb`, `aws-sdk-s3`) or the AppSync GraphQL endpoint via a
  thin `reqwest` wrapper that signs requests with SigV4 / Cognito JWT.
  Cache fetched templates locally so the editor works offline.
- [ ] `journal-app`: settings pane to log in / log out / pick "sync templates"
  toggle. Template manager grows tabs for "Local", "My (synced)", "Public".

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
  set visibility / delete their own.
- [ ] **Page template designer** — drag-and-drop editor mirroring the
  native template editor: a widget palette (TextBlock, Rectangle, Ellipse,
  Line, Grid/Lines/Dots Region, CalendarMonth, Timeline, Checklist,
  BigThree, PriorityList, DailyAppointments, WeeklyCompass), canvas with
  page outline + drag-place / drag-move / drag-resize, properties panel
  (stroke/fill colour, width, per-kind controls, text-variable insertion).
  Output is the **same TOML schema** consumed by the native client
  (`schema_version = 1`, `widgets = [...]`) so a template designed on the
  web loads unchanged on the desktop.
- [ ] **Notebook template designer** — drag-and-drop editor for planner
  structure: define `year_start`, `before_quarter`, `before_month`,
  `before_week` slots (each takes an ordered list of page templates,
  picked from the user's library by drag-and-drop); define `daily_slots`
  (day-of-week multi-select chips + ordered page-template list); pick
  `grouping = Month | Week`; edit `page_title_format` +
  `section_title_formats` with a live preview that uses tomorrow's date.
  Output matches the native notebook-template TOML at
  `~/.local/share/journal/notebook_templates/`.
- [ ] **Schema parity guarantee** — page-template + notebook-template
  schemas live in `journal-core` (already true for page templates via
  `journal_core::template`). The web SPA fetches a versioned JSON schema
  from a Lambda endpoint to render its forms, so adding a new
  `WidgetKind` variant on the desktop automatically becomes available
  in the web designer's palette without a separate web release.
- [ ] **Render preview** — the web designer renders previews client-side
  via a small TypeScript port of `widget_renderer` against an HTML5
  `<canvas>`. Same coord system, same default sizes, same `title_format`
  expansion (port of `journal_core::title_format`). Lambda-rendered PNG
  remains the source of truth for thumbnails (server side, headless
  Cairo) so browse-list previews match the native client byte-for-byte.
- [ ] **Out of scope:** drawing on a page (strokes, stylus input, ink) —
  that stays on the native client, full stop.

### 6.5 Remote notebook/stroke backend (later, gated on 6.3)

Same Amplify stack scaled to notebooks/strokes:

- [ ] DynamoDB tables: `Notebook`, `Section`, `Page`, `Stroke` (with `pageId`
  partition key + `id` sort key + a GSI for `(pageId, modifiedAt)`).
- [ ] Sync engine with conflict resolution (per-stroke is append-only, so
  last-writer-wins on `Stroke.id` is safe; page reorders need vector clocks
  or CRDT).
- [ ] End-to-end encryption option (notebooks are personal — server stores
  ciphertext, key lives on the client / derived from Cognito identity).
- [ ] Multi-device sync (replaces the standalone "Sync between devices" item).
- [ ] Collaborative notebooks (replaces the standalone "Collaborative
  notebooks" item) — depends on CRDT design above; AppSync subscriptions
  give us live updates for free.

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

(Cairo accessed via `gtk4::cairo` re-export — no separate dep.)

---

## Resolved Decisions

- **System color-scheme detection:** Uses `adw::StyleManager` (libadwaita 0.7) instead of `gtk4::Settings::is_gtk_application_prefer_dark_theme`. The `StyleManager` queries the XDG desktop portal so it reflects the user's OS-level dark/light preference and fires `notify::dark` when it changes, regardless of DE (GNOME, KDE, Hyprland, etc.).
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
