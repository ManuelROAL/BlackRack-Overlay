export type FuelScenarioMode = "total" | "refuel";

export const FUEL_FIELDS = [
  { id: "current", labelKey: "settings.fuelField.current" },
  { id: "autonomy", labelKey: "settings.fuelField.autonomy" },
  { id: "pitWindow", labelKey: "settings.fuelField.pitWindow" },
  { id: "postPit", labelKey: "settings.fuelField.postPit" },
  { id: "pitStatus", labelKey: "settings.fuelField.pitStatus" },
  { id: "level", labelKey: "settings.fuelField.level" },
  { id: "targets", labelKey: "settings.fuelField.targets" },
  { id: "average", labelKey: "settings.fuelField.average" },
  { id: "qualifying", labelKey: "settings.fuelField.qualifying" },
  { id: "last", labelKey: "settings.fuelField.last" },
  { id: "consumption", labelKey: "settings.fuelField.consumption" },
  { id: "scenarioAutonomy", labelKey: "settings.fuelField.scenarioAutonomy" },
  { id: "scenarioValue", labelKey: "settings.fuelField.scenarioValue" },
  { id: "fuelCard", labelKey: "settings.fuelField.fuelCard" },
  { id: "ratios", labelKey: "settings.fuelField.ratios" }
] as const;

export type FuelField = typeof FUEL_FIELDS[number]["id"];

export interface FuelSettings {
  scenarioMode: FuelScenarioMode;
  refuelMarginLiters: number;
  energyMarginPercent: number;
  visible: Record<FuelField, boolean>;
}

export const FUEL_SETTINGS_KEY = "blackrack-overlay.fuel-strategy.v1";

export const defaultFuelSettings = (): FuelSettings => ({
  scenarioMode: "total",
  refuelMarginLiters: 0.5,
  energyMarginPercent: 0,
  visible: Object.fromEntries(FUEL_FIELDS.map(({ id }) => [id, true])) as Record<FuelField, boolean>
});

export const isFuelScenarioMode = (value: unknown): value is FuelScenarioMode =>
  value === "total" || value === "refuel";

export const normalizeFuelSettings = (value: unknown): FuelSettings => {
  const fallback = defaultFuelSettings();
  if (!value || typeof value !== "object") return fallback;
  const stored = value as Partial<FuelSettings>;
  return {
    energyMarginPercent: typeof stored.energyMarginPercent === "number"
      && Number.isFinite(stored.energyMarginPercent)
      && stored.energyMarginPercent >= 0 && stored.energyMarginPercent <= 20
      ? stored.energyMarginPercent : fallback.energyMarginPercent,
    refuelMarginLiters: typeof stored.refuelMarginLiters === "number"
      && Number.isFinite(stored.refuelMarginLiters)
      && stored.refuelMarginLiters >= 0 && stored.refuelMarginLiters <= 20
      ? stored.refuelMarginLiters : fallback.refuelMarginLiters,
    visible: Object.fromEntries(FUEL_FIELDS.map(({ id }) => [
      id, typeof stored.visible?.[id] === "boolean" ? stored.visible[id] : true
    ])) as Record<FuelField, boolean>,
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
