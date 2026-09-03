export type DashboardOptionId =
  | "map"
  | "tc"
  | "tcslip"
  | "tccut"
  | "abs"
  | "bias"
  | "migration"
  | "arb"
  | "battery"
  | "regen"
  | "motor"
  | "status";

export interface DashboardSettings {
  visible: Record<DashboardOptionId, boolean>;
}

export const DASHBOARD_SETTINGS_KEY = "blackrack-overlay.dashboard.v1";

/**
 * `visible` is the driver's own shortlist, so the default is the set that is
 * changed from the wheel during a stint rather than every value the car
 * publishes: the fine traction-control trims, the motor temperature and the
 * lamp row start hidden and are opt-in.
 */
export const DASHBOARD_OPTIONS = [
  { id: "map", labelKey: "dashboard.map", default: true },
  { id: "tc", labelKey: "dashboard.tc", default: true },
  { id: "tcslip", labelKey: "dashboard.tcSlip", default: false },
  { id: "tccut", labelKey: "dashboard.tcCut", default: false },
  { id: "abs", labelKey: "dashboard.abs", default: true },
  { id: "bias", labelKey: "dashboard.bias", default: true },
  { id: "migration", labelKey: "dashboard.migration", default: true },
  { id: "arb", labelKey: "dashboard.arb", default: true },
  { id: "battery", labelKey: "dashboard.battery", default: true },
  { id: "regen", labelKey: "dashboard.regen", default: true },
  { id: "motor", labelKey: "dashboard.motor", default: false },
  { id: "status", labelKey: "dashboard.status", default: false }
] as const satisfies readonly { id: DashboardOptionId; labelKey: string; default: boolean }[];

export const defaultDashboardSettings = (): DashboardSettings => ({
  visible: Object.fromEntries(
    DASHBOARD_OPTIONS.map(({ id, default: enabled }) => [id, enabled])
  ) as Record<DashboardOptionId, boolean>
});

export const normalizeDashboardSettings = (value: unknown): DashboardSettings | null => {
  if (!value || typeof value !== "object") return null;
  const visible = (value as Partial<DashboardSettings>).visible;
  if (!visible || typeof visible !== "object") return null;
  const fallback = defaultDashboardSettings();
  for (const { id } of DASHBOARD_OPTIONS) {
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
