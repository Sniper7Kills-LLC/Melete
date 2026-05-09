// TypeScript mirrors of the Rust types in `crates/journal-core` and
// `crates/journal-templates`. Field names + casing match what `serde`
// emits (snake_case, externally tagged enums via `#[serde(tag = "kind",
// rename_all = "snake_case")]`). The WASM stub re-exports these; the
// real wasm-bindgen build later will produce values that satisfy these
// shapes byte-for-byte.
//
// Reference files (DO NOT diverge without porting):
//   crates/journal-core/src/lib.rs
//   crates/journal-core/src/template.rs
//   crates/journal-core/src/notebook.rs
//   crates/journal-core/src/page.rs
//   crates/journal-core/src/stroke.rs
//   crates/journal-core/src/pen.rs
//   crates/journal-core/src/viewport.rs

// ---------------------------------------------------------------------
// Primitives
// ---------------------------------------------------------------------

/** UUID v4 string. */
export type Uuid = string;

/** RGBA color, 8-bit channels. Matches `pen::Color`. */
export interface Color {
  r: number;
  g: number;
  b: number;
  a: number;
}

/** 2D point (mm or canvas units depending on context). Matches `Point`. */
export interface Point {
  x: number;
  y: number;
}

/** Axis-aligned rect. Matches `Rect`. */
export interface Rect {
  x: number;
  y: number;
  width: number;
  height: number;
}

/** Viewport pan/zoom/rotation. Matches `viewport::Viewport`. */
export interface Viewport {
  center: Point;
  zoom: number;
  rotation: number;
}

// ---------------------------------------------------------------------
// Template — page background & widgets
// ---------------------------------------------------------------------

/**
 * `BackgroundType` — externally tagged enum from `template.rs`.
 *
 * Rust serde does NOT use `tag` here (no `#[serde(tag=…)]`), so each
 * variant serializes as the standard Rust enum form. For the unit
 * variant `Blank` that's the bare string `"Blank"`. For data variants
 * it's a single-key object `{ "Dots": { "spacing": 5 } }`.
 *
 * To keep TS ergonomic we model this as a discriminated union via
 * a `kind` field and a sibling `serializeBackgroundType` helper turns
 * it into the wire shape. The WASM shim handles encoding when it lands.
 */
export type BackgroundType =
  | { kind: "Blank" }
  | { kind: "Dots"; spacing: number }
  | { kind: "Lines"; spacing: number }
  | { kind: "Grid"; spacing: number }
  | { kind: "Isometric"; spacing: number }
  | { kind: "Hexagonal"; spacing: number }
  | { kind: "Image"; path: string }
  | { kind: "Pdf"; path: string; page: number };

/** Background tiling. Rust `TilingMode`. */
export type TilingMode = "None" | "Repeat";

/** Widget rect in mm. Matches `WidgetRect`. */
export interface WidgetRect {
  x: number;
  y: number;
  width: number;
  height: number;
}

/** Widget stroke/fill style. Matches `WidgetStyle`. */
export interface WidgetStyle {
  stroke_color: Color;
  fill_color: Color | null;
  stroke_width_mm: number;
}

/**
 * `WidgetKind` — internally tagged via `#[serde(tag = "kind",
 * rename_all = "snake_case")]`. Variants below cover at minimum the
 * required set (TextBlock, Rectangle, Ellipse, Line, GridRegion,
 * Checklist, CalendarMonth) plus the rest of the Rust enum so the
 * designer palette can scale without another schema pass.
 */
export type WidgetKind =
  | { kind: "text_block"; text: string; font_size_mm: number }
  | { kind: "rectangle" }
  | { kind: "ellipse" }
  | {
      kind: "arc";
      start_deg: number;
      sweep_deg: number;
      thickness_mm: number;
    }
  | { kind: "line"; thickness_mm: number }
  | { kind: "grid_region"; spacing_mm: number }
  | { kind: "lines_region"; spacing_mm: number }
  | { kind: "dots_region"; spacing_mm: number }
  | { kind: "calendar_month" }
  | {
      kind: "timeline";
      start_hour: number;
      end_hour: number;
      slot_minutes: number;
    }
  | { kind: "checklist"; items: string[] }
  | { kind: "big_three" }
  | { kind: "priority_list"; count: number }
  | {
      kind: "daily_appointments";
      start_hour: number;
      end_hour: number;
    }
  | { kind: "weekly_compass" }
  | { kind: "habit_tracker"; habits: string[]; days: number }
  | { kind: "tally"; label: string; count: number }
  | {
      kind: "range_arcs";
      rings: number;
      interval_m: number;
      sweep_deg: number;
      sector_deg: number;
    }
  // Fetch-backed widgets — render from a per-page WidgetData payload
  // populated by the desktop's fetcher. Surfaced on the web for
  // schema parity; the SPA's designer palette deliberately doesn't
  // expose them since the fetch path doesn't exist on the web yet.
  | {
      kind: "weather";
      lat: number;
      lon: number;
      location_label: string;
      days: number;
    }
  | { kind: "quote"; source: string }
  | { kind: "bible_verse"; reference: string; translation: string }
  | { kind: "sunrise"; lat: number; lon: number }
  | { kind: "moon_phase" }
  | { kind: "on_this_day"; lang: string; max_events: number }
  | { kind: "word_of_day"; lang: string }
  | { kind: "rss_headline"; url: string; count: number }
  | { kind: "astronomy"; lat: number; lon: number };

/** Discriminator union of every `WidgetKind.kind` value. */
export type WidgetKindTag = WidgetKind["kind"];

/** A widget placed on a template canvas. Matches `TemplateWidget`. */
export interface Widget {
  id: Uuid;
  kind: WidgetKind;
  rect: WidgetRect;
  style: WidgetStyle;
}

/** A page template. Matches `template::PageTemplate`. */
export interface PageTemplate {
  id: { 0: Uuid } | Uuid;
  // serde emits TemplateId as `[uuid_str]` tuple-newtype-default — we
  // accept either the bare string (web-friendly) or the `{ "0": uuid }`
  // shape the Rust side may emit. Normalize via `templateIdString()`.
  name: string;
  description: string;
  background: BackgroundType;
  /** Page size in mm — Rust serializes `(f64, f64)` as a 2-tuple array. */
  size_mm: [number, number];
  tiling: TilingMode;
  default_viewport: Viewport | null;
  widgets: Widget[];
  category: string;
}

// ---------------------------------------------------------------------
// Notebook / Section / Page
// ---------------------------------------------------------------------

export type NotebookKind =
  | { kind: "Standard" }
  // serde emits the `Planner` variant with its named fields as a
  // single-key wrapping object: { "Planner": { template_id, … } }.
  // We model both shapes for forward-compat.
  | {
      kind: "Planner";
      template_id: Uuid;
      creation_date: string; // YYYY-MM-DD
    };

/** Matches `notebook::Notebook`. */
export interface Notebook {
  id: Uuid;
  name: string;
  kind: NotebookKind;
  assigned_templates: Uuid[];
}

/** Matches `notebook::Section`. */
export interface Section {
  id: Uuid;
  notebook_id: Uuid;
  name: string;
  position: number;
  allowed_templates: Uuid[] | null;
  parent_section_id: Uuid | null;
}

/** Matches `page::Page`. Stripped to fields the viewer needs. */
export interface Page {
  id: Uuid;
  template_id: Uuid | null;
  section_id: Uuid;
  position: number;
  name: string;
  planner_address: unknown | null;
  created_at: string; // RFC3339
  modified_at: string;
  widget_overrides: Record<string, unknown>;
  widget_data: Record<string, unknown>;
  /** Bookmark / flag toggle from the desktop sidebar. `serde(default)`
   * on the Rust side accepts older bundles without the field. */
  flagged?: boolean;
}

// ---------------------------------------------------------------------
// Stroke & pen
// ---------------------------------------------------------------------

export type ToolStyle =
  | "Pen"
  | "Pencil"
  | "Highlighter"
  | "Paintbrush"
  | "SprayCan"
  | "Calligraphy";

export type BlendMode =
  | "Normal"
  | "Multiply"
  | "Screen"
  | "Overlay"
  | "Darken"
  | "Lighten"
  | "Erase";

export interface PenSettings {
  color: Color;
  base_width: number;
  opacity: number;
  blend_mode: BlendMode;
  brush_style: ToolStyle;
}

export interface StrokePoint {
  x: number;
  y: number;
  pressure: number;
  tilt_x: number;
  tilt_y: number;
  timestamp_ms: number;
}

export interface Stroke {
  id: Uuid;
  points: StrokePoint[];
  pen: PenSettings;
  zoom_at_creation: number;
  bounding_box: Rect;
  brush_recipe: unknown | null;
}

// ---------------------------------------------------------------------
// Notebook export envelope (web-portal.md §5.4)
// ---------------------------------------------------------------------

/**
 * The JSON envelope the WASM viewer consumes. Matches the spec in
 * `docs/web-portal.md` §5.4. The desktop "Share notebook" flow will
 * produce this same shape via a future `journal-export` crate.
 */
export interface NotebookBundle {
  schema_version: 1;
  notebook: Notebook;
  sections: Section[];
  pages: Page[];
  page_templates: PageTemplate[];
  /** UUID v4 of `Page.id` → list of strokes on that page. */
  strokes_by_page: Record<string, Stroke[]>;
  /** Asset key → public URL (e.g. S3 / CloudFront). */
  asset_refs: Record<string, string>;
}

// ---------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------

/** Pull a UUID out of either `{ "0": uuid }` or a bare string. */
export function templateIdString(id: PageTemplate["id"]): Uuid {
  return typeof id === "string" ? id : id[0];
}

/** Convenience: black, fully opaque. */
export const BLACK: Color = { r: 0, g: 0, b: 0, a: 255 };

/** Convenience: pen-default mid-indigo stroke matching the desktop default. */
export const DEFAULT_STROKE: Color = { r: 60, g: 60, b: 80, a: 200 };
