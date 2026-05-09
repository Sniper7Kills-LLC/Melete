import { useState } from "react";

import { WidgetPalette } from "@/components/WidgetPalette";
import { DesignSurface } from "@/components/DesignSurface";
import { PropertyPanel } from "@/components/PropertyPanel";
import { SaveModal } from "@/components/SaveModal";
import { useDesigner } from "@/store/designerStore";
import {
  displayToMm,
  mmToDisplay,
  unitsLabel,
  useUnits,
} from "@/store/unitsStore";
import { shim } from "@/wasm";

/**
 * Designer route. Three-pane layout:
 *   - left   : widget palette (drag or click to add)
 *   - center : design surface in mm coordinates
 *   - right  : property panel for the selected widget
 *
 * Top toolbar provides Undo / Redo / Save. Save runs the (mock) WASM
 * shim's `serializeTemplateToml` and shows the would-be TOML in a
 * modal. When the real shim ships, the modal will show genuine TOML.
 */
export function Designer() {
  const undo = useDesigner((s) => s.undo);
  const redo = useDesigner((s) => s.redo);
  const reset = useDesigner((s) => s.reset);
  const undoStack = useDesigner((s) => s.undoStack);
  const redoStack = useDesigner((s) => s.redoStack);
  const template = useDesigner((s) => s.template);
  const snapMm = useDesigner((s) => s.snapMm);
  const setSnap = useDesigner((s) => s.setSnap);
  const showGuides = useDesigner((s) => s.showGuides);
  const setShowGuides = useDesigner((s) => s.setShowGuides);
  const units = useUnits((s) => s.units);
  const snapStep = units === "in" ? 0.05 : 1;
  const snapDisplay = mmToDisplay(snapMm, units);

  const [savedToml, setSavedToml] = useState<string | null>(null);

  function onSave() {
    const toml = shim.serializeTemplateToml(template);
    setSavedToml(toml);
  }

  return (
    <div className="flex h-full flex-col">
      <div className="flex items-center gap-2 border-b border-slate-200 bg-white px-3 py-2 text-sm">
        <button
          onClick={undo}
          disabled={undoStack.length === 0}
          className="rounded border border-slate-300 px-2 py-1 disabled:opacity-40 hover:bg-slate-100"
        >
          ↶ Undo
        </button>
        <button
          onClick={redo}
          disabled={redoStack.length === 0}
          className="rounded border border-slate-300 px-2 py-1 disabled:opacity-40 hover:bg-slate-100"
        >
          ↷ Redo
        </button>
        <span className="ml-3 text-slate-300">|</span>
        <label className="ml-2 flex items-center gap-1 text-xs text-slate-600">
          snap ({unitsLabel(units)})
          <input
            type="number"
            min={0}
            step={snapStep}
            value={
              units === "in"
                ? Number(snapDisplay.toFixed(2))
                : snapDisplay
            }
            onChange={(e) => setSnap(displayToMm(Number(e.target.value), units))}
            className="w-16 rounded border border-slate-300 bg-white px-1 py-0.5 text-xs"
          />
        </label>
        <label className="ml-2 flex items-center gap-1 text-xs text-slate-600">
          <input
            type="checkbox"
            checked={showGuides}
            onChange={(e) => setShowGuides(e.target.checked)}
          />
          smart-guides
        </label>
        <button
          onClick={reset}
          className="ml-3 rounded border border-slate-300 px-2 py-1 text-xs text-slate-600 hover:bg-slate-100"
          title="Reset template (clears undo)"
        >
          reset
        </button>

        <button
          onClick={onSave}
          className="ml-auto rounded bg-indigo-600 px-3 py-1.5 text-sm font-medium text-white hover:bg-indigo-700"
        >
          Save
        </button>
      </div>

      <div className="grid flex-1 overflow-hidden grid-cols-[220px_1fr_300px]">
        <WidgetPalette />
        <DesignSurface />
        <PropertyPanel />
      </div>

      {savedToml !== null && (
        <SaveModal toml={savedToml} onClose={() => setSavedToml(null)} />
      )}
    </div>
  );
}
