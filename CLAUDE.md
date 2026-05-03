# Journal App

Personal OneNote/rnote alternative for Linux. Framework 12 laptop, touchscreen + stylus.

## Core Concepts

- **Notebook** — Top-level container. Has assigned page templates.
- **Section** — Organizational group within a notebook. Can limit which templates are available.
- **Page** — Infinite-scroll canvas. Created from a template (or blank). Reorderable within section.
- **Page Template** — Defines background/layout for a page (grid, lines, imported image/PDF, custom designs).
- **Notebook Template** — Defines a planner structure: what pages auto-generate for year/month/week/day periods.

## Tech Stack

- **Language:** Rust
- **UI Framework:** GTK4 (via gtk4-rs) + libadwaita (`adw::Application` for system color-scheme detection via `adw::StyleManager`)
- **Canvas Rendering:** Cairo (via gtk4::cairo, on GTK4 DrawingArea). GPU acceleration deferred — GSK paths (GTK 4.14+) is the migration target if/when CPU rendering becomes a bottleneck. Skia via GLArea was attempted in Phase 1 but `direct_contexts::make_gl` fails on GTK4 + Wayland (Mesa) without a clear root cause.
- **Storage:** Single SQLite db at `~/.local/share/journal/journal.db` (the original "one `.journal` per notebook" plan was consolidated). Phase 6 will trait-abstract the store crate so a remote backend (AWS Amplify — Cognito + AppSync + DynamoDB + S3) can host templates first and notebooks later, without touching the GTK client.
- **Roadmap (Phase 6):** AWS Amplify-backed template sharing (Cognito auth, AppSync GraphQL, DynamoDB metadata, S3 TOML bodies); optional Amplify-Hosting web portal for browsing/forking public templates; eventual notebook sync via the same stack. Linux client stays native; web is for sharing/browsing templates only, never for drawing.

## Architecture Principles

- All stroke coordinates in canvas-space (world coordinates)
- Viewport transform at render time only
- Zoom-relative strokes: `canvas_width = pen_screen_px / zoom_at_creation`
- Infinite vertical scroll per page (no page boundaries)
- Templates = background layers rendered behind strokes
- Notebook templates = programmatic page generation rules
- Template widgets = vector regions (calendar/timeline/checklist + Franklin priority list / Full Focus big-three / weekly compass / day-schedule) painted between background and strokes
- `WidgetKind::TextBlock` text runs through the `title_format` engine — `{date}/{weekday}/{month_name}/{year}/{week}/{day}/{month}` expand using the page's bound date (today's date in the template editor preview). Engine lives in `journal_core::title_format`; `journal_templates` re-exports for back-compat
- Floating, draggable pen toolbar — overlay child positioned via dynamic margins, drag handle persists `(x, y)` to `~/.config/journal/config.toml`
- Template editor lives as a full-screen stack page (`TEMPLATE_EDITOR_NAME` in `window.rs`), not a modal — opens from `template_manager` via an `Rc<dyn Fn(Option<PageTemplate>)>` opener closure, returns to home or notebook canvas via `previous_view`

## Building

```bash
cargo build
cargo run -p journal-app
```

## Conventions

- Error handling: `thiserror` for library crates, `anyhow` in app crate
- Logging: `tracing` crate
- Serialization: `bincode` for stroke blobs, `serde` + TOML for config/templates
- IDs: UUID v4
- Dates: `chrono` with UTC storage
