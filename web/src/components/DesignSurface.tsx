import type { DragEvent as ReactDragEvent, PointerEvent as ReactPointerEvent } from "react";
import { useEffect, useRef, useState } from "react";

import type { NotebookBundle, PageTemplate, Widget, WidgetKindTag } from "@/types";
import { useDesigner } from "@/store/designerStore";
import { formatLength, useUnits } from "@/store/unitsStore";
import { viewer } from "@/wasm";

/**
 * Center pane: a millimeter-coordinate design surface. Renders widgets
 * as `<div>` placeholders for now — the real Vello WASM preview swaps
 * in later (docs/web-portal.md §5.3).
 *
 * Coordinates: page is in mm. We pick a pixel-per-mm zoom that fits
 * the page in the surface. Drag to move widgets, drag corners to
 * resize, drop from palette to spawn at cursor.
 */
export function DesignSurface() {
  const template = useDesigner((s) => s.template);
  const selectedId = useDesigner((s) => s.selectedWidgetId);
  const snapMm = useDesigner((s) => s.snapMm);
  const showGuides = useDesigner((s) => s.showGuides);
  const selectWidget = useDesigner((s) => s.selectWidget);
  const updateWidget = useDesigner((s) => s.updateWidget);
  const addWidget = useDesigner((s) => s.addWidget);

  const surfaceRef = useRef<HTMLDivElement>(null);
  const previewCanvasRef = useRef<HTMLCanvasElement>(null);
  const [previewReady, setPreviewReady] = useState(false);
  const [previewError, setPreviewError] = useState<string | null>(null);

  // Page → screen scale: fit page width into an 800-px-ish surface.
  // We compute on-render so the layout responds to window size.
  const [zoom, setZoom] = useState(2.2); // px/mm

  const [pageW, pageH] = template.size_mm;
  const units = useUnits((s) => s.units);

  // ---------------------------------------------------------------------
  // Vello live preview — see #38. We share the same `viewer` singleton
  // the / route uses (browsers cap WebGPU contexts per page; spinning
  // up a second instance fails on most setups). Each route's mount
  // re-binds the singleton to its canvas via `init()`, then the
  // designer wraps the live `template` in a synthetic NotebookBundle
  // and calls the standard loadNotebook + renderPage path. If init
  // fails (no WebGPU, headless), we silently fall back to the DOM
  // widget stand-ins — no scary banner needed for the POC.
  // ---------------------------------------------------------------------

  useEffect(() => {
    let cancelled = false;
    const c = previewCanvasRef.current;
    if (!c) {
      setPreviewError("preview canvas missing");
      return;
    }
    if (typeof navigator === "undefined" || !("gpu" in navigator)) {
      setPreviewError("navigator.gpu missing");
      return;
    }
    (async () => {
      try {
        console.info("[designer] viewer.init starting");
        await viewer.init(c);
        console.info("[designer] viewer.init done");
        if (!cancelled) {
          setPreviewReady(true);
          setPreviewError(null);
        }
      } catch (e) {
        const msg = e instanceof Error ? e.message : String(e);
        console.warn("[designer] WebGPU preview init failed:", e);
        if (!cancelled) setPreviewError(msg);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, []);

  useEffect(() => {
    if (!previewReady) return;
    const c = previewCanvasRef.current;
    if (!c) return;
    const w = Math.max(1, Math.floor(pageW * zoom));
    const h = Math.max(1, Math.floor(pageH * zoom));
    if (c.width !== w) c.width = w;
    if (c.height !== h) c.height = h;
    // Force the WASM viewer's viewport to match the SPA's mm→px zoom
    // exactly so the Vello render lines up with the DOM manipulator
    // divs. Without this, the viewer's load_notebook path fits the
    // template into the canvas with a 0.95 margin and the Vello widget
    // rects drift slightly off the SPA's overlay rects.
    const bundle = wrapTemplateInBundle(template, zoom);
    const json = JSON.stringify(bundle);
    const bytes = new TextEncoder().encode(json);
    (async () => {
      try {
        await viewer.loadNotebook(bytes);
        viewer.renderPage(0, w, h);
        console.debug(
          "[designer] rendered template",
          template.name,
          `${template.widgets.length} widget(s) at ${w}×${h}`,
        );
      } catch (e) {
        const msg = e instanceof Error ? e.message : String(e);
        console.warn("[designer] preview render failed:", e);
        setPreviewError(msg);
      }
    })();
  }, [template, pageW, pageH, zoom, previewReady]);

  function dropFromPalette(e: ReactDragEvent) {
    e.preventDefault();
    const tag = e.dataTransfer.getData(
      "application/x-widget-tag",
    ) as WidgetKindTag;
    if (!tag) return;
    // Add and then position at the drop location.
    addWidget(tag);
    // The most recently-added widget is the last one — re-position it.
    const surfaceRect = surfaceRef.current?.getBoundingClientRect();
    if (!surfaceRect) return;
    const xMm = (e.clientX - surfaceRect.left) / zoom;
    const yMm = (e.clientY - surfaceRect.top) / zoom;
    // Pull the last widget id from the store and update.
    const state = useDesigner.getState();
    const last = state.template.widgets[state.template.widgets.length - 1];
    if (!last) return;
    updateWidget(last.id, {
      rect: {
        ...last.rect,
        x: xMm,
        y: yMm,
      },
    });
  }

  return (
    <div className="flex h-full flex-col bg-slate-100">
      <div className="flex items-center gap-2 border-b border-slate-200 bg-white px-3 py-2 text-sm">
        <span className="text-slate-500">Page:</span>
        <span className="font-medium text-slate-800">
          {template.name || "(unnamed)"}
        </span>
        <span className="text-slate-400">
          · {formatLength(pageW, units)} × {formatLength(pageH, units)} · snap{" "}
          {formatLength(snapMm, units)}
        </span>
        <div className="ml-auto flex items-center gap-2">
          <label className="flex items-center gap-1 text-xs text-slate-600">
            zoom
            <input
              type="range"
              min={1}
              max={5}
              step={0.1}
              value={zoom}
              onChange={(e) => setZoom(Number(e.target.value))}
            />
            <span className="w-10 text-right tabular-nums">
              {zoom.toFixed(1)}×
            </span>
          </label>
        </div>
      </div>

      <div className="flex-1 overflow-auto p-6">
        <div
          ref={surfaceRef}
          onClick={(e) => {
            // Click on empty surface clears selection.
            if (e.target === e.currentTarget) selectWidget(null);
          }}
          onDragOver={(e) => e.preventDefault()}
          onDrop={dropFromPalette}
          className="relative bg-white shadow-sm"
          style={{
            width: pageW * zoom,
            height: pageH * zoom,
            backgroundImage: showGuides
              ? `linear-gradient(to right, rgba(99,102,241,0.06) 1px, transparent 1px),
                 linear-gradient(to bottom, rgba(99,102,241,0.06) 1px, transparent 1px)`
              : undefined,
            backgroundSize: showGuides
              ? `${snapMm * zoom}px ${snapMm * zoom}px`
              : undefined,
          }}
        >
          <canvas
            ref={previewCanvasRef}
            className="pointer-events-none absolute inset-0 h-full w-full"
          />
          {previewError && (
            <div className="pointer-events-none absolute right-2 top-2 max-w-xs rounded border border-amber-300 bg-amber-50 px-2 py-1 text-xs text-amber-900 shadow-sm">
              <div className="font-semibold">Preview unavailable</div>
              <div className="text-[11px] leading-snug opacity-90">
                {previewError}
              </div>
              <div className="mt-1 text-[11px] leading-snug opacity-75">
                Showing widget outlines instead. Enable WebGPU
                (chrome://flags or about:config dom.webgpu.enabled) for
                a live render.
              </div>
            </div>
          )}
          {template.widgets.map((w) => (
            <WidgetView
              key={w.id}
              widget={w}
              selected={w.id === selectedId}
              zoom={zoom}
              transparent={previewReady}
            />
          ))}
        </div>
      </div>
    </div>
  );
}

interface WidgetViewProps {
  widget: Widget;
  selected: boolean;
  zoom: number;
  /** When the Vello preview is rendering underneath, hide the DOM
   * stand-in (border, fill, glyph) so the real render shows through —
   * keep only the selection ring + drag handle. */
  transparent: boolean;
}

function WidgetView({ widget, selected, zoom, transparent }: WidgetViewProps) {
  const selectWidget = useDesigner((s) => s.selectWidget);
  const updateWidget = useDesigner((s) => s.updateWidget);

  const r = widget.rect;
  const stroke = colorCss(widget.style.stroke_color);
  const fill = widget.style.fill_color
    ? colorCss(widget.style.fill_color)
    : "transparent";

  function startDrag(e: ReactPointerEvent, mode: "move" | "resize") {
    e.preventDefault();
    e.stopPropagation();
    selectWidget(widget.id);
    (e.target as HTMLElement).setPointerCapture(e.pointerId);
    const startX = e.clientX;
    const startY = e.clientY;
    const startRect = { ...widget.rect };

    function onMove(ev: PointerEvent) {
      const dxMm = (ev.clientX - startX) / zoom;
      const dyMm = (ev.clientY - startY) / zoom;
      if (mode === "move") {
        updateWidget(widget.id, {
          rect: {
            ...startRect,
            x: startRect.x + dxMm,
            y: startRect.y + dyMm,
          },
        });
      } else {
        updateWidget(widget.id, {
          rect: {
            ...startRect,
            width: Math.max(2, startRect.width + dxMm),
            height: Math.max(2, startRect.height + dyMm),
          },
        });
      }
    }
    function onUp(ev: PointerEvent) {
      (e.target as HTMLElement).releasePointerCapture(ev.pointerId);
      window.removeEventListener("pointermove", onMove);
      window.removeEventListener("pointerup", onUp);
    }
    window.addEventListener("pointermove", onMove);
    window.addEventListener("pointerup", onUp);
  }

  return (
    <div
      onPointerDown={(e) => startDrag(e, "move")}
      onClick={(e) => {
        e.stopPropagation();
        selectWidget(widget.id);
      }}
      className={`absolute box-border ${selected ? "ring-2 ring-indigo-500" : transparent ? "ring-1 ring-slate-300/50 hover:ring-slate-400" : "ring-1 ring-slate-300"}`}
      style={{
        left: r.x * zoom,
        top: r.y * zoom,
        width: r.width * zoom,
        height: r.height * zoom,
        background: transparent ? "transparent" : fill,
        cursor: "move",
      }}
      title={widget.kind.kind}
    >
      {!transparent && <WidgetGlyph widget={widget} stroke={stroke} />}
      {selected && (
        <div
          onPointerDown={(e) => startDrag(e, "resize")}
          className="absolute -bottom-1 -right-1 h-3 w-3 cursor-se-resize border border-indigo-500 bg-white"
        />
      )}
    </div>
  );
}

function WidgetGlyph({ widget, stroke }: { widget: Widget; stroke: string }) {
  const k = widget.kind;
  switch (k.kind) {
    case "text_block":
      return (
        <div
          className="px-1 py-0.5 text-slate-700"
          style={{ fontSize: 11, color: stroke }}
        >
          {k.text}
          <span className="ml-1 text-[10px] text-slate-400">
            {k.font_size_mm}mm
          </span>
        </div>
      );
    case "rectangle":
      return null;
    case "ellipse":
      return (
        <div
          className="absolute inset-0 rounded-full"
          style={{ border: `1px solid ${stroke}` }}
        />
      );
    case "checklist":
      return (
        <div className="p-1 text-[10px] text-slate-700">
          {k.items.slice(0, 4).map((it, i) => (
            <div key={i}>☐ {it}</div>
          ))}
        </div>
      );
    case "calendar_month":
      return (
        <div
          className="grid h-full w-full grid-cols-7 grid-rows-5 gap-0.5 p-0.5 text-[8px]"
          style={{ color: stroke }}
        >
          {Array.from({ length: 35 }, (_, i) => (
            <div
              key={i}
              className="rounded-sm border border-slate-200 text-center"
            >
              {i + 1}
            </div>
          ))}
        </div>
      );
    default:
      return (
        <div
          className="flex h-full w-full items-center justify-center text-[10px] uppercase"
          style={{ color: stroke }}
        >
          {k.kind}
        </div>
      );
  }
}

function colorCss(c: { r: number; g: number; b: number; a: number }): string {
  return `rgba(${c.r},${c.g},${c.b},${c.a / 255})`;
}

/**
 * Wrap a single PageTemplate in a synthetic NotebookBundle the WASM
 * viewer can deserialize. Re-rendering the designer surface re-runs
 * `loadNotebook` + `renderPage(0)` against this bundle on every store
 * change, so the user sees the real Vello render of their template
 * while editing. UUIDs are stable per session — the viewer's internal
 * caches survive successive loads.
 */
const SYNTHETIC_NOTEBOOK_ID = "00000000-0000-0000-0000-00000000aaaa";
const SYNTHETIC_SECTION_ID = "00000000-0000-0000-0000-00000000bbbb";
const SYNTHETIC_PAGE_ID = "00000000-0000-0000-0000-00000000cccc";

function wrapTemplateInBundle(
  template: PageTemplate,
  spaZoom: number,
): NotebookBundle {
  const templateId =
    typeof template.id === "string" ? template.id : template.id["0"];
  const now = new Date().toISOString();
  const [pageW, pageH] = template.size_mm;
  // Force the viewport so the Rust viewer's render uses our exact
  // mm→px scale and centers the page in the canvas. The Rust side's
  // fit-to-canvas fallback would otherwise apply a 0.95 margin and
  // produce widgets that drift away from the SPA's overlay rects.
  // Substitute a Blank background — the designer's CSS smart-guide
  // overlay handles grid display, so painting the template's grid
  // bg through Vello would just produce a confusing double grid.
  const previewTemplate: PageTemplate = {
    ...template,
    background: { kind: "Blank" },
    default_viewport: {
      center: { x: pageW / 2, y: pageH / 2 },
      zoom: spaZoom,
      rotation: 0,
    },
  };
  return {
    schema_version: 1,
    notebook: {
      id: SYNTHETIC_NOTEBOOK_ID,
      name: "designer-preview",
      kind: { kind: "Standard" },
      assigned_templates: [templateId],
    },
    sections: [
      {
        id: SYNTHETIC_SECTION_ID,
        notebook_id: SYNTHETIC_NOTEBOOK_ID,
        name: "preview",
        position: 0,
        allowed_templates: null,
        parent_section_id: null,
      },
    ],
    pages: [
      {
        id: SYNTHETIC_PAGE_ID,
        template_id: templateId,
        section_id: SYNTHETIC_SECTION_ID,
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
    strokes_by_page: { [SYNTHETIC_PAGE_ID]: [] },
    asset_refs: {},
  };
}
