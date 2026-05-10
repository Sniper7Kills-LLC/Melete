// /n/:id route — fetches a notebook + its sections + pages + strokes
// from AppSync (one query per layer) and subscribes to RemoteStroke
// for live append-as-you-draw updates from the desktop.
//
// No S3 dep — every byte sits in DynamoDB rows the user can inspect
// or delete from the AppSync console.

import { useEffect, useMemo, useRef, useState } from "react";
import { useParams } from "react-router-dom";
import { useAuthenticator } from "@aws-amplify/ui-react";

import type { NotebookBundle, Stroke } from "@/types";
import { viewer } from "@/wasm";
import {
  client,
  type NotebookRow,
  type RemotePageRow,
  type RemoteSectionRow,
  type RemoteStrokeRow,
} from "@/amplify-client";

interface LoadState {
  status: "idle" | "loading" | "ready" | "error";
  message?: string;
}

export function NotebookViewer() {
  const { id: notebookId } = useParams<{ id: string }>();
  // Subscription / poll auth: prefer Cognito userPool when the user
  // is signed in (apiKey subscriptions throttle aggressively under
  // bursts; userPool is per-user-quota and has the higher cap).
  const { authStatus } = useAuthenticator((ctx) => [ctx.authStatus]);
  const liveAuthMode: "apiKey" | "userPool" =
    authStatus === "authenticated" ? "userPool" : "apiKey";
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const bundleRef = useRef<NotebookBundle | null>(null);
  const seenStrokeIds = useRef<Set<string>>(new Set());
  // Tombstones — stroke ids the user / desktop has deleted. We keep
  // them around forever so a stale poll tick (which fires every 2 s
  // and may briefly see a deleted row before AppSync's GET picks up
  // the delete) doesn't re-add the stroke and cause the visible
  // disappear/reappear flicker during eraser sessions.
  const deletedStrokeIds = useRef<Set<string>>(new Set());
  /// Per-id LWW clock — last `updatedAtIso` we've applied. Events
  /// with an older clock get discarded so out-of-order delivery
  /// can never resurrect a deleted stroke or undo a newer payload.
  const seenUpdatedAt = useRef<Map<string, string>>(new Map());
  // Debounced loadNotebook scheduler. Each subscription / poll event
  // marks the bundle dirty; a single 120 ms timer flushes the latest
  // bundle to the WASM viewer once. Without this, every stroke
  // create / update / delete triggered its own loadNotebook, which
  // resets the viewport (random zoom-in/zoom-out the user reported).
  const reloadTimer = useRef<number | null>(null);
  const pendingDelta = useRef<number>(0);
  // Viewport state mirrored on the JS side so we can replay it after
  // every loadNotebook. The WASM viewer resets pan + zoom on each
  // bundle reload — without this, panning out + drawing on the
  // desktop snaps the web view back to the origin every time strokes
  // arrive.
  const viewportRef = useRef<{ pan_x: number; pan_y: number; zoom: number }>({
    pan_x: 0,
    pan_y: 0,
    zoom: 1,
  });
  // Live mirror of `pageId` so the scheduleReload closure (which
  // doesn't re-bind on every state change) can resolve the current
  // global render index without going through React state.
  const pageIdRefForRender = useRef<string | null>(null);
  const [load, setLoad] = useState<LoadState>({ status: "idle" });
  const [bundle, setBundle] = useState<NotebookBundle | null>(null);
  // Page selection is keyed by stable id, not index. Pages reorder
  // every poll-tick when the desktop adds / deletes / reorders, so a
  // bare index would point at the wrong slot after each refresh.
  // `pageIndex` is *derived* from pageId + the sorted pages list and
  // passed to viewer.renderPage.
  const [pageId, setPageId] = useState<string | null>(null);
  const [sectionId, setSectionId] = useState<string | null>(null);
  const [meta, setMeta] = useState<NotebookRow | null>(null);
  const [webgpuMissing, setWebgpuMissing] = useState(false);
  const [liveCount, setLiveCount] = useState(0);
  const [liveStatus, setLiveStatus] = useState<"off" | "on" | "error">("off");
  const [liveEnabled, setLiveEnabled] = useState(true);

  const ready = load.status === "ready";

  // Keep the ref in sync with React state — used by `scheduleReload`
  // closure where re-binding to fresh state isn't an option.
  useEffect(() => {
    pageIdRefForRender.current = pageId;
  }, [pageId]);

  useEffect(() => {
    if (!notebookId) return;
    let cancelled = false;
    setLoad({ status: "loading" });

    const hasWebGpu = typeof navigator !== "undefined" && "gpu" in navigator;
    if (!hasWebGpu) setWebgpuMissing(true);

    (async () => {
      try {
        const auth = "apiKey" as const;
        const nb = await client.models.Notebook.get(
          { id: notebookId },
          { authMode: auth },
        );
        if (cancelled) return;
        if (!nb.data) throw new Error("Notebook not found");
        setMeta(nb.data);

        const [secsRes, pagesRes, strokesRes] = await Promise.all([
          client.models.RemoteSection.listRemoteSectionsByNotebook(
            { notebookId },
            { authMode: auth, limit: 1000 },
          ),
          client.models.RemotePage.listRemotePagesByNotebook(
            { notebookId },
            { authMode: auth, limit: 1000 },
          ),
          client.models.RemoteStroke.listRemoteStrokesByNotebook(
            { notebookId },
            { authMode: auth, limit: 5000 },
          ),
        ]);
        if (cancelled) return;

        // Filter out soft-deleted strokes from the initial pull —
        // cloud keeps the row as a tombstone but we don't want to
        // render or count it locally. Also seed seenUpdatedAt so a
        // late-arriving subscription event with an older clock for
        // any of these ids gets ignored (LWW).
        const liveStrokes = (strokesRes.data ?? []).filter(
          (s) => !s.deletedAtIso,
        );
        for (const s of strokesRes.data ?? []) {
          seenStrokeIds.current.add(s.id);
          const clock = s.updatedAtIso ?? s.createdAt;
          if (clock) seenUpdatedAt.current.set(s.id, clock);
          if (s.deletedAtIso) deletedStrokeIds.current.add(s.id);
        }
        const built = buildBundle(
          nb.data,
          secsRes.data ?? [],
          pagesRes.data ?? [],
          liveStrokes,
        );
        bundleRef.current = built;
        setBundle(built);

        if (!hasWebGpu) {
          setLoad({ status: "ready" });
          return;
        }
        const c = canvasRef.current;
        if (!c) return;
        try {
          await viewer.init(c);
        } catch (e) {
          console.warn("[notebook-viewer] WebGPU init failed:", e);
          if (!cancelled) {
            setWebgpuMissing(true);
            setLoad({ status: "ready" });
          }
          return;
        }
        const bytes = new TextEncoder().encode(JSON.stringify(built));
        await viewer.loadNotebook(bytes);
        if (!cancelled) setLoad({ status: "ready" });
      } catch (e) {
        if (cancelled) return;
        const msg = e instanceof Error ? e.message : String(e);
        setLoad({ status: "error", message: msg });
      }
    })();

    return () => {
      cancelled = true;
    };
  }, [notebookId]);

  // pageIndex is needed by the render effect below, so resolve it
  // here against the WASM viewer's bundle.pages order — which mirrors
  // the order we hand to viewer.loadNotebook (i.e. the raw bundle
  // pages array, not our display-sorted view).
  const renderIndex = useMemo(() => {
    if (!pageId || !bundle) return 0;
    const idx = bundle.pages.findIndex((p) => p.id === pageId);
    return idx >= 0 ? idx : 0;
  }, [bundle, pageId]);

  // Re-render on page change or container resize.
  useEffect(() => {
    if (!ready || webgpuMissing) return;
    const c = canvasRef.current;
    if (!c) return;
    const parent = c.parentElement;
    if (!parent) return;
    function render() {
      if (!c || !parent) return;
      const rect = parent.getBoundingClientRect();
      const w = Math.max(200, Math.floor(rect.width));
      const h = Math.max(200, Math.floor(rect.height));
      viewer.renderPage(renderIndex, w, h);
    }
    render();
    const ro = new ResizeObserver(render);
    ro.observe(parent);
    return () => ro.disconnect();
  }, [ready, renderIndex, webgpuMissing, liveCount]);

  // Live updates. We layer a 2 s poll over the WebSocket subscription:
  // the subscription is fast-but-fragile (apiKey auth on owner-scoped
  // models is finicky); the poll is slow-but-reliable (just a list
  // query). Whichever delivers first wins thanks to seenStrokeIds
  // dedup.
  useEffect(() => {
    if (!notebookId || !ready || !liveEnabled) {
      setLiveStatus("off");
      return;
    }
    console.info(
      "[notebook-viewer] live ON for",
      notebookId,
      "(subscription + 2s poll)",
    );
    setLiveStatus("on");

    /**
     * Mark the bundle dirty + schedule a single debounced reload.
     * Many subscription events arriving in a 120 ms window collapse
     * into one viewer.loadNotebook call so the WASM viewer doesn't
     * reset its viewport once per event.
     */
    const scheduleReload = (delta: number) => {
      pendingDelta.current += delta;
      if (reloadTimer.current !== null) return;
      reloadTimer.current = window.setTimeout(() => {
        reloadTimer.current = null;
        const cur = bundleRef.current;
        if (!cur) return;
        const json = JSON.stringify(cur);
        const bytes = new TextEncoder().encode(json);
        const accumulated = pendingDelta.current;
        pendingDelta.current = 0;
        void viewer.loadNotebook(bytes).then(() => {
          // The WASM viewer now preserves its transform across
          // reloads (see `load_notebook` — only inits when None),
          // so no JS-side viewport restore is needed. Reapplying
          // cached pan/zoom here would COMPOUND the transform on
          // every reload.
          // DON'T call renderPage here — the React useEffect that
          // watches `liveCount` already triggers it. Calling it
          // synchronously alongside React's render path triggers
          // wasm-bindgen "recursive use of an object" because the
          // viewer doesn't allow re-entry.
          setLiveCount((n) => n + accumulated);
        });
      }, 300);
    };

    const append = (rows: RemoteStrokeRow[]) => {
      const cur = bundleRef.current;
      if (!cur) {
        console.debug("[notebook-viewer] append: no bundle");
        return;
      }
      let appended = 0;
      let skipped = 0;
      let tombstoned = 0;
      for (const ev of rows) {
        if (!ev || ev.notebookId !== notebookId) continue;
        const lastSeen = seenUpdatedAt.current.get(ev.id);
        const incoming = ev.updatedAtIso ?? ev.createdAt;
        if (lastSeen && incoming && lastSeen >= incoming) {
          skipped += 1;
          continue;
        }
        if (ev.deletedAtIso) {
          deletedStrokeIds.current.add(ev.id);
          for (const [pageId, list] of Object.entries(cur.strokes_by_page)) {
            const next = list.filter(
              (s) => (s as unknown as { id?: string }).id !== ev.id,
            );
            if (next.length !== list.length) {
              cur.strokes_by_page[pageId] = next;
              tombstoned += 1;
            }
          }
          if (incoming) seenUpdatedAt.current.set(ev.id, incoming);
          continue;
        }
        const stroke = parseStroke(ev.strokeJson);
        if (!stroke) continue;
        let replaced = false;
        for (const list of Object.values(cur.strokes_by_page)) {
          const idx = list.findIndex(
            (s) => (s as unknown as { id?: string }).id === ev.id,
          );
          if (idx >= 0) {
            list[idx] = stroke;
            replaced = true;
            break;
          }
        }
        if (!replaced) {
          const list = cur.strokes_by_page[ev.pageId] ?? [];
          list.push(stroke);
          cur.strokes_by_page[ev.pageId] = list;
        }
        seenStrokeIds.current.add(ev.id);
        if (incoming) seenUpdatedAt.current.set(ev.id, incoming);
        appended += 1;
      }
      if (appended === 0 && tombstoned === 0) return;
      scheduleReload(appended || tombstoned);
    };

    /**
     * Reconcile deletes the subscription dropped. Compare full remote
     * stroke list to local; any local stroke whose id isn't present
     * remotely gets tombstoned + dropped. The earlier flicker bug
     * (delete-then-immediate-readd) is gone now that tombstones are
     * permanent — we never re-add a tombstoned id even if a stale
     * poll tick sees it briefly.
     */
    const reconcileDeletes = (rows: RemoteStrokeRow[]) => {
      const cur = bundleRef.current;
      if (!cur) return;
      const remoteIds = new Set<string>();
      for (const r of rows) {
        if (r.notebookId === notebookId) remoteIds.add(r.id);
      }
      let mutated = false;
      for (const [pageId, list] of Object.entries(cur.strokes_by_page)) {
        const kept = list.filter((s) => {
          const localId = (s as unknown as { id?: string }).id;
          if (!localId) return true;
          if (remoteIds.has(localId)) return true;
          // Local has it; remote doesn't. Treat as deletion.
          deletedStrokeIds.current.add(localId);
          mutated = true;
          return false;
        });
        if (kept.length !== list.length) {
          cur.strokes_by_page[pageId] = kept;
        }
      }
      if (mutated) scheduleReload(0);
    };

    /**
     * Reconcile updates: same id, different body. Replace in place.
     */
    const reconcileUpdates = (rows: RemoteStrokeRow[]) => {
      const cur = bundleRef.current;
      if (!cur) return;
      const remoteById = new Map<string, RemoteStrokeRow>();
      for (const r of rows) {
        if (r.notebookId === notebookId) remoteById.set(r.id, r);
      }
      let mutated = false;
      for (const list of Object.values(cur.strokes_by_page)) {
        for (let i = 0; i < list.length; i++) {
          const localId = (list[i] as unknown as { id?: string }).id;
          if (!localId) continue;
          const remote = remoteById.get(localId);
          if (!remote) continue;
          const decoded = parseStroke(remote.strokeJson);
          if (!decoded) continue;
          const lp = JSON.stringify(
            (list[i] as unknown as { points?: unknown }).points,
          );
          const rp = JSON.stringify(
            (decoded as unknown as { points?: unknown }).points,
          );
          if (lp !== rp) {
            list[i] = decoded;
            mutated = true;
          }
        }
      }
      if (mutated) scheduleReload(0);
    };

    // Auto-generated onCreate/onDelete subs: kept here in case the
    // legacy per-row mutations fire (e.g. from a back-fill). Errors
    // are non-fatal — `onStrokesBatchSync` is the primary path.
    const subCreate = client.models.RemoteStroke.onCreate({
      filter: { notebookId: { eq: notebookId } },
      authMode: liveAuthMode,
    }).subscribe({
      next: (ev) => {
        // Log every event the subscription delivers, regardless of
        // dedup / tombstone outcome. Lets you trace whether the WS
        // is alive at all.
        console.log("[notebook-viewer] sub onCreate", {
          id: ev?.id,
          page: ev?.pageId,
          notebook: ev?.notebookId,
        });
        if (ev) append([ev]);
      },
      error: (err) => {
        console.warn("[notebook-viewer] onCreate error", err);
        setLiveStatus("error");
      },
    });

    // Per-event delete: drop the stroke immediately rather than
    // waiting for the next poll tick. Big difference for eraser
    // sessions where dozens of deletes fire at once.
    const subDelete = client.models.RemoteStroke.onDelete({
      filter: { notebookId: { eq: notebookId } },
      authMode: liveAuthMode,
    }).subscribe({
      next: (ev) => {
        console.log("[notebook-viewer] sub onDelete", { id: ev?.id });
        const cur = bundleRef.current;
        if (!cur || !ev?.id) return;
        deletedStrokeIds.current.add(ev.id);
        let mutated = false;
        for (const [pageId, list] of Object.entries(cur.strokes_by_page)) {
          const next = list.filter(
            (s) => (s as unknown as { id?: string }).id !== ev.id,
          );
          if (next.length !== list.length) {
            cur.strokes_by_page[pageId] = next;
            mutated = true;
          }
        }
        if (!mutated) return;
        scheduleReload(0);
      },
      error: (err) => {
        console.warn("[notebook-viewer] onDelete error", err);
      },
    });

    // Batch fan-out subscription. The desktop's worker pool now
    // sends create+delete ops via the Lambda-backed
    // `syncStrokesBatch` mutation, which writes to DynamoDB
    // BatchWriteItem and BYPASSES AppSync's auto-generated
    // onCreate/onDelete subscriptions. This subscription fires
    // once per batch with the affected id arrays so we can fan
    // them out as create / delete events on the bundle.
    const subBatch = client.subscriptions.onStrokesBatchSync(
      { notebookId },
      { authMode: liveAuthMode },
    ).subscribe({
      next: (ev) => {
        if (!ev) return;
        const ids = ev.ids ?? [];
        console.log("[notebook-viewer] sub onStrokesBatchSync", {
          ids: ids.length,
        });
        if (ids.length === 0) return;
        // Event payload only carries ids — fetch only those rows
        // via a filtered list query. Avoids hauling the entire
        // notebook back per subscription event.
        void client.models.RemoteStroke.list({
          filter: { id: { in: ids as unknown[] } as { eq?: unknown } },
          authMode: liveAuthMode,
          limit: ids.length + 5,
        } as unknown as Parameters<typeof client.models.RemoteStroke.list>[0]).then(
          (res) => {
            if (!res.data) return;
            if (res.data.length > 0) append(res.data);
          },
        );
      },
      error: (err) => {
        const e = err as { errors?: { message?: string }[] };
        const msgs = e.errors?.map((x) => x.message).filter(Boolean) ?? [];
        console.warn(
          "[notebook-viewer] onStrokesBatchSync error:",
          msgs.length > 0 ? msgs : err,
        );
      },
    });

    // Per-event update: replace the stroke's points in place.
    const subUpdate = client.models.RemoteStroke.onUpdate({
      filter: { notebookId: { eq: notebookId } },
      authMode: liveAuthMode,
    }).subscribe({
      next: (ev) => {
        console.log("[notebook-viewer] sub onUpdate", {
          id: ev?.id,
          hasJson: !!ev?.strokeJson,
        });
        const cur = bundleRef.current;
        if (!cur || !ev?.id || !ev.strokeJson) return;
        const decoded = parseStroke(ev.strokeJson);
        if (!decoded) return;
        let mutated = false;
        for (const [pageId, list] of Object.entries(cur.strokes_by_page)) {
          const idx = list.findIndex(
            (s) => (s as unknown as { id?: string }).id === ev.id,
          );
          if (idx >= 0) {
            list[idx] = decoded;
            cur.strokes_by_page[pageId] = list;
            mutated = true;
          }
        }
        if (!mutated) return;
        scheduleReload(0);
      },
      error: (err) => {
        console.warn("[notebook-viewer] onUpdate error", err);
      },
    });

    const poll = window.setInterval(async () => {
      try {
        const [strokesRes, pagesRes, secsRes] = await Promise.all([
          client.models.RemoteStroke.listRemoteStrokesByNotebook(
            { notebookId },
            { authMode: liveAuthMode, limit: 5000 },
          ),
          client.models.RemotePage.listRemotePagesByNotebook(
            { notebookId },
            { authMode: liveAuthMode, limit: 1000 },
          ),
          client.models.RemoteSection.listRemoteSectionsByNotebook(
            { notebookId },
            { authMode: liveAuthMode, limit: 1000 },
          ),
        ]);
        if (strokesRes.data) {
          append(strokesRes.data);
          reconcileDeletes(strokesRes.data);
          reconcileUpdates(strokesRes.data);
        }
        // Page / section deltas: rebuild bundle when the count or any
        // id / position differs. Cheap O(N) compare; any change
        // triggers a viewer reload so new pages appear in the
        // selector and deleted ones drop out.
        const cur = bundleRef.current;
        if (cur && pagesRes.data && secsRes.data) {
          const remotePages = pagesRes.data;
          const remoteSecs = secsRes.data;
          // Order-independent diff: compare id sets + per-id (name,
          // position, section) tuples. Index-based comparison
          // misfires every poll since the GSI sort can re-shuffle
          // when pages get added/deleted.
          const same = (
            cur: NotebookBundle,
            secs: typeof remoteSecs,
            pgs: typeof remotePages,
          ): boolean => {
            if (cur.sections.length !== secs.length) return false;
            if (cur.pages.length !== pgs.length) return false;
            const localSecs = new Map<string, { name: string; position: number }>();
            for (const s of cur.sections)
              localSecs.set(s.id, { name: s.name, position: s.position });
            for (const s of secs) {
              const l = localSecs.get(s.id);
              if (!l || l.name !== s.name || l.position !== s.position) return false;
            }
            const localPages = new Map<string, { name: string; position: number; section_id: string }>();
            for (const p of cur.pages)
              localPages.set(p.id, {
                name: p.name,
                position: p.position,
                section_id: p.section_id,
              });
            for (const p of pgs) {
              const l = localPages.get(p.id);
              if (
                !l ||
                l.name !== (p.name ?? "") ||
                l.position !== p.position ||
                l.section_id !== p.sectionId
              )
                return false;
            }
            return true;
          };
          if (!same(cur, remoteSecs, remotePages)) {
            console.info(
              "[notebook-viewer] page/section delta detected — reloading bundle",
            );
            const next = buildBundleMerge(cur, remoteSecs, remotePages);
            bundleRef.current = next;
            setBundle(next);
            scheduleReload(0);
          }
        }
      } catch (e) {
        console.warn("[notebook-viewer] poll failed", e);
      }
    }, 1000);

    return () => {
      console.info("[notebook-viewer] live OFF");
      setLiveStatus("off");
      subCreate.unsubscribe();
      subDelete.unsubscribe();
      subUpdate.unsubscribe();
      subBatch.unsubscribe();
      window.clearInterval(poll);
      if (reloadTimer.current !== null) {
        window.clearTimeout(reloadTimer.current);
        reloadTimer.current = null;
      }
    };
  }, [notebookId, ready, liveEnabled]);

  const sectionsById = useMemo(() => {
    const m = new Map<string, { name: string; position: number }>();
    for (const s of bundle?.sections ?? [])
      m.set(s.id, { name: s.name, position: s.position });
    return m;
  }, [bundle]);
  // Notebook order: section.position ascending, then page.position
  // ascending within each section. Pages whose section is missing from
  // the bundle sort last (shouldn't happen in steady state).
  const pages = useMemo(() => {
    const list = [...(bundle?.pages ?? [])];
    list.sort((a, b) => {
      const sa = sectionsById.get(a.section_id)?.position ?? Number.MAX_SAFE_INTEGER;
      const sb = sectionsById.get(b.section_id)?.position ?? Number.MAX_SAFE_INTEGER;
      if (sa !== sb) return sa - sb;
      return a.position - b.position;
    });
    return list;
  }, [bundle, sectionsById]);
  // Sections in notebook order (by position).
  const sortedSections = useMemo(() => {
    const list = [...(bundle?.sections ?? [])];
    list.sort((a, b) => a.position - b.position);
    return list;
  }, [bundle]);

  // Pages within the selected section, in notebook order.
  const sectionPages = useMemo(() => {
    if (!sectionId) return [];
    return pages.filter((p) => p.section_id === sectionId);
  }, [pages, sectionId]);

  // Re-anchor selection when bundle changes:
  //   - missing sectionId → first section
  //   - sectionId no longer exists → first section
  //   - pageId no longer in selected section → first page in section
  useEffect(() => {
    if (sortedSections.length === 0) return;
    let nextSection = sectionId;
    if (!nextSection || !sortedSections.some((s) => s.id === nextSection)) {
      nextSection = sortedSections[0].id;
    }
    if (nextSection !== sectionId) setSectionId(nextSection);
    const inSection = pages.filter((p) => p.section_id === nextSection);
    if (
      !pageId ||
      !inSection.some((p) => p.id === pageId)
    ) {
      const first = inSection[0]?.id ?? null;
      if (first !== pageId) setPageId(first);
    }
  }, [pages, sortedSections, sectionId, pageId]);

  const pageLabel = (
    p: NotebookBundle["pages"][number],
    i: number,
  ): string => p.name || `Page ${i + 1}`;

  function panBy(dx: number, dy: number) {
    viewportRef.current.pan_x += dx;
    viewportRef.current.pan_y += dy;
    viewer.pan(dx, dy);
  }
  function zoomCanvas(factor: number) {
    const c = canvasRef.current;
    if (!c) return;
    viewportRef.current.zoom *= factor;
    viewer.zoomAt(c.width / 2, c.height / 2, factor);
  }

  /**
   * Replay the cached viewport on top of the freshly-loaded bundle.
   * The WASM viewer's `loadNotebook` resets transforms, so any pan /
   * zoom the user has accumulated needs to be re-applied. We diff
   * against the starting (1,0,0) state and call pan + zoomAt in the
   * same order user actions would.
   */
  function restoreViewport() {
    const c = canvasRef.current;
    if (!c) return;
    const { pan_x, pan_y, zoom } = viewportRef.current;
    if (zoom !== 1) {
      viewer.zoomAt(c.width / 2, c.height / 2, zoom);
    }
    if (pan_x !== 0 || pan_y !== 0) {
      viewer.pan(pan_x, pan_y);
    }
  }

  return (
    <div className="flex h-full flex-col">
      <div className="flex flex-wrap items-center gap-3 border-b border-slate-200 bg-white px-4 py-2">
        <div className="text-sm font-semibold text-slate-700">
          {meta?.name ?? "Notebook"}
        </div>
        <span className="text-xs text-slate-500">
          {notebookId?.slice(0, 8)}…
        </span>
        <label className="ml-4 flex items-center gap-2 text-sm text-slate-700">
          Section:
          <select
            className="rounded border border-slate-300 bg-white px-2 py-1 text-sm"
            value={sectionId ?? ""}
            onChange={(e) => {
              const sid = e.target.value;
              setSectionId(sid);
              // Jump to first page in the newly-selected section so
              // the canvas isn't stuck rendering a page outside it.
              const first = pages.find((p) => p.section_id === sid);
              setPageId(first?.id ?? null);
            }}
            disabled={sortedSections.length === 0}
          >
            {sortedSections.map((s) => (
              <option key={s.id} value={s.id}>
                {s.name || "(unnamed section)"}
              </option>
            ))}
            {sortedSections.length === 0 && <option>(no sections)</option>}
          </select>
        </label>
        <label className="flex items-center gap-2 text-sm text-slate-700">
          Page:
          <select
            className="rounded border border-slate-300 bg-white px-2 py-1 text-sm"
            value={pageId ?? ""}
            onChange={(e) => setPageId(e.target.value || null)}
            disabled={sectionPages.length === 0}
          >
            {sectionPages.map((p, i) => (
              <option key={p.id} value={p.id}>
                {pageLabel(p, i)}
              </option>
            ))}
            {sectionPages.length === 0 && <option>(no pages)</option>}
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
        <label className="ml-2 flex items-center gap-1 text-xs text-slate-700">
          <input
            type="checkbox"
            checked={liveEnabled}
            onChange={(e) => setLiveEnabled(e.target.checked)}
          />
          Live updates
        </label>
        <span
          className={`rounded px-2 py-0.5 text-xs ${
            liveStatus === "on"
              ? "bg-emerald-100 text-emerald-800"
              : liveStatus === "error"
                ? "bg-red-100 text-red-800"
                : "bg-slate-100 text-slate-600"
          }`}
        >
          {liveStatus} · +{liveCount}
        </span>
        <span className="ml-auto text-xs text-slate-400">
          {load.status === "loading" && "loading…"}
          {load.status === "error" && (
            <span className="text-red-600">{load.message}</span>
          )}
          {load.status === "ready" &&
            bundle &&
            `${bundle.pages.length} page(s) · ${
              Object.values(bundle.strokes_by_page).flat().length
            } stroke(s)`}
        </span>
      </div>

      <div className="relative flex-1 overflow-hidden bg-slate-100">
        <canvas
          ref={canvasRef}
          className="absolute inset-0 h-full w-full"
          style={{ touchAction: "none", userSelect: "none" }}
        />
        {webgpuMissing && (
          <div className="absolute inset-0 flex items-center justify-center bg-slate-100/95 p-8">
            <div className="max-w-md rounded-lg border border-amber-300 bg-amber-50 px-6 py-5 text-amber-900 shadow-sm">
              <div className="text-base font-semibold">WebGPU required</div>
              <div className="mt-2 text-sm leading-relaxed">
                The viewer renders strokes via wgpu/Vello on a WebGPU surface.
                Your browser doesn't expose <code>navigator.gpu</code>.
              </div>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}

/**
 * Replace the bundle's pages + sections with the remote-fetched lists
 * while preserving the existing strokes_by_page payload. New pages
 * come in stroke-less; deleted pages have their strokes implicitly
 * dropped because the page id leaves `pages`.
 */
function buildBundleMerge(
  cur: NotebookBundle,
  sections: RemoteSectionRow[],
  pages: RemotePageRow[],
): NotebookBundle {
  const next: NotebookBundle = {
    ...cur,
    sections: sections.map((s) => ({
      id: s.id,
      notebook_id: s.notebookId,
      name: s.name,
      position: s.position,
      allowed_templates: null,
      parent_section_id: s.parentSectionId ?? null,
    })) as unknown as NotebookBundle["sections"],
    pages: pages.map((p) => ({
      id: p.id,
      template_id: p.templateId ?? null,
      section_id: p.sectionId,
      position: p.position,
      name: p.name ?? "",
      planner_address: null,
      created_at: p.createdAtIso ?? "",
      modified_at: p.modifiedAtIso ?? "",
      widget_overrides: {},
      widget_data: {},
      flagged: false,
    })) as unknown as NotebookBundle["pages"],
  };
  // Drop strokes whose page is no longer in the new pages list.
  const live: Record<string, RemotePageRow["id"]> = {};
  for (const p of pages) live[p.id] = p.id;
  const filteredStrokes: typeof cur.strokes_by_page = {};
  for (const [pageId, list] of Object.entries(cur.strokes_by_page)) {
    if (live[pageId]) filteredStrokes[pageId] = list;
  }
  next.strokes_by_page = filteredStrokes;
  return next;
}

function btn(): string {
  return "rounded border border-slate-300 bg-white px-2 py-1 text-sm hover:bg-slate-100 active:bg-slate-200";
}

function parseStroke(raw: string | null | undefined): Stroke | null {
  if (!raw) return null;
  try {
    return JSON.parse(raw) as Stroke;
  } catch (e) {
    console.warn("[notebook-viewer] parseStroke failed", e);
    return null;
  }
}

/**
 * Stitch the cloud-fetched rows into the existing `NotebookBundle`
 * shape the WASM viewer already understands. We make minimal
 * transformations: bare UUID strings stay strings; planner addresses
 * stay opaque (the viewer ignores them).
 */
function buildBundle(
  nb: NotebookRow,
  sections: RemoteSectionRow[],
  pages: RemotePageRow[],
  strokes: RemoteStrokeRow[],
): NotebookBundle {
  const strokes_by_page: Record<string, Stroke[]> = {};
  for (const s of strokes) {
    const decoded = parseStroke(s.strokeJson);
    if (!decoded) continue;
    const list = strokes_by_page[s.pageId] ?? [];
    list.push(decoded);
    strokes_by_page[s.pageId] = list;
  }
  return {
    schema_version: 1,
    notebook: {
      id: nb.id,
      name: nb.name,
      kind: parseKind(nb.kindJson),
      assigned_templates: parseAssigned(nb.assignedTemplatesJson),
    },
    sections: sections.map((s) => ({
      id: s.id,
      notebook_id: s.notebookId,
      name: s.name,
      position: s.position,
      allowed_templates: null,
      parent_section_id: s.parentSectionId ?? null,
    })),
    pages: pages.map((p) => ({
      id: p.id,
      template_id: p.templateId ?? null,
      section_id: p.sectionId,
      position: p.position,
      name: p.name ?? "",
      planner_address: null,
      created_at: p.createdAtIso ?? "",
      modified_at: p.modifiedAtIso ?? "",
      widget_overrides: {},
      widget_data: {},
      flagged: !!p.flagged,
    })),
    page_templates: [],
    strokes_by_page,
    asset_refs: {},
  } as unknown as NotebookBundle;
}

function parseKind(raw: string | null | undefined): unknown {
  if (!raw) return { kind: "Standard" };
  try {
    return JSON.parse(raw);
  } catch {
    return { kind: "Standard" };
  }
}

function parseAssigned(raw: string | null | undefined): string[] {
  if (!raw) return [];
  try {
    const v = JSON.parse(raw);
    return Array.isArray(v) ? (v as string[]) : [];
  } catch {
    return [];
  }
}
