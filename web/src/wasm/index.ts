// WASM API surface for the Journal web POC.
//
// This module exposes:
//
//   1. `Viewer` interface       — pan/zoom/render API mirroring the
//                                  desktop's `VelloRenderer`. Real impl
//                                  lives in `journal-web-viewer` (wasm).
//   2. `Shim` interface          — TOML parse/serialize for designer
//                                  output. Real impl in `journal-web-shim`.
//   3. `mockViewer()` / `mockShim()` factories returning fake impls
//      that log and persist enough state for the UI to keep working
//      when the WASM bindings haven't been built yet (e.g. the first
//      `pnpm dev` before someone runs `bash web/build-wasm.sh`).
//
// The real WASM modules are compiled by `web/build-wasm.sh` into
// `web/src/wasm/generated/{shim,viewer}/journal_web_{shim,viewer}.{js,wasm}`.
// That directory is gitignored — the binaries are build artefacts.

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
// Real WASM bindings (lazy-loaded so unbuilt artefacts don't break the SPA)
// ---------------------------------------------------------------------

/**
 * Real WASM viewer. Wraps the wasm-bindgen-generated `Viewer` class
 * from `journal-web-viewer` so it satisfies the TS `Viewer` interface
 * exactly (the wasm-bindgen output already maps the renamed methods —
 * `loadNotebook`, `renderPage`, `zoomAt` — via `js_name` attributes).
 *
 * Module load is deferred until `init()` is first called. If the WASM
 * generated bundle is missing (e.g. the developer hasn't run
 * `bash web/build-wasm.sh`), the loader logs a warning and returns the
 * mock so the SPA's UI still works on first run.
 */
function realViewer(): Viewer {
  // Lazy box so `import()` only fires once. Falls back to mock on
  // module-resolve failure (the most common case: generated/ doesn't
  // exist yet).
  let inner:
    | {
        kind: "real";
        viewer: {
          init: (c: HTMLCanvasElement) => Promise<void>;
          loadNotebook: (bytes: Uint8Array) => void;
          renderPage: (i: number, w: number, h: number) => void;
          pan: (dx: number, dy: number) => void;
          zoomAt: (sx: number, sy: number, factor: number) => void;
          free?: () => void;
        };
      }
    | { kind: "mock"; viewer: Viewer }
    | null = null;

  async function ensure(): Promise<void> {
    if (inner) return;
    try {
      // The wasm-bindgen `--target web` output exports a `default`
      // initializer plus the named class. The init call points the
      // module at the `.wasm` URL Vite resolves at build time.
      // Path is built dynamically so TS doesn't try to resolve the
      // generated file at typecheck (the `generated/` dir is
      // gitignored and may not exist when `pnpm typecheck` runs).
      const path = "/wasm/viewer/journal_web_viewer.js";
      const mod: any = await import(/* @vite-ignore */ path);
      // Default export is the wasm initializer fn.
      await mod.default();
      const v = new mod.Viewer();
      inner = { kind: "real", viewer: v };
    } catch (e) {
      console.warn(
        "[journal-web-viewer] real WASM unavailable, falling back to mock. " +
          "Run `bash web/build-wasm.sh` to build it.",
        e,
      );
      inner = { kind: "mock", viewer: mockViewer() };
    }
  }

  return {
    async init(canvas: HTMLCanvasElement) {
      await ensure();
      if (!inner) return;
      await inner.viewer.init(canvas);
    },
    async loadNotebook(bytes: Uint8Array) {
      await ensure();
      if (!inner) return;
      // Real viewer's `loadNotebook` is sync (it returns
      // `Result<(), JsValue>` which wasm-bindgen surfaces as a thrown
      // promise rejection synchronously). The TS interface promises
      // a Promise so callers can `await`.
      if (inner.kind === "real") {
        inner.viewer.loadNotebook(bytes);
      } else {
        await inner.viewer.loadNotebook(bytes);
      }
    },
    renderPage(index: number, w: number, h: number) {
      if (!inner) return;
      inner.viewer.renderPage(index, w, h);
    },
    pan(dx: number, dy: number) {
      if (!inner) return;
      inner.viewer.pan(dx, dy);
    },
    zoomAt(sx: number, sy: number, factor: number) {
      if (!inner) return;
      inner.viewer.zoomAt(sx, sy, factor);
    },
  };
}

/**
 * Real WASM shim. Lazily resolves on first call and falls back to the
 * mock if the generated bundle is missing.
 */
function realShim(): Shim {
  let mod: {
    parse_template_toml: (toml: string) => PageTemplate;
    serialize_template_toml: (t: PageTemplate) => string;
  } | null = null;
  let mockFallback: Shim | null = null;

  // Sync warm-up via `void`-d promise; first invocation may still hit
  // the not-loaded path and fall through to mock. Designed to be
  // called eagerly on module import for production builds, lazily in
  // dev. We keep both bodies tiny so a "shim not loaded yet" error is
  // visible in the SaveModal preview rather than a silent mock.
  async function ensure(): Promise<void> {
    if (mod || mockFallback) return;
    try {
      // Dynamic path so TS doesn't try to resolve the gitignored
      // generated module at typecheck.
      const path = "/wasm/shim/journal_web_shim.js";
      const m: any = await import(/* @vite-ignore */ path);
      await m.default();
      mod = {
        parse_template_toml: m.parse_template_toml,
        serialize_template_toml: m.serialize_template_toml,
      };
    } catch (e) {
      console.warn(
        "[journal-web-shim] real WASM unavailable, falling back to mock. " +
          "Run `bash web/build-wasm.sh` to build it.",
        e,
      );
      mockFallback = mockShim();
    }
  }
  // Kick off the load eagerly; both paths are non-blocking for the UI.
  void ensure();

  return {
    parseTemplateToml(toml: string): PageTemplate {
      if (mod) return mod.parse_template_toml(toml);
      if (mockFallback) return mockFallback.parseTemplateToml(toml);
      throw new Error(
        "shim WASM not loaded yet — call this after the module resolves, " +
          "or run bash web/build-wasm.sh.",
      );
    },
    serializeTemplateToml(t: PageTemplate): string {
      if (mod) return mod.serialize_template_toml(t);
      if (mockFallback) return mockFallback.serializeTemplateToml(t);
      throw new Error(
        "shim WASM not loaded yet — call this after the module resolves, " +
          "or run bash web/build-wasm.sh.",
      );
    },
  };
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

// ---------------------------------------------------------------------
// Singletons
// ---------------------------------------------------------------------
//
// The `realViewer` / `realShim` factories internally fall back to the
// mock when the generated WASM bundle isn't present — letting the SPA
// keep building and running in dev environments where someone hasn't
// rebuilt the WASM yet. Once the bindings exist, behaviour is
// indistinguishable from importing them directly.

export const viewer: Viewer = realViewer();
export const shim: Shim = realShim();
