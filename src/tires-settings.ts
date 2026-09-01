export interface TiresSettings {
  showTireTemperature: boolean;
  showBrakeTemperature: boolean;
  showFlatSpot: boolean;
  showTireWear: boolean;
  showOilTemperature: boolean;
  showWaterTemperature: boolean;
}

export const TIRES_SETTINGS_KEY = "blackrack-overlay.tires.v1";

export const defaultTiresSettings = (): TiresSettings => ({
  showTireTemperature: true,
  showBrakeTemperature: true,
  showFlatSpot: true,
  showTireWear: true,
  showOilTemperature: false,
  showWaterTemperature: false
});

export const normalizeTiresSettings = (value: unknown): TiresSettings | null => {
  const defaults = defaultTiresSettings();
  if (value === null || value === undefined) return defaults;
  if (typeof value !== "object" || Array.isArray(value)) return null;
  const saved = value as Partial<Record<keyof TiresSettings, unknown>>;
  const keys = Object.keys(defaults) as Array<keyof TiresSettings>;
  if (keys.some((key) => saved[key] !== undefined && typeof saved[key] !== "boolean")) return null;
  return Object.fromEntries(keys.map((key) => [key, saved[key] ?? defaults[key]])) as unknown as TiresSettings;
};

export const readTiresSettings = (): TiresSettings => {
  const defaults = defaultTiresSettings();
  try {
    const saved = JSON.parse(localStorage.getItem(TIRES_SETTINGS_KEY) ?? "null") as unknown;
    const normalized = normalizeTiresSettings(saved);
    if (normalized) return normalized;
    localStorage.removeItem(TIRES_SETTINGS_KEY);
    return defaults;
  } catch {
    localStorage.removeItem(TIRES_SETTINGS_KEY);
    return defaults;
  }
};
