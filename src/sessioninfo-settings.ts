export type SessionInfoFieldId =
  | "sessionType"
  | "clock"
  | "trackName"
  | "timeRemaining"
  | "lapsRemaining"
  | "lapProgress"
  | "trackTemperature"
  | "airTemperature"
  | "weather"
  | "trackLimits";

export interface SessionInfoSettings {
  layout: "line" | "column";
  useSystemClock: boolean;
  visible: Record<SessionInfoFieldId, boolean>;
}

export const SESSIONINFO_SETTINGS_KEY = "blackrack-overlay.sessioninfo.v1";

export const SESSIONINFO_FIELDS = [
  { id: "clock", labelKey: "sessioninfo.clock" },
  { id: "trackTemperature", labelKey: "sessioninfo.trackTemperature" },
  { id: "sessionType", labelKey: "sessioninfo.sessionType" },
  { id: "weather", labelKey: "sessioninfo.weather" },
  { id: "timeRemaining", labelKey: "sessioninfo.timeRemaining" },
  { id: "trackLimits", labelKey: "sessioninfo.trackLimits" },
  { id: "trackName", labelKey: "sessioninfo.trackName" },
  { id: "lapsRemaining", labelKey: "sessioninfo.lapsRemaining" },
  { id: "lapProgress", labelKey: "sessioninfo.lapProgress" },
  { id: "airTemperature", labelKey: "sessioninfo.airTemperature" }
] as const satisfies readonly { id: SessionInfoFieldId; labelKey: TranslationKey }[];

export const defaultSessionInfoSettings = (): SessionInfoSettings => ({
  layout: "line", useSystemClock: false,
  visible: {
    sessionType: true, clock: true, trackName: true,
    timeRemaining: true, lapsRemaining: true, lapProgress: false,
    trackTemperature: true, airTemperature: false, weather: true, trackLimits: true
  }
});

export const normalizeSessionInfoSettings = (value: unknown): SessionInfoSettings | null => {
  if (!value || typeof value !== "object") return null;
  const candidate = value as Partial<SessionInfoSettings>;
  if (candidate.layout !== undefined && candidate.layout !== "line" && candidate.layout !== "column") return null;
  if (!candidate.visible || typeof candidate.visible !== "object") return null;
  const fallback = defaultSessionInfoSettings();
  for (const { id } of SESSIONINFO_FIELDS) {
    const field = candidate.visible[id];
    if (field !== undefined && typeof field !== "boolean") return null;
    fallback.visible[id] = field ?? fallback.visible[id];
  }
  if (candidate.useSystemClock !== undefined && typeof candidate.useSystemClock !== "boolean") return null;
  return { layout: candidate.layout ?? fallback.layout, useSystemClock: candidate.useSystemClock ?? false, visible: fallback.visible };
};

export const readSessionInfoSettings = (): SessionInfoSettings => {
  try {
    return normalizeSessionInfoSettings(JSON.parse(localStorage.getItem(SESSIONINFO_SETTINGS_KEY) ?? "null"))
      ?? defaultSessionInfoSettings();
  } catch {
    return defaultSessionInfoSettings();
  }
};
import type { TranslationKey } from "./i18n";
