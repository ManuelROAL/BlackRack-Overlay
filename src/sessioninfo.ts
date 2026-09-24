import "./sessioninfo.css";
import type { TelemetryFrame } from "./telemetry-types";
import { fitOverlayToContentBox } from "./overlay-fit";
import { bindOverlayInteractionMode } from "./overlay-interaction";
import { bindOverlayTransparency } from "./overlay-appearance";
import { createOverlayPerformanceTracker } from "./overlay-performance";
import { isTauriRuntime, listenRuntimeEvent, listenTelemetry } from "./runtime-events";
import { t } from "./i18n";
import { weatherIconUrl } from "./weather-icons";
import { formatTemperature } from "./display-units";
import { applyTrackLimitTone, formatTrackLimitPoints } from "./track-limit-tone";
import type { DisplayUnits } from "./display-units";
import { readSessionInfoSettings, SESSIONINFO_FIELDS, type SessionInfoFieldId, type SessionInfoSettings } from "./sessioninfo-settings";

const shell = document.querySelector<HTMLElement>(".sessioninfo-shell")!;
const items = document.getElementById("sessioninfo-items")!;
const weatherIcon = document.getElementById("weather-icon") as HTMLImageElement;
const fields = Object.fromEntries(SESSIONINFO_FIELDS.map(({ id }) => [
  id, document.querySelector<HTMLElement>(`[data-field="${id}"]`)!
])) as Record<SessionInfoFieldId, HTMLElement>;
const values = {
  sessionType: document.getElementById("session-type")!, clock: document.getElementById("clock-value")!,
  weather: document.getElementById("weather-value")!, trackLimits: document.getElementById("track-limits")!,
  trackName: document.getElementById("track-name")!,
  timeRemaining: document.getElementById("time-remaining")!, lapsRemaining: document.getElementById("laps-remaining")!,
  lapProgress: document.getElementById("lap-progress")!, trackTemperature: document.getElementById("track-temperature")!,
  airTemperature: document.getElementById("air-temperature")!
};
bindOverlayTransparency("sessioninfo");
bindOverlayInteractionMode();
const synchronizeFit = fitOverlayToContentBox(items);
const measurePerformance = createOverlayPerformanceTracker("sessioninfo");
let settings = readSessionInfoSettings();
let latestFrame: TelemetryFrame | null = null;
let systemClockTimer: number | null = null;

const setText = (element: HTMLElement, value: string): void => {
  if (element.textContent !== value) element.textContent = value;
};
const updateSettings = (next: SessionInfoSettings): void => {
  settings = next;
  shell.dataset.layout = settings.layout;
  for (const { id } of SESSIONINFO_FIELDS) fields[id].hidden = !settings.visible[id];
  synchronizeFit();
  synchronizeSystemClock();
};

const sessionLabel = (type: number): string => {
  if (type >= 10 && type <= 13) return type === 10 ? t("session.race") : t("session.raceNumber", { number: type - 9 });
  if (type >= 5 && type <= 8) return type === 5 ? t("session.qualifying") : t("session.qualifyingNumber", { number: type - 4 });
  if (type === 9) return t("session.warmup");
  return type === 0 ? t("session.practice") : t("session.practiceNumber", { number: type });
};
const formatTime = (seconds: number): string => {
  if (!Number.isFinite(seconds) || seconds < 0) return "--";
  const total = Math.ceil(seconds);
  const hours = Math.floor(total / 3600);
  const minutes = Math.floor(total % 3600 / 60);
  const remainder = total % 60;
  return hours > 0 ? `${hours}:${String(minutes).padStart(2, "0")}:${String(remainder).padStart(2, "0")}` : `${minutes}:${String(remainder).padStart(2, "0")}`;
};
const synchronizeSystemClock = (): void => {
  if (systemClockTimer !== null) {
    window.clearInterval(systemClockTimer);
    systemClockTimer = null;
  }
  if (!settings.useSystemClock || !settings.visible.clock) return;
  const updateClock = (): void => {
    const now = new Date();
    setText(values.clock, `${String(now.getHours()).padStart(2, "0")}:${String(now.getMinutes()).padStart(2, "0")}`);
  };
  updateClock();
  systemClockTimer = window.setInterval(updateClock, 1000);
};
const render = (frame: TelemetryFrame): void => {
  setText(values.sessionType, frame.connected ? sessionLabel(frame.session_type) : "--");
  const now = new Date();
  const clockSeconds = settings.useSystemClock
    ? now.getHours() * 3600 + now.getMinutes() * 60 + now.getSeconds()
    : frame.game_time_of_day_seconds;
  setText(values.clock, (settings.useSystemClock || frame.connected) && Number.isFinite(clockSeconds) && clockSeconds >= 0
    ? `${String(Math.floor(clockSeconds / 3600) % 24).padStart(2, "0")}:${String(Math.floor(clockSeconds / 60) % 60).padStart(2, "0")}`
    : "--:--");
  setText(values.trackName, frame.connected ? frame.track_name?.trim() || "--" : "--");
  setText(values.timeRemaining, frame.connected
    && (frame.session_time_remaining > 0 || frame.game_phase >= 8)
    ? formatTime(frame.session_time_remaining)
    : "--");
  const laps = frame.session_laps_remaining_estimated >= 0
    ? frame.session_laps_remaining_estimated : frame.session_laps_remaining;
  setText(values.lapsRemaining, frame.connected && Number.isFinite(laps)
    && (laps > 0 || frame.game_phase >= 8)
    ? `~${Math.ceil(laps)} ${t("sessioninfo.lapUnit")}`
    : "--");
  setText(values.lapProgress, frame.connected && frame.lap_number > 0
    ? `${frame.lap_number} / ${frame.session_total_laps_estimated > 0 ? frame.session_total_laps_estimated : "--"}`
    : "--");
  setText(values.trackTemperature, formatTemperature(frame.connected && frame.rest_weather_available ? frame.track_temperature_c : NaN));
  setText(values.airTemperature, formatTemperature(frame.connected && frame.rest_weather_available ? frame.ambient_temperature_c : NaN));
  const sky = Math.max(0, Math.min(10, Math.round(frame.cloud_coverage)));
  weatherIcon.src = weatherIconUrl(sky);
  weatherIcon.hidden = !frame.connected || !frame.rest_weather_available;
  const weatherKeys = ["conditions.clear", "conditions.lightClouds", "conditions.partlyCloudy", "conditions.mostlyCloudy", "conditions.overcast", "conditions.drizzle", "conditions.lightRain", "conditions.overcastLightRain", "conditions.rainWeather", "conditions.heavyRain", "conditions.storm"] as const;
  setText(values.weather, frame.connected && frame.rest_weather_available
    ? `${t(weatherKeys[sky])} · ${Number.isFinite(frame.rain_percent) ? Math.round(frame.rain_percent) : 0}%`
    : "--");
  const steps = frame.track_limits_steps;
  const threshold = frame.track_limits_steps_per_penalty;
  values.trackLimits.removeAttribute("data-track-limit-tone");
  const limitsAvailable = frame.connected && Number.isFinite(steps) && steps >= 0
    && Number.isFinite(threshold) && threshold > 0;
  setText(values.trackLimits, limitsAvailable
    ? `× ${formatTrackLimitPoints(steps)} / ${formatTrackLimitPoints(threshold)}`
    : "--");
  if (limitsAvailable) applyTrackLimitTone(values.trackLimits, steps, threshold);
};

updateSettings(settings);
if (!isTauriRuntime()) {
  render({ connected: true, session_type: 10, game_phase: 4, game_time_of_day_seconds: 57_600,
    track_name: "Circuit de la Sarthe", session_time_remaining: 2478,
    session_laps_remaining: 12, session_laps_remaining_estimated: 12, lap_number: 18,
    session_total_laps_estimated: 24, rest_weather_available: true, track_temperature_c: 31,
    ambient_temperature_c: 22, cloud_coverage: 6, rain_percent: 22, track_limits_steps: 16,
    track_limits_steps_per_penalty: 68 } as TelemetryFrame);
}
void listenTelemetry((frame) => {
  latestFrame = frame;
  measurePerformance.measure(() => render(frame));
});
if (isTauriRuntime()) void listenRuntimeEvent<SessionInfoSettings>("sessioninfo://settings", (next) => {
  updateSettings(next);
  if (latestFrame) render(latestFrame);
});
void listenRuntimeEvent<DisplayUnits>("display-units://change", () => {
  if (latestFrame) render(latestFrame);
});
