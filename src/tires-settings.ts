export interface TiresSettings {
  showOilTemperature: boolean;
  showWaterTemperature: boolean;
}

export const TIRES_SETTINGS_KEY = "blackrack-overlay.tires.v1";

export const defaultTiresSettings = (): TiresSettings => ({
  showOilTemperature: false,
  showWaterTemperature: false
});

export const readTiresSettings = (): TiresSettings => {
  const defaults = defaultTiresSettings();
  try {
    const saved = JSON.parse(localStorage.getItem(TIRES_SETTINGS_KEY) ?? "null") as Partial<TiresSettings> | null;
    return {
      showOilTemperature: typeof saved?.showOilTemperature === "boolean"
        ? saved.showOilTemperature
        : defaults.showOilTemperature,
      showWaterTemperature: typeof saved?.showWaterTemperature === "boolean"
        ? saved.showWaterTemperature
        : defaults.showWaterTemperature
    };
  } catch {
    localStorage.removeItem(TIRES_SETTINGS_KEY);
    return defaults;
  }
};
