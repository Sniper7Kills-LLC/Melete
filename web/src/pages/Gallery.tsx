import { useEffect, useMemo, useState } from "react";
import { useNavigate } from "react-router-dom";
import { useAuthenticator } from "@aws-amplify/ui-react";

import {
  client,
  type BrushRow,
  type NotebookTemplateRow,
  type PageTemplateRow,
  type SavedKind,
} from "@/amplify-client";
import { isStubBackend } from "@/amplify-config";
import {
  normalizeAssets,
  publicAssetUrlByName,
  type AssetMeta,
} from "@/amplify-storage";
import {
  EXAMPLE_TEMPLATES,
  type ExampleTemplate,
} from "@/types/example-templates";
import {
  DEFAULT_BRUSH,
  newBrush,
  type Brush,
} from "@/types/brush";
import type { PageTemplate } from "@/types";
import { shim } from "@/wasm";

/**
 * Gallery — public-facing browse of shareable artefacts (page
 * templates, notebook templates, brushes).
 *
 * Data source:
 *   - Live mode (`amplify_outputs.json` present): AppSync `*.list({
 *     filter: { visibility: { eq: 'PUBLIC' } } })` with `apiKey` auth.
 *   - Stub mode: hardcoded `EXAMPLE_TEMPLATES` + locally-authored
 *     brush stand-ins so the UI is still useful for offline dev.
 *
 * Per-row "Download" copies the canonical desktop TOML to the
 * clipboard. In live mode that's the row's `bodyToml` verbatim. In
 * stub mode we route through `shim.serialize*Toml` for parity.
 */
export function Gallery() {
  const [tab, setTab] = useState<TabKey>("page_templates");

  const pages = usePagesData();
  const notebooks = useNotebooksData();
  const brushes = useBrushesData();

  const counts = {
    page_templates:
      pages.status === "ok" ? pages.entries.length : pages.fallbackCount,
    notebook_templates:
      notebooks.status === "ok"
        ? notebooks.entries.length
        : notebooks.fallbackCount,
    brushes:
      brushes.status === "ok" ? brushes.entries.length : brushes.fallbackCount,
  };

  return (
    <div className="flex h-full flex-col bg-slate-50">
      <header className="border-b border-slate-200 bg-white px-6 py-4">
        <div className="flex items-center gap-3">
          <h1 className="text-xl font-semibold text-slate-900">Gallery</h1>
          <span className="text-sm text-slate-500">
            Browse public templates &amp; brushes. Download copies the
            TOML to your clipboard.
          </span>
          <span
            className={`ml-auto rounded px-2 py-0.5 text-[11px] font-medium ${
              isStubBackend
                ? "bg-amber-100 text-amber-800"
                : "bg-emerald-100 text-emerald-800"
            }`}
          >
            {isStubBackend ? "Backend: stub (showing samples)" : "Backend: live"}
          </span>
        </div>
        <nav className="mt-3 flex gap-1">
          {TABS.map((t) => (
            <button
              key={t.key}
              onClick={() => setTab(t.key)}
              className={`rounded px-3 py-1.5 text-sm transition-colors ${
                tab === t.key
                  ? "bg-slate-900 text-white"
                  : "text-slate-600 hover:bg-slate-200"
              }`}
            >
              {t.label}
              <span className="ml-2 text-[11px] opacity-70">
                {counts[t.key]}
              </span>
            </button>
          ))}
        </nav>
      </header>
      <div className="flex-1 overflow-y-auto px-6 py-6">
        {tab === "page_templates" && <PageTemplateTab data={pages} />}
        {tab === "notebook_templates" && (
          <NotebookTemplateTab data={notebooks} />
        )}
        {tab === "brushes" && <BrushTab data={brushes} />}
      </div>
    </div>
  );
}

type TabKey = "page_templates" | "notebook_templates" | "brushes";

const TABS: { key: TabKey; label: string }[] = [
  { key: "page_templates", label: "Page templates" },
  { key: "notebook_templates", label: "Notebook templates" },
  { key: "brushes", label: "Brushes" },
];

// ---------------------------------------------------------------------
// Loader hooks
// ---------------------------------------------------------------------

type LoadState<T> =
  | { status: "loading"; fallbackCount: number }
  | { status: "err"; message: string; fallbackCount: number }
  | { status: "ok"; entries: T[]; fallbackCount: number };

interface PageEntry {
  id: string;
  name: string;
  description: string | null;
  category: string | null;
  bodyToml: string;
  parsed: PageTemplate | null;
  assets: AssetMeta[];
  swatch: string;
}

interface NotebookEntry {
  id: string;
  name: string;
  description: string | null;
  bodyToml: string;
  swatch: string;
}

interface BrushEntry {
  id: string;
  name: string;
  description: string | null;
  bodyToml: string;
  parsed: Brush | null;
  swatch: string;
}

const PAGE_SWATCHES = [
  "from-indigo-100 to-indigo-200",
  "from-rose-100 to-rose-200",
  "from-amber-100 to-amber-200",
  "from-emerald-100 to-emerald-200",
  "from-sky-100 to-sky-200",
  "from-violet-100 to-violet-200",
  "from-teal-100 to-teal-200",
  "from-fuchsia-100 to-fuchsia-200",
];

const NOTEBOOK_SWATCHES = [
  "from-emerald-100 to-emerald-200",
  "from-amber-100 to-amber-200",
  "from-sky-100 to-sky-200",
  "from-rose-100 to-rose-200",
];

const BRUSH_SWATCHES = [
  "from-slate-100 to-slate-200",
  "from-yellow-100 to-yellow-200",
  "from-stone-100 to-stone-200",
  "from-indigo-50 to-indigo-100",
];

// FNV-1a so swatch is stable per-id.
function pickSwatch(palette: string[], key: string): number {
  let h = 0x811c9dc5;
  for (let i = 0; i < key.length; i++) {
    h ^= key.charCodeAt(i);
    h = (h * 0x01000193) >>> 0;
  }
  return h % palette.length;
}

function fallbackPages(): PageEntry[] {
  return EXAMPLE_TEMPLATES.map((e: ExampleTemplate, i) => {
    const id = tplId(e.template);
    let bodyToml = "";
    try {
      bodyToml = shim.serializeTemplateToml(e.template);
    } catch {
      bodyToml = JSON.stringify(e.template, null, 2);
    }
    return {
      id,
      name: e.template.name,
      description: e.template.description ?? null,
      category: e.template.category ?? null,
      bodyToml,
      parsed: e.template,
      assets: [],
      swatch: e.swatch ?? PAGE_SWATCHES[i % PAGE_SWATCHES.length]!,
    };
  });
}

function fallbackNotebooks(): NotebookEntry[] {
  return [
    {
      id: "stub-daily",
      name: "Daily Planner",
      description: "Mon–Fri daily + monthly cover.",
      bodyToml: "# stub-mode preview only\n",
      swatch: NOTEBOOK_SWATCHES[0]!,
    },
    {
      id: "stub-full-focus",
      name: "Full Focus",
      description: "Year + quarter + month + week + day.",
      bodyToml: "# stub-mode preview only\n",
      swatch: NOTEBOOK_SWATCHES[1]!,
    },
    {
      id: "stub-weekly",
      name: "Weekly Compass",
      description: "Week-grouped, 7 dailies per week.",
      bodyToml: "# stub-mode preview only\n",
      swatch: NOTEBOOK_SWATCHES[2]!,
    },
  ];
}

function fallbackBrushes(): BrushEntry[] {
  function fromBrush(
    brush: Brush,
    description: string,
    swatchIdx: number,
  ): BrushEntry {
    let bodyToml = "";
    try {
      bodyToml = shim.serializeBrushToml(brush);
    } catch {
      bodyToml = JSON.stringify(brush, null, 2);
    }
    return {
      id: brush.id,
      name: brush.name,
      description,
      bodyToml,
      parsed: brush,
      swatch: BRUSH_SWATCHES[swatchIdx % BRUSH_SWATCHES.length]!,
    };
  }

  const inkPen = newBrush();
  inkPen.name = "Ink Pen";
  inkPen.default_color = [28, 28, 36, 255];
  inkPen.layers[0] = {
    enabled: true,
    geometry: { type: "smooth", resample_step_mm: 0.4 },
    width: { type: "pressure", floor: 0.15, amp: 0.85 },
    tip: { type: "round" },
    tip_scale: 1,
    color: { alpha_mult: 1, hue_shift_deg: 0 },
    blend: "Normal",
  };

  const highlighter = newBrush();
  highlighter.name = "Highlighter";
  highlighter.default_color = [255, 230, 90, 110];
  highlighter.layers[0] = {
    enabled: true,
    geometry: { type: "smooth", resample_step_mm: 0.6 },
    width: { type: "constant", width_mult: 5 },
    tip: { type: "flat_nib", angle_deg: 0, aspect: 4 },
    tip_scale: 1,
    color: { alpha_mult: 1, hue_shift_deg: 0 },
    blend: "Multiply",
  };

  const pencil = newBrush();
  pencil.name = "Pencil";
  pencil.default_color = [80, 80, 90, 220];
  pencil.layers[0] = {
    enabled: true,
    geometry: {
      type: "scatter",
      density: 18,
      spread_mm: 0.4,
      falloff: 1,
      directional_bias_deg: null,
    },
    width: { type: "pressure", floor: 0.25, amp: 0.55 },
    tip: { type: "round" },
    tip_scale: 1,
    color: { alpha_mult: 0.75, hue_shift_deg: 0 },
    blend: "Normal",
  };

  return [
    fromBrush(
      inkPen,
      "Pressure-driven smooth ribbon. Crisp ink linework.",
      0,
    ),
    fromBrush(
      highlighter,
      "Flat-nib tip with Multiply blend — translucent highlights.",
      1,
    ),
    fromBrush(
      pencil,
      "Scatter geometry + reduced alpha for graphite tooth.",
      2,
    ),
    fromBrush({ ...DEFAULT_BRUSH, name: "Default" }, "Stock brush.", 3),
  ];
}

function usePagesData(): LoadState<PageEntry> {
  const fallback = useMemo(fallbackPages, []);
  const [state, setState] = useState<LoadState<PageEntry>>(
    isStubBackend
      ? { status: "ok", entries: fallback, fallbackCount: fallback.length }
      : { status: "loading", fallbackCount: fallback.length },
  );

  useEffect(() => {
    if (isStubBackend) return;
    let cancelled = false;
    client.models.PageTemplate.list({
      filter: { visibility: { eq: "PUBLIC" } },
      authMode: "apiKey",
      limit: 200,
    })
      .then((r) => {
        if (cancelled) return;
        if (r.errors?.length) {
          setState({
            status: "err",
            message: r.errors.map((e) => e.message).join("; "),
            fallbackCount: fallback.length,
          });
          return;
        }
        const rows = r.data ?? [];
        const entries: PageEntry[] = rows.map((row: PageTemplateRow) => {
          let parsed: PageTemplate | null = null;
          try {
            parsed = shim.parseTemplateToml(row.bodyToml);
          } catch {
            parsed = null;
          }
          return {
            id: row.id,
            name: row.name,
            description: row.description ?? null,
            category: row.category ?? null,
            bodyToml: row.bodyToml,
            parsed,
            assets: normalizeAssets(row.assets),
            swatch:
              PAGE_SWATCHES[pickSwatch(PAGE_SWATCHES, row.id)]!,
          };
        });
        setState({
          status: "ok",
          entries,
          fallbackCount: fallback.length,
        });
      })
      .catch((e: unknown) => {
        if (cancelled) return;
        setState({
          status: "err",
          message: e instanceof Error ? e.message : String(e),
          fallbackCount: fallback.length,
        });
      });
    return () => {
      cancelled = true;
    };
  }, [fallback.length]);

  return state;
}

function useNotebooksData(): LoadState<NotebookEntry> {
  const fallback = useMemo(fallbackNotebooks, []);
  const [state, setState] = useState<LoadState<NotebookEntry>>(
    isStubBackend
      ? { status: "ok", entries: fallback, fallbackCount: fallback.length }
      : { status: "loading", fallbackCount: fallback.length },
  );

  useEffect(() => {
    if (isStubBackend) return;
    let cancelled = false;
    client.models.NotebookTemplate.list({
      filter: { visibility: { eq: "PUBLIC" } },
      authMode: "apiKey",
      limit: 200,
    })
      .then((r) => {
        if (cancelled) return;
        if (r.errors?.length) {
          setState({
            status: "err",
            message: r.errors.map((e) => e.message).join("; "),
            fallbackCount: fallback.length,
          });
          return;
        }
        const rows = r.data ?? [];
        const entries: NotebookEntry[] = rows.map(
          (row: NotebookTemplateRow) => ({
            id: row.id,
            name: row.name,
            description: row.description ?? null,
            bodyToml: row.bodyToml,
            swatch:
              NOTEBOOK_SWATCHES[pickSwatch(NOTEBOOK_SWATCHES, row.id)]!,
          }),
        );
        setState({
          status: "ok",
          entries,
          fallbackCount: fallback.length,
        });
      })
      .catch((e: unknown) => {
        if (cancelled) return;
        setState({
          status: "err",
          message: e instanceof Error ? e.message : String(e),
          fallbackCount: fallback.length,
        });
      });
    return () => {
      cancelled = true;
    };
  }, [fallback.length]);

  return state;
}

function useBrushesData(): LoadState<BrushEntry> {
  const fallback = useMemo(fallbackBrushes, []);
  const [state, setState] = useState<LoadState<BrushEntry>>(
    isStubBackend
      ? { status: "ok", entries: fallback, fallbackCount: fallback.length }
      : { status: "loading", fallbackCount: fallback.length },
  );

  useEffect(() => {
    if (isStubBackend) return;
    let cancelled = false;
    client.models.Brush.list({
      filter: { visibility: { eq: "PUBLIC" } },
      authMode: "apiKey",
      limit: 200,
    })
      .then((r) => {
        if (cancelled) return;
        if (r.errors?.length) {
          setState({
            status: "err",
            message: r.errors.map((e) => e.message).join("; "),
            fallbackCount: fallback.length,
          });
          return;
        }
        const rows = r.data ?? [];
        const entries: BrushEntry[] = rows.map((row: BrushRow) => {
          let parsed: Brush | null = null;
          try {
            parsed = shim.parseBrushToml(row.bodyToml);
          } catch {
            parsed = null;
          }
          return {
            id: row.id,
            name: row.name,
            description: row.description ?? null,
            bodyToml: row.bodyToml,
            parsed,
            swatch: BRUSH_SWATCHES[pickSwatch(BRUSH_SWATCHES, row.id)]!,
          };
        });
        setState({
          status: "ok",
          entries,
          fallbackCount: fallback.length,
        });
      })
      .catch((e: unknown) => {
        if (cancelled) return;
        setState({
          status: "err",
          message: e instanceof Error ? e.message : String(e),
          fallbackCount: fallback.length,
        });
      });
    return () => {
      cancelled = true;
    };
  }, [fallback.length]);

  return state;
}

// ---------------------------------------------------------------------
// Auth-gated actions: Edit + Fork
// ---------------------------------------------------------------------

// ForkKind narrows SavedKind by excluding `'Notebook'` — user-owned
// notebooks aren't shareable templates and have no fork mutation.
type ForkKind = Exclude<SavedKind, 'Notebook'>;

async function callFork(kind: ForkKind, id: string) {
  switch (kind) {
    case "PageTemplate":
      return client.mutations.forkPageTemplate({ id });
    case "NotebookTemplate":
      return client.mutations.forkNotebookTemplate({ id });
    case "Brush":
      return client.mutations.forkBrush({ id });
    default: {
      // Exhaustiveness guard — narrows the return type so callers don't
      // see `... | undefined` when ForkKind grows a new variant.
      const _exhaustive: never = kind;
      throw new Error(`Unknown ForkKind: ${String(_exhaustive)}`);
    }
  }
}

interface ActionRowProps {
  forkKind: ForkKind;
  rowId: string;
  rowName: string;
  /** Number badge text (widget count, layer count, etc.). Optional. */
  meta?: string;
  /** Download handler — clipboard copy is the canonical action. */
  onDownload: () => void;
  downloadLabel: string;
}

function ActionRow({
  forkKind,
  rowId,
  rowName,
  meta,
  onDownload,
  downloadLabel,
}: ActionRowProps) {
  const navigate = useNavigate();
  // useAuthenticator throws if no Provider — but main.tsx wraps the
  // whole app, so this is safe everywhere.
  const { authStatus } = useAuthenticator((c) => [c.authStatus]);
  const signedIn = authStatus === "authenticated";

  const [forking, setForking] = useState(false);
  const [saving, setSaving] = useState(false);
  const [actionErr, setActionErr] = useState<string | null>(null);
  const [actionMsg, setActionMsg] = useState<string | null>(null);

  async function fork() {
    if (!signedIn || forking) return;
    setForking(true);
    setActionErr(null);
    setActionMsg(null);
    try {
      const r = await callFork(forkKind, rowId);
      if (r.errors?.length) {
        setActionErr(r.errors.map((e) => e.message).join("; "));
        return;
      }
      navigate("/my");
    } catch (e) {
      setActionErr(e instanceof Error ? e.message : String(e));
    } finally {
      setForking(false);
    }
  }

  async function save() {
    if (!signedIn || saving) return;
    setSaving(true);
    setActionErr(null);
    setActionMsg(null);
    try {
      const r = await client.models.SavedTemplate.create(
        {
          kind: forkKind,
          sourceId: rowId,
          sourceName: rowName,
          savedAt: new Date().toISOString(),
        },
        { authMode: "userPool" },
      );
      if (r.errors?.length) {
        setActionErr(r.errors.map((e) => e.message).join("; "));
        return;
      }
      setActionMsg("Saved to library");
      setTimeout(() => setActionMsg(null), 1600);
    } catch (e) {
      setActionErr(e instanceof Error ? e.message : String(e));
    } finally {
      setSaving(false);
    }
  }

  return (
    <>
      <div className="mt-3 flex items-center justify-between">
        {meta ? (
          <span className="text-[11px] text-slate-400">{meta}</span>
        ) : (
          <span />
        )}
        <div className="flex items-center gap-1">
          <button
            type="button"
            onClick={save}
            disabled={!signedIn || saving}
            title={
              signedIn
                ? "Subscribe — keep a reference, receive author updates"
                : "Sign in to save"
            }
            className="rounded border border-emerald-300 bg-emerald-50 px-2 py-1 text-xs font-medium text-emerald-800 hover:bg-emerald-100 disabled:cursor-not-allowed disabled:opacity-50"
          >
            {saving ? "Saving…" : "Save"}
          </button>
          <button
            type="button"
            onClick={fork}
            disabled={!signedIn || forking}
            title={
              signedIn
                ? "Fork — clone an editable private copy"
                : "Sign in to fork"
            }
            className="rounded border border-slate-300 bg-white px-2 py-1 text-xs font-medium text-slate-700 hover:bg-slate-100 disabled:cursor-not-allowed disabled:opacity-50"
          >
            {forking ? "Forking…" : "Fork"}
          </button>
          <button
            type="button"
            onClick={onDownload}
            className="rounded bg-indigo-600 px-3 py-1 text-xs font-medium text-white hover:bg-indigo-700"
          >
            {downloadLabel}
          </button>
        </div>
      </div>
      {actionMsg && (
        <div className="mt-1 text-[10px] text-emerald-700">{actionMsg}</div>
      )}
      {actionErr && (
        <div className="mt-1 text-[10px] text-rose-600">{actionErr}</div>
      )}
    </>
  );
}

// ---------------------------------------------------------------------
// Search + filter chrome
// ---------------------------------------------------------------------

interface FilterBarProps {
  search: string;
  onSearch: (s: string) => void;
  category?: string;
  onCategory?: (c: string) => void;
  categories?: string[];
  resultCount: number;
  totalCount: number;
}

function FilterBar({
  search,
  onSearch,
  category,
  onCategory,
  categories,
  resultCount,
  totalCount,
}: FilterBarProps) {
  return (
    <div className="mb-4 flex flex-wrap items-center gap-3">
      <input
        type="search"
        value={search}
        onChange={(e) => onSearch(e.target.value)}
        placeholder="Search by name or description…"
        className="w-72 rounded border border-slate-300 bg-white px-3 py-1.5 text-sm focus:border-indigo-400 focus:outline-none"
      />
      {categories && onCategory && (
        <select
          value={category ?? ""}
          onChange={(e) => onCategory(e.target.value)}
          className="rounded border border-slate-300 bg-white px-2 py-1.5 text-sm"
        >
          <option value="">All categories</option>
          {categories.map((c) => (
            <option key={c} value={c}>
              {c}
            </option>
          ))}
        </select>
      )}
      <span className="ml-auto text-xs text-slate-500">
        {resultCount} of {totalCount}
      </span>
    </div>
  );
}

function StatusBlock({ data }: { data: LoadState<unknown> }) {
  if (data.status === "loading") {
    return (
      <div className="py-12 text-center text-sm text-slate-400">
        Loading from AppSync…
      </div>
    );
  }
  if (data.status === "err") {
    return (
      <div className="rounded border border-rose-300 bg-rose-50 px-4 py-3 text-sm text-rose-800">
        <div className="font-semibold">Failed to load</div>
        <div className="text-xs">{data.message}</div>
      </div>
    );
  }
  return null;
}

function matchesSearch(
  q: string,
  ...fields: (string | null | undefined)[]
): boolean {
  if (!q) return true;
  const needle = q.trim().toLowerCase();
  if (!needle) return true;
  return fields.some(
    (f) => typeof f === "string" && f.toLowerCase().includes(needle),
  );
}

// ---------------------------------------------------------------------
// Page templates tab
// ---------------------------------------------------------------------

function PageTemplateTab({ data }: { data: LoadState<PageEntry> }) {
  const [search, setSearch] = useState("");
  const [category, setCategory] = useState("");

  const entries = data.status === "ok" ? data.entries : [];
  const categories = useMemo(() => {
    const set = new Set<string>();
    for (const e of entries) {
      if (e.category) set.add(e.category);
    }
    return Array.from(set).sort();
  }, [entries]);

  if (data.status !== "ok") return <StatusBlock data={data} />;

  const filtered = entries.filter(
    (e) =>
      matchesSearch(search, e.name, e.description, e.category) &&
      (category === "" || e.category === category),
  );

  return (
    <>
      <FilterBar
        search={search}
        onSearch={setSearch}
        category={category}
        onCategory={setCategory}
        categories={categories}
        resultCount={filtered.length}
        totalCount={entries.length}
      />
      {filtered.length === 0 ? (
        <EmptyState message="No page templates match." />
      ) : (
        <div className="grid grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4">
          {filtered.map((e) => (
            <PageTemplateCard key={e.id} entry={e} />
          ))}
        </div>
      )}
    </>
  );
}

function PageTemplateCard({ entry }: { entry: PageEntry }) {
  const [copied, setCopied] = useState(false);
  function download() {
    void navigator.clipboard.writeText(entry.bodyToml).then(() => {
      setCopied(true);
      setTimeout(() => setCopied(false), 1600);
    });
  }
  const widgetCount = entry.parsed?.widgets.length ?? null;
  return (
    <article className="flex flex-col overflow-hidden rounded border border-slate-200 bg-white shadow-sm transition-shadow hover:shadow-md">
      <div
        className={`flex aspect-[3/4] items-center justify-center bg-gradient-to-br ${entry.swatch} relative`}
      >
        {entry.parsed ? (
          <PageThumbnail
            template={entry.parsed}
            templateId={entry.id}
            assets={entry.assets}
          />
        ) : (
          <div className="rounded bg-white/80 px-2 py-1 text-[11px] uppercase tracking-wide text-slate-500">
            no preview
          </div>
        )}
        {entry.category && (
          <span className="absolute right-2 top-2 rounded bg-white/80 px-2 py-0.5 text-[10px] uppercase tracking-wide text-slate-600">
            {entry.category}
          </span>
        )}
      </div>
      <div className="flex flex-1 flex-col px-3 py-3">
        <div className="text-sm font-semibold text-slate-800">{entry.name}</div>
        <div className="mt-1 flex-1 text-xs leading-snug text-slate-500">
          {entry.description}
        </div>
        <ActionRow
          forkKind="PageTemplate"
          rowId={entry.id}
          rowName={entry.name}
          meta={
            widgetCount === null
              ? "—"
              : `${widgetCount} widget${widgetCount === 1 ? "" : "s"}`
          }
          onDownload={download}
          downloadLabel={copied ? "Copied!" : "Download"}
        />
      </div>
    </article>
  );
}

function PageThumbnail({
  template,
  templateId,
  assets,
}: {
  template: PageTemplate;
  templateId: string;
  assets: AssetMeta[];
}) {
  const [pageW, pageH] = template.size_mm;
  const bg = template.background;
  const bgImageUrl =
    bg.kind === "Image" ? publicAssetUrlByName(templateId, bg.path, assets) : null;
  return (
    <svg
      viewBox={`0 0 ${pageW} ${pageH}`}
      className="h-[88%] w-[80%] rounded bg-white shadow"
      preserveAspectRatio="xMidYMid meet"
    >
      {bgImageUrl && (
        <image
          href={bgImageUrl}
          x={0}
          y={0}
          width={pageW}
          height={pageH}
          preserveAspectRatio="xMidYMid slice"
        />
      )}
      {bg.kind === "Pdf" && (
        <g>
          <rect
            x={0}
            y={0}
            width={pageW}
            height={pageH}
            fill="rgba(120,120,140,0.08)"
          />
          <text
            x={pageW / 2}
            y={pageH / 2}
            fontSize={Math.max(6, pageW / 30)}
            textAnchor="middle"
            fill="rgba(60,60,80,0.6)"
          >
            PDF page {bg.page}
          </text>
        </g>
      )}
      {template.widgets.map((w, i) => (
        <rect
          key={i}
          x={w.rect.x}
          y={w.rect.y}
          width={w.rect.width}
          height={w.rect.height}
          fill={
            w.style.fill_color
              ? rgba(w.style.fill_color)
              : "rgba(99,102,241,0.08)"
          }
          stroke={rgba(w.style.stroke_color)}
          strokeWidth={Math.max(0.4, w.style.stroke_width_mm)}
        />
      ))}
    </svg>
  );
}

function rgba(c: { r: number; g: number; b: number; a: number }): string {
  return `rgba(${c.r},${c.g},${c.b},${c.a / 255})`;
}

function rgbaTuple(c: [number, number, number, number] | null): string {
  if (!c) return "rgba(60,60,80,0.86)";
  return `rgba(${c[0]},${c[1]},${c[2]},${c[3] / 255})`;
}

function tplId(t: PageTemplate): string {
  const id = t.id;
  return typeof id === "string" ? id : id["0"];
}

// ---------------------------------------------------------------------
// Notebook templates tab
// ---------------------------------------------------------------------

function NotebookTemplateTab({ data }: { data: LoadState<NotebookEntry> }) {
  const [search, setSearch] = useState("");

  if (data.status !== "ok") return <StatusBlock data={data} />;

  const filtered = data.entries.filter((e) =>
    matchesSearch(search, e.name, e.description),
  );

  return (
    <>
      <FilterBar
        search={search}
        onSearch={setSearch}
        resultCount={filtered.length}
        totalCount={data.entries.length}
      />
      {filtered.length === 0 ? (
        <EmptyState message="No notebook templates match." />
      ) : (
        <div className="grid grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-3">
          {filtered.map((e) => (
            <NotebookCard key={e.id} entry={e} />
          ))}
        </div>
      )}
    </>
  );
}

function NotebookCard({ entry }: { entry: NotebookEntry }) {
  const [copied, setCopied] = useState(false);
  function download() {
    void navigator.clipboard.writeText(entry.bodyToml).then(() => {
      setCopied(true);
      setTimeout(() => setCopied(false), 1600);
    });
  }
  return (
    <article className="flex flex-col overflow-hidden rounded border border-slate-200 bg-white shadow-sm transition-shadow hover:shadow-md">
      <div
        className={`relative flex aspect-[16/9] items-center justify-center bg-gradient-to-br ${entry.swatch} px-4 py-3`}
      >
        <div className="rounded bg-white/85 px-3 py-1 text-[11px] uppercase tracking-wide text-slate-700">
          notebook template
        </div>
      </div>
      <div className="flex flex-1 flex-col px-3 py-3">
        <div className="text-sm font-semibold text-slate-800">{entry.name}</div>
        <div className="mt-1 flex-1 text-xs leading-snug text-slate-500">
          {entry.description}
        </div>
        <ActionRow
          forkKind="NotebookTemplate"
          rowId={entry.id}
          rowName={entry.name}
          onDownload={download}
          downloadLabel={copied ? "Copied!" : "Download"}
        />
      </div>
    </article>
  );
}

// ---------------------------------------------------------------------
// Brushes tab
// ---------------------------------------------------------------------

function BrushTab({ data }: { data: LoadState<BrushEntry> }) {
  const [search, setSearch] = useState("");

  if (data.status !== "ok") return <StatusBlock data={data} />;

  const filtered = data.entries.filter((e) =>
    matchesSearch(search, e.name, e.description),
  );

  return (
    <>
      <FilterBar
        search={search}
        onSearch={setSearch}
        resultCount={filtered.length}
        totalCount={data.entries.length}
      />
      {filtered.length === 0 ? (
        <EmptyState message="No brushes match." />
      ) : (
        <div className="grid grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4">
          {filtered.map((e) => (
            <BrushCard key={e.id} entry={e} />
          ))}
        </div>
      )}
    </>
  );
}

function BrushCard({ entry }: { entry: BrushEntry }) {
  const [copied, setCopied] = useState(false);
  function download() {
    void navigator.clipboard.writeText(entry.bodyToml).then(() => {
      setCopied(true);
      setTimeout(() => setCopied(false), 1600);
    });
  }
  const brush = entry.parsed;
  const layerCount = brush?.layers.length ?? null;
  return (
    <article className="flex flex-col overflow-hidden rounded border border-slate-200 bg-white shadow-sm transition-shadow hover:shadow-md">
      <div
        className={`relative flex aspect-[3/2] items-center justify-center bg-gradient-to-br ${entry.swatch}`}
      >
        {brush ? (
          <BrushPreview brush={brush} />
        ) : (
          <div className="rounded bg-white/80 px-2 py-1 text-[11px] uppercase tracking-wide text-slate-500">
            no preview
          </div>
        )}
      </div>
      <div className="flex flex-1 flex-col px-3 py-3">
        <div className="flex items-center gap-2">
          {brush && (
            <span
              className="h-3 w-3 shrink-0 rounded-full border border-white shadow"
              style={{ background: rgbaTuple(brush.default_color) }}
            />
          )}
          <span className="text-sm font-semibold text-slate-800">
            {entry.name}
          </span>
        </div>
        <div className="mt-1 flex-1 text-xs leading-snug text-slate-500">
          {entry.description}
        </div>
        <ActionRow
          forkKind="Brush"
          rowId={entry.id}
          rowName={entry.name}
          meta={
            layerCount === null
              ? "—"
              : `${layerCount} layer${layerCount === 1 ? "" : "s"}`
          }
          onDownload={download}
          downloadLabel={copied ? "Copied!" : "Download"}
        />
      </div>
    </article>
  );
}

function BrushPreview({ brush }: { brush: Brush }) {
  const color = rgbaTuple(brush.default_color);
  const layer = brush.layers.find((l) => l.enabled) ?? brush.layers[0];
  const widthMult =
    layer && layer.width.type === "constant" ? layer.width.width_mult : 1;
  const blend = layer && layer.blend === "Multiply" ? "multiply" : "normal";
  return (
    <svg viewBox="0 0 220 60" className="h-[80%] w-[80%]">
      <defs>
        <linearGradient id="brushFade" x1="0" y1="0" x2="1" y2="0">
          <stop offset="0" stopColor={color} stopOpacity="0.4" />
          <stop offset="0.2" stopColor={color} stopOpacity="1" />
          <stop offset="0.8" stopColor={color} stopOpacity="1" />
          <stop offset="1" stopColor={color} stopOpacity="0.4" />
        </linearGradient>
      </defs>
      <path
        d="M 12 30 C 60 8, 110 52, 160 26 S 200 36, 210 30"
        fill="none"
        stroke="url(#brushFade)"
        strokeLinecap="round"
        strokeWidth={Math.max(1.2, widthMult * 2.5)}
        style={{ mixBlendMode: blend as "normal" | "multiply" }}
      />
    </svg>
  );
}

// ---------------------------------------------------------------------
// Shared
// ---------------------------------------------------------------------

function EmptyState({ message }: { message: string }) {
  return (
    <div className="rounded border border-dashed border-slate-300 bg-white px-6 py-10 text-center text-sm text-slate-500">
      {message}
    </div>
  );
}
