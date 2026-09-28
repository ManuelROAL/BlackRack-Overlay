export type MinimapOrientation = "heading" | "north";

export interface MinimapSettings {
  /** Metres of circuit between the player and the rim of the disc. */
  viewRadiusMeters: number;
  /** `heading` turns the map with the car; `north` keeps the world fixed. */
  orientation: MinimapOrientation;
}

export const MINIMAP_SETTINGS_KEY = "blackrack-overlay.minimap-settings.v1";
export const MINIMAP_SETTINGS_EVENT = "minimap://settings";
export const MINIMAP_MIN_VIEW_RADIUS = 80;
export const MINIMAP_MAX_VIEW_RADIUS = 500;
export const MINIMAP_VIEW_RADIUS_STEP = 10;
export const MINIMAP_DEFAULT_VIEW_RADIUS = 190;

export const defaultMinimapSettings = (): MinimapSettings => ({
  viewRadiusMeters: MINIMAP_DEFAULT_VIEW_RADIUS,
  orientation: "heading"
});

export const isMinimapOrientation = (value: unknown): value is MinimapOrientation =>
  value === "heading" || value === "north";

export const isValidMinimapViewRadius = (value: unknown): value is number =>
  typeof value === "number" && Number.isInteger(value)
  && value >= MINIMAP_MIN_VIEW_RADIUS && value <= MINIMAP_MAX_VIEW_RADIUS
  && (value - MINIMAP_MIN_VIEW_RADIUS) % MINIMAP_VIEW_RADIUS_STEP === 0;

export const normalizeMinimapSettings = (value: unknown): MinimapSettings => {
  const source = value !== null && typeof value === "object" && !Array.isArray(value)
    ? value as Record<string, unknown>
    : {};
  const radius = source.viewRadiusMeters;
  const viewRadiusMeters = typeof radius === "number" && Number.isFinite(radius)
    ? Math.max(MINIMAP_MIN_VIEW_RADIUS, Math.min(
      MINIMAP_MAX_VIEW_RADIUS,
      MINIMAP_MIN_VIEW_RADIUS
        + Math.round((radius - MINIMAP_MIN_VIEW_RADIUS) / MINIMAP_VIEW_RADIUS_STEP) * MINIMAP_VIEW_RADIUS_STEP
    ))
    : MINIMAP_DEFAULT_VIEW_RADIUS;
  const orientation = isMinimapOrientation(source.orientation) ? source.orientation : "heading";
  return { viewRadiusMeters, orientation };
};

export const readMinimapSettings = (): MinimapSettings => {
  try {
    const stored = localStorage.getItem(MINIMAP_SETTINGS_KEY);
    return stored ? normalizeMinimapSettings(JSON.parse(stored)) : defaultMinimapSettings();
  } catch {
    return defaultMinimapSettings();
  }
};
