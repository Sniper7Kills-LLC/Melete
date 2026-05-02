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
- **UI Framework:** GTK4 (via gtk4-rs)
- **Canvas Rendering:** Skia (via rust-skia) — GPU-accelerated through GLArea
- **Storage:** SQLite per notebook (one `.journal` file)

## Architecture Principles

- All stroke coordinates in canvas-space (world coordinates)
- Viewport transform at render time only
- Zoom-relative strokes: `canvas_width = pen_screen_px / zoom_at_creation`
- Infinite vertical scroll per page (no page boundaries)
- Templates = background layers rendered behind strokes
- Notebook templates = programmatic page generation rules

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
