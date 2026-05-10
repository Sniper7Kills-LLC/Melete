import { useEffect, useMemo, useState } from "react";
import { useSearchParams } from "react-router-dom";

import { client } from "@/amplify-client";
import { isStubBackend } from "@/amplify-config";
import type { BlendMode } from "@/types";
import {
  BLEND_LABELS,
  type Brush,
  type BrushLayer,
  CURSOR_LABELS,
  type CursorShape,
  type CursorShapeKind,
  GEOMETRY_LABELS,
  type Geometry,
  type GeometryKind,
  TIP_LABELS,
  type TipShape,
  type TipShapeKind,
  WIDTH_LABELS,
  type WidthMode,
  type WidthModeKind,
  defaultCursor,
  defaultGeometry,
  defaultLayer,
  defaultTip,
  defaultWidth,
  newBrush,
} from "@/types/brush";
import { shim } from "@/wasm";

const BLEND_MODES: BlendMode[] = [
  "Normal",
  "Multiply",
  "Screen",
  "Overlay",
  "Darken",
  "Lighten",
  "Erase",
];

const GEOMETRY_KINDS: GeometryKind[] = [
  "smooth",
  "outline",
  "scatter",
  "dab_stamp",
  "fan_offset",
];

const WIDTH_KINDS: WidthModeKind[] = [
  "constant",
  "clamped_constant",
  "pressure",
  "direction_angled",
  "tilt_band",
];

const TIP_KINDS: TipShapeKind[] = [
  "round",
  "square",
  "flat_nib",
  "diamond",
  "star_n",
  "custom",
];

const CURSOR_KINDS: CursorShapeKind[] = [
  "auto",
  "circle",
  "oval",
  "exact_tip",
  "custom",
];

/**
 * Tooler — brush composition POC (#37).
 *
 * Three-pane layout:
 *   - left   : layer-type palette + layer list
 *   - center : preview (static SVG stroke) + per-layer property editor
 *   - right  : brush-level metadata (name, color, cursor) + TOML preview
 *
 * Drawing input + live Vello render are intentionally deferred. This
 * route is a pure design surface — output is a TOML pane the user can
 * copy back to the desktop's `~/.config/journal/brushes.toml`.
 */
export function Tooler() {
  const [brush, setBrush] = useState<Brush>(() => newBrush());
  const [selectedLayerIdx, setSelectedLayerIdx] = useState<number | null>(0);
  const [searchParams, setSearchParams] = useSearchParams();
  const editId = searchParams.get("edit");
  const [loadStatus, setLoadStatus] = useState<
    null | { kind: "loading" } | { kind: "err"; message: string }
  >(null);

  // Load a public Brush by id when ?edit=<id> is on the URL. apiKey
  // auth keeps the lookup anonymous; the WASM shim parses the TOML.
  useEffect(() => {
    if (!editId) return;
    if (isStubBackend) {
      setLoadStatus({ kind: "err", message: "Backend not configured." });
      return;
    }
    let cancelled = false;
    setLoadStatus({ kind: "loading" });
    client.models.Brush.get({ id: editId }, { authMode: "apiKey" })
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
          setLoadStatus({ kind: "err", message: "Brush not found." });
          return;
        }
        try {
          const parsed = shim.parseBrushToml(r.data.bodyToml);
          setBrush(parsed);
          setSelectedLayerIdx(parsed.layers.length > 0 ? 0 : null);
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
  }, [editId, setSearchParams]);

  const selectedLayer =
    selectedLayerIdx !== null ? brush.layers[selectedLayerIdx] : null;

  // ---- mutations ------------------------------------------------------

  function patchBrush(patch: Partial<Brush>) {
    setBrush((b) => ({ ...b, ...patch }));
  }

  function addLayer(kind: GeometryKind) {
    const layer = defaultLayer(kind);
    setBrush((b) => ({ ...b, layers: [...b.layers, layer] }));
    setSelectedLayerIdx(brush.layers.length);
  }

  function removeLayer(idx: number) {
    setBrush((b) => ({
      ...b,
      layers: b.layers.filter((_, i) => i !== idx),
    }));
    setSelectedLayerIdx((cur) => {
      if (cur === null) return null;
      if (cur === idx) return null;
      if (cur > idx) return cur - 1;
      return cur;
    });
  }

  function moveLayer(idx: number, delta: -1 | 1) {
    const target = idx + delta;
    if (target < 0 || target >= brush.layers.length) return;
    setBrush((b) => {
      const layers = b.layers.slice();
      const tmp = layers[idx];
      layers[idx] = layers[target];
      layers[target] = tmp;
      return { ...b, layers };
    });
    setSelectedLayerIdx(target);
  }

  function patchLayer(idx: number, patch: Partial<BrushLayer>) {
    setBrush((b) => ({
      ...b,
      layers: b.layers.map((l, i) => (i === idx ? { ...l, ...patch } : l)),
    }));
  }

  function reset() {
    setBrush(newBrush());
    setSelectedLayerIdx(0);
  }

  // ---- preview --------------------------------------------------------

  const previewWidthMm = useMemo(() => {
    // Pick the highest-tip-scale enabled layer's width as the "headline"
    // width — purely cosmetic, just ensures the SVG preview reacts to
    // edits in the dominant layer.
    const enabled = brush.layers.filter((l) => l.enabled);
    if (enabled.length === 0) return 1.0;
    const dominant = enabled.reduce((best, l) =>
      l.tip_scale > best.tip_scale ? l : best,
    );
    return Math.max(0.2, headlineWidth(dominant.width) * dominant.tip_scale);
  }, [brush.layers]);

  const previewColor = colorToCss(brush.default_color) ?? "rgb(60, 60, 80)";

  const tomlPreview = useMemo(() => {
    try {
      return shim.serializeBrushToml(brush);
    } catch {
      // Fallback when WASM signature isn't available yet (older bundle
      // or first-load before `realShim` resolves).
      return JSON.stringify(brush, null, 2);
    }
  }, [brush]);

  const jsonPreview = useMemo(() => JSON.stringify(brush, null, 2), [brush]);

  // ---- render ---------------------------------------------------------

  return (
    <div className="flex h-full flex-col">
      {loadStatus?.kind === "loading" && (
        <div className="border-b border-slate-200 bg-slate-50 px-3 py-1 text-xs text-slate-500">
          Loading brush…
        </div>
      )}
      {loadStatus?.kind === "err" && (
        <div className="border-b border-rose-200 bg-rose-50 px-3 py-1 text-xs text-rose-700">
          Could not load brush: {loadStatus.message}
        </div>
      )}
      <div className="flex flex-1 overflow-hidden">
      {/* Left — palette + layers */}
      <aside className="flex w-64 shrink-0 flex-col border-r border-slate-200 bg-white">
        <div className="border-b border-slate-200 px-4 py-3 text-xs font-semibold uppercase tracking-wide text-slate-500">
          Add layer
        </div>
        <div className="grid grid-cols-1 gap-1 p-3">
          {GEOMETRY_KINDS.map((kind) => (
            <button
              key={kind}
              onClick={() => addLayer(kind)}
              className="rounded border border-slate-200 bg-white px-2 py-1.5 text-left text-sm text-slate-700 hover:border-indigo-400 hover:bg-indigo-50"
            >
              <span className="font-medium">{GEOMETRY_LABELS[kind]}</span>
              <span className="ml-1 text-[11px] text-slate-400">
                {kind}
              </span>
            </button>
          ))}
        </div>

        <div className="border-y border-slate-200 px-4 py-2 text-xs font-semibold uppercase tracking-wide text-slate-500">
          Layers
        </div>
        <ol className="flex-1 min-h-0 space-y-1 overflow-y-auto p-3">
          {brush.layers.length === 0 ? (
            <li className="rounded border border-dashed border-slate-300 bg-slate-50 px-2 py-3 text-center text-xs text-slate-400">
              No layers yet
            </li>
          ) : (
            brush.layers.map((layer, i) => {
              const selected = i === selectedLayerIdx;
              return (
                <li
                  key={i}
                  className={`rounded border ${
                    selected
                      ? "border-indigo-400 bg-indigo-50"
                      : "border-slate-200 bg-white hover:border-slate-300"
                  } shadow-sm`}
                >
                  <button
                    onClick={() => setSelectedLayerIdx(i)}
                    className="flex w-full items-baseline justify-between px-2 py-1 text-left"
                  >
                    <span className="text-xs font-medium text-slate-800">
                      #{i + 1} {GEOMETRY_LABELS[layer.geometry.type]}
                    </span>
                    <span className="text-[10px] text-slate-400">
                      {TIP_LABELS[layer.tip.type]}
                    </span>
                  </button>
                  <div className="flex items-center gap-1 border-t border-slate-100 px-2 py-1 text-[11px]">
                    <label className="flex items-center gap-1 text-slate-500">
                      <input
                        type="checkbox"
                        checked={layer.enabled}
                        onChange={(e) =>
                          patchLayer(i, { enabled: e.target.checked })
                        }
                      />
                      on
                    </label>
                    <button
                      onClick={() => moveLayer(i, -1)}
                      disabled={i === 0}
                      className="ml-auto rounded px-1 disabled:opacity-30 hover:bg-slate-100"
                    >
                      ↑
                    </button>
                    <button
                      onClick={() => moveLayer(i, 1)}
                      disabled={i === brush.layers.length - 1}
                      className="rounded px-1 disabled:opacity-30 hover:bg-slate-100"
                    >
                      ↓
                    </button>
                    <button
                      onClick={() => removeLayer(i)}
                      className="rounded px-1 text-rose-600 hover:bg-rose-50"
                    >
                      ×
                    </button>
                  </div>
                </li>
              );
            })
          )}
        </ol>
        <button
          onClick={reset}
          className="border-t border-slate-200 px-3 py-2 text-xs text-slate-500 hover:bg-slate-50"
          title="Reset brush"
        >
          reset brush
        </button>
      </aside>

      {/* Center — preview + property editor */}
      <main className="flex flex-1 min-h-0 flex-col overflow-y-auto p-6">
        <header className="mb-4 flex flex-wrap items-baseline gap-3">
          <h1 className="text-base font-semibold text-slate-800">Tooler</h1>
          <span className="text-xs text-slate-500">
            Brush composer · static preview only — no live drawing
          </span>
        </header>

        <Section title="Preview">
          <div className="rounded border border-slate-200 bg-slate-50 p-4">
            <StrokePreview
              widthMm={previewWidthMm}
              color={previewColor}
              brush={brush}
            />
            <div className="mt-2 text-[11px] text-slate-500">
              Stylised SVG curve. Width tracks the dominant layer's width
              mode; color tracks `default_color`. Vello-driven preview
              with real geometry / tip stamps is deferred.
            </div>
          </div>
        </Section>

        {selectedLayer && selectedLayerIdx !== null ? (
          <Section title={`Layer #${selectedLayerIdx + 1} properties`}>
            <LayerPropertyEditor
              layer={selectedLayer}
              onPatch={(p) => patchLayer(selectedLayerIdx, p)}
            />
          </Section>
        ) : (
          <Section title="Layer properties">
            <p className="text-xs text-slate-500">
              Select a layer on the left to edit its properties.
            </p>
          </Section>
        )}

        <Section title="Preview">
          <Accordion title="TOML" defaultOpen>
            <pre className="max-h-72 overflow-auto rounded bg-slate-900 p-3 text-xs leading-relaxed text-slate-100">
              {tomlPreview}
            </pre>
            <div className="mt-2 flex justify-end">
              <button
                onClick={() => navigator.clipboard.writeText(tomlPreview)}
                className="rounded border border-slate-300 px-3 py-1 text-xs hover:bg-slate-100"
              >
                copy TOML
              </button>
            </div>
          </Accordion>
          <Accordion title="JSON">
            <pre className="max-h-72 overflow-auto rounded bg-slate-900 p-3 text-xs leading-relaxed text-slate-100">
              {jsonPreview}
            </pre>
          </Accordion>
        </Section>
      </main>

      {/* Right — brush meta */}
      <aside className="w-72 shrink-0 overflow-y-auto border-l border-slate-200 bg-white p-4">
        <h3 className="mb-3 text-xs font-semibold uppercase tracking-wide text-slate-500">
          Brush
        </h3>

        <Field label="Name">
          <input
            value={brush.name}
            onChange={(e) => patchBrush({ name: e.target.value })}
            className="w-full rounded border border-slate-300 bg-white px-2 py-1 text-sm"
          />
        </Field>

        <Field label="Default color">
          <div className="flex items-center gap-2">
            <input
              type="color"
              value={rgbaToHex(brush.default_color) ?? "#3c3c50"}
              onChange={(e) =>
                patchBrush({ default_color: hexToRgba(e.target.value) })
              }
              className="h-8 w-12 cursor-pointer rounded border border-slate-300"
            />
            <label className="flex items-center gap-1 text-xs text-slate-600">
              <input
                type="checkbox"
                checked={brush.default_color !== null}
                onChange={(e) => {
                  const fallback: [number, number, number, number] = [
                    60, 60, 80, 255,
                  ];
                  patchBrush({
                    default_color: e.target.checked
                      ? brush.default_color ?? fallback
                      : null,
                  });
                }}
              />
              override toolbar
            </label>
          </div>
          {brush.default_color && (
            <Field label="Alpha">
              <input
                type="number"
                min={0}
                max={255}
                value={brush.default_color[3]}
                onChange={(e) => {
                  const c = brush.default_color!;
                  const a = clamp(parseInt(e.target.value || "0", 10), 0, 255);
                  patchBrush({ default_color: [c[0], c[1], c[2], a] });
                }}
                className="w-20 rounded border border-slate-300 bg-white px-2 py-1 text-sm"
              />
            </Field>
          )}
        </Field>

        <Field label="Cursor shape">
          <select
            value={brush.cursor.type}
            onChange={(e) =>
              patchBrush({ cursor: defaultCursor(e.target.value as CursorShapeKind) })
            }
            className="w-full rounded border border-slate-300 bg-white px-2 py-1 text-sm"
          >
            {CURSOR_KINDS.map((k) => (
              <option key={k} value={k}>
                {CURSOR_LABELS[k]}
              </option>
            ))}
          </select>
          <CursorEditor
            cursor={brush.cursor}
            onChange={(c) => patchBrush({ cursor: c })}
          />
        </Field>

        <p className="mt-6 rounded border border-amber-200 bg-amber-50 px-3 py-2 text-[11px] leading-snug text-amber-800">
          POC scope. Drawing input, live Vello rendering, brush library &
          sharing are deferred — see issue #37.
        </p>
      </aside>
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------
// Layer property editor
// ---------------------------------------------------------------------

function LayerPropertyEditor({
  layer,
  onPatch,
}: {
  layer: BrushLayer;
  onPatch: (p: Partial<BrushLayer>) => void;
}) {
  return (
    <div className="space-y-4 rounded border border-slate-200 bg-white p-4 shadow-sm">
      <Field label="Geometry">
        <select
          value={layer.geometry.type}
          onChange={(e) =>
            onPatch({ geometry: defaultGeometry(e.target.value as GeometryKind) })
          }
          className="w-full rounded border border-slate-300 bg-white px-2 py-1 text-sm"
        >
          {GEOMETRY_KINDS.map((k) => (
            <option key={k} value={k}>
              {GEOMETRY_LABELS[k]}
            </option>
          ))}
        </select>
        <GeometryParams
          geometry={layer.geometry}
          onChange={(g) => onPatch({ geometry: g })}
        />
      </Field>

      <Field label="Width mode">
        <select
          value={layer.width.type}
          onChange={(e) =>
            onPatch({ width: defaultWidth(e.target.value as WidthModeKind) })
          }
          className="w-full rounded border border-slate-300 bg-white px-2 py-1 text-sm"
        >
          {WIDTH_KINDS.map((k) => (
            <option key={k} value={k}>
              {WIDTH_LABELS[k]}
            </option>
          ))}
        </select>
        <WidthParams
          width={layer.width}
          onChange={(w) => onPatch({ width: w })}
        />
      </Field>

      <Field label="Tip shape">
        <select
          value={layer.tip.type}
          onChange={(e) =>
            onPatch({ tip: defaultTip(e.target.value as TipShapeKind) })
          }
          className="w-full rounded border border-slate-300 bg-white px-2 py-1 text-sm"
        >
          {TIP_KINDS.map((k) => (
            <option key={k} value={k}>
              {TIP_LABELS[k]}
            </option>
          ))}
        </select>
        <TipParams tip={layer.tip} onChange={(t) => onPatch({ tip: t })} />
      </Field>

      <Field label="Tip scale">
        <NumberInput
          value={layer.tip_scale}
          step={0.1}
          min={0.1}
          onChange={(v) => onPatch({ tip_scale: v })}
        />
      </Field>

      <Field label="Color modulation">
        <div className="grid grid-cols-2 gap-2">
          <label className="text-xs text-slate-600">
            <span className="block">alpha mult</span>
            <NumberInput
              value={layer.color.alpha_mult}
              step={0.05}
              min={0}
              max={2}
              onChange={(v) =>
                onPatch({ color: { ...layer.color, alpha_mult: v } })
              }
            />
          </label>
          <label className="text-xs text-slate-600">
            <span className="block">hue shift (°)</span>
            <NumberInput
              value={layer.color.hue_shift_deg}
              step={1}
              min={-180}
              max={180}
              onChange={(v) =>
                onPatch({ color: { ...layer.color, hue_shift_deg: v } })
              }
            />
          </label>
        </div>
      </Field>

      <Field label="Blend">
        <select
          value={layer.blend}
          onChange={(e) => onPatch({ blend: e.target.value as BlendMode })}
          className="w-full rounded border border-slate-300 bg-white px-2 py-1 text-sm"
        >
          {BLEND_MODES.map((b) => (
            <option key={b} value={b}>
              {BLEND_LABELS[b]}
            </option>
          ))}
        </select>
      </Field>
    </div>
  );
}

// ---------------------------------------------------------------------
// Per-kind parameter editors
// ---------------------------------------------------------------------

function GeometryParams({
  geometry,
  onChange,
}: {
  geometry: Geometry;
  onChange: (g: Geometry) => void;
}) {
  switch (geometry.type) {
    case "smooth":
      return (
        <Field label="resample step (mm)">
          <NumberInput
            value={geometry.resample_step_mm}
            step={0.1}
            min={0.05}
            onChange={(v) => onChange({ ...geometry, resample_step_mm: v })}
          />
        </Field>
      );
    case "outline":
      return (
        <>
          <Field label="resample step (mm)">
            <NumberInput
              value={geometry.resample_step_mm}
              step={0.1}
              min={0.05}
              onChange={(v) =>
                onChange({ ...geometry, resample_step_mm: v })
              }
            />
          </Field>
          <Field label="smooth outline">
            <input
              type="checkbox"
              checked={geometry.smooth_outline}
              onChange={(e) =>
                onChange({ ...geometry, smooth_outline: e.target.checked })
              }
            />
          </Field>
        </>
      );
    case "scatter":
      return (
        <>
          <Field label="density">
            <NumberInput
              value={geometry.density}
              step={1}
              min={1}
              onChange={(v) =>
                onChange({ ...geometry, density: Math.round(v) })
              }
            />
          </Field>
          <Field label="spread (mm)">
            <NumberInput
              value={geometry.spread_mm}
              step={0.1}
              min={0}
              onChange={(v) => onChange({ ...geometry, spread_mm: v })}
            />
          </Field>
          <Field label="falloff">
            <NumberInput
              value={geometry.falloff}
              step={0.1}
              min={0}
              onChange={(v) => onChange({ ...geometry, falloff: v })}
            />
          </Field>
          <Field label="directional bias (°)">
            <div className="flex items-center gap-2">
              <input
                type="checkbox"
                checked={geometry.directional_bias_deg !== null}
                onChange={(e) =>
                  onChange({
                    ...geometry,
                    directional_bias_deg: e.target.checked ? 0 : null,
                  })
                }
              />
              {geometry.directional_bias_deg !== null && (
                <NumberInput
                  value={geometry.directional_bias_deg}
                  step={1}
                  onChange={(v) =>
                    onChange({ ...geometry, directional_bias_deg: v })
                  }
                />
              )}
            </div>
          </Field>
        </>
      );
    case "dab_stamp":
      return (
        <Field label="step mult">
          <NumberInput
            value={geometry.step_mult}
            step={0.1}
            min={0.1}
            onChange={(v) => onChange({ ...geometry, step_mult: v })}
          />
        </Field>
      );
    case "fan_offset":
      return (
        <>
          <Field label="count">
            <NumberInput
              value={geometry.count}
              step={1}
              min={1}
              onChange={(v) =>
                onChange({ ...geometry, count: Math.round(v) })
              }
            />
          </Field>
          <Field label="spread mult">
            <NumberInput
              value={geometry.spread_mult}
              step={0.05}
              min={0}
              onChange={(v) => onChange({ ...geometry, spread_mult: v })}
            />
          </Field>
        </>
      );
  }
}

function WidthParams({
  width,
  onChange,
}: {
  width: WidthMode;
  onChange: (w: WidthMode) => void;
}) {
  switch (width.type) {
    case "constant":
      return (
        <Field label="width mult">
          <NumberInput
            value={width.width_mult}
            step={0.1}
            min={0}
            onChange={(v) => onChange({ ...width, width_mult: v })}
          />
        </Field>
      );
    case "clamped_constant":
      return (
        <>
          <Field label="width mult">
            <NumberInput
              value={width.width_mult}
              step={0.1}
              min={0}
              onChange={(v) => onChange({ ...width, width_mult: v })}
            />
          </Field>
          <Field label="min (mm)">
            <NumberInput
              value={width.min_mm}
              step={0.05}
              min={0}
              onChange={(v) => onChange({ ...width, min_mm: v })}
            />
          </Field>
          <Field label="max (mm)">
            <NumberInput
              value={width.max_mm}
              step={0.05}
              min={0}
              onChange={(v) => onChange({ ...width, max_mm: v })}
            />
          </Field>
        </>
      );
    case "pressure":
      return (
        <>
          <Field label="floor">
            <NumberInput
              value={width.floor}
              step={0.05}
              min={0}
              max={1}
              onChange={(v) => onChange({ ...width, floor: v })}
            />
          </Field>
          <Field label="amp">
            <NumberInput
              value={width.amp}
              step={0.05}
              min={0}
              onChange={(v) => onChange({ ...width, amp: v })}
            />
          </Field>
        </>
      );
    case "direction_angled":
      return (
        <>
          <Field label="nib angle (°)">
            <NumberInput
              value={width.nib_deg}
              step={1}
              onChange={(v) => onChange({ ...width, nib_deg: v })}
            />
          </Field>
          <Field label="min ratio">
            <NumberInput
              value={width.min_ratio}
              step={0.05}
              min={0}
              max={1}
              onChange={(v) => onChange({ ...width, min_ratio: v })}
            />
          </Field>
        </>
      );
    case "tilt_band":
      return (
        <>
          <Field label="threshold">
            <NumberInput
              value={width.threshold}
              step={0.05}
              min={0}
              max={1}
              onChange={(v) => onChange({ ...width, threshold: v })}
            />
          </Field>
          <Field label="band mult">
            <NumberInput
              value={width.band_mult}
              step={0.05}
              min={0}
              onChange={(v) => onChange({ ...width, band_mult: v })}
            />
          </Field>
          <Field label="alpha scale">
            <NumberInput
              value={width.alpha_scale}
              step={0.05}
              min={0}
              max={1}
              onChange={(v) => onChange({ ...width, alpha_scale: v })}
            />
          </Field>
        </>
      );
  }
}

function TipParams({
  tip,
  onChange,
}: {
  tip: TipShape;
  onChange: (t: TipShape) => void;
}) {
  switch (tip.type) {
    case "round":
    case "square":
    case "diamond":
      return null;
    case "flat_nib":
      return (
        <>
          <Field label="angle (°)">
            <NumberInput
              value={tip.angle_deg}
              step={1}
              onChange={(v) => onChange({ ...tip, angle_deg: v })}
            />
          </Field>
          <Field label="aspect">
            <NumberInput
              value={tip.aspect}
              step={0.05}
              min={0.05}
              max={1}
              onChange={(v) => onChange({ ...tip, aspect: v })}
            />
          </Field>
        </>
      );
    case "star_n":
      return (
        <>
          <Field label="points">
            <NumberInput
              value={tip.points}
              step={1}
              min={3}
              max={32}
              onChange={(v) =>
                onChange({ ...tip, points: Math.round(v) })
              }
            />
          </Field>
          <Field label="inner ratio">
            <NumberInput
              value={tip.inner_ratio}
              step={0.05}
              min={0.1}
              max={1}
              onChange={(v) => onChange({ ...tip, inner_ratio: v })}
            />
          </Field>
        </>
      );
    case "custom":
      return (
        <Field label={`polygon points (${tip.points.length})`}>
          <PointListEditor
            points={tip.points}
            onChange={(pts) => onChange({ ...tip, points: pts })}
          />
        </Field>
      );
  }
}

function CursorEditor({
  cursor,
  onChange,
}: {
  cursor: CursorShape;
  onChange: (c: CursorShape) => void;
}) {
  switch (cursor.type) {
    case "auto":
    case "circle":
    case "exact_tip":
      return null;
    case "oval":
      return (
        <Field label="aspect">
          <NumberInput
            value={cursor.aspect}
            step={0.05}
            min={0.05}
            max={4}
            onChange={(v) => onChange({ ...cursor, aspect: v })}
          />
        </Field>
      );
    case "custom":
      return (
        <Field label={`cursor points (${cursor.points.length})`}>
          <PointListEditor
            points={cursor.points}
            onChange={(pts) => onChange({ ...cursor, points: pts })}
          />
        </Field>
      );
  }
}

/**
 * Lightweight polygon editor — numeric inputs per vertex plus add /
 * remove. The desktop's Tool Editor has a draggable canvas; we surface
 * the same data here in a simpler form. Drawing-canvas drag editor is
 * deferred along with the Vello preview.
 */
function PointListEditor({
  points,
  onChange,
}: {
  points: [number, number][];
  onChange: (pts: [number, number][]) => void;
}) {
  function setPoint(i: number, p: [number, number]) {
    onChange(points.map((q, k) => (k === i ? p : q)));
  }
  function addPoint() {
    onChange([...points, [0, 0]]);
  }
  function removePoint(i: number) {
    if (points.length <= 3) return;
    onChange(points.filter((_, k) => k !== i));
  }

  return (
    <div className="space-y-1">
      {points.map((p, i) => (
        <div key={i} className="flex items-center gap-1 text-xs">
          <span className="w-6 text-slate-400">{i + 1}</span>
          <NumberInput
            value={p[0]}
            step={0.05}
            onChange={(v) => setPoint(i, [v, p[1]])}
          />
          <NumberInput
            value={p[1]}
            step={0.05}
            onChange={(v) => setPoint(i, [p[0], v])}
          />
          <button
            onClick={() => removePoint(i)}
            disabled={points.length <= 3}
            className="rounded px-1 text-rose-600 disabled:opacity-30 hover:bg-rose-50"
            title="remove (min 3)"
          >
            ×
          </button>
        </div>
      ))}
      <button
        onClick={addPoint}
        className="rounded border border-dashed border-slate-300 px-2 py-0.5 text-xs text-slate-500 hover:border-indigo-400 hover:text-indigo-700"
      >
        + add point
      </button>
      <p className="text-[11px] text-slate-400">
        Unit space (-1..1). Polygon auto-closes.
      </p>
    </div>
  );
}

// ---------------------------------------------------------------------
// Stylised SVG stroke preview
// ---------------------------------------------------------------------

function StrokePreview({
  widthMm,
  color,
  brush,
}: {
  widthMm: number;
  color: string;
  brush: Brush;
}) {
  // Render a stylised bezier curve. SVG width is in pixels; treat 1mm
  // ≈ 4px so a 1mm line reads as a chunky pen stroke. Capped to keep
  // the preview readable when users crank width_mult up.
  const px = Math.min(48, widthMm * 4);
  const dominant = brush.layers.find((l) => l.enabled) ?? brush.layers[0];
  const dasharray = dominant && dasharrayFor(dominant.geometry);
  return (
    <svg
      viewBox="0 0 400 80"
      className="h-20 w-full rounded bg-white"
      preserveAspectRatio="none"
    >
      <path
        d="M 10 60 C 80 10, 160 70, 240 30 S 380 50, 390 25"
        fill="none"
        stroke={color}
        strokeOpacity={dominant?.color.alpha_mult ?? 1}
        strokeWidth={px}
        strokeLinecap="round"
        strokeLinejoin="round"
        strokeDasharray={dasharray}
      />
    </svg>
  );
}

function dasharrayFor(g: Geometry): string | undefined {
  switch (g.type) {
    case "smooth":
    case "outline":
    case "fan_offset":
      return undefined;
    case "dab_stamp":
      return `${1 + g.step_mult * 4} ${g.step_mult * 4}`;
    case "scatter":
      return `1 ${Math.max(2, g.spread_mm * 4)}`;
  }
}

function headlineWidth(w: WidthMode): number {
  switch (w.type) {
    case "constant":
      return w.width_mult;
    case "clamped_constant":
      return Math.min(Math.max(w.width_mult, w.min_mm), w.max_mm);
    case "pressure":
      return w.floor + w.amp * 0.5;
    case "direction_angled":
      return 1.0;
    case "tilt_band":
      return w.band_mult;
  }
}

// ---------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------

function colorToCss(rgba: [number, number, number, number] | null): string | null {
  if (!rgba) return null;
  const [r, g, b, a] = rgba;
  return `rgba(${r}, ${g}, ${b}, ${a / 255})`;
}

function rgbaToHex(rgba: [number, number, number, number] | null): string | null {
  if (!rgba) return null;
  const [r, g, b] = rgba;
  return (
    "#" +
    [r, g, b]
      .map((c) => clamp(c, 0, 255).toString(16).padStart(2, "0"))
      .join("")
  );
}

function hexToRgba(hex: string): [number, number, number, number] {
  const v = hex.replace("#", "");
  const r = parseInt(v.slice(0, 2), 16) || 0;
  const g = parseInt(v.slice(2, 4), 16) || 0;
  const b = parseInt(v.slice(4, 6), 16) || 0;
  return [r, g, b, 255];
}

function clamp(v: number, lo: number, hi: number): number {
  return Math.max(lo, Math.min(hi, v));
}

// ---------------------------------------------------------------------
// Generic UI bits
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

function Field({
  label,
  children,
}: {
  label: string;
  children: React.ReactNode;
}) {
  return (
    <label className="mb-2 block text-xs text-slate-600">
      <span className="mb-1 block">{label}</span>
      {children}
    </label>
  );
}

function NumberInput({
  value,
  step,
  min,
  max,
  onChange,
}: {
  value: number;
  step?: number;
  min?: number;
  max?: number;
  onChange: (v: number) => void;
}) {
  return (
    <input
      type="number"
      value={Number.isFinite(value) ? value : 0}
      step={step}
      min={min}
      max={max}
      onChange={(e) => {
        const n = parseFloat(e.target.value);
        if (Number.isFinite(n)) onChange(n);
      }}
      className="w-full rounded border border-slate-300 bg-white px-2 py-1 text-sm"
    />
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
