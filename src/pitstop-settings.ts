export interface PitStopSettings {
  showChanges: boolean;
}

export const PITSTOP_SETTINGS_KEY = "blackrack-overlay.pitstop.v1";

export const defaultPitStopSettings = (): PitStopSettings => ({ showChanges: true });

export const normalizePitStopSettings = (value: unknown): PitStopSettings | null => {
  if (!value || typeof value !== "object") return null;
  const showChanges = (value as Partial<PitStopSettings>).showChanges;
  return typeof showChanges === "boolean" ? { showChanges } : null;
};

export const readPitStopSettings = (): PitStopSettings => {
  try {
    return normalizePitStopSettings(
      JSON.parse(localStorage.getItem(PITSTOP_SETTINGS_KEY) ?? "null")
    ) ?? defaultPitStopSettings();
  } catch {
    return defaultPitStopSettings();
  }
};
