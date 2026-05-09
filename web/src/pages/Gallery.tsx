import { useMemo, useState } from "react";

import {
  EXAMPLE_TEMPLATES,
  type ExampleTemplate,
} from "@/types/example-templates";
import {
  DEFAULT_NOTEBOOK_TEMPLATE,
  type NotebookTemplate,
} from "@/types/notebook-template";
import {
  DEFAULT_BRUSH,
  newBrush,
  type Brush,
} from "@/types/brush";
import { shim } from "@/wasm";

/**
 * Gallery — public-facing browse + preview surface for the three
 * shareable artefact types: page templates, notebook templates, and
 * brushes. Each item renders a small swatch / mini-preview, name,
 * description, category. "Download" copies the TOML to the clipboard
 * (no remote backend on the POC; the desktop user pastes into
 * `~/.local/share/journal/templates/*.toml` or
 * `~/.config/journal/brushes.toml` themselves).
 *
 * When the Amplify backend ships, the same component can swap its
 * data source from the hard-coded `EXAMPLE_*` lists to a fetched
 * AppSync query without changing the UI shape.
 */
export function Gallery() {
  const [tab, setTab] = useState<TabKey>("page_templates");
  return (
    <div className="flex h-full flex-col bg-slate-50">
      <header className="border-b border-slate-200 bg-white px-6 py-4">
        <div className="flex items-center gap-3">
          <h1 className="text-xl font-semibold text-slate-900">Gallery</h1>
          <span className="text-sm text-slate-500">
            Preview + download community templates &amp; brushes.
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
                {t.count}
              </span>
            </button>
          ))}
        </nav>
      </header>
      <div className="flex-1 overflow-y-auto px-6 py-6">
        {tab === "page_templates" && <PageTemplateGrid />}
        {tab === "notebook_templates" && <NotebookTemplateGrid />}
        {tab === "brushes" && <BrushGrid />}
      </div>
    </div>
  );
}

type TabKey = "page_templates" | "notebook_templates" | "brushes";

interface TabMeta {
  key: TabKey;
  label: string;
  count: number;
}

// Counts are deliberately read from the hard-coded sources at module
// load. When the Amplify backend ships these become subscription
// counts.
const TABS: TabMeta[] = [
  {
    key: "page_templates",
    label: "Page templates",
    count: EXAMPLE_TEMPLATES.length,
  },
  {
    key: "notebook_templates",
    label: "Notebook templates",
    count: NOTEBOOK_GALLERY().length,
  },
  { key: "brushes", label: "Brushes", count: BRUSH_GALLERY().length },
];

// ---------------------------------------------------------------------
// Page templates
// ---------------------------------------------------------------------

function PageTemplateGrid() {
  return (
    <div className="grid grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4">
      {EXAMPLE_TEMPLATES.map((e) => (
        <PageTemplateCard key={tplId(e)} ex={e} />
      ))}
    </div>
  );
}

function PageTemplateCard({ ex }: { ex: ExampleTemplate }) {
  const [copied, setCopied] = useState(false);
  function download() {
    let toml: string;
    try {
      toml = shim.serializeTemplateToml(ex.template);
    } catch {
      toml = JSON.stringify(ex.template, null, 2);
    }
    void navigator.clipboard.writeText(toml).then(() => {
      setCopied(true);
      setTimeout(() => setCopied(false), 1600);
    });
  }
  return (
    <article className="flex flex-col overflow-hidden rounded border border-slate-200 bg-white shadow-sm transition-shadow hover:shadow-md">
      <div
        className={`flex aspect-[3/4] items-center justify-center bg-gradient-to-br ${ex.swatch} relative`}
      >
        <PageThumbnail ex={ex} />
        <span className="absolute right-2 top-2 rounded bg-white/80 px-2 py-0.5 text-[10px] uppercase tracking-wide text-slate-600">
          {ex.template.category || "page"}
        </span>
      </div>
      <div className="flex flex-1 flex-col px-3 py-3">
        <div className="text-sm font-semibold text-slate-800">
          {ex.template.name}
        </div>
        <div className="mt-1 flex-1 text-xs leading-snug text-slate-500">
          {ex.template.description}
        </div>
        <div className="mt-3 flex items-center justify-between">
          <span className="text-[11px] text-slate-400">
            {ex.template.widgets.length} widget
            {ex.template.widgets.length === 1 ? "" : "s"}
          </span>
          <button
            onClick={download}
            className="rounded bg-indigo-600 px-3 py-1 text-xs font-medium text-white hover:bg-indigo-700"
          >
            {copied ? "Copied!" : "Download"}
          </button>
        </div>
      </div>
    </article>
  );
}

/**
 * Mini thumbnail for a page template — paper-coloured rectangle with
 * the widget rects drawn at scale. Cheap SVG, no Vello yet.
 */
function PageThumbnail({ ex }: { ex: ExampleTemplate }) {
  const [pageW, pageH] = ex.template.size_mm;
  return (
    <svg
      viewBox={`0 0 ${pageW} ${pageH}`}
      className="h-[88%] w-[80%] rounded bg-white shadow"
      preserveAspectRatio="xMidYMid meet"
    >
      {ex.template.widgets.map((w, i) => (
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

/** Brush.default_color is a `[r,g,b,a]` tuple (or null = use toolbar color). */
function rgbaTuple(
  c: [number, number, number, number] | null,
): string {
  if (!c) return "rgba(60,60,80,0.86)";
  return `rgba(${c[0]},${c[1]},${c[2]},${c[3] / 255})`;
}

function tplId(e: ExampleTemplate): string {
  const id = e.template.id;
  return typeof id === "string" ? id : id["0"];
}

// ---------------------------------------------------------------------
// Notebook templates
// ---------------------------------------------------------------------

interface NotebookEntry {
  template: NotebookTemplate;
  description: string;
  swatch: string;
}

function NOTEBOOK_GALLERY(): NotebookEntry[] {
  // Hand-authored notebook-template stand-ins until the desktop's
  // built-in library is exposed via journal-web-shim.
  return [
    {
      template: {
        ...DEFAULT_NOTEBOOK_TEMPLATE,
        id: "00000000-0000-0000-0000-0000n0planner1",
        name: "Daily Planner",
        description:
          "One daily page Mon–Fri, blank weekend, monthly cover at the start of each month.",
        before_month: ["00000000-0000-0000-0000-e3eaa3eaa3ea"],
        daily_slots: [
          {
            days: ["Mon", "Tue", "Wed", "Thu", "Fri"],
            templates: ["00000000-0000-0000-0000-e1d0a11d0a11"],
          },
        ],
        grouping: "Month",
      },
      description: "Mon-Fri daily + monthly cover",
      swatch: "from-emerald-100 to-emerald-200",
    },
    {
      template: {
        ...DEFAULT_NOTEBOOK_TEMPLATE,
        id: "00000000-0000-0000-0000-0000n0planner2",
        name: "Full Focus",
        description:
          "Year vision + quarterly goals + monthly + weekly + daily — full Franklin-style cadence.",
        year_start: ["00000000-0000-0000-0000-e5e5e5e5e5e5"],
        before_quarter: ["00000000-0000-0000-0000-e4eaa4eaa4ea"],
        before_month: ["00000000-0000-0000-0000-e3eaa3eaa3ea"],
        before_week: ["00000000-0000-0000-0000-e2eee2eee2ee"],
        daily_slots: [
          {
            days: ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"],
            templates: ["00000000-0000-0000-0000-e1d0a11d0a11"],
          },
        ],
        grouping: "Month",
      },
      description: "Year + quarter + month + week + day",
      swatch: "from-amber-100 to-amber-200",
    },
    {
      template: {
        ...DEFAULT_NOTEBOOK_TEMPLATE,
        id: "00000000-0000-0000-0000-0000n0planner3",
        name: "Weekly Compass",
        description:
          "Week-grouped planner — one weekly compass page + 7 dailies per week.",
        before_week: ["00000000-0000-0000-0000-e2eee2eee2ee"],
        daily_slots: [
          {
            days: ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"],
            templates: ["00000000-0000-0000-0000-e1d0a11d0a11"],
          },
        ],
        grouping: "Week",
      },
      description: "Week-grouped, daily 7×",
      swatch: "from-sky-100 to-sky-200",
    },
  ];
}

function NotebookTemplateGrid() {
  const items = useMemo(NOTEBOOK_GALLERY, []);
  return (
    <div className="grid grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-3">
      {items.map((e) => (
        <NotebookCard key={e.template.id} entry={e} />
      ))}
    </div>
  );
}

function NotebookCard({ entry }: { entry: NotebookEntry }) {
  const [copied, setCopied] = useState(false);
  const t = entry.template;
  function download() {
    // No journal-web-shim helper for notebook-template TOML yet — fall
    // back to JSON. When `serialize_notebook_template_toml` lands in
    // the shim, swap this branch.
    const text = JSON.stringify(t, null, 2);
    void navigator.clipboard.writeText(text).then(() => {
      setCopied(true);
      setTimeout(() => setCopied(false), 1600);
    });
  }
  const totalPages =
    t.year_start.length +
    t.before_quarter.length +
    t.before_month.length +
    t.before_week.length +
    t.daily_slots.reduce((acc, s) => acc + s.templates.length, 0);
  return (
    <article className="flex flex-col overflow-hidden rounded border border-slate-200 bg-white shadow-sm transition-shadow hover:shadow-md">
      <div
        className={`relative flex aspect-[16/9] flex-col items-center justify-center gap-2 bg-gradient-to-br ${entry.swatch} px-4 py-3`}
      >
        <NotebookSchematic tpl={t} />
        <span className="absolute right-2 top-2 rounded bg-white/80 px-2 py-0.5 text-[10px] uppercase tracking-wide text-slate-600">
          {t.grouping === "Week" ? "weekly" : "monthly"} planner
        </span>
      </div>
      <div className="flex flex-1 flex-col px-3 py-3">
        <div className="text-sm font-semibold text-slate-800">{t.name}</div>
        <div className="mt-1 flex-1 text-xs leading-snug text-slate-500">
          {t.description || entry.description}
        </div>
        <div className="mt-3 flex items-center justify-between">
          <span className="text-[11px] text-slate-400">
            {totalPages} page slot{totalPages === 1 ? "" : "s"} · groups by{" "}
            {t.grouping}
          </span>
          <button
            onClick={download}
            className="rounded bg-indigo-600 px-3 py-1 text-xs font-medium text-white hover:bg-indigo-700"
          >
            {copied ? "Copied!" : "Download"}
          </button>
        </div>
      </div>
    </article>
  );
}

/**
 * Tiny schematic of a notebook template — five bands of colored dots
 * representing year / quarter / month / week / day slots, one dot per
 * assigned page.
 */
function NotebookSchematic({ tpl }: { tpl: NotebookTemplate }) {
  const bands: { label: string; n: number; tone: string }[] = [
    { label: "Y", n: tpl.year_start.length, tone: "bg-rose-500" },
    { label: "Q", n: tpl.before_quarter.length, tone: "bg-amber-500" },
    { label: "M", n: tpl.before_month.length, tone: "bg-emerald-500" },
    { label: "W", n: tpl.before_week.length, tone: "bg-sky-500" },
    {
      label: "D",
      n: tpl.daily_slots.reduce((acc, s) => acc + s.templates.length, 0),
      tone: "bg-indigo-500",
    },
  ];
  return (
    <div className="flex flex-col gap-1">
      {bands.map((b) => (
        <div key={b.label} className="flex items-center gap-2">
          <span className="w-4 text-right text-[10px] font-semibold text-slate-700">
            {b.label}
          </span>
          <div className="flex flex-1 gap-1">
            {b.n === 0 ? (
              <span className="text-[10px] italic text-slate-500/70">
                (empty)
              </span>
            ) : (
              Array.from({ length: Math.min(b.n, 8) }).map((_, i) => (
                <span
                  key={i}
                  className={`h-2 w-2 rounded-full ${b.tone}`}
                />
              ))
            )}
            {b.n > 8 && (
              <span className="text-[10px] text-slate-600">+{b.n - 8}</span>
            )}
          </div>
        </div>
      ))}
    </div>
  );
}

// ---------------------------------------------------------------------
// Brushes
// ---------------------------------------------------------------------

interface BrushEntry {
  brush: Brush;
  description: string;
  swatch: string;
}

function BRUSH_GALLERY(): BrushEntry[] {
  // Hand-authored brush stand-ins. Output via shim.serializeBrushToml
  // matches the desktop's brushes.toml format.
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
    {
      brush: inkPen,
      description: "Pressure-driven smooth ribbon. Crisp ink linework.",
      swatch: "from-slate-100 to-slate-200",
    },
    {
      brush: highlighter,
      description:
        "Flat-nib tip with Multiply blend — translucent highlights.",
      swatch: "from-yellow-100 to-yellow-200",
    },
    {
      brush: pencil,
      description: "Scatter geometry + reduced alpha for graphite tooth.",
      swatch: "from-stone-100 to-stone-200",
    },
    {
      brush: { ...DEFAULT_BRUSH, name: "Default" },
      description: "Stock brush — single smooth layer, default tip.",
      swatch: "from-indigo-50 to-indigo-100",
    },
  ];
}

function BrushGrid() {
  const items = useMemo(BRUSH_GALLERY, []);
  return (
    <div className="grid grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4">
      {items.map((e) => (
        <BrushCard key={e.brush.id} entry={e} />
      ))}
    </div>
  );
}

function BrushCard({ entry }: { entry: BrushEntry }) {
  const [copied, setCopied] = useState(false);
  function download() {
    let toml: string;
    try {
      toml = shim.serializeBrushToml(entry.brush);
    } catch {
      toml = JSON.stringify(entry.brush, null, 2);
    }
    void navigator.clipboard.writeText(toml).then(() => {
      setCopied(true);
      setTimeout(() => setCopied(false), 1600);
    });
  }
  return (
    <article className="flex flex-col overflow-hidden rounded border border-slate-200 bg-white shadow-sm transition-shadow hover:shadow-md">
      <div
        className={`relative flex aspect-[3/2] items-center justify-center bg-gradient-to-br ${entry.swatch}`}
      >
        <BrushPreview brush={entry.brush} />
      </div>
      <div className="flex flex-1 flex-col px-3 py-3">
        <div className="flex items-center gap-2">
          <span
            className="h-3 w-3 shrink-0 rounded-full border border-white shadow"
            style={{ background: rgbaTuple(entry.brush.default_color) }}
          />
          <span className="text-sm font-semibold text-slate-800">
            {entry.brush.name}
          </span>
        </div>
        <div className="mt-1 flex-1 text-xs leading-snug text-slate-500">
          {entry.description}
        </div>
        <div className="mt-3 flex items-center justify-between">
          <span className="text-[11px] text-slate-400">
            {entry.brush.layers.length} layer
            {entry.brush.layers.length === 1 ? "" : "s"}
          </span>
          <button
            onClick={download}
            className="rounded bg-indigo-600 px-3 py-1 text-xs font-medium text-white hover:bg-indigo-700"
          >
            {copied ? "Copied!" : "Download"}
          </button>
        </div>
      </div>
    </article>
  );
}

/**
 * Cheap SVG stroke preview — paints a smooth bezier in the brush's
 * default color, varying stroke-width across length to hint at width
 * modulation. Doesn't drive the wgpu pipeline; lives here only so the
 * gallery card has *something* visual without launching the full
 * Vello path.
 */
function BrushPreview({ brush }: { brush: Brush }) {
  const color = rgbaTuple(brush.default_color);
  const layer = brush.layers.find((l) => l.enabled) ?? brush.layers[0];
  const widthMult =
    layer && layer.width.type === "constant"
      ? layer.width.width_mult
      : 1;
  const blend =
    layer && layer.blend === "Multiply" ? "multiply" : "normal";
  return (
    <svg viewBox="0 0 220 60" className="h-[80%] w-[80%]">
      <defs>
        <linearGradient id="brushFade" x1="0" y1="0" x2="1" y2="0">
          <stop offset="0" stop-color={color} stop-opacity="0.4" />
          <stop offset="0.2" stop-color={color} stop-opacity="1" />
          <stop offset="0.8" stop-color={color} stop-opacity="1" />
          <stop offset="1" stop-color={color} stop-opacity="0.4" />
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
