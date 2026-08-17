import { isTauriRuntime, listenRuntimeEvent } from "./runtime-events";

export type OverlayId = "dashboard" | "delta" | "timing" | "driving" | "tires" | "damage" | "standings" | "relative" | "fuel" | "pitstop" | "flags" | "rejoin" | "trackmap";

export type OverlayTransparencySettings = Record<OverlayId, number>;

export interface OverlayTransparencyChange {
  overlay: OverlayId;
  transparency: number;
}

export type OverlaySettingMode = "global" | "individual";

export interface OverlayTransparencyScope {
  mode: OverlaySettingMode;
  globalTransparency: number;
}

export const OVERLAY_TRANSPARENCY_KEY = "lmu-overlay.background-transparency.v1";
export const OVERLAY_TRANSPARENCY_SCOPE_KEY = "lmu-overlay.background-transparency-scope.v1";

export const DEFAULT_OVERLAY_TRANSPARENCY: OverlayTransparencySettings = {
  dashboard: 5,
  delta: 5,
  timing: 5,
  driving: 5,
  tires: 5,
  damage: 5,
  standings: 3,
  relative: 3,
  fuel: 5,
  pitstop: 5,
  flags: 0,
  rejoin: 5,
  trackmap: 100
};

const percentage = (value: unknown, fallback: number): number => {
  const numeric = Number(value);
  return Number.isFinite(numeric) ? Math.max(0, Math.min(100, Math.round(numeric))) : fallback;
};

export const readOverlayTransparencyScope = (): OverlayTransparencyScope => {
  const fallback: OverlayTransparencyScope = { mode: "individual", globalTransparency: 5 };
  try {
    const stored = JSON.parse(localStorage.getItem(OVERLAY_TRANSPARENCY_SCOPE_KEY) ?? "null") as
      Partial<OverlayTransparencyScope> | null;
    if (!stored) return fallback;
    return {
      mode: stored.mode === "global" ? "global" : "individual",
      globalTransparency: percentage(stored.globalTransparency, fallback.globalTransparency)
    };
  } catch {
    localStorage.removeItem(OVERLAY_TRANSPARENCY_SCOPE_KEY);
    return fallback;
  }
};

export const effectiveOverlayTransparency = (
  settings: OverlayTransparencySettings,
  scope: OverlayTransparencyScope
): OverlayTransparencySettings => {
  if (scope.mode === "individual") return { ...settings };
  return Object.fromEntries(
    (Object.keys(settings) as OverlayId[]).map((overlay) => [overlay, scope.globalTransparency])
  ) as OverlayTransparencySettings;
};

export const readOverlayTransparency = (): OverlayTransparencySettings => {
  const settings = { ...DEFAULT_OVERLAY_TRANSPARENCY };
  try {
    const stored = JSON.parse(localStorage.getItem(OVERLAY_TRANSPARENCY_KEY) ?? "{}") as
      Partial<Record<OverlayId, unknown>>;
    for (const overlay of Object.keys(settings) as OverlayId[]) {
      settings[overlay] = percentage(stored[overlay], settings[overlay]);
    }
  } catch {
    localStorage.removeItem(OVERLAY_TRANSPARENCY_KEY);
  }
  return settings;
};

export const applyOverlayTransparency = (transparency: number): void => {
  const opacity = 1 - percentage(transparency, 0) / 100;
  document.documentElement.style.setProperty("--overlay-background-opacity", opacity.toFixed(2));
};

export const bindOverlayTransparency = (overlay: OverlayId): void => {
  const settings = readOverlayTransparency();
  const scope = readOverlayTransparencyScope();
  applyOverlayTransparency(effectiveOverlayTransparency(settings, scope)[overlay]);
  if (!isTauriRuntime()) return;
  void listenRuntimeEvent<OverlayTransparencyChange>("overlay://background-transparency", (payload) => {
    if (payload.overlay === overlay) applyOverlayTransparency(payload.transparency);
  });
};
