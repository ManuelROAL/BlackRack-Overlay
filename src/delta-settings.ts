export type DeltaMode =
  | "off"
  | "overall_best"
  | "overall_optimal_lap"
  | "overall_optimal_sectors"
  | "session_best"
  | "session_optimal_lap"
  | "session_optimal_sectors"
  | "stint_best"
  | "last_lap";

export interface DeltaSettings {
  mode: DeltaMode;
  displayRange: number;
}

export const DELTA_SETTINGS_KEY = "lmu-overlay.delta.v1";

export const DELTA_MODES: readonly { value: DeltaMode; label: string }[] = [
  { value: "off", label: "Desactivado" },
  { value: "overall_best", label: "Mejor global" },
  { value: "overall_optimal_lap", label: "Óptima global" },
  { value: "overall_optimal_sectors", label: "Sectores globales" },
  { value: "session_best", label: "Mejor de sesión" },
  { value: "session_optimal_lap", label: "Óptima de sesión" },
  { value: "session_optimal_sectors", label: "Sectores de sesión" },
  { value: "stint_best", label: "Mejor de stint" },
  { value: "last_lap", label: "Última vuelta" }
];

const modeValues = new Set<DeltaMode>(DELTA_MODES.map(({ value }) => value));
const displayRanges = new Set([0.5, 1, 2, 5]);

export const defaultDeltaSettings = (): DeltaSettings => ({
  mode: "session_best",
  displayRange: 2
});

export const isDeltaMode = (value: unknown): value is DeltaMode =>
  typeof value === "string" && modeValues.has(value as DeltaMode);

export const normalizeDeltaSettings = (value: unknown): DeltaSettings => {
  const fallback = defaultDeltaSettings();
  if (!value || typeof value !== "object") return fallback;
  const stored = value as Partial<DeltaSettings>;
  return {
    mode: isDeltaMode(stored.mode) ? stored.mode : fallback.mode,
    displayRange: typeof stored.displayRange === "number" && displayRanges.has(stored.displayRange)
      ? stored.displayRange
      : fallback.displayRange
  };
};

export const readDeltaSettings = (): DeltaSettings => {
  try {
    return normalizeDeltaSettings(JSON.parse(localStorage.getItem(DELTA_SETTINGS_KEY) ?? "null"));
  } catch {
    localStorage.removeItem(DELTA_SETTINGS_KEY);
    return defaultDeltaSettings();
  }
};
