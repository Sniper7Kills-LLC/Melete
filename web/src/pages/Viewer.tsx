import { useEffect, useRef, useState } from "react";

import type { NotebookBundle } from "@/types";
import { viewer } from "@/wasm";

/**
 * Viewer route — fetches `/sample-notebook.json`, hands the bytes to
 * the (mock) WASM viewer, and exposes pan / zoom controls + a page
 * selector. When real WASM lands, the placeholder canvas already has
 * the right life-cycle (init → loadNotebook → renderPage on size /
 * page changes), so swapping the import in `@/wasm` is enough.
 */
export function Viewer() {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const [bundle, setBundle] = useState<NotebookBundle | null>(null);
  const [pageIndex, setPageIndex] = useState(0);
  const [error, setError] = useState<string | null>(null);
  const [webgpuMissing, setWebgpuMissing] = useState(false);
  const [ready, setReady] = useState(false);

  // Load bundle once on mount.
  useEffect(() => {
    let cancelled = false;
    const hasWebGpu =
      typeof navigator !== "undefined" && "gpu" in navigator;
    if (!hasWebGpu) setWebgpuMissing(true);

    fetch("/sample-notebook.json")
      .then((r) => {
        if (!r.ok) throw new Error(`Fetch failed: ${r.status}`);
        return r.arrayBuffer();
      })
      .then(async (buf) => {
        if (cancelled) return;
        const bytes = new Uint8Array(buf);
        const parsed = JSON.parse(
          new TextDecoder().decode(bytes),
        ) as NotebookBundle;
        setBundle(parsed);

        if (!hasWebGpu) return;

        const c = canvasRef.current;
        if (!c) return;
        try {
          await viewer.init(c);
        } catch (e) {
          console.warn("[viewer] WebGPU init failed:", e);
          if (!cancelled) setWebgpuMissing(true);
          return;
        }
        await viewer.loadNotebook(bytes);
        setReady(true);
      })
      .catch((e: unknown) => {
        console.error(e);
        setError(e instanceof Error ? e.message : String(e));
      });
    return () => {
      cancelled = true;
    };
  }, []);

  // Re-render on page change or container resize.
  useEffect(() => {
    if (!ready) return;
    const c = canvasRef.current;
    if (!c) return;
    const parent = c.parentElement;
    if (!parent) return;

    function render() {
      if (!c || !parent) return;
      const rect = parent.getBoundingClientRect();
      const w = Math.max(200, Math.floor(rect.width));
      const h = Math.max(200, Math.floor(rect.height));
      viewer.renderPage(pageIndex, w, h);
    }
    render();

    const ro = new ResizeObserver(render);
    ro.observe(parent);
    return () => ro.disconnect();
  }, [ready, pageIndex]);

  // Pan + zoom gestures (#36). Wheel for trackpad/mouse, two-finger
  // pointer events for touchscreen/trackpad pinch. Mirrors the
  // desktop `attach_pan_zoom` behaviour: anchor zoom at the cursor,
  // apply pan + zoom each frame from the gesture's running delta.
  useEffect(() => {
    if (!ready) return;
    const c = canvasRef.current;
    if (!c) return;

    // Cursor-anchored re-render helper. Re-renders the current page
    // so the wasm viewer's pan/zoom state shows up.
    function rerenderCurrent() {
      const parent = c?.parentElement;
      if (!c || !parent) return;
      const rect = parent.getBoundingClientRect();
      const w = Math.max(200, Math.floor(rect.width));
      const h = Math.max(200, Math.floor(rect.height));
      viewer.renderPage(pageIndex, w, h);
    }

    function canvasPoint(ev: { clientX: number; clientY: number }): [number, number] {
      const r = c!.getBoundingClientRect();
      // Scale CSS pixels → backing-store pixels (devicePixelRatio
      // already baked into c.width).
      const sx = (ev.clientX - r.left) * (c!.width / r.width);
      const sy = (ev.clientY - r.top) * (c!.height / r.height);
      return [sx, sy];
    }

    function onWheel(ev: WheelEvent) {
      ev.preventDefault();
      // Per-notch factor matches the desktop default (1.1× per click).
      // deltaMode === DOM_DELTA_LINE on Firefox needs the same sign
      // handling as DOM_DELTA_PIXEL on Chrome.
      const factor = ev.deltaY > 0 ? 1 / 1.1 : 1.1;
      const [sx, sy] = canvasPoint(ev);
      viewer.zoomAt(sx, sy, factor);
      rerenderCurrent();
    }

    // Pinch + pan via PointerEvent. Maintain at most two active
    // pointers; on each move recompute the center + distance and
    // apply (1) pan from center delta and (2) zoom from distance
    // ratio. One-finger drags pan only.
    type TrackedPointer = { x: number; y: number };
    const active = new Map<number, TrackedPointer>();
    let lastCenter: [number, number] | null = null;
    let lastDistance: number | null = null;

    function recomputeCentre(): [number, number, number] {
      const pts = Array.from(active.values()).slice(0, 2);
      const cx = pts.reduce((s, p) => s + p.x, 0) / pts.length;
      const cy = pts.reduce((s, p) => s + p.y, 0) / pts.length;
      const dist =
        pts.length === 2
          ? Math.hypot(pts[0].x - pts[1].x, pts[0].y - pts[1].y)
          : 0;
      return [cx, cy, dist];
    }

    function onPointerDown(ev: PointerEvent) {
      // Only handle touch + pen — leaves mouse drags (toolbar usage,
      // selection in the wrapping page) to the browser defaults.
      if (ev.pointerType === "mouse") return;
      c!.setPointerCapture(ev.pointerId);
      active.set(ev.pointerId, { x: ev.clientX, y: ev.clientY });
      const [cx, cy, dist] = recomputeCentre();
      lastCenter = [cx, cy];
      lastDistance = active.size === 2 ? dist : null;
    }

    function onPointerMove(ev: PointerEvent) {
      if (ev.pointerType === "mouse") return;
      if (!active.has(ev.pointerId)) return;
      active.set(ev.pointerId, { x: ev.clientX, y: ev.clientY });
      const [cx, cy, dist] = recomputeCentre();

      if (lastCenter) {
        // Pan delta in client (CSS) pixels → backing-store pixels.
        const r = c!.getBoundingClientRect();
        const sxRatio = c!.width / r.width;
        const syRatio = c!.height / r.height;
        const dx = (cx - lastCenter[0]) * sxRatio;
        const dy = (cy - lastCenter[1]) * syRatio;
        if (dx !== 0 || dy !== 0) viewer.pan(dx, dy);
      }

      if (active.size === 2 && lastDistance && dist > 0) {
        const factor = dist / lastDistance;
        if (factor > 0 && Number.isFinite(factor) && factor !== 1) {
          // Anchor at the midpoint of the two fingers in canvas
          // backing coords.
          const anchor = canvasPoint({ clientX: cx, clientY: cy });
          viewer.zoomAt(anchor[0], anchor[1], factor);
        }
      }

      lastCenter = [cx, cy];
      lastDistance = active.size === 2 ? dist : null;
      rerenderCurrent();
    }

    function onPointerUp(ev: PointerEvent) {
      if (ev.pointerType === "mouse") return;
      active.delete(ev.pointerId);
      c!.releasePointerCapture(ev.pointerId);
      if (active.size === 0) {
        lastCenter = null;
        lastDistance = null;
      } else {
        const [cx, cy, dist] = recomputeCentre();
        lastCenter = [cx, cy];
        lastDistance = active.size === 2 ? dist : null;
      }
    }

    c.addEventListener("wheel", onWheel, { passive: false });
    c.addEventListener("pointerdown", onPointerDown);
    c.addEventListener("pointermove", onPointerMove);
    c.addEventListener("pointerup", onPointerUp);
    c.addEventListener("pointercancel", onPointerUp);

    return () => {
      c.removeEventListener("wheel", onWheel);
      c.removeEventListener("pointerdown", onPointerDown);
      c.removeEventListener("pointermove", onPointerMove);
      c.removeEventListener("pointerup", onPointerUp);
      c.removeEventListener("pointercancel", onPointerUp);
    };
  }, [ready, pageIndex]);

  function panBy(dx: number, dy: number) {
    viewer.pan(dx, dy);
  }
  function zoomCanvas(factor: number) {
    const c = canvasRef.current;
    if (!c) return;
    viewer.zoomAt(c.width / 2, c.height / 2, factor);
  }

  return (
    <div className="flex h-full flex-col">
      <div className="flex items-center gap-3 border-b border-slate-200 bg-white px-4 py-2">
        <label className="flex items-center gap-2 text-sm text-slate-700">
          Page:
          <select
            className="rounded border border-slate-300 bg-white px-2 py-1 text-sm"
            value={pageIndex}
            onChange={(e) => setPageIndex(Number(e.target.value))}
            disabled={!bundle}
          >
            {bundle?.pages.map((p, i) => (
              <option key={p.id} value={i}>
                {p.name || `Page ${i + 1}`}
              </option>
            ))}
            {!bundle && <option>(loading…)</option>}
          </select>
        </label>

        <div className="ml-4 flex items-center gap-1 text-sm">
          <span className="text-slate-500">Pan:</span>
          <button onClick={() => panBy(30, 0)} className={btn()}>
            ←
          </button>
          <button onClick={() => panBy(-30, 0)} className={btn()}>
            →
          </button>
          <button onClick={() => panBy(0, 30)} className={btn()}>
            ↑
          </button>
          <button onClick={() => panBy(0, -30)} className={btn()}>
            ↓
          </button>
        </div>

        <div className="ml-4 flex items-center gap-1 text-sm">
          <span className="text-slate-500">Zoom:</span>
          <button onClick={() => zoomCanvas(1.2)} className={btn()}>
            +
          </button>
          <button onClick={() => zoomCanvas(1 / 1.2)} className={btn()}>
            −
          </button>
        </div>

        <div className="ml-auto text-xs text-slate-400">
          {bundle ? (
            <>
              <span className="font-medium text-slate-600">
                {bundle.notebook.name}
              </span>{" "}
              · {bundle.pages.length} page(s) · schema v{bundle.schema_version}
            </>
          ) : error ? (
            <span className="text-red-600">{error}</span>
          ) : (
            "loading sample notebook…"
          )}
        </div>
      </div>

      <div className="relative flex-1 overflow-hidden bg-slate-100">
        <canvas
          ref={canvasRef}
          className="absolute inset-0 h-full w-full"
          // Block native long-press / selection menus on the canvas
          // (matches the docs/web-portal.md §5.4 read-only constraint).
          style={{ touchAction: "none", userSelect: "none" }}
        />
        {webgpuMissing && (
          <div className="absolute inset-0 flex items-center justify-center bg-slate-100/95 p-8">
            <div className="max-w-md rounded-lg border border-amber-300 bg-amber-50 px-6 py-5 text-amber-900 shadow-sm">
              <div className="text-base font-semibold">
                WebGPU required
              </div>
              <div className="mt-2 text-sm leading-relaxed">
                The viewer renders strokes via wgpu/Vello on a WebGPU
                surface. Your browser doesn't expose{" "}
                <code className="rounded bg-amber-100 px-1">
                  navigator.gpu
                </code>{" "}
                or no compatible GPU adapter is available.
              </div>
              <ul className="mt-3 list-disc pl-5 text-sm">
                <li>
                  Chrome / Edge 113+ on Linux: enable{" "}
                  <code className="rounded bg-amber-100 px-1">
                    chrome://flags/#enable-unsafe-webgpu
                  </code>
                  .
                </li>
                <li>
                  Firefox: set{" "}
                  <code className="rounded bg-amber-100 px-1">
                    dom.webgpu.enabled
                  </code>{" "}
                  in <code>about:config</code>, restart.
                </li>
                <li>
                  Headless / VM environments may have no GPU adapter
                  even with the flag on.
                </li>
              </ul>
            </div>
          </div>
        )}
        {!ready && !error && !webgpuMissing && (
          <div className="absolute inset-0 flex items-center justify-center text-slate-500">
            loading…
          </div>
        )}
      </div>
    </div>
  );
}

function btn(): string {
  return "rounded border border-slate-300 bg-white px-2 py-1 text-sm hover:bg-slate-100 active:bg-slate-200";
}
