import { isTauriRuntime, listenRuntimeEvent } from "./runtime-events";

export type OverlayId = "delta" | "timing" | "stinthistory" | "driving" | "liftcoast" | "tires" | "damage" | "standings" | "relative" | "fuel" | "pitstop" | "flags" | "rejoin" | "trackmap" | "forecast" | "conditions";

export type OverlayTransparencySettings = Record<OverlayId, number>;
export type OverlayFontSizeSettings = Record<OverlayId, number>;

export interface OverlayTransparencyChange {
  overlay: OverlayId;
  transparency: number;
}

export interface OverlayFontSizeChange {
  overlay: OverlayId;
  fontSize: number;
}

export type OverlaySettingMode = "global" | "individual";

export interface OverlayTransparencyScope {
  mode: OverlaySettingMode;
  globalTransparency: number;
}

export interface OverlayFontSizeScope {
  mode: OverlaySettingMode;
  globalFontSize: number;
}

export const OVERLAY_TRANSPARENCY_KEY = "blackrack-overlay.background-transparency.v1";
export const OVERLAY_TRANSPARENCY_SCOPE_KEY = "blackrack-overlay.background-transparency-scope.v1";
export const OVERLAY_FONT_SIZE_KEY = "blackrack-overlay.font-size.v1";
export const OVERLAY_FONT_SIZE_SCOPE_KEY = "blackrack-overlay.font-size-scope.v1";
export const OVERLAY_FONT_SIZE_MIN = 75;
export const OVERLAY_FONT_SIZE_MAX = 200;

export const DEFAULT_OVERLAY_TRANSPARENCY: OverlayTransparencySettings = {
  delta: 5,
  timing: 5,
  stinthistory: 5,
  driving: 5,
  liftcoast: 5,
  tires: 5,
  damage: 5,
  standings: 3,
  relative: 3,
  fuel: 5,
  pitstop: 5,
  flags: 0,
  rejoin: 5,
  trackmap: 100,
  forecast: 5,
  conditions: 5
};

export const DEFAULT_OVERLAY_FONT_SIZE: OverlayFontSizeSettings = Object.fromEntries(
  (Object.keys(DEFAULT_OVERLAY_TRANSPARENCY) as OverlayId[]).map((overlay) => [overlay, 100])
) as OverlayFontSizeSettings;

const percentage = (value: unknown, fallback: number): number => {
  const numeric = Number(value);
  return Number.isFinite(numeric) ? Math.max(0, Math.min(100, Math.round(numeric))) : fallback;
};

const fontSizePercentage = (value: unknown, fallback: number): number => {
  const numeric = Number(value);
  return Number.isFinite(numeric)
    ? Math.max(OVERLAY_FONT_SIZE_MIN, Math.min(OVERLAY_FONT_SIZE_MAX, Math.round(numeric)))
    : fallback;
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

export const readOverlayFontSizeScope = (): OverlayFontSizeScope => {
  const fallback: OverlayFontSizeScope = { mode: "individual", globalFontSize: 100 };
  try {
    const stored = JSON.parse(localStorage.getItem(OVERLAY_FONT_SIZE_SCOPE_KEY) ?? "null") as
      Partial<OverlayFontSizeScope> | null;
    if (!stored) return fallback;
    return {
      mode: stored.mode === "global" ? "global" : "individual",
      globalFontSize: fontSizePercentage(stored.globalFontSize, fallback.globalFontSize)
    };
  } catch {
    localStorage.removeItem(OVERLAY_FONT_SIZE_SCOPE_KEY);
    return fallback;
  }
};

export const effectiveOverlayFontSize = (
  settings: OverlayFontSizeSettings,
  scope: OverlayFontSizeScope
): OverlayFontSizeSettings => {
  if (scope.mode === "individual") return { ...settings };
  return Object.fromEntries(
    (Object.keys(settings) as OverlayId[]).map((overlay) => [overlay, scope.globalFontSize])
  ) as OverlayFontSizeSettings;
};

export const readOverlayFontSize = (): OverlayFontSizeSettings => {
  const settings = { ...DEFAULT_OVERLAY_FONT_SIZE };
  try {
    const stored = JSON.parse(localStorage.getItem(OVERLAY_FONT_SIZE_KEY) ?? "{}") as
      Partial<Record<OverlayId, unknown>>;
    for (const overlay of Object.keys(settings) as OverlayId[]) {
      settings[overlay] = fontSizePercentage(stored[overlay], settings[overlay]);
    }
  } catch {
    localStorage.removeItem(OVERLAY_FONT_SIZE_KEY);
  }
  return settings;
};

export const applyOverlayTransparency = (transparency: number): void => {
  const opacity = 1 - percentage(transparency, 0) / 100;
  document.documentElement.style.setProperty("--overlay-background-opacity", opacity.toFixed(2));
};

export const applyOverlayFontSize = (fontSize: number): void => {
  const scale = fontSizePercentage(fontSize, 100) / 100;
  document.documentElement.style.setProperty(
    "--overlay-font-scale",
    scale.toFixed(2)
  );
  window.dispatchEvent(new CustomEvent("overlay-font-size-change", { detail: { scale } }));
};

export const bindOverlayTransparency = (overlay: OverlayId): void => {
  const settings = readOverlayTransparency();
  const scope = readOverlayTransparencyScope();
  applyOverlayTransparency(effectiveOverlayTransparency(settings, scope)[overlay]);
  const fontSizes = readOverlayFontSize();
  const fontSizeScope = readOverlayFontSizeScope();
  applyOverlayFontSize(effectiveOverlayFontSize(fontSizes, fontSizeScope)[overlay]);
  if (!isTauriRuntime()) return;
  void listenRuntimeEvent<OverlayTransparencyChange>("overlay://background-transparency", (payload) => {
    if (payload.overlay === overlay) applyOverlayTransparency(payload.transparency);
  });
  void listenRuntimeEvent<OverlayFontSizeChange>("overlay://font-size", (payload) => {
    if (payload.overlay === overlay) applyOverlayFontSize(payload.fontSize);
  });
};
