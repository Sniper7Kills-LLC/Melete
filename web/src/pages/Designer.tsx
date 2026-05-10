import { useEffect, useState } from "react";
import { useSearchParams } from "react-router-dom";
import { useAuthenticator } from "@aws-amplify/ui-react";

import { WidgetPalette } from "@/components/WidgetPalette";
import { DesignSurface } from "@/components/DesignSurface";
import { PropertyPanel } from "@/components/PropertyPanel";
import { SaveModal } from "@/components/SaveModal";
import { client } from "@/amplify-client";
import { isStubBackend } from "@/amplify-config";
import { templateIdString as tplId } from "@/types";
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
  const loadTemplate = useDesigner((s) => s.loadTemplate);
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
  const [searchParams, setSearchParams] = useSearchParams();
  const editId = searchParams.get("edit");
  const [loadStatus, setLoadStatus] = useState<
    null | { kind: "loading" } | { kind: "err"; message: string }
  >(null);
  // Server-row id, set when ?edit= hydrates and used to disambiguate
  // create-vs-update on Save. Cleared by Reset.
  const [currentRowId, setCurrentRowId] = useState<string | null>(null);
  const [saveStatus, setSaveStatus] = useState<
    | null
    | { kind: "saving" }
    | { kind: "ok"; at: number }
    | { kind: "err"; message: string }
  >(null);
  const { authStatus } = useAuthenticator((c) => [c.authStatus]);
  const signedIn = authStatus === "authenticated";

  // Load a public PageTemplate by id when ?edit=<id> is on the URL.
  // Uses apiKey auth so the lookup works for anonymous visitors. Once
  // fetched + parsed via the WASM shim, hydrates the designer store
  // and clears the query param so a refresh doesn't re-import.
  useEffect(() => {
    if (!editId) return;
    if (isStubBackend) {
      setLoadStatus({ kind: "err", message: "Backend not configured." });
      return;
    }
    let cancelled = false;
    setLoadStatus({ kind: "loading" });
    // userPool auth — Edit is reachable only from /my (signed-in
    // path), so the lookup must respect owner-scoped reads.
    client.models.PageTemplate.get(
      { id: editId },
      { authMode: "userPool" },
    )
      .then((r) => {
        if (cancelled) return;
        if (r.errors?.length) {
          setLoadStatus({
            kind: "err",
            message: r.errors.map((e) => e.message).join("; "),
          });
          return;
        }
        if (!r.data) {
          setLoadStatus({ kind: "err", message: "Template not found." });
          return;
        }
        try {
          const parsed = shim.parseTemplateToml(r.data.bodyToml);
          loadTemplate(parsed);
          setCurrentRowId(r.data.id);
          setLoadStatus(null);
          setSearchParams({}, { replace: true });
        } catch (e) {
          setLoadStatus({
            kind: "err",
            message: `Parse failed: ${e instanceof Error ? e.message : String(e)}`,
          });
        }
      })
      .catch((e: unknown) => {
        if (cancelled) return;
        setLoadStatus({
          kind: "err",
          message: e instanceof Error ? e.message : String(e),
        });
      });
    return () => {
      cancelled = true;
    };
  }, [editId, loadTemplate, setSearchParams]);

  function onSave() {
    const toml = shim.serializeTemplateToml(template);
    setSavedToml(toml);
  }

  // Persist to AppSync. Anonymous visitors get the TOML modal only;
  // signed-in callers create a new PRIVATE row or update the existing
  // one identified by `currentRowId`.
  async function onSaveToAccount() {
    if (!signedIn || isStubBackend) return;
    setSaveStatus({ kind: "saving" });
    try {
      const toml = shim.serializeTemplateToml(template);
      const now = new Date().toISOString();
      const id = currentRowId ?? tplId(template.id);
      if (currentRowId) {
        const r = await client.models.PageTemplate.update(
          {
            id,
            name: template.name,
            description: template.description ?? null,
            category: template.category ?? null,
            bodyToml: toml,
            updatedAtSort: now,
          },
          { authMode: "userPool" },
        );
        if (r.errors?.length)
          throw new Error(r.errors.map((e) => e.message).join("; "));
      } else {
        const r = await client.models.PageTemplate.create(
          {
            id,
            name: template.name,
            description: template.description ?? null,
            category: template.category ?? null,
            visibility: "PRIVATE",
            bodyToml: toml,
            updatedAtSort: now,
          },
          { authMode: "userPool" },
        );
        if (r.errors?.length)
          throw new Error(r.errors.map((e) => e.message).join("; "));
        setCurrentRowId(id);
      }
      setSaveStatus({ kind: "ok", at: Date.now() });
    } catch (e) {
      setSaveStatus({
        kind: "err",
        message: e instanceof Error ? e.message : String(e),
      });
    }
  }

  function onReset() {
    reset();
    setCurrentRowId(null);
    setSaveStatus(null);
  }

  return (
    <div className="flex h-full flex-col">
      {loadStatus?.kind === "loading" && (
        <div className="border-b border-slate-200 bg-slate-50 px-3 py-1 text-xs text-slate-500">
          Loading template…
        </div>
      )}
      {loadStatus?.kind === "err" && (
        <div className="border-b border-rose-200 bg-rose-50 px-3 py-1 text-xs text-rose-700">
          Could not load template: {loadStatus.message}
        </div>
      )}
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
          onClick={onReset}
          className="ml-3 rounded border border-slate-300 px-2 py-1 text-xs text-slate-600 hover:bg-slate-100"
          title="Reset template (clears undo + edit row)"
        >
          reset
        </button>

        <div className="ml-auto flex items-center gap-2">
          {saveStatus?.kind === "saving" && (
            <span className="text-xs text-slate-500">Saving…</span>
          )}
          {saveStatus?.kind === "ok" && (
            <span className="text-xs text-emerald-700">Saved ✓</span>
          )}
          {saveStatus?.kind === "err" && (
            <span
              title={saveStatus.message}
              className="max-w-[200px] truncate text-xs text-rose-700"
            >
              Save failed
            </span>
          )}
          <button
            onClick={onSave}
            className="rounded border border-slate-300 bg-white px-3 py-1.5 text-sm font-medium text-slate-700 hover:bg-slate-100"
            title="Show TOML in a modal — copy to your local config"
          >
            TOML
          </button>
          <button
            onClick={onSaveToAccount}
            disabled={!signedIn || isStubBackend}
            title={
              signedIn
                ? currentRowId
                  ? "Save changes to this row"
                  : "Create a new PRIVATE row in your library"
                : "Sign in to save to your library"
            }
            className="rounded bg-indigo-600 px-3 py-1.5 text-sm font-medium text-white hover:bg-indigo-700 disabled:cursor-not-allowed disabled:opacity-50"
          >
            {currentRowId ? "Save" : "Save to library"}
          </button>
        </div>
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
