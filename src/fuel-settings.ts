export type FuelScenarioMode = "total" | "refuel";

export interface FuelSettings {
  scenarioMode: FuelScenarioMode;
}

export const FUEL_SETTINGS_KEY = "blackrack-overlay.fuel-strategy.v1";

export const defaultFuelSettings = (): FuelSettings => ({
  scenarioMode: "total"
});

export const isFuelScenarioMode = (value: unknown): value is FuelScenarioMode =>
  value === "total" || value === "refuel";

const normalizeFuelSettings = (value: unknown): FuelSettings => {
  const fallback = defaultFuelSettings();
  if (!value || typeof value !== "object") return fallback;
  const stored = value as Partial<FuelSettings>;
  return {
    scenarioMode: isFuelScenarioMode(stored.scenarioMode)
      ? stored.scenarioMode
      : fallback.scenarioMode
  };
};

export const readFuelSettings = (): FuelSettings => {
  try {
    return normalizeFuelSettings(JSON.parse(localStorage.getItem(FUEL_SETTINGS_KEY) ?? "null"));
  } catch {
    localStorage.removeItem(FUEL_SETTINGS_KEY);
    return defaultFuelSettings();
  }
};
