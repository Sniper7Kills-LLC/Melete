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
