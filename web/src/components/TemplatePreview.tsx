// Vello WASM template preview (#92).
//
// Reusable canvas-backed preview that runs the same `melete-web-viewer`
// crate the desktop uses. Mirrors the DesignSurface plumbing —
// `viewer.init(canvas)` once, `viewer.loadNotebook` + `renderPage(0)`
// on every template change, loading spinner until the first frame
// lands, fallback banner if WebGPU is missing.
//
// Use on the per-template detail (`Share.tsx`) so users see the real
// Vello output before they Download / Fork / Save. Not used for the
// N-card Gallery grid (WebGPU caps contexts per page; a per-card
// canvas would exhaust the budget on most setups).

import { useEffect, useRef, useState } from "react";

import type { NotebookBundle, PageTemplate } from "@/types";
import { viewer } from "@/wasm";

/** Normalise an externally-tagged serde enum value into the
 *  internally-tagged `{ kind, ...payload }` shape the WASM viewer's
 *  `JsonBackgroundType` (and the SPA's TS types) expect.
 *
 *  Why: `melete_core::BackgroundType` uses Rust's default external
 *  tagging, so `melete-web-shim::parse_template_toml` emits the
 *  template via serde-wasm-bindgen as either a bare string
 *  (`"Blank"`) or a single-key object (`{"Grid":{"spacing":5}}`).
 *  The Designer's local templates are already internally-tagged
 *  (built directly from TS), so we only need this fix on the
 *  "parsed from TOML" code path — Gallery thumbnails + Share
 *  preview. Idempotent: an already-internally-tagged value passes
 *  through unchanged. */
// eslint-disable-next-line @typescript-eslint/no-explicit-any
function toInternallyTagged(v: any): any {
  if (typeof v === "string") return { kind: v };
  if (v && typeof v === "object" && "kind" in v) return v;
  if (v && typeof v === "object") {
    const keys = Object.keys(v);
    if (keys.length === 1) {
      const k = keys[0];
      const inner = v[k];
      return inner && typeof inner === "object"
        ? { kind: k, ...inner }
        : { kind: k };
    }
  }
  return v;
}

/** Wrap a single PageTemplate in a synthetic NotebookBundle the WASM
 *  viewer can deserialize. Keeps the template's actual background so
 *  the preview shows exactly what the desktop will paint. */
export function wrapTemplateForPreview(
  template: PageTemplate,
  spaZoom: number,
  options?: { keepBackground?: boolean },
): NotebookBundle {
  const keep = options?.keepBackground ?? true;
  const templateId =
    typeof template.id === "string" ? template.id : template.id["0"];
  const now = new Date().toISOString();
  const [pageW, pageH] = template.size_mm;
  const previewTemplate: PageTemplate = {
    ...template,
    background: keep
      ? (toInternallyTagged(template.background) as PageTemplate["background"])
      : { kind: "Blank" },
    default_viewport: {
      center: { x: pageW / 2, y: pageH / 2 },
      zoom: spaZoom,
      rotation: 0,
    },
  };
  return {
    schema_version: 1,
    notebook: {
      id: "00000000-0000-0000-0000-00000000aaaa",
      name: "preview",
      kind: { kind: "Standard" },
      assigned_templates: [templateId],
    },
    sections: [
      {
        id: "00000000-0000-0000-0000-00000000bbbb",
        notebook_id: "00000000-0000-0000-0000-00000000aaaa",
        name: "preview",
        position: 0,
        allowed_templates: null,
        parent_section_id: null,
      },
    ],
    pages: [
      {
        id: "00000000-0000-0000-0000-00000000cccc",
        template_id: templateId,
        section_id: "00000000-0000-0000-0000-00000000bbbb",
        position: 0,
        name: "preview",
        planner_address: null,
        created_at: now,
        modified_at: now,
        widget_overrides: {},
        widget_data: {},
      },
    ],
    page_templates: [previewTemplate],
    strokes_by_page: {
      "00000000-0000-0000-0000-00000000cccc": [],
    },
    asset_refs: {},
  };
}

export interface TemplatePreviewProps {
  template: PageTemplate;
  /** px-per-mm zoom. Defaults to 2.2 to match the designer surface. */
  zoom?: number;
  /** True keeps the template's background; false swaps to Blank
   *  (designer prefers blank since the CSS guide overlay would
   *  double the grid). */
  keepBackground?: boolean;
  className?: string;
}

export function TemplatePreview({
  template,
  zoom = 2.2,
  keepBackground = true,
  className,
}: TemplatePreviewProps) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const [previewReady, setPreviewReady] = useState(false);
  const [firstRenderDone, setFirstRenderDone] = useState(false);
  const [previewError, setPreviewError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    const c = canvasRef.current;
    if (!c) {
      setPreviewError("preview canvas missing");
      return;
    }
    if (typeof navigator === "undefined" || !("gpu" in navigator)) {
      setPreviewError("WebGPU unavailable (navigator.gpu missing)");
      return;
    }
    (async () => {
      try {
        await viewer.init(c);
        if (!cancelled) {
          setPreviewReady(true);
          setPreviewError(null);
        }
      } catch (e) {
        const msg = e instanceof Error ? e.message : String(e);
        console.warn("[TemplatePreview] viewer.init failed:", e);
        if (!cancelled) setPreviewError(msg);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, []);

  const [pageW, pageH] = template.size_mm;

  useEffect(() => {
    if (!previewReady) return;
    const c = canvasRef.current;
    if (!c) return;
    const w = Math.max(1, Math.floor(pageW * zoom));
    const h = Math.max(1, Math.floor(pageH * zoom));
    if (c.width !== w) c.width = w;
    if (c.height !== h) c.height = h;
    const bundle = wrapTemplateForPreview(template, zoom, { keepBackground });
    const bytes = new TextEncoder().encode(JSON.stringify(bundle));
    (async () => {
      try {
        await viewer.loadNotebook(bytes);
        viewer.renderPage(0, w, h);
        setFirstRenderDone(true);
      } catch (e) {
        const msg = e instanceof Error ? e.message : String(e);
        console.warn("[TemplatePreview] render failed:", e);
        setPreviewError(msg);
      }
    })();
  }, [template, pageW, pageH, zoom, keepBackground, previewReady]);

  return (
    <div
      className={`relative bg-white shadow-sm ${className ?? ""}`}
      style={{ width: pageW * zoom, height: pageH * zoom }}
    >
      <canvas
        ref={canvasRef}
        className="pointer-events-none absolute inset-0 h-full w-full"
      />
      {previewError && (
        <div className="absolute inset-0 flex flex-col items-center justify-center gap-2 bg-white p-4 text-center text-slate-500">
          <div className="text-sm font-semibold text-amber-700">
            Preview unavailable
          </div>
          <div className="text-xs text-slate-500">{previewError}</div>
          <div className="text-[11px] leading-snug text-slate-400">
            Enable WebGPU (chrome://flags or about:config dom.webgpu.enabled).
          </div>
        </div>
      )}
      {!firstRenderDone && !previewError && (
        <div className="pointer-events-none absolute inset-0 flex flex-col items-center justify-center gap-3 bg-white text-slate-500">
          <div className="h-10 w-10 animate-spin rounded-full border-4 border-slate-200 border-t-indigo-500" />
          <div className="text-sm font-medium text-slate-700">
            Loading preview…
          </div>
          <div className="max-w-xs text-center text-xs text-slate-500">
            Compiling the Vello WASM renderer. A couple of seconds on
            first visit.
          </div>
        </div>
      )}
    </div>
  );
}
