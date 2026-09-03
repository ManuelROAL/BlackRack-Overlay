export type DashboardFieldId =
  | "abs"
  | "air"
  | "battery"
  | "fuel"
  | "tc"
  | "delta"
  | "energy"
  | "map"
  | "gear"
  | "bestlap"
  | "track"
  | "position"
  | "revs"
  | "rpm"
  | "bias"
  | "session"
  | "lastlap"
  | "speed"
  | "lap"
  | "predicted"
  | "laps";

export interface DashboardSettings {
  visible: Record<DashboardFieldId, boolean>;
}

export const DASHBOARD_SETTINGS_KEY = "blackrack-overlay.dashboard.v1";

/**
 * The panel is one strip, so it stays readable only while it stays short. The
 * cap is what keeps it that way: the driver picks which readouts matter for the
 * car and the session instead of accumulating every field the game publishes.
 */
export const DASHBOARD_MAX_FIELDS = 8;

/**
 * Listed in the order the control panel shows them, which is alphabetical by
 * the Spanish label. The default eight are the ones that read the same in every
 * car and every session; anything car-specific is opt-in.
 */
export const DASHBOARD_FIELDS = [
  { id: "abs", labelKey: "dashboard.abs", default: false },
  { id: "air", labelKey: "dashboard.air", default: true },
  { id: "battery", labelKey: "dashboard.battery", default: false },
  { id: "fuel", labelKey: "dashboard.fuel", default: true },
  { id: "tc", labelKey: "dashboard.tc", default: false },
  { id: "delta", labelKey: "dashboard.delta", default: false },
  { id: "energy", labelKey: "dashboard.energy", default: false },
  { id: "map", labelKey: "dashboard.map", default: false },
  { id: "gear", labelKey: "dashboard.gear", default: true },
  { id: "bestlap", labelKey: "dashboard.bestlap", default: false },
  { id: "track", labelKey: "dashboard.track", default: true },
  { id: "position", labelKey: "dashboard.position", default: true },
  { id: "revs", labelKey: "dashboard.revs", default: true },
  { id: "rpm", labelKey: "dashboard.rpm", default: false },
  { id: "bias", labelKey: "dashboard.bias", default: false },
  { id: "session", labelKey: "dashboard.session", default: false },
  { id: "lastlap", labelKey: "dashboard.lastlap", default: true },
  { id: "speed", labelKey: "dashboard.speed", default: true },
  { id: "lap", labelKey: "dashboard.lap", default: false },
  { id: "predicted", labelKey: "dashboard.predicted", default: false },
  { id: "laps", labelKey: "dashboard.laps", default: false }
] as const satisfies readonly { id: DashboardFieldId; labelKey: string; default: boolean }[];

export const countVisibleDashboardFields = (settings: DashboardSettings): number =>
  DASHBOARD_FIELDS.filter(({ id }) => settings.visible[id]).length;

export const defaultDashboardSettings = (): DashboardSettings => ({
  visible: Object.fromEntries(
    DASHBOARD_FIELDS.map(({ id, default: enabled }) => [id, enabled])
  ) as Record<DashboardFieldId, boolean>
});

export const normalizeDashboardSettings = (value: unknown): DashboardSettings | null => {
  if (!value || typeof value !== "object") return null;
  const visible = (value as Partial<DashboardSettings>).visible;
  if (!visible || typeof visible !== "object") return null;
  const fallback = defaultDashboardSettings();
  for (const { id } of DASHBOARD_FIELDS) {
    if (visible[id] !== undefined && typeof visible[id] !== "boolean") return null;
    fallback.visible[id] = visible[id] ?? fallback.visible[id];
  }
  // A document written before the cap existed, or edited by hand, can ask for
  // more fields than the strip can carry. Keep the first ones in list order
  // rather than rejecting the whole configuration.
  let remaining = DASHBOARD_MAX_FIELDS;
  for (const { id } of DASHBOARD_FIELDS) {
    if (!fallback.visible[id]) continue;
    if (remaining > 0) remaining -= 1;
    else fallback.visible[id] = false;
  }
  return fallback;
};

export const readDashboardSettings = (): DashboardSettings => {
  try {
    return normalizeDashboardSettings(
      JSON.parse(localStorage.getItem(DASHBOARD_SETTINGS_KEY) ?? "null")
    ) ?? defaultDashboardSettings();
  } catch {
    return defaultDashboardSettings();
  }
};
