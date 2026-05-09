import { create } from "zustand";

/**
 * Global length-unit preference. The canvas + serde data are always in
 * millimetres — `units` only affects how the SPA *displays* values to
 * the user and parses their input. Conversion happens at the UI edge
 * via `mmToDisplay` / `displayToMm`.
 *
 * Persisted in `localStorage` so the choice survives page reloads.
 */
export type LengthUnits = "mm" | "in";

interface UnitsState {
  units: LengthUnits;
  setUnits: (u: LengthUnits) => void;
}

const LS_KEY = "journal.units";

function loadInitial(): LengthUnits {
  try {
    const v = localStorage.getItem(LS_KEY);
    return v === "in" ? "in" : "mm";
  } catch {
    return "mm";
  }
}

export const useUnits = create<UnitsState>((set) => ({
  units: loadInitial(),
  setUnits: (u) => {
    try {
      localStorage.setItem(LS_KEY, u);
    } catch {
      // ignore — non-persisting environments still work for the session
    }
    set({ units: u });
  },
}));

export const MM_PER_INCH = 25.4;

/** Convert a millimetre value to the active display unit. */
export function mmToDisplay(mm: number, units: LengthUnits): number {
  return units === "in" ? mm / MM_PER_INCH : mm;
}

/** Inverse of `mmToDisplay` — convert user-entered display value back to mm. */
export function displayToMm(value: number, units: LengthUnits): number {
  return units === "in" ? value * MM_PER_INCH : value;
}

/** Format a millimetre value for read-only display. */
export function formatLength(mm: number, units: LengthUnits): string {
  if (units === "in") {
    const inches = mm / MM_PER_INCH;
    return `${inches.toFixed(2)} in`;
  }
  return `${mm.toFixed(mm === Math.round(mm) ? 0 : 1)} mm`;
}

/** Short suffix shown next to numeric inputs. */
export function unitsLabel(units: LengthUnits): string {
  return units;
}
