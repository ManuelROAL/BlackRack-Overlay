import "./forecast.css";
import type { TelemetryFrame } from "./telemetry-types";
import { fitOverlay } from "./overlay-fit";
import { bindOverlayInteractionMode } from "./overlay-interaction";
import { bindOverlayTransparency } from "./overlay-appearance";
import { createOverlayPerformanceTracker } from "./overlay-performance";
import { isTauriRuntime, listenRuntimeEvent, listenTelemetry } from "./runtime-events";
import { t } from "./i18n";
import { weatherIconUrl } from "./weather-icons";
import { applyDisplayUnits, formatTemperature, type DisplayUnits } from "./display-units";

const MAX_NODES = 5;
const COLUMN_WIDTH = 64;
const GAP = 4;
const PADDING = 8;
const BODY_CHROME = 14;
const MIN_WIDTH = 120;
const HEIGHT = 112;
const BASE_WIDTH = BODY_CHROME + PADDING * 2
  + MAX_NODES * COLUMN_WIDTH + (MAX_NODES - 1) * GAP;

const setOverlaySize = fitOverlay(
  { width: BASE_WIDTH, height: HEIGHT },
  { heightTextRatio: 0.5 }
);
bindOverlayTransparency("forecast");
const renderPerformance = createOverlayPerformanceTracker("forecast");

const setText = (element: HTMLElement, text: string): void => {
  if (element.textContent !== text) element.textContent = text;
};

const formatPercent = (value: number): string =>
  Number.isFinite(value) ? `${Math.round(value)}%` : "--%";

const strip = document.getElementById("forecast-strip")!;
let latestFrame: TelemetryFrame | null = null;

interface ForecastCell {
  root: HTMLElement;
  label: HTMLElement;
  icon: HTMLImageElement;
  temp: HTMLElement;
  rain: HTMLElement;
}

const cells: ForecastCell[] = Array.from({ length: MAX_NODES }, () => {
  const root = document.createElement("div");
  root.className = "forecast-node";
  const label = document.createElement("span");
  label.className = "forecast-node-label";
  const icon = document.createElement("img");
  icon.className = "forecast-node-icon";
  icon.alt = "";
  const temp = document.createElement("strong");
  temp.className = "forecast-node-temp";
  const rain = document.createElement("span");
  rain.className = "forecast-node-rain";
  root.append(label, icon, temp, rain);
  strip.append(root);
  return { root, label, icon, temp, rain };
});

let lastWidth = BASE_WIDTH;

const render = (frame: TelemetryFrame): void => {
  const model = frame.weather_forecast;
  const available = frame.rest_weather_available || model.available;
  document.body.dataset.available = available ? "true" : "false";

  const currentIndex = model.available && model.nodes.length > 0
    ? Math.max(0, Math.min(model.current_index, model.nodes.length - 1))
    : 0;
  const nextIndex = model.available
    ? Math.max(currentIndex + 1, Math.min(model.next_index, model.nodes.length))
    : 0;
  const currentNode = model.available ? model.nodes[currentIndex] : undefined;
  const futureNodes = model.available
    ? model.nodes.slice(nextIndex)
    : [];

  const nowCell = cells[0];
  nowCell.root.hidden = !available;
  if (available) {
    nowCell.label.textContent = t("forecast.now");
    nowCell.icon.src = weatherIconUrl(
      frame.rest_weather_available ? frame.cloud_coverage : currentNode?.sky ?? 0
    );
    nowCell.icon.hidden = false;
    setText(nowCell.temp, formatTemperature(
      frame.rest_weather_available ? frame.ambient_temperature_c : currentNode?.temperature_c ?? NaN
    ));
    setText(nowCell.rain, formatPercent(
      frame.rest_weather_available ? frame.rain_percent : currentNode?.rain_chance_percent ?? NaN
    ));
    nowCell.root.dataset.rain = (frame.rest_weather_available
      ? frame.rain_percent
      : currentNode?.rain_chance_percent ?? 0) >= 50 ? "high" : "low";
  }

  for (let index = 1; index < MAX_NODES; index += 1) {
    const cell = cells[index];
    const node = futureNodes[index - 1];
    cell.root.hidden = !node;
    if (!node) continue;
    cell.label.textContent = node.minutes_from_now === null
      ? "--"
      : `+${node.minutes_from_now}M`;
    cell.icon.src = weatherIconUrl(node.sky);
    cell.icon.hidden = false;
    setText(cell.temp, formatTemperature(node.temperature_c));
    setText(cell.rain, formatPercent(node.rain_chance_percent));
    cell.root.dataset.rain = node.rain_chance_percent >= 50 ? "high" : "low";
  }

  // Keep the forecast surface stable while the first REST snapshot is pending.
  const count = available ? 1 + futureNodes.length : 0;
  const width = available
    ? Math.max(MIN_WIDTH, BODY_CHROME + PADDING * 2
      + count * COLUMN_WIDTH + (count - 1) * GAP)
    : BASE_WIDTH;
  if (width !== lastWidth) {
    lastWidth = width;
    setOverlaySize({ width, height: HEIGHT });
  }
};

const previewFrame = {
  rest_weather_available: true,
  ambient_temperature_c: 19.4,
  cloud_coverage: 1,
  rain_percent: 10,
  weather_forecast: {
    available: true,
    session: "RACE",
    current_index: 0,
    next_index: 1,
    nodes: [
      { sky: 1, sky_label: "Light Cloud", temperature_c: 19.4, rain_chance_percent: 10, humidity_percent: 62, minutes_from_now: null },
      { sky: 2, sky_label: "Partially Cloudy", temperature_c: 19.1, rain_chance_percent: 25, humidity_percent: 66, minutes_from_now: 24 },
      { sky: 3, sky_label: "Mostly Cloudy", temperature_c: 18.6, rain_chance_percent: 45, humidity_percent: 72, minutes_from_now: 46 },
      { sky: 4, sky_label: "Overcast", temperature_c: 18.2, rain_chance_percent: 70, humidity_percent: 80, minutes_from_now: 62 },
      { sky: 9, sky_label: "Overcast and Heavy Rain", temperature_c: 17.9, rain_chance_percent: 85, humidity_percent: 88, minutes_from_now: 78 }
    ]
  }
} as TelemetryFrame;

bindOverlayInteractionMode();
if (!isTauriRuntime()) render(previewFrame);
void listenTelemetry((frame) => {
  latestFrame = frame;
  renderPerformance.measure(() => render(frame));
});
void listenRuntimeEvent<DisplayUnits>("display-units://change", (next) => {
  applyDisplayUnits(next);
  if (latestFrame) render(latestFrame);
});
