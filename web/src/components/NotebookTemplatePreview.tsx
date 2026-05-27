// Notebook-template preview surface (#92).
//
// A notebook template is a *plan* for which page templates appear when —
// year-start, before-quarter, before-month, before-week, daily slots by
// day-of-week. There's nothing to render for the notebook template
// itself; the preview value is in showing the page templates it
// references so the user can see what the planner will look like once
// generated.
//
// MVP: fetch every referenced PageTemplate row from AppSync, group by
// section (year-start, monthly, weekly, daily), render each via the
// shared `TemplatePreview` component. Cards are small (200 px wide) so
// a dozen fit comfortably on one screen.

import { useEffect, useMemo, useState } from "react";
import { client } from "@/amplify-client";
import { shim } from "@/wasm";
import { TemplatePreview } from "@/components/TemplatePreview";
import type { PageTemplate, Uuid } from "@/types";
import type { NotebookTemplate } from "@/types/notebook-template";

interface Section {
  label: string;
  templateIds: Uuid[];
}

function buildSections(nt: NotebookTemplate): Section[] {
  const out: Section[] = [];
  if (nt.year_start.length) out.push({ label: "Year start", templateIds: nt.year_start });
  if (nt.before_quarter.length)
    out.push({ label: "Before each quarter", templateIds: nt.before_quarter });
  if (nt.before_month.length)
    out.push({ label: "Before each month", templateIds: nt.before_month });
  if (nt.before_week.length)
    out.push({ label: "Before each week", templateIds: nt.before_week });
  nt.daily_slots.forEach((slot, i) => {
    if (!slot.templates.length) return;
    const days = slot.days.join(", ");
    out.push({
      label: days ? `Daily — ${days}` : `Daily slot ${i + 1}`,
      templateIds: slot.templates,
    });
  });
  return out;
}

export function NotebookTemplatePreview({
  notebookTemplate,
}: {
  notebookTemplate: NotebookTemplate;
}) {
  const sections = useMemo(
    () => buildSections(notebookTemplate),
    [notebookTemplate],
  );

  // Flatten the unique referenced ids so we make one fetch per id and
  // share results across sections.
  const referencedIds = useMemo(() => {
    const set = new Set<Uuid>();
    for (const s of sections) for (const id of s.templateIds) set.add(id);
    return Array.from(set);
  }, [sections]);

  const [templates, setTemplates] = useState<Map<Uuid, PageTemplate | "missing">>(
    new Map(),
  );

  useEffect(() => {
    if (!referencedIds.length) return;
    let cancelled = false;
    (async () => {
      await shim.ready();
      const results = await Promise.all(
        referencedIds.map(async (id) => {
          try {
            const r = await client.models.PageTemplate.get(
              { id },
              { authMode: "apiKey" },
            );
            if (!r.data) return [id, "missing"] as const;
            try {
              const parsed = shim.parseTemplateToml(r.data.bodyToml);
              return [id, parsed] as const;
            } catch (e) {
              console.warn("[NTPreview] parse failed for", id, e);
              return [id, "missing"] as const;
            }
          } catch (e) {
            console.warn("[NTPreview] fetch failed for", id, e);
            return [id, "missing"] as const;
          }
        }),
      );
      if (cancelled) return;
      const next = new Map<Uuid, PageTemplate | "missing">();
      for (const [id, t] of results) next.set(id, t);
      setTemplates(next);
    })();
    return () => {
      cancelled = true;
    };
  }, [referencedIds]);

  if (!sections.length) {
    return (
      <div className="mt-4 rounded border border-amber-200 bg-amber-50 px-3 py-2 text-xs text-amber-800">
        This notebook template doesn&rsquo;t reference any page templates yet.
      </div>
    );
  }

  return (
    <div className="mt-4 space-y-6">
      <div>
        <span className="text-xs uppercase tracking-wide text-slate-500">
          Pages this planner generates
        </span>
        <p className="mt-1 text-xs text-slate-500">
          Grouped by when the template inserts each page (year start, weekly
          wrappers, day-of-week slots). Renders use the same Vello engine as
          the desktop.
        </p>
      </div>
      {sections.map((section, i) => (
        <section key={`${section.label}-${i}`}>
          <h2 className="mb-2 text-sm font-semibold text-slate-700">
            {section.label}
          </h2>
          <div className="grid grid-cols-2 gap-4 sm:grid-cols-3 md:grid-cols-4">
            {section.templateIds.map((id, j) => {
              const t = templates.get(id);
              return (
                <div
                  key={`${id}-${j}`}
                  className="flex flex-col items-center rounded border border-slate-200 bg-slate-50 p-2"
                >
                  {!t ? (
                    <div className="flex h-40 w-full items-center justify-center text-xs text-slate-400">
                      Loading…
                    </div>
                  ) : t === "missing" ? (
                    <div className="flex h-40 w-full flex-col items-center justify-center gap-1 text-center text-xs text-amber-700">
                      <div className="font-semibold">Template missing</div>
                      <div className="font-mono text-[10px] text-amber-600">
                        {id.slice(0, 8)}…
                      </div>
                    </div>
                  ) : (
                    <TemplatePreview template={t} zoom={0.9} />
                  )}
                  <div className="mt-2 line-clamp-1 w-full text-center text-[11px] text-slate-600">
                    {typeof t === "object" && t !== null ? t.name : id.slice(0, 8)}
                  </div>
                </div>
              );
            })}
          </div>
        </section>
      ))}
    </div>
  );
}
