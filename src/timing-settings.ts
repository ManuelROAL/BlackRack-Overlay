export interface TimingSettings {
  historyLaps: 0 | 3 | 5;
}

export const TIMING_SETTINGS_KEY = "blackrack-overlay.timing.v1";

export const defaultTimingSettings = (): TimingSettings => ({ historyLaps: 3 });

export const normalizeTimingSettings = (value: unknown): TimingSettings => {
  const historyLaps = Number((value as Partial<TimingSettings> | null)?.historyLaps);
  return { historyLaps: historyLaps === 0 || historyLaps === 5 ? historyLaps : 3 };
};

export const readTimingSettings = (): TimingSettings => {
  try {
    return normalizeTimingSettings(JSON.parse(localStorage.getItem(TIMING_SETTINGS_KEY) ?? "null"));
  } catch {
    return defaultTimingSettings();
  }
};
