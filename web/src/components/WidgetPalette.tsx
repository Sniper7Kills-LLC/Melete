import type { WidgetKindTag } from "@/types";
import { useDesigner } from "@/store/designerStore";

interface PaletteEntry {
  tag: WidgetKindTag;
  label: string;
  hint: string;
}

// Match the WidgetKind discriminator order the desktop editor uses.
// Keep the snake_case `tag` strings byte-identical to serde's output.
const PALETTE: PaletteEntry[] = [
  { tag: "text_block", label: "Text Block", hint: "Heading / freeform text" },
  { tag: "rectangle", label: "Rectangle", hint: "Outline / filled rect" },
  { tag: "ellipse", label: "Ellipse", hint: "Circle / oval" },
  { tag: "arc", label: "Arc", hint: "Partial ellipse / sector" },
  { tag: "line", label: "Line", hint: "Straight rule" },
  { tag: "grid_region", label: "Grid Region", hint: "Sub-grid area" },
  { tag: "lines_region", label: "Lines Region", hint: "Ruled area" },
  { tag: "dots_region", label: "Dots Region", hint: "Dotted area" },
  { tag: "calendar_month", label: "Calendar Month", hint: "Month view" },
  { tag: "timeline", label: "Timeline", hint: "Hour-slot column" },
  { tag: "checklist", label: "Checklist", hint: "Boxed task list" },
  { tag: "big_three", label: "Big Three", hint: "Daily top-3 boxes" },
  { tag: "priority_list", label: "Priority List", hint: "A/B/C list" },
  {
    tag: "daily_appointments",
    label: "Daily Appointments",
    hint: "Hour-by-hour schedule",
  },
  { tag: "weekly_compass", label: "Weekly Compass", hint: "Roles + goals" },
  {
    tag: "habit_tracker",
    label: "Habit Tracker",
    hint: "Habits × days grid",
  },
  { tag: "tally", label: "Tally", hint: "N empty circles + label" },
  { tag: "range_arcs", label: "Range Arcs", hint: "Concentric range fan" },
];

export function WidgetPalette() {
  const addWidget = useDesigner((s) => s.addWidget);

  return (
    <div className="flex h-full min-h-0 flex-col gap-2 border-r border-slate-200 bg-white p-3">
      <div className="text-xs font-semibold uppercase tracking-wide text-slate-500">
        Widgets
      </div>
      <div className="flex min-h-0 flex-1 flex-col gap-1 overflow-y-auto">
        {PALETTE.map((p) => (
          <button
            key={p.tag}
            type="button"
            draggable
            onDragStart={(e) => {
              e.dataTransfer.setData("application/x-widget-tag", p.tag);
              e.dataTransfer.effectAllowed = "copy";
            }}
            onClick={() => addWidget(p.tag)}
            className="flex flex-col gap-0.5 rounded border border-slate-200 bg-slate-50 px-2 py-1.5 text-left text-sm hover:border-indigo-400 hover:bg-indigo-50 active:bg-indigo-100"
            title={p.hint}
          >
            <span className="font-medium text-slate-800">{p.label}</span>
            <span className="text-xs text-slate-500">{p.hint}</span>
          </button>
        ))}
      </div>
      <p className="mt-2 text-xs text-slate-400">
        Click to drop at default position, or drag onto the canvas.
      </p>
    </div>
  );
}

export { PALETTE };
