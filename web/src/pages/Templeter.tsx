import { useState } from "react";

import {
  DEFAULT_NOTEBOOK_TEMPLATE,
  type DailySlot,
  type NotebookTemplate,
  type Weekday,
  WEEKDAYS,
} from "@/types/notebook-template";

/**
 * Templeter — notebook-template designer (#11).
 *
 * Lets the user describe a planner: which page templates auto-generate
 * at year-start / before each quarter / month / week, plus daily slots
 * with weekday selectors. Output schema mirrors
 * `journal_core::template::NotebookTemplate`.
 *
 * POC scope: structural editor + JSON preview. Real round-trip via
 * `journal-web-shim::serialize_notebook_template_toml` is a follow-up
 * (the shim only handles page templates today).
 */
export function Templeter() {
  const [tpl, setTpl] = useState<NotebookTemplate>(() => ({
    ...DEFAULT_NOTEBOOK_TEMPLATE,
    id: crypto.randomUUID(),
  }));

  function update(patch: Partial<NotebookTemplate>) {
    setTpl((t) => ({ ...t, ...patch }));
  }

  function addDailySlot() {
    const slot: DailySlot = { days: [...WEEKDAYS], templates: [] };
    update({ daily_slots: [...tpl.daily_slots, slot] });
  }

  function updateDailySlot(idx: number, patch: Partial<DailySlot>) {
    const next = tpl.daily_slots.map((s, i) => (i === idx ? { ...s, ...patch } : s));
    update({ daily_slots: next });
  }

  function removeDailySlot(idx: number) {
    update({ daily_slots: tpl.daily_slots.filter((_, i) => i !== idx) });
  }

  function toggleWeekday(slotIdx: number, day: Weekday) {
    const slot = tpl.daily_slots[slotIdx];
    const has = slot.days.includes(day);
    const days = has ? slot.days.filter((d) => d !== day) : [...slot.days, day];
    updateDailySlot(slotIdx, { days });
  }

  return (
    <div className="flex h-full">
      <aside className="w-64 shrink-0 overflow-y-auto border-r border-slate-200 bg-white p-4 text-sm">
        <h2 className="mb-3 text-xs font-semibold uppercase tracking-wide text-slate-500">
          Slots
        </h2>
        <ul className="space-y-1">
          {SLOTS.map((s) => (
            <li
              key={s.key}
              className="rounded border border-slate-200 bg-slate-50 px-2 py-1.5 text-slate-700"
            >
              <div className="font-medium">{s.label}</div>
              <div className="text-xs text-slate-500">{s.hint}</div>
            </li>
          ))}
        </ul>
        <p className="mt-4 text-xs text-slate-400">
          Drag page templates from the desktop library onto each slot. Web-side
          template picker is a follow-up — for now the slots accept manual
          template-id entry.
        </p>
      </aside>

      <main className="flex flex-1 min-h-0 flex-col overflow-y-auto p-6">
        <header className="mb-4 flex items-center gap-3">
          <input
            value={tpl.name}
            onChange={(e) => update({ name: e.target.value })}
            placeholder="Planner name"
            className="rounded border border-slate-300 bg-white px-3 py-1.5 text-base font-semibold text-slate-900 shadow-sm focus:border-indigo-500 focus:outline-none"
          />
          <select
            value={tpl.grouping}
            onChange={(e) =>
              update({ grouping: e.target.value as NotebookTemplate["grouping"] })
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
                section_title_formats: { ...tpl.section_title_formats, year: v },
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

        <Section title="Daily slots">
          <div className="space-y-3">
            {tpl.daily_slots.map((slot, i) => (
              <div
                key={i}
                className="rounded border border-slate-200 bg-white p-3 shadow-sm"
              >
                <div className="mb-2 flex items-center justify-between text-xs text-slate-500">
                  <span>Slot #{i + 1}</span>
                  <button
                    onClick={() => removeDailySlot(i)}
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
                        onClick={() => toggleWeekday(i, d)}
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
                <div className="text-xs text-slate-500">
                  {slot.templates.length === 0
                    ? "No page templates assigned."
                    : `${slot.templates.length} template(s)`}
                </div>
              </div>
            ))}
            <button
              onClick={addDailySlot}
              className="rounded border border-dashed border-slate-300 px-3 py-1.5 text-sm text-slate-600 hover:border-indigo-400 hover:text-indigo-700"
            >
              + Add daily slot
            </button>
          </div>
        </Section>

        <Section title="Preview (JSON)">
          <pre className="max-h-72 overflow-auto rounded bg-slate-900 p-3 text-xs leading-relaxed text-slate-100">
            {JSON.stringify(tpl, null, 2)}
          </pre>
        </Section>
      </main>
    </div>
  );
}

interface SlotMeta {
  key: keyof Pick<
    NotebookTemplate,
    "year_start" | "before_quarter" | "before_month" | "before_week"
  >;
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
