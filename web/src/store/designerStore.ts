import { create } from "zustand";

import type {
  PageTemplate,
  Uuid,
  Widget,
  WidgetKind,
  WidgetKindTag,
  WidgetRect,
  WidgetStyle,
} from "@/types";

// ---------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------

function uuid(): Uuid {
  // crypto.randomUUID is widely available; fall back to a v4-shaped
  // string for older runtimes.
  if (typeof crypto !== "undefined" && "randomUUID" in crypto) {
    return crypto.randomUUID();
  }
  return "xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx".replace(/[xy]/g, (c) => {
    const r = (Math.random() * 16) | 0;
    const v = c === "x" ? r : (r & 0x3) | 0x8;
    return v.toString(16);
  });
}

const DEFAULT_STYLE: WidgetStyle = {
  stroke_color: { r: 60, g: 60, b: 80, a: 200 },
  fill_color: null,
  stroke_width_mm: 0.3,
};

/** Default `WidgetKind` payload for every kind tag. */
export function defaultKindFor(tag: WidgetKindTag): WidgetKind {
  switch (tag) {
    case "text_block":
      return { kind: "text_block", text: "New text", font_size_mm: 5 };
    case "rectangle":
      return { kind: "rectangle" };
    case "ellipse":
      return { kind: "ellipse" };
    case "arc":
      return { kind: "arc", start_deg: 0, sweep_deg: 180, thickness_mm: 0.4 };
    case "line":
      return { kind: "line", thickness_mm: 0.4 };
    case "grid_region":
      return { kind: "grid_region", spacing_mm: 5 };
    case "lines_region":
      return { kind: "lines_region", spacing_mm: 7 };
    case "dots_region":
      return { kind: "dots_region", spacing_mm: 5 };
    case "calendar_month":
      return { kind: "calendar_month" };
    case "timeline":
      return {
        kind: "timeline",
        start_hour: 7,
        end_hour: 19,
        slot_minutes: 30,
      };
    case "checklist":
      return { kind: "checklist", items: ["Task 1", "Task 2", "Task 3"] };
    case "big_three":
      return { kind: "big_three" };
    case "priority_list":
      return { kind: "priority_list", count: 7 };
    case "daily_appointments":
      return { kind: "daily_appointments", start_hour: 7, end_hour: 19 };
    case "weekly_compass":
      return { kind: "weekly_compass" };
    case "habit_tracker":
      return {
        kind: "habit_tracker",
        habits: ["Read", "Workout", "Hydrate"],
        days: 31,
      };
    case "tally":
      return { kind: "tally", label: "Glasses of water", count: 8 };
    case "range_arcs":
      return {
        kind: "range_arcs",
        rings: 3,
        interval_m: 100,
        sweep_deg: 180,
        sector_deg: 60,
      };
    case "weather":
      return {
        kind: "weather",
        lat: 0,
        lon: 0,
        location_label: "",
        days: 3,
      };
    case "quote":
      return { kind: "quote", source: "zen" };
    case "bible_verse":
      return { kind: "bible_verse", reference: "random", translation: "kjv" };
    case "sunrise":
      return { kind: "sunrise", lat: 0, lon: 0 };
    case "moon_phase":
      return { kind: "moon_phase" };
    case "on_this_day":
      return { kind: "on_this_day", lang: "en", max_events: 5 };
    case "word_of_day":
      return { kind: "word_of_day", lang: "en" };
    case "rss_headline":
      return { kind: "rss_headline", url: "", count: 5 };
    case "astronomy":
      return { kind: "astronomy", lat: 0, lon: 0 };
  }
}

const DEFAULT_RECT: WidgetRect = { x: 20, y: 30, width: 60, height: 40 };

// ---------------------------------------------------------------------
// Initial template
// ---------------------------------------------------------------------

function makeBlankTemplate(): PageTemplate {
  return {
    id: uuid(),
    name: "Untitled Template",
    description: "",
    background: { kind: "Grid", spacing: 5 },
    size_mm: [215.9, 279.4],
    tiling: "None",
    default_viewport: null,
    widgets: [],
    category: "",
  };
}

// ---------------------------------------------------------------------
// Store
// ---------------------------------------------------------------------

interface DesignerState {
  template: PageTemplate;
  selectedWidgetId: Uuid | null;
  /** Snap-to-grid spacing, mm. */
  snapMm: number;
  /** Render guides toggle. */
  showGuides: boolean;

  // history
  undoStack: PageTemplate[];
  redoStack: PageTemplate[];

  // actions
  addWidget: (tag: WidgetKindTag) => void;
  removeWidget: (id: Uuid) => void;
  selectWidget: (id: Uuid | null) => void;
  updateWidget: (id: Uuid, patch: Partial<Widget>) => void;
  updateTemplateMeta: (patch: Partial<Omit<PageTemplate, "widgets">>) => void;
  setSnap: (mm: number) => void;
  setShowGuides: (on: boolean) => void;
  undo: () => void;
  redo: () => void;
  reset: () => void;
  /** Replace the current template wholesale; clears history. */
  loadTemplate: (t: PageTemplate) => void;
}

export const useDesigner = create<DesignerState>((set, get) => {
  const HISTORY_LIMIT = 100;

  function pushHistory(prev: PageTemplate) {
    const stack = [...get().undoStack, prev];
    if (stack.length > HISTORY_LIMIT) stack.shift();
    set({ undoStack: stack, redoStack: [] });
  }

  function snap(value: number, mm: number): number {
    if (mm <= 0) return value;
    return Math.round(value / mm) * mm;
  }

  return {
    template: makeBlankTemplate(),
    selectedWidgetId: null,
    snapMm: 5,
    showGuides: true,

    undoStack: [],
    redoStack: [],

    addWidget(tag) {
      const prev = get().template;
      pushHistory(prev);
      const id = uuid();
      const widget: Widget = {
        id,
        kind: defaultKindFor(tag),
        rect: { ...DEFAULT_RECT },
        style: { ...DEFAULT_STYLE },
      };
      set({
        template: { ...prev, widgets: [...prev.widgets, widget] },
        selectedWidgetId: id,
      });
    },

    removeWidget(id) {
      const prev = get().template;
      pushHistory(prev);
      set({
        template: {
          ...prev,
          widgets: prev.widgets.filter((w) => w.id !== id),
        },
        selectedWidgetId:
          get().selectedWidgetId === id ? null : get().selectedWidgetId,
      });
    },

    selectWidget(id) {
      set({ selectedWidgetId: id });
    },

    updateWidget(id, patch) {
      const prev = get().template;
      pushHistory(prev);
      const snapMm = get().snapMm;
      set({
        template: {
          ...prev,
          widgets: prev.widgets.map((w) => {
            if (w.id !== id) return w;
            const merged: Widget = { ...w, ...patch };
            // Snap rect to mm grid when present.
            if (patch.rect) {
              merged.rect = {
                x: snap(patch.rect.x, snapMm),
                y: snap(patch.rect.y, snapMm),
                width: Math.max(snap(patch.rect.width, snapMm), snapMm),
                height: Math.max(snap(patch.rect.height, snapMm), snapMm),
              };
            }
            return merged;
          }),
        },
      });
    },

    updateTemplateMeta(patch) {
      const prev = get().template;
      pushHistory(prev);
      set({ template: { ...prev, ...patch } });
    },

    setSnap(mm) {
      set({ snapMm: Math.max(0, mm) });
    },

    setShowGuides(on) {
      set({ showGuides: on });
    },

    undo() {
      const { undoStack, redoStack, template } = get();
      if (undoStack.length === 0) return;
      const next = undoStack[undoStack.length - 1];
      set({
        template: next,
        undoStack: undoStack.slice(0, -1),
        redoStack: [...redoStack, template],
      });
    },

    redo() {
      const { undoStack, redoStack, template } = get();
      if (redoStack.length === 0) return;
      const next = redoStack[redoStack.length - 1];
      set({
        template: next,
        redoStack: redoStack.slice(0, -1),
        undoStack: [...undoStack, template],
      });
    },

    reset() {
      set({
        template: makeBlankTemplate(),
        selectedWidgetId: null,
        undoStack: [],
        redoStack: [],
      });
    },

    loadTemplate(t) {
      set({
        template: t,
        selectedWidgetId: null,
        undoStack: [],
        redoStack: [],
      });
    },
  };
});

// Convenience selector — current selection's full Widget object.
export function useSelectedWidget(): Widget | null {
  return useDesigner((s) => {
    if (!s.selectedWidgetId) return null;
    return s.template.widgets.find((w) => w.id === s.selectedWidgetId) ?? null;
  });
}
