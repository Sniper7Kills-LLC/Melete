// TypeScript mirrors of the Rust brush types in
// `crates/journal-core/src/brush.rs`. Field names + casing match what
// serde emits.
//
// Internal-tagged enums use `#[serde(tag = "type", rename_all =
// "snake_case")]` on the Rust side, so the TS discriminator is `type`
// (not `kind` like template widgets). The wrapping `Brush` /
// `BrushLayer` structs serialize fields verbatim.
//
// Reference: crates/journal-core/src/brush.rs.

import type { BlendMode, Uuid } from "@/types";

// ---------------------------------------------------------------------
// Geometry — internal-tagged
// ---------------------------------------------------------------------

export type Geometry =
  | { type: "smooth"; resample_step_mm: number }
  | {
      type: "outline";
      resample_step_mm: number;
      smooth_outline: boolean;
    }
  | {
      type: "scatter";
      density: number;
      spread_mm: number;
      falloff: number;
      directional_bias_deg: number | null;
    }
  | { type: "dab_stamp"; step_mult: number }
  | { type: "fan_offset"; count: number; spread_mult: number };

export type GeometryKind = Geometry["type"];

// ---------------------------------------------------------------------
// WidthMode — internal-tagged
// ---------------------------------------------------------------------

export type WidthMode =
  | { type: "constant"; width_mult: number }
  | {
      type: "clamped_constant";
      width_mult: number;
      min_mm: number;
      max_mm: number;
    }
  | { type: "pressure"; floor: number; amp: number }
  | { type: "direction_angled"; nib_deg: number; min_ratio: number }
  | {
      type: "tilt_band";
      threshold: number;
      band_mult: number;
      alpha_scale: number;
    };

export type WidthModeKind = WidthMode["type"];

// ---------------------------------------------------------------------
// TipShape — internal-tagged
// ---------------------------------------------------------------------

export type TipShape =
  | { type: "round" }
  | { type: "square" }
  | { type: "flat_nib"; angle_deg: number; aspect: number }
  | { type: "diamond" }
  | { type: "star_n"; points: number; inner_ratio: number }
  | { type: "custom"; points: [number, number][] };

export type TipShapeKind = TipShape["type"];

// ---------------------------------------------------------------------
// CursorShape — internal-tagged
// ---------------------------------------------------------------------

export type CursorShape =
  | { type: "auto" }
  | { type: "circle" }
  | { type: "oval"; aspect: number }
  | { type: "exact_tip" }
  | { type: "custom"; points: [number, number][] };

export type CursorShapeKind = CursorShape["type"];

// ---------------------------------------------------------------------
// ColorMod — plain struct
// ---------------------------------------------------------------------

export interface ColorMod {
  alpha_mult: number;
  hue_shift_deg: number;
}

// ---------------------------------------------------------------------
// BrushLayer — plain struct
// ---------------------------------------------------------------------

export interface BrushLayer {
  enabled: boolean;
  geometry: Geometry;
  width: WidthMode;
  tip: TipShape;
  tip_scale: number;
  color: ColorMod;
  blend: BlendMode;
}

// ---------------------------------------------------------------------
// Brush — plain struct
// ---------------------------------------------------------------------

/**
 * Top-level brush definition. `default_color` is RGBA8 — the desktop
 * encodes it as a 4-element array, so we mirror that here. `null` keeps
 * the active toolbar color when the brush is applied.
 */
export interface Brush {
  id: Uuid;
  name: string;
  layers: BrushLayer[];
  cursor: CursorShape;
  default_color: [number, number, number, number] | null;
}

// ---------------------------------------------------------------------
// Defaults / helpers
// ---------------------------------------------------------------------

function uuid(): Uuid {
  if (typeof crypto !== "undefined" && "randomUUID" in crypto) {
    return crypto.randomUUID();
  }
  return "xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx".replace(/[xy]/g, (c) => {
    const r = (Math.random() * 16) | 0;
    const v = c === "x" ? r : (r & 0x3) | 0x8;
    return v.toString(16);
  });
}

export function defaultGeometry(kind: GeometryKind): Geometry {
  switch (kind) {
    case "smooth":
      return { type: "smooth", resample_step_mm: 0.5 };
    case "outline":
      return {
        type: "outline",
        resample_step_mm: 0.5,
        smooth_outline: true,
      };
    case "scatter":
      return {
        type: "scatter",
        density: 6,
        spread_mm: 1.0,
        falloff: 1.0,
        directional_bias_deg: null,
      };
    case "dab_stamp":
      return { type: "dab_stamp", step_mult: 1.0 };
    case "fan_offset":
      return { type: "fan_offset", count: 5, spread_mult: 0.6 };
  }
}

export function defaultWidth(kind: WidthModeKind): WidthMode {
  switch (kind) {
    case "constant":
      return { type: "constant", width_mult: 1.0 };
    case "clamped_constant":
      return {
        type: "clamped_constant",
        width_mult: 1.0,
        min_mm: 0.2,
        max_mm: 1.5,
      };
    case "pressure":
      return { type: "pressure", floor: 0.2, amp: 1.0 };
    case "direction_angled":
      return { type: "direction_angled", nib_deg: 45, min_ratio: 0.2 };
    case "tilt_band":
      return {
        type: "tilt_band",
        threshold: 0.4,
        band_mult: 1.5,
        alpha_scale: 0.5,
      };
  }
}

export function defaultTip(kind: TipShapeKind): TipShape {
  switch (kind) {
    case "round":
      return { type: "round" };
    case "square":
      return { type: "square" };
    case "flat_nib":
      return { type: "flat_nib", angle_deg: 45, aspect: 0.25 };
    case "diamond":
      return { type: "diamond" };
    case "star_n":
      return { type: "star_n", points: 5, inner_ratio: 0.5 };
    case "custom":
      return {
        type: "custom",
        points: [
          [0, -1],
          [0.6, -0.4],
          [0.4, 0.4],
          [0, 1],
          [-0.4, 0.4],
          [-0.6, -0.4],
        ],
      };
  }
}

export function defaultCursor(kind: CursorShapeKind): CursorShape {
  switch (kind) {
    case "auto":
      return { type: "auto" };
    case "circle":
      return { type: "circle" };
    case "oval":
      return { type: "oval", aspect: 0.5 };
    case "exact_tip":
      return { type: "exact_tip" };
    case "custom":
      return {
        type: "custom",
        points: [
          [0, -1],
          [1, 0],
          [0, 1],
          [-1, 0],
        ],
      };
  }
}

export function defaultColorMod(): ColorMod {
  return { alpha_mult: 1.0, hue_shift_deg: 0.0 };
}

/**
 * Default `BrushLayer` for a given geometry kind. Mirrors the
 * page-template designer's `defaultKindFor` pattern — picking a layer
 * type from the palette produces a sensible starter shape that the
 * user can then refine in the property panel.
 */
export function defaultLayer(kind: GeometryKind): BrushLayer {
  return {
    enabled: true,
    geometry: defaultGeometry(kind),
    width: defaultWidth("pressure"),
    tip: defaultTip("round"),
    tip_scale: 1.0,
    color: defaultColorMod(),
    blend: "Normal",
  };
}

/** Initial brush — one pressure-driven smooth round layer, ink-blue. */
export const DEFAULT_BRUSH: Brush = {
  id: uuid(),
  name: "Untitled brush",
  layers: [
    {
      enabled: true,
      geometry: { type: "smooth", resample_step_mm: 0.5 },
      width: { type: "pressure", floor: 0.2, amp: 1.0 },
      tip: { type: "round" },
      tip_scale: 1.0,
      color: { alpha_mult: 1.0, hue_shift_deg: 0.0 },
      blend: "Normal",
    },
  ],
  cursor: { type: "auto" },
  default_color: [60, 60, 80, 255],
};

/** Convenience — produce a fresh `Brush` with a new UUID. */
export function newBrush(): Brush {
  return {
    ...DEFAULT_BRUSH,
    id: uuid(),
    layers: DEFAULT_BRUSH.layers.map((l) => ({ ...l })),
  };
}

// ---------------------------------------------------------------------
// Display helpers
// ---------------------------------------------------------------------

export const GEOMETRY_LABELS: Record<GeometryKind, string> = {
  smooth: "Smooth",
  outline: "Outline",
  scatter: "Scatter",
  dab_stamp: "Dab stamp",
  fan_offset: "Fan offset",
};

export const WIDTH_LABELS: Record<WidthModeKind, string> = {
  constant: "Constant",
  clamped_constant: "Clamped constant",
  pressure: "Pressure",
  direction_angled: "Direction-angled",
  tilt_band: "Tilt band",
};

export const TIP_LABELS: Record<TipShapeKind, string> = {
  round: "Round",
  square: "Square",
  flat_nib: "Flat nib",
  diamond: "Diamond",
  star_n: "Star",
  custom: "Custom polygon",
};

export const CURSOR_LABELS: Record<CursorShapeKind, string> = {
  auto: "Auto (derive from layer)",
  circle: "Circle",
  oval: "Oval",
  exact_tip: "Exact tip",
  custom: "Custom polygon",
};

export const BLEND_LABELS: Record<BlendMode, string> = {
  Normal: "Normal",
  Multiply: "Multiply",
  Screen: "Screen",
  Overlay: "Overlay",
  Darken: "Darken",
  Lighten: "Lighten",
  Erase: "Erase",
};
