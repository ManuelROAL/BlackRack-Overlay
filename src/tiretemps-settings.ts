export interface TireTempsSettings {
  showSurface: boolean;
  showInnerLayer: boolean;
  showCarcass: boolean;
  showBrakes: boolean;
}

export const TIRE_TEMPS_SETTINGS_KEY = "blackrack-overlay.tire-temperatures.v1";

export const defaultTireTempsSettings = (): TireTempsSettings => ({
  showSurface: true,
  showInnerLayer: false,
  showCarcass: false,
  showBrakes: true
});

export const readTireTempsSettings = (): TireTempsSettings => {
  const settings = defaultTireTempsSettings();
  try {
    const stored = JSON.parse(localStorage.getItem(TIRE_TEMPS_SETTINGS_KEY) ?? "{}") as Partial<TireTempsSettings>;
    for (const key of Object.keys(settings) as (keyof TireTempsSettings)[]) {
      if (typeof stored[key] === "boolean") settings[key] = stored[key];
    }
  } catch {
    localStorage.removeItem(TIRE_TEMPS_SETTINGS_KEY);
  }
  return settings;
};
