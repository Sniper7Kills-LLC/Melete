// Sequential Vello-rendered thumbnail pipeline for Gallery cards (#92).
//
// Browsers cap WebGPU canvas contexts per page (~8 in Chrome). An
// N-card grid can't each own a viewer. Instead this module:
//
// 1. Constructs a single private `Viewer` instance (separate from the
//    `viewer` singleton used by /designer + /demo) bound to a hidden
//    off-screen canvas.
// 2. Queues thumbnail render jobs and processes them serially.
// 3. After each render, `canvas.toDataURL('image/png')` snapshots the
//    canvas and resolves the job with a data URL the card can pop
//    into an `<img>`.
//
// Falls back to `null` on init failure (WebGPU missing / WASM not
// loaded) so callers can render the SVG `PageThumbnail` instead.

import type { PageTemplate } from "@/types";
import { wrapTemplateForPreview } from "@/components/TemplatePreview";

interface ViewerMod {
  default(): Promise<unknown>;
  Viewer: new () => {
    init(canvas: HTMLCanvasElement): Promise<void>;
    loadNotebook(bytes: Uint8Array): void;
    renderPage(i: number, w: number, h: number): void;
    renderPageToRgba(i: number, w: number, h: number): Promise<Uint8Array>;
  };
}

interface Job {
  template: PageTemplate;
  w: number;
  h: number;
  resolve(url: string | null): void;
}

let modPromise: Promise<ViewerMod | null> | null = null;

function loadMod(): Promise<ViewerMod | null> {
  if (modPromise) return modPromise;
  modPromise = (async () => {
    if (typeof navigator === "undefined" || !("gpu" in navigator)) {
      console.warn("[thumbnailer] WebGPU not available; falling back to SVG");
      return null;
    }
    try {
      const path = "/wasm/viewer/melete_web_viewer.js";
      const m: ViewerMod = await import(/* @vite-ignore */ path);
      await m.default();
      return m;
    } catch (e) {
      console.warn("[thumbnailer] WASM viewer load failed", e);
      return null;
    }
  })();
  return modPromise;
}

let queue: Job[] = [];
let busy = false;
let canvas: HTMLCanvasElement | null = null;
let viewer: InstanceType<ViewerMod["Viewer"]> | null = null;

async function ensureViewer(mod: ViewerMod): Promise<void> {
  if (canvas && viewer) return;
  canvas = document.createElement("canvas");
  canvas.width = 1;
  canvas.height = 1;
  canvas.style.position = "fixed";
  canvas.style.left = "-9999px";
  canvas.style.top = "-9999px";
  canvas.style.pointerEvents = "none";
  document.body.appendChild(canvas);
  viewer = new mod.Viewer();
  await viewer.init(canvas);
}

async function pump(): Promise<void> {
  if (busy) return;
  busy = true;
  try {
    const mod = await loadMod();
    if (!mod) {
      for (const job of queue) job.resolve(null);
      queue = [];
      return;
    }
    await ensureViewer(mod);
    while (queue.length) {
      const job = queue.shift()!;
      try {
        const bundle = wrapTemplateForPreview(job.template, 1.0, {
          keepBackground: true,
        });
        const bytes = new TextEncoder().encode(JSON.stringify(bundle));
        viewer!.loadNotebook(bytes);
        // Render via the Rust off-screen path — `canvas.toDataURL` on
        // a live wgpu surface returns transparent on Chrome/Firefox
        // because the swap chain doesn't preserve the drawing buffer.
        // `renderPageToRgba` allocates a separate texture, copies to
        // a staging buffer, maps async, returns the raw RGBA bytes.
        const rgba = await viewer!.renderPageToRgba(0, job.w, job.h);
        const url = rgbaToPngDataUrl(rgba, job.w, job.h);
        job.resolve(url);
      } catch (e) {
        console.warn("[thumbnailer] render failed", e);
        job.resolve(null);
      }
    }
  } finally {
    busy = false;
  }
}

/** Pack a Vello-emitted RGBA8 buffer into a `data:image/png` URL via a
 *  scratch 2D canvas. Cheap (<5 ms for 280×360); a single scratch
 *  canvas is reused across calls. */
let encodeCanvas: HTMLCanvasElement | null = null;
function rgbaToPngDataUrl(rgba: Uint8Array, w: number, h: number): string {
  if (!encodeCanvas) encodeCanvas = document.createElement("canvas");
  encodeCanvas.width = w;
  encodeCanvas.height = h;
  const ctx = encodeCanvas.getContext("2d");
  if (!ctx) throw new Error("2d context unavailable");
  const imageData = ctx.createImageData(w, h);
  imageData.data.set(rgba);
  ctx.putImageData(imageData, 0, 0);
  return encodeCanvas.toDataURL("image/png");
}

/** Queue a thumbnail render. Resolves with a `data:image/png` URL on
 *  success or `null` on WebGPU/WASM failure. Calls are serialized; the
 *  N+1th job waits for the Nth to finish. */
export function thumbnailFor(
  template: PageTemplate,
  w: number,
  h: number,
): Promise<string | null> {
  return new Promise((resolve) => {
    queue.push({ template, w, h, resolve });
    void pump();
  });
}
