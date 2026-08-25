import "./conditions.css";
import type { TelemetryFrame } from "./telemetry-types";
import { fitOverlay } from "./overlay-fit";
import { bindOverlayInteractionMode } from "./overlay-interaction";
import { bindOverlayTransparency } from "./overlay-appearance";
import { createOverlayPerformanceTracker } from "./overlay-performance";
import { isTauriRuntime, listenTelemetry } from "./runtime-events";
import { t } from "./i18n";
import { weatherIconUrl } from "./weather-icons";

type ConditionValue = "air" | "track" | "wind" | "humidity" | "rain" | "grip" | "wetness";

const setOverlaySize = fitOverlay({ width: 500, height: 96 });
bindOverlayTransparency("conditions");
const renderPerformance = createOverlayPerformanceTracker("conditions");

const values = Object.fromEntries(
  Array.from(document.querySelectorAll<HTMLElement>("[data-conditions-value]")).map((element) => [
    element.dataset.conditionsValue as ConditionValue,
    element
  ])
) as Record<ConditionValue, HTMLElement>;
const icon = document.getElementById("conditions-icon") as HTMLImageElement;
const state = document.getElementById("conditions-state")!;

const setText = (element: HTMLElement, text: string): void => {
  if (element.textContent !== text) element.textContent = text;
};

const formatTemp = (celsius: number): string =>
  Number.isFinite(celsius) ? `${Math.round(celsius)}°` : "--°";

const formatPercent = (value: number): string =>
  Number.isFinite(value) ? `${Math.round(value)}%` : "--%";

const WIND_ARROWS = ["↑", "↗", "→", "↘", "↓", "↙", "←", "↖"] as const;

const windValue = (speedMs: number, bearing: number): string => {
  if (!Number.isFinite(speedMs) || speedMs <= 0) return "--";
  const arrow = WIND_ARROWS[Math.round(((bearing + 360) % 360) / 45) % 8];
  const kmh = Math.round(speedMs * 3.6);
  return `${arrow} ${kmh} km/h`;
};

const render = (frame: TelemetryFrame): void => {
  const available = frame.rest_weather_available || frame.weather_forecast.available;
  document.body.dataset.available = available ? "true" : "false";

  const currentSky = frame.rest_weather_available
    ? frame.cloud_coverage
    : frame.weather_forecast.available && frame.weather_forecast.nodes.length > 0
      ? frame.weather_forecast.nodes[
        Math.max(0, Math.min(frame.weather_forecast.current_index, frame.weather_forecast.nodes.length - 1))
      ].sky
      : frame.cloud_coverage;
  icon.src = weatherIconUrl(currentSky);
  icon.hidden = !available;

  state.textContent = t(`conditions.${frame.track_grip_state}`);
  state.dataset.state = frame.track_grip_state;

  setText(values.air, formatTemp(frame.rest_weather_available ? frame.ambient_temperature_c : NaN));
  setText(values.track, formatTemp(frame.rest_weather_available ? frame.track_temperature_c : NaN));
  setText(values.wind, windValue(frame.wind_speed_ms, frame.wind_direction_degrees));
  setText(values.humidity, formatPercent(frame.current_humidity_percent > 0 ? frame.current_humidity_percent : NaN));
  setText(values.rain, formatPercent(frame.rest_weather_available ? frame.rain_percent : NaN));
  setText(values.grip, formatPercent(frame.player_grip_percent > 0 ? frame.player_grip_percent : NaN));
  setText(values.wetness, formatPercent(frame.rest_weather_available ? frame.track_wetness_percent : NaN));
};

const previewFrame = {
  rest_weather_available: true,
  ambient_temperature_c: 19.4,
  track_temperature_c: 27.8,
  rain_percent: 8,
  track_wetness_percent: 12,
  wind_speed_ms: 3.5,
  wind_direction_degrees: 290,
  player_grip_percent: 87,
  track_grip_state: "dry",
  cloud_coverage: 2,
  current_humidity_percent: 62,
  weather_forecast: {
    available: true,
    session: "RACE",
    current_index: 0,
    nodes: [
      { sky: 2, sky_label: "Partially Cloudy", temperature_c: 19.4, rain_chance_percent: 10, humidity_percent: 62 }
    ]
  }
} as TelemetryFrame;

bindOverlayInteractionMode();
if (!isTauriRuntime()) render(previewFrame);
void listenTelemetry((frame) => renderPerformance.measure(() => render(frame)));
