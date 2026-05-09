import type { DragEvent as ReactDragEvent, PointerEvent as ReactPointerEvent } from "react";
import { useRef, useState } from "react";

import type { Widget, WidgetKindTag } from "@/types";
import { useDesigner } from "@/store/designerStore";

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

  // Page → screen scale: fit page width into an 800-px-ish surface.
  // We compute on-render so the layout responds to window size.
  const [zoom, setZoom] = useState(2.2); // px/mm

  const [pageW, pageH] = template.size_mm;

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
          · {pageW} × {pageH} mm · snap {snapMm} mm
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
          {template.widgets.map((w) => (
            <WidgetView
              key={w.id}
              widget={w}
              selected={w.id === selectedId}
              zoom={zoom}
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
}

function WidgetView({ widget, selected, zoom }: WidgetViewProps) {
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
      className={`absolute box-border ${selected ? "ring-2 ring-indigo-500" : "ring-1 ring-slate-300"}`}
      style={{
        left: r.x * zoom,
        top: r.y * zoom,
        width: r.width * zoom,
        height: r.height * zoom,
        background: fill,
        cursor: "move",
      }}
      title={widget.kind.kind}
    >
      <WidgetGlyph widget={widget} stroke={stroke} />
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

// Re-export for tests; defaultKindFor isn't used here but the module
// hierarchy lints if unused imports are present.
void defaultKindFor;
