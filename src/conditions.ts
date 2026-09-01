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

fitOverlay(
  { width: 390, height: 90 },
  { widthTextRatio: 0.72, heightTextRatio: 0.12 }
);
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
const surfacePercent = document.getElementById("conditions-surface-percent")!;
const weatherLabel = document.getElementById("conditions-weather-label")!;
const rainSummary = document.getElementById("conditions-rain-summary")!;
const windArrow = document.getElementById("conditions-wind-arrow")!;

const setText = (element: HTMLElement, text: string): void => {
  if (element.textContent !== text) element.textContent = text;
};

const formatTemp = (celsius: number): string =>
  Number.isFinite(celsius) ? `${Math.round(celsius)}°` : "--°";

const formatPercent = (value: number): string =>
  Number.isFinite(value) ? `${Math.round(value)}%` : "--%";

const WIND_DIRECTIONS = ["N", "NE", "E", "SE", "S", "SW", "W", "NW"] as const;
const WEATHER_LABEL_KEYS = [
  "conditions.clear", "conditions.lightClouds", "conditions.partlyCloudy",
  "conditions.mostlyCloudy", "conditions.overcast", "conditions.drizzle",
  "conditions.lightRain", "conditions.overcastLightRain", "conditions.rainWeather",
  "conditions.heavyRain", "conditions.storm"
] as const;

const windValue = (speedMs: number, bearing: number): string => {
  if (!Number.isFinite(speedMs) || speedMs <= 0) return "--";
  const directionIndex = Math.round(((bearing + 360) % 360) / 45) % 8;
  const direction = WIND_DIRECTIONS[directionIndex];
  const kmh = Math.round(speedMs * 3.6);
  return `${kmh} km/h ${direction}`;
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
  setText(weatherLabel, t(WEATHER_LABEL_KEYS[Math.max(0, Math.min(Math.round(currentSky), 10))]));

  setText(state, t(`conditions.${frame.track_grip_state}`));
  state.dataset.state = frame.track_grip_state;
  setText(surfacePercent, formatPercent(
    frame.track_grip_state === "dry" ? frame.track_rubber_percent : frame.track_wetness_percent
  ));

  setText(values.air, formatTemp(frame.rest_weather_available ? frame.ambient_temperature_c : NaN));
  setText(values.track, formatTemp(frame.rest_weather_available ? frame.track_temperature_c : NaN));
  setText(values.wind, windValue(frame.wind_speed_ms, frame.wind_direction_degrees));
  const windAvailable = Number.isFinite(frame.wind_speed_ms) && frame.wind_speed_ms > 0;
  windArrow.hidden = !windAvailable;
  if (windAvailable && Number.isFinite(frame.wind_relative_direction_degrees)) {
    const rotation = `rotate(${frame.wind_relative_direction_degrees.toFixed(1)}deg)`;
    if (windArrow.style.transform !== rotation) windArrow.style.transform = rotation;
  }
  setText(values.humidity, formatPercent(frame.current_humidity_percent > 0 ? frame.current_humidity_percent : NaN));
  setText(values.rain, formatPercent(frame.rest_weather_available ? frame.rain_percent : NaN));
  setText(values.grip, formatPercent(frame.player_grip_percent > 0 ? frame.player_grip_percent : NaN));
  setText(values.wetness, formatPercent(frame.rest_weather_available ? frame.track_wetness_percent : NaN));
  rainSummary.hidden = !frame.rest_weather_available || frame.rain_percent <= 0;
};

const previewFrame = {
  rest_weather_available: true,
  ambient_temperature_c: 19.4,
  track_temperature_c: 27.8,
  rain_percent: 0,
  track_wetness_percent: 0,
  wind_speed_ms: 3.5,
  wind_direction_degrees: 290,
  wind_relative_direction_degrees: 35,
  player_grip_percent: 75,
  track_rubber_percent: 26,
  track_grip_state: "dry",
  cloud_coverage: 3,
  current_humidity_percent: 80,
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
