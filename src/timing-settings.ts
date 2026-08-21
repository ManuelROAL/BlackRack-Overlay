export type TimingSectorReference = "lmu" | "session" | "overall";

export interface TimingSettings {
  historyLaps: 0 | 3 | 5;
  sectorReference: TimingSectorReference;
}

export const TIMING_SETTINGS_KEY = "blackrack-overlay.timing.v1";

export const TIMING_SECTOR_REFERENCES = [
  { value: "lmu", labelKey: "timing.lmu" },
  { value: "session", labelKey: "timing.session" },
  { value: "overall", labelKey: "timing.overall" }
] as const;

const sectorReferenceValues = new Set<TimingSectorReference>(
  TIMING_SECTOR_REFERENCES.map(({ value }) => value)
);

export const defaultTimingSettings = (): TimingSettings => ({
  historyLaps: 3,
  sectorReference: "lmu"
});

export const isTimingSectorReference = (value: unknown): value is TimingSectorReference =>
  typeof value === "string" && sectorReferenceValues.has(value as TimingSectorReference);

export const normalizeTimingSettings = (value: unknown): TimingSettings => {
  const fallback = defaultTimingSettings();
  if (!value || typeof value !== "object") return fallback;
  const stored = value as Partial<TimingSettings>;
  const historyLaps = Number(stored.historyLaps);
  return {
    historyLaps: historyLaps === 0 || historyLaps === 5 ? historyLaps : fallback.historyLaps,
    sectorReference: isTimingSectorReference(stored.sectorReference)
      ? stored.sectorReference
      : fallback.sectorReference
  };
};

export const readTimingSettings = (): TimingSettings => {
  try {
    return normalizeTimingSettings(JSON.parse(localStorage.getItem(TIMING_SETTINGS_KEY) ?? "null"));
  } catch {
    return defaultTimingSettings();
  }
};
