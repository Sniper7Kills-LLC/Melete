// WASM API surface for the Journal web POC.
//
// The real bindings (compiled from `journal-web-shim` + `journal-web-viewer`
// per docs/web-portal.md §5.3-§5.4) will be slotted in here later. For
// now this file exposes:
//
//   1. `Viewer` interface       — pan/zoom/render API mirroring the
//                                  desktop's `VelloRenderer`.
//   2. `Shim` interface          — TOML parse/serialize for designer
//                                  output.
//   3. `mockViewer()` / `mockShim()` factories returning fake impls
//      that log and persist enough state for the UI to be developed
//      without WebAssembly.
//
// IMPORTANT for the WASM agent: the interfaces below are the contract.
// Match these signatures exactly when emitting wasm-bindgen output, or
// add a thin adapter layer to map. Do not break method names/casing.

import type { NotebookBundle, PageTemplate } from "@/types";

/**
 * Read-only viewer. Mirrors §5.4 of docs/web-portal.md:
 *   - `init` binds the viewer to a `<canvas>` element and warms wgpu.
 *   - `loadNotebook` ingests the gzipped (or raw) JSON envelope bytes.
 *   - `renderPage` paints page `index` at viewport size `(w, h)`.
 *   - Pan / zoom mutate viewport state on the Rust side.
 */
export interface Viewer {
  init(canvas: HTMLCanvasElement): Promise<void>;
  loadNotebook(bytes: Uint8Array): Promise<void>;
  renderPage(index: number, w: number, h: number): void;
  pan(dx: number, dy: number): void;
  zoomAt(sx: number, sy: number, factor: number): void;
}

/**
 * Designer/serialization shim. The real impl wraps
 * `journal_templates::format::{parse_template_toml, serialize_template_toml}`.
 *
 * `parseTemplateToml` consumes a TOML string and returns the same
 * `PageTemplate` shape the desktop holds. `serializeTemplateToml` is
 * the inverse — its output must round-trip through the desktop parser
 * byte-for-byte (see docs/web-portal.md §10).
 */
export interface Shim {
  parseTemplateToml(toml: string): PageTemplate;
  serializeTemplateToml(t: PageTemplate): string;
}

// ---------------------------------------------------------------------
// Mock implementations
// ---------------------------------------------------------------------

/**
 * Mock viewer. Logs every call and renders a placeholder gradient via
 * the canvas 2D context so the UI exists without WASM. Replace with the
 * real wasm-bindgen-generated Viewer when bindings ship.
 */
export function mockViewer(): Viewer {
  let canvas: HTMLCanvasElement | null = null;
  let bundle: NotebookBundle | null = null;
  let pageIndex = 0;
  let panX = 0;
  let panY = 0;
  let zoom = 1;

  function repaint(w: number, h: number) {
    if (!canvas) return;
    canvas.width = w;
    canvas.height = h;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;

    // Background — page paper
    ctx.fillStyle = "#fdfdf6";
    ctx.fillRect(0, 0, w, h);

    // Subtle viewport indicator so pan/zoom are visible
    ctx.save();
    ctx.translate(w / 2 + panX, h / 2 + panY);
    ctx.scale(zoom, zoom);

    // Mock page outline
    ctx.strokeStyle = "#cfcfd6";
    ctx.lineWidth = 1 / zoom;
    ctx.strokeRect(-200, -260, 400, 520);

    // Mock template / page identifier
    ctx.fillStyle = "#3a3a55";
    ctx.font = `${14 / zoom}px ui-sans-serif, system-ui, sans-serif`;
    const pageName =
      bundle?.pages?.[pageIndex]?.name ??
      (bundle ? `Page ${pageIndex + 1}` : "(no notebook loaded)");
    ctx.fillText(`[mock viewer] ${pageName}`, -180, -240);

    if (bundle) {
      const page = bundle.pages?.[pageIndex];
      const strokes = page ? (bundle.strokes_by_page[page.id] ?? []) : [];
      ctx.strokeStyle = "#1f2937";
      ctx.lineWidth = 2 / zoom;
      ctx.lineCap = "round";
      ctx.lineJoin = "round";
      for (const stroke of strokes) {
        if (stroke.points.length < 2) continue;
        ctx.beginPath();
        ctx.moveTo(stroke.points[0].x, stroke.points[0].y);
        for (let i = 1; i < stroke.points.length; i++) {
          ctx.lineTo(stroke.points[i].x, stroke.points[i].y);
        }
        ctx.stroke();
      }
    }

    ctx.restore();
  }

  return {
    async init(c: HTMLCanvasElement) {
      console.info("[mock-viewer] init");
      canvas = c;
    },
    async loadNotebook(bytes: Uint8Array) {
      console.info("[mock-viewer] loadNotebook", bytes.byteLength, "bytes");
      const text = new TextDecoder().decode(bytes);
      try {
        bundle = JSON.parse(text) as NotebookBundle;
      } catch (e) {
        console.error("[mock-viewer] not JSON; real WASM will gunzip first", e);
        bundle = null;
      }
    },
    renderPage(index: number, w: number, h: number) {
      pageIndex = index;
      repaint(w, h);
    },
    pan(dx: number, dy: number) {
      panX += dx;
      panY += dy;
      if (canvas) repaint(canvas.width, canvas.height);
    },
    zoomAt(sx: number, sy: number, factor: number) {
      // Zoom about the screen-space (sx, sy). Translate so that point
      // is at origin, scale, translate back.
      if (!canvas) {
        zoom *= factor;
        return;
      }
      const cx = canvas.width / 2 + panX;
      const cy = canvas.height / 2 + panY;
      panX += (cx - sx) * (factor - 1) * -1;
      panY += (cy - sy) * (factor - 1) * -1;
      zoom *= factor;
      repaint(canvas.width, canvas.height);
    },
  };
}

/**
 * Mock shim. Pretends to TOML-encode/decode by wrapping a JSON payload
 * in a TOML-ish header so the designer can show "would-be TOML" in a
 * preview modal. The real shim runs the Rust `toml` crate end-to-end.
 */
export function mockShim(): Shim {
  return {
    parseTemplateToml(toml: string): PageTemplate {
      console.info("[mock-shim] parseTemplateToml (stub)", toml.slice(0, 80));
      // Real impl will deserialize via journal-templates::format. For
      // mock, throw — the designer never calls parse on this code path
      // for the POC, only serialize.
      throw new Error(
        "mockShim.parseTemplateToml is not implemented; wire real WASM.",
      );
    },
    serializeTemplateToml(t: PageTemplate): string {
      // Pretty-print as JSON with a TOML-ish banner so designers can
      // sanity-check field names. The real shim emits genuine TOML;
      // this is a placeholder for the Save modal to show *something*.
      const json = JSON.stringify(t, null, 2);
      return [
        "# WARNING: this is a mock TOML preview.",
        "# Real output will come from journal-templates::format.",
        "# Field names below already match the Rust schema.",
        "",
        "[preview-as-json]",
        ...json.split("\n").map((line) => `# ${line}`),
      ].join("\n");
    },
  };
}

// Singleton instances. Components import these directly; the WASM
// agent can swap the factory bodies (or replace this module wholesale)
// once the bindings exist.
export const viewer: Viewer = mockViewer();
export const shim: Shim = mockShim();
