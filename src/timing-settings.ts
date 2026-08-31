export type TimingSectorReference = "lmu" | "session" | "overall";
export type TimingTimeId = "current" | "sessionPersonalBest" | "personalBest" | "last" | "average" | "optimal" | "estimated";

export interface TimingSettings {
  historyLaps: 0 | 3 | 5;
  sectorReference: TimingSectorReference;
  times: Record<TimingTimeId, boolean>;
}

export const TIMING_SETTINGS_KEY = "blackrack-overlay.timing.v1";

export const TIMING_SECTOR_REFERENCES = [
  { value: "lmu", labelKey: "timing.lmu" },
  { value: "session", labelKey: "timing.session" },
  { value: "overall", labelKey: "timing.overall" }
] as const;

export const TIMING_TIMES = [
  { id: "current", labelKey: "timing.current" },
  { id: "sessionPersonalBest", labelKey: "timing.sessionPersonalBest" },
  { id: "personalBest", labelKey: "timing.personalBest" },
  { id: "last", labelKey: "timing.last" },
  { id: "average", labelKey: "timing.average" },
  { id: "optimal", labelKey: "timing.optimal" },
  { id: "estimated", labelKey: "timing.estimated" }
] as const satisfies readonly { id: TimingTimeId; labelKey: string }[];

const sectorReferenceValues = new Set<TimingSectorReference>(
  TIMING_SECTOR_REFERENCES.map(({ value }) => value)
);

export const defaultTimingSettings = (): TimingSettings => ({
  historyLaps: 3,
  sectorReference: "lmu",
  times: Object.fromEntries(TIMING_TIMES.map(({ id }) => [id, true])) as Record<TimingTimeId, boolean>
});

export const isTimingSectorReference = (value: unknown): value is TimingSectorReference =>
  typeof value === "string" && sectorReferenceValues.has(value as TimingSectorReference);

const normalizeTimingSettings = (value: unknown): TimingSettings => {
  const fallback = defaultTimingSettings();
  if (!value || typeof value !== "object") return fallback;
  const stored = value as Partial<TimingSettings>;
  const historyLaps = Number(stored.historyLaps);
  const times: Partial<Record<TimingTimeId, boolean>> =
    stored.times && typeof stored.times === "object" ? stored.times : {};
  return {
    historyLaps: historyLaps === 0 || historyLaps === 5 ? historyLaps : fallback.historyLaps,
    sectorReference: isTimingSectorReference(stored.sectorReference)
      ? stored.sectorReference
      : fallback.sectorReference,
    times: Object.fromEntries(TIMING_TIMES.map(({ id }) => [
      id,
      typeof times[id] === "boolean" ? times[id] : fallback.times[id]
    ])) as Record<TimingTimeId, boolean>
  };
};

export const readTimingSettings = (): TimingSettings => {
  try {
    return normalizeTimingSettings(JSON.parse(localStorage.getItem(TIMING_SETTINGS_KEY) ?? "null"));
  } catch {
    return defaultTimingSettings();
  }
};
