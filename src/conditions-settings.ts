export type ConditionsOptionId =
  | "weather"
  | "surface"
  | "grip"
  | "rain"
  | "air"
  | "track"
  | "wind"
  | "humidity"
  | "wetness";

export interface ConditionsSettings {
  visible: Record<ConditionsOptionId, boolean>;
}

export const CONDITIONS_SETTINGS_KEY = "blackrack-overlay.conditions.v1";

export const CONDITIONS_OPTIONS = [
  { id: "weather", labelKey: "conditions.condition" },
  { id: "surface", labelKey: "conditions.surface" },
  { id: "grip", labelKey: "conditions.grip" },
  { id: "rain", labelKey: "conditions.rain" },
  { id: "air", labelKey: "conditions.air" },
  { id: "track", labelKey: "conditions.track" },
  { id: "wind", labelKey: "conditions.wind" },
  { id: "humidity", labelKey: "conditions.humidity" },
  { id: "wetness", labelKey: "conditions.wetness" }
] as const satisfies readonly { id: ConditionsOptionId; labelKey: string }[];

export const defaultConditionsSettings = (): ConditionsSettings => ({
  visible: Object.fromEntries(CONDITIONS_OPTIONS.map(({ id }) => [id, true])) as Record<
    ConditionsOptionId,
    boolean
  >
});

export const normalizeConditionsSettings = (value: unknown): ConditionsSettings | null => {
  if (!value || typeof value !== "object") return null;
  const visible = (value as Partial<ConditionsSettings>).visible;
  if (!visible || typeof visible !== "object") return null;
  const fallback = defaultConditionsSettings();
  for (const { id } of CONDITIONS_OPTIONS) {
    if (visible[id] !== undefined && typeof visible[id] !== "boolean") return null;
    fallback.visible[id] = visible[id] ?? fallback.visible[id];
  }
  return fallback;
};

export const readConditionsSettings = (): ConditionsSettings => {
  try {
    return normalizeConditionsSettings(
      JSON.parse(localStorage.getItem(CONDITIONS_SETTINGS_KEY) ?? "null")
    ) ?? defaultConditionsSettings();
  } catch {
    return defaultConditionsSettings();
  }
};
