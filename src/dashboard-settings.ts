export type DashboardOptionId =
  | "status"
  | "delta"
  | "session"
  | "tires"
  | "core"
  | "laptimes"
  | "fuel"
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
  | "motor";

export interface DashboardSettings {
  visible: Record<DashboardOptionId, boolean>;
}

export const DASHBOARD_SETTINGS_KEY = "blackrack-overlay.dashboard.v1";

/**
 * The panel is a full dash, so the first group turns whole blocks on and off
 * and the rest picks the individual readouts inside the electronics and hybrid
 * rows. The defaults are the shortlist a driver actually changes or watches
 * during a stint: the fine traction-control trims and the motor temperature
 * start hidden and are opt-in.
 */
export const DASHBOARD_OPTIONS = [
  { id: "core", labelKey: "dashboard.core", default: true },
  { id: "delta", labelKey: "dashboard.delta", default: true },
  { id: "laptimes", labelKey: "dashboard.laptimes", default: true },
  { id: "session", labelKey: "dashboard.session", default: true },
  { id: "tires", labelKey: "dashboard.tires", default: true },
  { id: "fuel", labelKey: "dashboard.fuel", default: true },
  { id: "status", labelKey: "dashboard.status", default: true },
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
  { id: "motor", labelKey: "dashboard.motor", default: false }
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
