export type DashboardFieldId =
  | "abs"
  | "battery"
  | "fuel"
  | "tc"
  | "delta"
  | "energy"
  | "limiter"
  | "revs"
  | "map"
  | "gear"
  | "bestlap"
  | "position"
  | "rpm"
  | "bias"
  | "tccut"
  | "tcslip"
  | "track"
  | "air"
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
 *
 * `counts: false` marks a field that draws only while its state is active, so
 * it costs no room on the strip the rest of the time and is not charged against
 * the cap. The pit limiter is the only one: it is a warning, not a readout.
 */
export const DASHBOARD_FIELDS = [
  { id: "abs", labelKey: "dashboard.abs", default: false, counts: true },
  { id: "battery", labelKey: "dashboard.battery", default: false, counts: true },
  { id: "fuel", labelKey: "dashboard.fuel", default: true, counts: true },
  { id: "tc", labelKey: "dashboard.tc", default: false, counts: true },
  { id: "delta", labelKey: "dashboard.delta", default: false, counts: true },
  { id: "energy", labelKey: "dashboard.energy", default: false, counts: true },
  { id: "limiter", labelKey: "dashboard.limiter", default: true, counts: false },
  { id: "revs", labelKey: "dashboard.revs", default: true, counts: true },
  { id: "map", labelKey: "dashboard.map", default: false, counts: true },
  { id: "gear", labelKey: "dashboard.gear", default: true, counts: true },
  { id: "bestlap", labelKey: "dashboard.bestlap", default: false, counts: true },
  { id: "position", labelKey: "dashboard.position", default: true, counts: true },
  { id: "rpm", labelKey: "dashboard.rpm", default: false, counts: true },
  { id: "bias", labelKey: "dashboard.bias", default: false, counts: true },
  { id: "tccut", labelKey: "dashboard.tccut", default: false, counts: true },
  { id: "tcslip", labelKey: "dashboard.tcslip", default: false, counts: true },
  { id: "track", labelKey: "dashboard.track", default: true, counts: true },
  { id: "air", labelKey: "dashboard.air", default: true, counts: true },
  { id: "session", labelKey: "dashboard.session", default: false, counts: true },
  { id: "lastlap", labelKey: "dashboard.lastlap", default: true, counts: true },
  { id: "speed", labelKey: "dashboard.speed", default: true, counts: true },
  { id: "lap", labelKey: "dashboard.lap", default: false, counts: true },
  { id: "predicted", labelKey: "dashboard.predicted", default: false, counts: true },
  { id: "laps", labelKey: "dashboard.laps", default: false, counts: true }
] as const satisfies readonly {
  id: DashboardFieldId;
  labelKey: string;
  default: boolean;
  counts: boolean;
}[];

export const countVisibleDashboardFields = (settings: DashboardSettings): number =>
  DASHBOARD_FIELDS.filter(({ id, counts }) => counts && settings.visible[id]).length;

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
  for (const { id, counts } of DASHBOARD_FIELDS) {
    if (!counts || !fallback.visible[id]) continue;
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
