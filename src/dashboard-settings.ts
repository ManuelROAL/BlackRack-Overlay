export type DashboardFieldId =
  | "abs"
  | "battery"
  | "fuel"
  | "tc"
  | "delta"
  | "energy"
  | "liftcoast"
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
  pitWarningTarget: "gear" | "overlay";
}

export const DASHBOARD_SETTINGS_KEY = "blackrack-overlay.dashboard.v1";

/**
 * Listed in the order the control panel shows them, which is alphabetical by
 * the Spanish label. The default eight are the ones that read the same in every
 * car and every session; anything car-specific is opt-in.
 */
export const DASHBOARD_FIELDS = [
  { id: "abs", labelKey: "dashboard.abs", default: false },
  { id: "battery", labelKey: "dashboard.battery", default: false },
  { id: "fuel", labelKey: "dashboard.fuel", default: true },
  { id: "tc", labelKey: "dashboard.tc", default: false },
  { id: "delta", labelKey: "dashboard.delta", default: false },
  { id: "energy", labelKey: "dashboard.energy", default: false },
  { id: "liftcoast", labelKey: "dashboard.liftcoast", default: true },
  { id: "limiter", labelKey: "dashboard.limiter", default: true },
  { id: "revs", labelKey: "dashboard.revs", default: true },
  { id: "map", labelKey: "dashboard.map", default: false },
  { id: "gear", labelKey: "dashboard.gear", default: true },
  { id: "bestlap", labelKey: "dashboard.bestlap", default: false },
  { id: "position", labelKey: "dashboard.position", default: true },
  { id: "rpm", labelKey: "dashboard.rpm", default: false },
  { id: "bias", labelKey: "dashboard.bias", default: false },
  { id: "tccut", labelKey: "dashboard.tccut", default: false },
  { id: "tcslip", labelKey: "dashboard.tcslip", default: false },
  { id: "track", labelKey: "dashboard.track", default: true },
  { id: "air", labelKey: "dashboard.air", default: true },
  { id: "session", labelKey: "dashboard.session", default: false },
  { id: "lastlap", labelKey: "dashboard.lastlap", default: true },
  { id: "speed", labelKey: "dashboard.speed", default: true },
  { id: "lap", labelKey: "dashboard.lap", default: false },
  { id: "predicted", labelKey: "dashboard.predicted", default: false },
  { id: "laps", labelKey: "dashboard.laps", default: false }
] as const satisfies readonly {
  id: DashboardFieldId;
  labelKey: string;
  default: boolean;
}[];

export const defaultDashboardSettings = (): DashboardSettings => ({
  pitWarningTarget: "gear",
  visible: Object.fromEntries(
    DASHBOARD_FIELDS.map(({ id, default: enabled }) => [id, enabled])
  ) as Record<DashboardFieldId, boolean>
});

export const normalizeDashboardSettings = (value: unknown): DashboardSettings | null => {
  if (!value || typeof value !== "object") return null;
  const visible = (value as Partial<DashboardSettings>).visible;
  if (!visible || typeof visible !== "object") return null;
  const fallback = defaultDashboardSettings();
  const target = (value as Partial<DashboardSettings>).pitWarningTarget;
  if (target !== undefined && target !== "gear" && target !== "overlay") return null;
  fallback.pitWarningTarget = target ?? fallback.pitWarningTarget;
  for (const { id } of DASHBOARD_FIELDS) {
    if (visible[id] !== undefined && typeof visible[id] !== "boolean") return null;
    fallback.visible[id] = visible[id] ?? fallback.visible[id];
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
