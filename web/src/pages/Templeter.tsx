import { useState } from "react";
import type { DragEvent as ReactDragEvent } from "react";

import {
  DEFAULT_NOTEBOOK_TEMPLATE,
  type DailySlot,
  type NotebookTemplate,
  type Weekday,
  WEEKDAYS,
} from "@/types/notebook-template";
import {
  EXAMPLE_TEMPLATES,
  findExampleTemplate,
  type ExampleTemplate,
} from "@/types/example-templates";
import type { Uuid } from "@/types";

const DRAG_MIME = "application/x-page-template-id";

type SlotKey =
  | "year_start"
  | "before_quarter"
  | "before_month"
  | "before_week";

interface SlotMeta {
  key: SlotKey;
  label: string;
  hint: string;
}

const SLOTS: SlotMeta[] = [
  { key: "year_start", label: "Year start", hint: "Pages at year boundary" },
  {
    key: "before_quarter",
    label: "Before quarter",
    hint: "Pages before each quarter",
  },
  {
    key: "before_month",
    label: "Before month",
    hint: "Pages before each month",
  },
  {
    key: "before_week",
    label: "Before week",
    hint: "Pages before each week",
  },
];

/**
 * Templeter — notebook-template designer (#11).
 *
 * Three-pane layout: a drag-source library of example page templates
 * on the left, the slot editor in the middle, and a "page view"
 * preview accordion that mocks the desktop's hierarchical planner
 * sidebar (Year → Month/Week wrapper → daily pages).
 */
export function Templeter() {
  const [tpl, setTpl] = useState<NotebookTemplate>(() => ({
    ...DEFAULT_NOTEBOOK_TEMPLATE,
    id: crypto.randomUUID(),
    // Seed with one empty Mon-Sun daily slot so the planner produces
    // something out of the box — drag a page template onto it to fill.
    daily_slots: [{ days: [...WEEKDAYS], templates: [] }],
  }));

  function update(patch: Partial<NotebookTemplate>) {
    setTpl((t) => ({ ...t, ...patch }));
  }

  function appendSlot(key: SlotKey, id: Uuid) {
    setTpl((t) => ({ ...t, [key]: [...t[key], id] }));
  }
  function removeFromSlot(key: SlotKey, idx: number) {
    setTpl((t) => ({ ...t, [key]: t[key].filter((_, i) => i !== idx) }));
  }

  function addDailySlot() {
    const slot: DailySlot = { days: [...WEEKDAYS], templates: [] };
    update({ daily_slots: [...tpl.daily_slots, slot] });
  }
  function updateDailySlot(idx: number, patch: Partial<DailySlot>) {
    const next = tpl.daily_slots.map((s, i) =>
      i === idx ? { ...s, ...patch } : s,
    );
    update({ daily_slots: next });
  }
  function removeDailySlot(idx: number) {
    update({ daily_slots: tpl.daily_slots.filter((_, i) => i !== idx) });
  }
  function appendDailyTemplate(slotIdx: number, id: Uuid) {
    const slot = tpl.daily_slots[slotIdx];
    updateDailySlot(slotIdx, { templates: [...slot.templates, id] });
  }
  function removeDailyTemplate(slotIdx: number, tplIdx: number) {
    const slot = tpl.daily_slots[slotIdx];
    updateDailySlot(slotIdx, {
      templates: slot.templates.filter((_, i) => i !== tplIdx),
    });
  }
  function toggleWeekday(slotIdx: number, day: Weekday) {
    const slot = tpl.daily_slots[slotIdx];
    const has = slot.days.includes(day);
    const days = has ? slot.days.filter((d) => d !== day) : [...slot.days, day];
    updateDailySlot(slotIdx, { days });
  }

  return (
    <div className="flex h-full">
      <ExampleLibrary />

      <main className="flex flex-1 min-h-0 flex-col overflow-y-auto p-6">
        <header className="mb-4 flex flex-wrap items-center gap-3">
          <input
            value={tpl.name}
            onChange={(e) => update({ name: e.target.value })}
            placeholder="Planner name"
            className="rounded border border-slate-300 bg-white px-3 py-1.5 text-base font-semibold text-slate-900 shadow-sm focus:border-indigo-500 focus:outline-none"
          />
          <select
            value={tpl.grouping}
            onChange={(e) =>
              update({
                grouping: e.target.value as NotebookTemplate["grouping"],
              })
            }
            className="rounded border border-slate-300 bg-white px-2 py-1 text-sm"
          >
            <option value="Month">Group by Month</option>
            <option value="Week">Group by Week</option>
          </select>
        </header>

        <textarea
          value={tpl.description}
          onChange={(e) => update({ description: e.target.value })}
          placeholder="Description (markdown)"
          rows={2}
          className="mb-6 w-full max-w-2xl rounded border border-slate-300 bg-white px-3 py-2 text-sm"
        />

        <Section title="Slots">
          <div className="grid grid-cols-1 gap-3 lg:grid-cols-2">
            {SLOTS.map((s) => (
              <SlotDropZone
                key={s.key}
                meta={s}
                ids={tpl[s.key]}
                onDrop={(id) => appendSlot(s.key, id)}
                onRemove={(i) => removeFromSlot(s.key, i)}
              />
            ))}
          </div>
        </Section>

        <Section title="Daily slots">
          <div className="space-y-3">
            {tpl.daily_slots.map((slot, i) => (
              <DailySlotCard
                key={i}
                slot={slot}
                index={i}
                onRemove={() => removeDailySlot(i)}
                onToggleWeekday={(d) => toggleWeekday(i, d)}
                onDropTemplate={(id) => appendDailyTemplate(i, id)}
                onRemoveTemplate={(t) => removeDailyTemplate(i, t)}
              />
            ))}
            <button
              onClick={addDailySlot}
              className="rounded border border-dashed border-slate-300 px-3 py-1.5 text-sm text-slate-600 hover:border-indigo-400 hover:text-indigo-700"
            >
              + Add daily slot
            </button>
          </div>
        </Section>

        <Section title="Title formats">
          <FormatRow
            label="Page title"
            value={tpl.page_title_format}
            onChange={(v) => update({ page_title_format: v })}
            hint="Tokens: {date} {weekday} {month_name} {year} {week} {day} {month}"
          />
          <FormatRow
            label="Year section"
            value={tpl.section_title_formats.year}
            onChange={(v) =>
              update({
                section_title_formats: {
                  ...tpl.section_title_formats,
                  year: v,
                },
              })
            }
          />
          <FormatRow
            label="Month section"
            value={tpl.section_title_formats.month}
            onChange={(v) =>
              update({
                section_title_formats: {
                  ...tpl.section_title_formats,
                  month: v,
                },
              })
            }
          />
          <FormatRow
            label="Week section"
            value={tpl.section_title_formats.week}
            onChange={(v) =>
              update({
                section_title_formats: {
                  ...tpl.section_title_formats,
                  week: v,
                },
              })
            }
          />
        </Section>

        <Section title="Preview">
          <Accordion title="Page view" defaultOpen>
            <DesktopSidebarMock tpl={tpl} />
          </Accordion>
          <Accordion title="JSON">
            <pre className="max-h-72 overflow-auto rounded bg-slate-900 p-3 text-xs leading-relaxed text-slate-100">
              {JSON.stringify(tpl, null, 2)}
            </pre>
          </Accordion>
        </Section>
      </main>
    </div>
  );
}

// ---------------------------------------------------------------------
// Example template library (drag source)
// ---------------------------------------------------------------------

function ExampleLibrary() {
  return (
    <aside className="flex w-64 shrink-0 flex-col border-r border-slate-200 bg-white">
      <div className="border-b border-slate-200 px-4 py-3 text-xs font-semibold uppercase tracking-wide text-slate-500">
        Example page templates
      </div>
      <div className="flex-1 min-h-0 overflow-y-auto p-3 space-y-2">
        {EXAMPLE_TEMPLATES.map((e) => (
          <ExampleCard key={tplId(e)} ex={e} />
        ))}
      </div>
      <p className="border-t border-slate-200 px-3 py-2 text-[11px] leading-snug text-slate-500">
        Drag onto a slot to assign. The desktop's full library will sync once
        the Amplify backend is wired.
      </p>
    </aside>
  );
}

function ExampleCard({ ex }: { ex: ExampleTemplate }) {
  return (
    <div
      draggable
      onDragStart={(e) => {
        e.dataTransfer.setData(DRAG_MIME, tplId(ex));
        e.dataTransfer.effectAllowed = "copy";
      }}
      className="cursor-grab rounded border border-slate-200 bg-white shadow-sm hover:border-indigo-400 active:cursor-grabbing"
      title={ex.template.description}
    >
      <div
        className={`flex h-16 items-center justify-center rounded-t bg-gradient-to-br ${ex.swatch} text-[10px] uppercase tracking-wide text-slate-700/70`}
      >
        {ex.template.category || "—"}
      </div>
      <div className="px-2 py-1.5">
        <div className="text-sm font-medium text-slate-800">
          {ex.template.name}
        </div>
        <div className="mt-0.5 line-clamp-2 text-[11px] leading-snug text-slate-500">
          {ex.template.description}
        </div>
      </div>
    </div>
  );
}

function tplId(e: ExampleTemplate): string {
  const id = e.template.id;
  return typeof id === "string" ? id : id["0"];
}

// ---------------------------------------------------------------------
// Slot drop zones
// ---------------------------------------------------------------------

function SlotDropZone({
  meta,
  ids,
  onDrop,
  onRemove,
}: {
  meta: SlotMeta;
  ids: Uuid[];
  onDrop: (id: Uuid) => void;
  onRemove: (idx: number) => void;
}) {
  const [over, setOver] = useState(false);
  function handleDragOver(e: ReactDragEvent) {
    if (!Array.from(e.dataTransfer.types).includes(DRAG_MIME)) return;
    e.preventDefault();
    e.dataTransfer.dropEffect = "copy";
    if (!over) setOver(true);
  }
  function handleDrop(e: ReactDragEvent) {
    e.preventDefault();
    setOver(false);
    const id = e.dataTransfer.getData(DRAG_MIME);
    if (id) onDrop(id);
  }
  return (
    <div
      onDragOver={handleDragOver}
      onDragLeave={() => setOver(false)}
      onDrop={handleDrop}
      className={`rounded border ${
        over
          ? "border-indigo-400 bg-indigo-50"
          : "border-slate-200 bg-white"
      } p-3 shadow-sm transition-colors`}
    >
      <div className="flex items-baseline justify-between">
        <div>
          <div className="text-sm font-semibold text-slate-800">
            {meta.label}
          </div>
          <div className="text-[11px] text-slate-500">{meta.hint}</div>
        </div>
        <span className="text-[11px] text-slate-400">
          {ids.length} template{ids.length === 1 ? "" : "s"}
        </span>
      </div>
      {ids.length === 0 ? (
        <div className="mt-2 rounded border border-dashed border-slate-300 bg-slate-50 px-2 py-3 text-center text-xs text-slate-400">
          Drop a page template here
        </div>
      ) : (
        <ol className="mt-2 space-y-1">
          {ids.map((id, i) => (
            <AssignedTemplateRow
              key={`${id}-${i}`}
              id={id}
              index={i}
              onRemove={() => onRemove(i)}
            />
          ))}
        </ol>
      )}
    </div>
  );
}

function AssignedTemplateRow({
  id,
  index,
  onRemove,
}: {
  id: Uuid;
  index: number;
  onRemove: () => void;
}) {
  const ex = findExampleTemplate(id);
  const name = ex?.template.name ?? id.slice(0, 8);
  const swatch = ex?.swatch ?? "from-slate-100 to-slate-200";
  return (
    <li className="flex items-center gap-2 rounded border border-slate-200 bg-white px-2 py-1 text-sm">
      <span
        className={`h-6 w-6 shrink-0 rounded bg-gradient-to-br ${swatch}`}
      />
      <span className="text-[11px] text-slate-400">#{index + 1}</span>
      <span className="flex-1 truncate text-slate-700">{name}</span>
      <button
        onClick={onRemove}
        className="text-[11px] text-rose-600 hover:underline"
      >
        remove
      </button>
    </li>
  );
}

// ---------------------------------------------------------------------
// Daily slot card
// ---------------------------------------------------------------------

function DailySlotCard({
  slot,
  index,
  onRemove,
  onToggleWeekday,
  onDropTemplate,
  onRemoveTemplate,
}: {
  slot: DailySlot;
  index: number;
  onRemove: () => void;
  onToggleWeekday: (d: Weekday) => void;
  onDropTemplate: (id: Uuid) => void;
  onRemoveTemplate: (i: number) => void;
}) {
  const [over, setOver] = useState(false);
  function handleDragOver(e: ReactDragEvent) {
    if (!Array.from(e.dataTransfer.types).includes(DRAG_MIME)) return;
    e.preventDefault();
    e.dataTransfer.dropEffect = "copy";
    if (!over) setOver(true);
  }
  function handleDrop(e: ReactDragEvent) {
    e.preventDefault();
    setOver(false);
    const id = e.dataTransfer.getData(DRAG_MIME);
    if (id) onDropTemplate(id);
  }
  return (
    <div
      onDragOver={handleDragOver}
      onDragLeave={() => setOver(false)}
      onDrop={handleDrop}
      className={`rounded border ${
        over ? "border-indigo-400 bg-indigo-50" : "border-slate-200 bg-white"
      } p-3 shadow-sm transition-colors`}
    >
      <div className="mb-2 flex items-center justify-between text-xs text-slate-500">
        <span>Daily slot #{index + 1}</span>
        <button
          onClick={onRemove}
          className="text-rose-600 hover:underline"
        >
          remove
        </button>
      </div>
      <div className="mb-2 flex flex-wrap gap-1">
        {WEEKDAYS.map((d) => {
          const on = slot.days.includes(d);
          return (
            <button
              key={d}
              onClick={() => onToggleWeekday(d)}
              className={`rounded px-2 py-0.5 text-xs ${
                on
                  ? "bg-indigo-600 text-white"
                  : "border border-slate-300 bg-white text-slate-600 hover:bg-slate-100"
              }`}
            >
              {d}
            </button>
          );
        })}
      </div>
      {slot.templates.length === 0 ? (
        <div className="rounded border border-dashed border-slate-300 bg-slate-50 px-2 py-2 text-center text-xs text-slate-400">
          Drop page templates here
        </div>
      ) : (
        <ol className="space-y-1">
          {slot.templates.map((id, i) => (
            <AssignedTemplateRow
              key={`${id}-${i}`}
              id={id}
              index={i}
              onRemove={() => onRemoveTemplate(i)}
            />
          ))}
        </ol>
      )}
    </div>
  );
}

// ---------------------------------------------------------------------
// "Page view" — desktop planner sidebar mock
// ---------------------------------------------------------------------

/**
 * Mock of the desktop's "What this generates" preview from
 * `crates/journal-app/src/notebook_template_creator.rs::refresh_layout_preview`.
 *
 * Status line up top with template name, page count, daily slot count,
 * and grouping mode. Below it: a horizontal strip of mini-page chips
 * grouped by section (Year ×1, Quarter ×4, Month ×3, Week ×4–5, then
 * one entry per weekday group, collapsing adjacent same-slot
 * weekdays). Empty groups show a dashed placeholder. Strip scrolls
 * horizontally — never vertically — so the editor stays compact.
 */
function DesktopSidebarMock({ tpl }: { tpl: NotebookTemplate }) {
  // For each weekday Mon-Sun, gather templates from EVERY daily slot
  // that includes that weekday (in slot order). A Monday listed in two
  // slots gets both slots' templates stacked. Then collapse adjacent
  // weekdays whose template lists are identical so a Mon-Fri slot
  // renders once as "Mon–Fri" instead of five identical columns. This
  // diverges from the desktop's first-owner-wins preview (which is a
  // visual simplification); the actual planner runtime emits one page
  // per matching slot, which is what we mirror here.
  const dayOrder: Weekday[] = [
    "Mon",
    "Tue",
    "Wed",
    "Thu",
    "Fri",
    "Sat",
    "Sun",
  ];
  const perDay: Uuid[][] = dayOrder.map((d) => {
    const out: Uuid[] = [];
    for (const slot of tpl.daily_slots) {
      if (!slot.days.includes(d)) continue;
      out.push(...slot.templates);
    }
    return out;
  });
  const collapsed: { label: string; days: number; ids: Uuid[] }[] = [];
  let wi = 0;
  while (wi < 7) {
    let wj = wi + 1;
    while (wj < 7 && sameIds(perDay[wj], perDay[wi])) wj++;
    const label =
      wj - wi === 1 ? dayOrder[wi] : `${dayOrder[wi]}–${dayOrder[wj - 1]}`;
    collapsed.push({ label, days: wj - wi, ids: perDay[wi] });
    wi = wj;
  }

  const totalPages =
    tpl.year_start.length +
    tpl.before_quarter.length +
    tpl.before_month.length +
    tpl.before_week.length +
    tpl.daily_slots.reduce((acc, s) => acc + s.templates.length, 0);
  const dailyCount = tpl.daily_slots.length;
  const grouping = tpl.grouping;
  const summaryName = tpl.name.trim() || "(untitled)";

  return (
    <div className="space-y-3">
      <div className="text-xs text-slate-500">
        <span className="font-semibold text-slate-700">{summaryName}</span>{" "}
        · {totalPages} page{totalPages === 1 ? "" : "s"}, {dailyCount} daily
        slot{dailyCount === 1 ? "" : "s"} · groups by {grouping}
      </div>
      <div className="overflow-x-auto rounded border border-slate-200 bg-slate-50 p-3">
        <div className="flex items-stretch gap-2">
          <SectionGroup label="Year" repeats="×1" ids={tpl.year_start} />
          <StripDivider />
          <SectionGroup
            label="Quarter"
            repeats="×4"
            ids={tpl.before_quarter}
          />
          <StripDivider />
          <SectionGroup label="Month" repeats="×3" ids={tpl.before_month} />
          <StripDivider />
          <SectionGroup
            label="Week"
            repeats="×4–5"
            ids={tpl.before_week}
          />
          <StripDivider />
          {collapsed.map((c, i) => (
            <span
              key={`${c.label}-${i}`}
              className="flex items-stretch gap-2"
            >
              <SectionGroup
                label={c.label}
                repeats={`×${c.days}`}
                ids={c.ids}
              />
              {i < collapsed.length - 1 && <StripDivider />}
            </span>
          ))}
        </div>
      </div>
    </div>
  );
}

function SectionGroup({
  label,
  repeats,
  ids,
}: {
  label: string;
  repeats: string;
  ids: Uuid[];
}) {
  return (
    <div className="flex items-center gap-2">
      <div className="flex flex-col items-end pr-1 text-right">
        <span className="text-[11px] font-semibold uppercase tracking-wide text-slate-600">
          {label}
        </span>
        <span className="text-[10px] text-slate-400">{repeats}</span>
      </div>
      {ids.length === 0 ? (
        <MiniEmpty />
      ) : (
        ids.map((id, i) => <MiniPagePreview key={`${id}-${i}`} templateId={id} />)
      )}
    </div>
  );
}

function StripDivider() {
  return <div className="mx-1 self-stretch border-l border-slate-300" />;
}

/**
 * Mini page-template chip — paper-coloured rounded rect with a
 * gradient swatch top-band for category and the page name underneath.
 * Mirrors the desktop's `mini_page_preview` (Cairo render in a Frame
 * with tooltip carrying the name).
 */
function MiniPagePreview({ templateId }: { templateId: Uuid }) {
  const ex = findExampleTemplate(templateId);
  const name = ex?.template.name ?? templateId.slice(0, 8);
  const swatch = ex?.swatch ?? "from-slate-200 to-slate-300";
  return (
    <div
      title={ex?.template.description || name}
      className="flex h-[80px] w-[60px] shrink-0 flex-col overflow-hidden rounded border border-slate-300 bg-white shadow-sm"
    >
      <div className={`h-3 bg-gradient-to-br ${swatch}`} />
      <div className="flex flex-1 items-center justify-center px-1 text-center text-[9px] leading-tight text-slate-700">
        {name}
      </div>
    </div>
  );
}

function sameIds(a: Uuid[], b: Uuid[]): boolean {
  if (a.length !== b.length) return false;
  for (let i = 0; i < a.length; i++) {
    if (a[i] !== b[i]) return false;
  }
  return true;
}

function MiniEmpty() {
  return (
    <div
      title="No page template assigned"
      className="flex h-[80px] w-[60px] shrink-0 items-center justify-center rounded border border-dashed border-slate-300 bg-white text-[10px] text-slate-400"
    >
      empty
    </div>
  );
}

// ---------------------------------------------------------------------
// Generic helpers
// ---------------------------------------------------------------------

function Section({
  title,
  children,
}: {
  title: string;
  children: React.ReactNode;
}) {
  return (
    <section className="mb-6">
      <h3 className="mb-2 text-xs font-semibold uppercase tracking-wide text-slate-500">
        {title}
      </h3>
      {children}
    </section>
  );
}

function FormatRow({
  label,
  value,
  onChange,
  hint,
}: {
  label: string;
  value: string;
  onChange: (v: string) => void;
  hint?: string;
}) {
  return (
    <label className="mb-2 flex items-center gap-3 text-sm">
      <span className="w-32 shrink-0 text-slate-600">{label}</span>
      <input
        value={value}
        onChange={(e) => onChange(e.target.value)}
        className="w-full max-w-md rounded border border-slate-300 bg-white px-2 py-1 font-mono text-xs"
      />
      {hint && (
        <span className="hidden text-xs text-slate-400 lg:inline">{hint}</span>
      )}
    </label>
  );
}

function Accordion({
  title,
  defaultOpen = false,
  children,
}: {
  title: string;
  defaultOpen?: boolean;
  children: React.ReactNode;
}) {
  return (
    <details
      open={defaultOpen}
      className="mb-2 rounded border border-slate-200 bg-white open:shadow-sm"
    >
      <summary className="cursor-pointer select-none px-3 py-2 text-sm font-medium text-slate-700 hover:bg-slate-50">
        {title}
      </summary>
      <div className="border-t border-slate-200 px-3 py-3">{children}</div>
    </details>
  );
}
