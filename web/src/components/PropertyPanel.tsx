import type { ReactNode } from "react";

import type { Color, Widget, WidgetKind } from "@/types";
import { useDesigner, useSelectedWidget } from "@/store/designerStore";

/**
 * Right pane: edit every field of the selected widget. Each kind gets
 * its own block of inputs that mirror the Rust `WidgetKind` enum.
 */
export function PropertyPanel() {
  const widget = useSelectedWidget();
  const updateWidget = useDesigner((s) => s.updateWidget);
  const removeWidget = useDesigner((s) => s.removeWidget);
  const template = useDesigner((s) => s.template);
  const updateTemplateMeta = useDesigner((s) => s.updateTemplateMeta);

  if (!widget) {
    return (
      <div className="flex h-full flex-col gap-3 border-l border-slate-200 bg-white p-3 text-sm">
        <div className="text-xs font-semibold uppercase tracking-wide text-slate-500">
          Template
        </div>
        <Field label="Name">
          <input
            value={template.name}
            onChange={(e) => updateTemplateMeta({ name: e.target.value })}
            className={inputCls()}
          />
        </Field>
        <Field label="Description">
          <textarea
            value={template.description}
            onChange={(e) =>
              updateTemplateMeta({ description: e.target.value })
            }
            className={`${inputCls()} h-20`}
          />
        </Field>
        <Field label="Category">
          <input
            value={template.category}
            onChange={(e) => updateTemplateMeta({ category: e.target.value })}
            className={inputCls()}
          />
        </Field>
        <Field label="Page width (mm)">
          <input
            type="number"
            value={template.size_mm[0]}
            onChange={(e) =>
              updateTemplateMeta({
                size_mm: [Number(e.target.value), template.size_mm[1]],
              })
            }
            className={inputCls()}
          />
        </Field>
        <Field label="Page height (mm)">
          <input
            type="number"
            value={template.size_mm[1]}
            onChange={(e) =>
              updateTemplateMeta({
                size_mm: [template.size_mm[0], Number(e.target.value)],
              })
            }
            className={inputCls()}
          />
        </Field>
        <p className="mt-3 text-xs text-slate-400">
          Select a widget to edit its fields.
        </p>
      </div>
    );
  }

  return (
    <div className="flex h-full flex-col gap-3 overflow-auto border-l border-slate-200 bg-white p-3 text-sm">
      <div className="flex items-center justify-between">
        <div className="text-xs font-semibold uppercase tracking-wide text-slate-500">
          {widget.kind.kind}
        </div>
        <button
          type="button"
          onClick={() => removeWidget(widget.id)}
          className="text-xs text-red-600 hover:underline"
        >
          delete
        </button>
      </div>

      <RectFields widget={widget} />
      <StyleFields widget={widget} />
      <KindFields widget={widget} updateKind={(k) => updateWidget(widget.id, { kind: k })} />
    </div>
  );
}

function RectFields({ widget }: { widget: Widget }) {
  const updateWidget = useDesigner((s) => s.updateWidget);
  const r = widget.rect;
  function set(k: keyof typeof r, v: number) {
    updateWidget(widget.id, { rect: { ...r, [k]: v } });
  }
  return (
    <fieldset className="rounded border border-slate-200 p-2">
      <legend className="px-1 text-xs text-slate-500">Rect (mm)</legend>
      <div className="grid grid-cols-2 gap-2">
        <Field label="x">
          <input
            type="number"
            value={r.x}
            onChange={(e) => set("x", Number(e.target.value))}
            className={inputCls()}
          />
        </Field>
        <Field label="y">
          <input
            type="number"
            value={r.y}
            onChange={(e) => set("y", Number(e.target.value))}
            className={inputCls()}
          />
        </Field>
        <Field label="width">
          <input
            type="number"
            value={r.width}
            onChange={(e) => set("width", Number(e.target.value))}
            className={inputCls()}
          />
        </Field>
        <Field label="height">
          <input
            type="number"
            value={r.height}
            onChange={(e) => set("height", Number(e.target.value))}
            className={inputCls()}
          />
        </Field>
      </div>
    </fieldset>
  );
}

function StyleFields({ widget }: { widget: Widget }) {
  const updateWidget = useDesigner((s) => s.updateWidget);
  const s = widget.style;
  function setStroke(c: Color) {
    updateWidget(widget.id, { style: { ...s, stroke_color: c } });
  }
  function setFill(c: Color | null) {
    updateWidget(widget.id, { style: { ...s, fill_color: c } });
  }
  function setStrokeWidth(n: number) {
    updateWidget(widget.id, { style: { ...s, stroke_width_mm: n } });
  }
  return (
    <fieldset className="rounded border border-slate-200 p-2">
      <legend className="px-1 text-xs text-slate-500">Style</legend>
      <Field label="Stroke">
        <ColorInput color={s.stroke_color} onChange={setStroke} />
      </Field>
      <Field label="Fill">
        <div className="flex items-center gap-2">
          <input
            type="checkbox"
            checked={s.fill_color !== null}
            onChange={(e) =>
              setFill(
                e.target.checked
                  ? s.fill_color ?? { r: 255, g: 255, b: 255, a: 255 }
                  : null,
              )
            }
          />
          {s.fill_color && (
            <ColorInput color={s.fill_color} onChange={setFill} />
          )}
        </div>
      </Field>
      <Field label="Stroke width (mm)">
        <input
          type="number"
          step={0.1}
          value={s.stroke_width_mm}
          onChange={(e) => setStrokeWidth(Number(e.target.value))}
          className={inputCls()}
        />
      </Field>
    </fieldset>
  );
}

function KindFields({
  widget,
  updateKind,
}: {
  widget: Widget;
  updateKind: (k: WidgetKind) => void;
}) {
  const k = widget.kind;
  switch (k.kind) {
    case "text_block":
      return (
        <fieldset className="rounded border border-slate-200 p-2">
          <legend className="px-1 text-xs text-slate-500">Text Block</legend>
          <Field label="Text">
            <textarea
              value={k.text}
              onChange={(e) => updateKind({ ...k, text: e.target.value })}
              className={`${inputCls()} h-16`}
            />
          </Field>
          <Field label="Font size (mm)">
            <input
              type="number"
              step={0.5}
              value={k.font_size_mm}
              onChange={(e) =>
                updateKind({ ...k, font_size_mm: Number(e.target.value) })
              }
              className={inputCls()}
            />
          </Field>
        </fieldset>
      );
    case "arc":
      return (
        <fieldset className="rounded border border-slate-200 p-2">
          <legend className="px-1 text-xs text-slate-500">Arc</legend>
          <Field label="Start (deg)">
            <input
              type="number"
              value={k.start_deg}
              onChange={(e) =>
                updateKind({ ...k, start_deg: Number(e.target.value) })
              }
              className={inputCls()}
            />
          </Field>
          <Field label="Sweep (deg)">
            <input
              type="number"
              value={k.sweep_deg}
              onChange={(e) =>
                updateKind({ ...k, sweep_deg: Number(e.target.value) })
              }
              className={inputCls()}
            />
          </Field>
          <Field label="Thickness (mm)">
            <input
              type="number"
              step={0.1}
              value={k.thickness_mm}
              onChange={(e) =>
                updateKind({ ...k, thickness_mm: Number(e.target.value) })
              }
              className={inputCls()}
            />
          </Field>
        </fieldset>
      );
    case "line":
      return (
        <fieldset className="rounded border border-slate-200 p-2">
          <legend className="px-1 text-xs text-slate-500">Line</legend>
          <Field label="Thickness (mm)">
            <input
              type="number"
              step={0.1}
              value={k.thickness_mm}
              onChange={(e) =>
                updateKind({ ...k, thickness_mm: Number(e.target.value) })
              }
              className={inputCls()}
            />
          </Field>
        </fieldset>
      );
    case "grid_region":
    case "lines_region":
    case "dots_region":
      return (
        <fieldset className="rounded border border-slate-200 p-2">
          <legend className="px-1 text-xs text-slate-500">{k.kind}</legend>
          <Field label="Spacing (mm)">
            <input
              type="number"
              step={0.5}
              value={k.spacing_mm}
              onChange={(e) =>
                updateKind({ ...k, spacing_mm: Number(e.target.value) })
              }
              className={inputCls()}
            />
          </Field>
        </fieldset>
      );
    case "timeline":
      return (
        <fieldset className="rounded border border-slate-200 p-2">
          <legend className="px-1 text-xs text-slate-500">Timeline</legend>
          <Field label="Start hour">
            <input
              type="number"
              min={0}
              max={23}
              value={k.start_hour}
              onChange={(e) =>
                updateKind({ ...k, start_hour: Number(e.target.value) })
              }
              className={inputCls()}
            />
          </Field>
          <Field label="End hour">
            <input
              type="number"
              min={0}
              max={23}
              value={k.end_hour}
              onChange={(e) =>
                updateKind({ ...k, end_hour: Number(e.target.value) })
              }
              className={inputCls()}
            />
          </Field>
          <Field label="Slot minutes">
            <input
              type="number"
              min={5}
              value={k.slot_minutes}
              onChange={(e) =>
                updateKind({ ...k, slot_minutes: Number(e.target.value) })
              }
              className={inputCls()}
            />
          </Field>
        </fieldset>
      );
    case "checklist":
      return (
        <fieldset className="rounded border border-slate-200 p-2">
          <legend className="px-1 text-xs text-slate-500">Checklist</legend>
          <Field label="Items (one per line)">
            <textarea
              value={k.items.join("\n")}
              onChange={(e) =>
                updateKind({ ...k, items: e.target.value.split("\n") })
              }
              className={`${inputCls()} h-24`}
            />
          </Field>
        </fieldset>
      );
    case "priority_list":
      return (
        <fieldset className="rounded border border-slate-200 p-2">
          <legend className="px-1 text-xs text-slate-500">
            Priority List
          </legend>
          <Field label="Count">
            <input
              type="number"
              min={1}
              value={k.count}
              onChange={(e) =>
                updateKind({ ...k, count: Number(e.target.value) })
              }
              className={inputCls()}
            />
          </Field>
        </fieldset>
      );
    case "daily_appointments":
      return (
        <fieldset className="rounded border border-slate-200 p-2">
          <legend className="px-1 text-xs text-slate-500">
            Daily Appointments
          </legend>
          <Field label="Start hour">
            <input
              type="number"
              min={0}
              max={23}
              value={k.start_hour}
              onChange={(e) =>
                updateKind({ ...k, start_hour: Number(e.target.value) })
              }
              className={inputCls()}
            />
          </Field>
          <Field label="End hour">
            <input
              type="number"
              min={0}
              max={23}
              value={k.end_hour}
              onChange={(e) =>
                updateKind({ ...k, end_hour: Number(e.target.value) })
              }
              className={inputCls()}
            />
          </Field>
        </fieldset>
      );
    case "habit_tracker":
      return (
        <fieldset className="rounded border border-slate-200 p-2">
          <legend className="px-1 text-xs text-slate-500">Habit Tracker</legend>
          <Field label="Habits (one per line)">
            <textarea
              value={k.habits.join("\n")}
              onChange={(e) =>
                updateKind({ ...k, habits: e.target.value.split("\n") })
              }
              className={`${inputCls()} h-20`}
            />
          </Field>
          <Field label="Days">
            <input
              type="number"
              min={1}
              max={31}
              value={k.days}
              onChange={(e) =>
                updateKind({ ...k, days: Number(e.target.value) })
              }
              className={inputCls()}
            />
          </Field>
        </fieldset>
      );
    case "tally":
      return (
        <fieldset className="rounded border border-slate-200 p-2">
          <legend className="px-1 text-xs text-slate-500">Tally</legend>
          <Field label="Label">
            <input
              value={k.label}
              onChange={(e) => updateKind({ ...k, label: e.target.value })}
              className={inputCls()}
            />
          </Field>
          <Field label="Count">
            <input
              type="number"
              min={1}
              value={k.count}
              onChange={(e) =>
                updateKind({ ...k, count: Number(e.target.value) })
              }
              className={inputCls()}
            />
          </Field>
        </fieldset>
      );
    case "range_arcs":
      return (
        <fieldset className="rounded border border-slate-200 p-2">
          <legend className="px-1 text-xs text-slate-500">Range Arcs</legend>
          <Field label="Rings">
            <input
              type="number"
              min={1}
              value={k.rings}
              onChange={(e) =>
                updateKind({ ...k, rings: Number(e.target.value) })
              }
              className={inputCls()}
            />
          </Field>
          <Field label="Interval (m)">
            <input
              type="number"
              min={1}
              value={k.interval_m}
              onChange={(e) =>
                updateKind({ ...k, interval_m: Number(e.target.value) })
              }
              className={inputCls()}
            />
          </Field>
          <Field label="Sweep (deg)">
            <input
              type="number"
              value={k.sweep_deg}
              onChange={(e) =>
                updateKind({ ...k, sweep_deg: Number(e.target.value) })
              }
              className={inputCls()}
            />
          </Field>
          <Field label="Sector (deg)">
            <input
              type="number"
              value={k.sector_deg}
              onChange={(e) =>
                updateKind({ ...k, sector_deg: Number(e.target.value) })
              }
              className={inputCls()}
            />
          </Field>
        </fieldset>
      );
    default:
      return (
        <p className="text-xs text-slate-400">
          No editable fields for {widget.kind.kind}.
        </p>
      );
  }
}

// ---------------------------------------------------------------------
// Small UI helpers
// ---------------------------------------------------------------------

function Field({
  label,
  children,
}: {
  label: string;
  children: ReactNode;
}) {
  return (
    <label className="mb-2 flex flex-col gap-1 text-xs text-slate-600">
      <span>{label}</span>
      {children}
    </label>
  );
}

function ColorInput({
  color,
  onChange,
}: {
  color: Color;
  onChange: (c: Color) => void;
}) {
  // <input type="color"> only does #rrggbb; alpha gets a separate slider.
  const hex = `#${[color.r, color.g, color.b]
    .map((c) => c.toString(16).padStart(2, "0"))
    .join("")}`;
  return (
    <div className="flex items-center gap-2">
      <input
        type="color"
        value={hex}
        onChange={(e) => {
          const h = e.target.value.replace("#", "");
          onChange({
            r: parseInt(h.slice(0, 2), 16),
            g: parseInt(h.slice(2, 4), 16),
            b: parseInt(h.slice(4, 6), 16),
            a: color.a,
          });
        }}
        className="h-8 w-10 cursor-pointer rounded border border-slate-300"
      />
      <input
        type="range"
        min={0}
        max={255}
        value={color.a}
        onChange={(e) => onChange({ ...color, a: Number(e.target.value) })}
        title={`alpha ${color.a}`}
        className="flex-1"
      />
      <span className="w-8 text-right tabular-nums text-xs text-slate-500">
        {color.a}
      </span>
    </div>
  );
}

function inputCls() {
  return "w-full rounded border border-slate-300 bg-white px-2 py-1 text-sm focus:border-indigo-500 focus:outline-none";
}
