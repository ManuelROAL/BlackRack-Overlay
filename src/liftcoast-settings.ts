export type LiftCoastDisplayMode = "always" | "active";

export interface LiftCoastSettings {
  displayMode: LiftCoastDisplayMode;
}

export const LIFTCOAST_SETTINGS_KEY = "blackrack-overlay.liftcoast.v1";

export const isLiftCoastDisplayMode = (value: unknown): value is LiftCoastDisplayMode =>
  value === "always" || value === "active";

export const defaultLiftCoastSettings = (): LiftCoastSettings => ({
  displayMode: "always"
});

export const normalizeLiftCoastSettings = (value: unknown): LiftCoastSettings | null => {
  if (!value || typeof value !== "object") return null;
  const displayMode = (value as Partial<LiftCoastSettings>).displayMode;
  if (displayMode !== undefined && !isLiftCoastDisplayMode(displayMode)) return null;
  return { displayMode: displayMode ?? "always" };
};

export const readLiftCoastSettings = (): LiftCoastSettings => {
  try {
    return normalizeLiftCoastSettings(
      JSON.parse(localStorage.getItem(LIFTCOAST_SETTINGS_KEY) ?? "null")
    ) ?? defaultLiftCoastSettings();
  } catch {
    return defaultLiftCoastSettings();
  }
};
