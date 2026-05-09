// Example PageTemplate library exposed to the Templeter for
// drag-and-drop onto notebook-template slots. Hand-authored stand-ins
// for the desktop's `journal-templates::builtin` library (the real
// library lives in the Rust crate; surfacing it on the web requires
// either bundling its TOML strings via journal-web-shim or duplicating
// here for the POC — duplicating is the lighter path).

import type { PageTemplate, Widget } from "./index";

function uuid(s: string): string {
  // Fixed-suffix UUIDs so the example references stay stable across
  // sessions and slot.templates references survive HMR.
  return `00000000-0000-0000-0000-${s.padStart(12, "0")}`;
}

function w(id: string, kind: Widget["kind"], rect: Widget["rect"]): Widget {
  return {
    id,
    kind,
    rect,
    style: {
      stroke_color: { r: 60, g: 60, b: 80, a: 220 },
      fill_color: null,
      stroke_width_mm: 0.4,
    },
  };
}

const A4: [number, number] = [210, 297];

function dailyTemplate(): PageTemplate {
  return {
    id: uuid("e1d0a11d0a11"),
    name: "Daily Plan",
    description: "Day at a glance — header + checklist + appointments.",
    background: { kind: "Lines", spacing: 7 },
    size_mm: A4,
    tiling: "None",
    default_viewport: null,
    category: "Daily",
    widgets: [
      w("d-title", { kind: "text_block", text: "{date}", font_size_mm: 12 }, {
        x: 15,
        y: 15,
        width: 180,
        height: 14,
      }),
      w(
        "d-weekday",
        { kind: "text_block", text: "{weekday}", font_size_mm: 6 },
        { x: 15, y: 30, width: 180, height: 8 },
      ),
      w(
        "d-checklist",
        {
          kind: "checklist",
          items: ["Top priority", "Second priority", "Reflect"],
        },
        { x: 15, y: 50, width: 90, height: 50 },
      ),
      w(
        "d-schedule",
        { kind: "daily_appointments", start_hour: 7, end_hour: 21 },
        { x: 110, y: 50, width: 85, height: 200 },
      ),
    ],
  };
}

function weeklyTemplate(): PageTemplate {
  return {
    id: uuid("e2eee2eee2ee"),
    name: "Weekly Compass",
    description: "Roles + weekly goals across the week.",
    background: { kind: "Grid", spacing: 5 },
    size_mm: A4,
    tiling: "None",
    default_viewport: null,
    category: "Weekly",
    widgets: [
      w(
        "w-title",
        { kind: "text_block", text: "Week of {date}", font_size_mm: 10 },
        { x: 15, y: 15, width: 180, height: 12 },
      ),
      w(
        "w-compass",
        { kind: "weekly_compass" },
        { x: 15, y: 35, width: 180, height: 240 },
      ),
    ],
  };
}

function monthlyTemplate(): PageTemplate {
  return {
    id: uuid("e3eaa3eaa3ea"),
    name: "Monthly Calendar",
    description: "Full-month grid for at-a-glance planning.",
    background: { kind: "Blank" },
    size_mm: A4,
    tiling: "None",
    default_viewport: null,
    category: "Monthly",
    widgets: [
      w(
        "m-title",
        { kind: "text_block", text: "{month_name} {year}", font_size_mm: 14 },
        { x: 15, y: 15, width: 180, height: 16 },
      ),
      w(
        "m-cal",
        { kind: "calendar_month" },
        { x: 15, y: 38, width: 180, height: 220 },
      ),
    ],
  };
}

function quarterlyTemplate(): PageTemplate {
  return {
    id: uuid("e4eaa4eaa4ea"),
    name: "Quarter Goals",
    description: "Big-three quarterly outcomes + tracking.",
    background: { kind: "Lines", spacing: 8 },
    size_mm: A4,
    tiling: "None",
    default_viewport: null,
    category: "Quarterly",
    widgets: [
      w(
        "q-title",
        { kind: "text_block", text: "Quarter Goals", font_size_mm: 14 },
        { x: 15, y: 15, width: 180, height: 16 },
      ),
      w(
        "q-big",
        { kind: "big_three" },
        { x: 15, y: 35, width: 180, height: 90 },
      ),
      w(
        "q-habits",
        {
          kind: "habit_tracker",
          habits: ["Daily review", "Exercise", "Sleep 8h"],
          days: 90,
        },
        { x: 15, y: 130, width: 180, height: 130 },
      ),
    ],
  };
}

function yearStartTemplate(): PageTemplate {
  return {
    id: uuid("e5e5e5e5e5e5"),
    name: "Year Vision",
    description: "Yearly direction page — used at year boundary.",
    background: { kind: "Dots", spacing: 5 },
    size_mm: A4,
    tiling: "None",
    default_viewport: null,
    category: "Yearly",
    widgets: [
      w(
        "y-title",
        { kind: "text_block", text: "{year} Vision", font_size_mm: 18 },
        { x: 15, y: 18, width: 180, height: 22 },
      ),
      w(
        "y-priorities",
        { kind: "priority_list", count: 8 },
        { x: 15, y: 50, width: 180, height: 200 },
      ),
    ],
  };
}

export interface ExampleTemplate {
  template: PageTemplate;
  /** Tailwind palette swatch used for the drag thumbnail. */
  swatch: string;
}

export const EXAMPLE_TEMPLATES: ExampleTemplate[] = [
  { template: yearStartTemplate(), swatch: "from-rose-100 to-rose-200" },
  { template: quarterlyTemplate(), swatch: "from-amber-100 to-amber-200" },
  { template: monthlyTemplate(), swatch: "from-emerald-100 to-emerald-200" },
  { template: weeklyTemplate(), swatch: "from-sky-100 to-sky-200" },
  { template: dailyTemplate(), swatch: "from-indigo-100 to-indigo-200" },
];

export function findExampleTemplate(id: string): ExampleTemplate | null {
  return (
    EXAMPLE_TEMPLATES.find((e) => {
      const tid =
        typeof e.template.id === "string" ? e.template.id : e.template.id["0"];
      return tid === id;
    }) ?? null
  );
}
