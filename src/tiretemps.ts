import "./tiretemps.css";
import type { TelemetryFrame } from "./telemetry-types";
import { fitOverlay } from "./overlay-fit";
import { bindOverlayInteractionMode } from "./overlay-interaction";
import { bindOverlayTransparency } from "./overlay-appearance";
import { createOverlayPerformanceTracker } from "./overlay-performance";
import { isTauriRuntime, listenTelemetry } from "./runtime-events";
import { formatNumber, t, type TranslationKey } from "./i18n";

type Zone = "inside" | "center" | "outside";
const zoneIndexes: Record<Zone, number> = { inside: 0, center: 1, outside: 2 };
const zoneTitleKeys: Record<Zone, TranslationKey> = {
  inside: "tiretemps.inside",
  center: "tiretemps.center",
  outside: "tiretemps.outside"
};
const wheels = Array.from(document.querySelectorAll<HTMLElement>("[data-wheel]"));
const brakes = Array.from(document.querySelectorAll<HTMLElement>("[data-brake]"));
const renderPerformance = createOverlayPerformanceTracker("tiretemps");

const temperatureColor = (temperature: number): string => {
  if (!Number.isFinite(temperature) || temperature < 0) return "#687481";
  if (temperature < 55) return "#4b91ff";
  if (temperature < 70) return "#55c8be";
  if (temperature <= 105) return "#7ddd55";
  if (temperature <= 125) return "#efbd3d";
  return "#f05252";
};

const brakeColor = (temperature: number): string => {
  if (!Number.isFinite(temperature) || temperature < 0) return "#69737d";
  if (temperature < 200) return "#78838e";
  if (temperature < 350) return "#b58b4c";
  if (temperature < 500) return "#efb83f";
  if (temperature < 700) return "#f57835";
  if (temperature < 900) return "#f0443e";
  return "#fff0cf";
};

const readable = (value: number, digits = 0): string =>
  Number.isFinite(value) && value >= 0 ? `${formatNumber(value, digits)}°` : "--°";

const setText = (element: Element, value: string): void => {
  if (element.textContent !== value) element.textContent = value;
};

const setStyle = (element: HTMLElement, property: string, value: string): void => {
  if (element.style.getPropertyValue(property) !== value) element.style.setProperty(property, value);
};

const render = (frame: TelemetryFrame): void => {
  for (let wheelIndex = 0; wheelIndex < 4; wheelIndex += 1) {
    const wheel = wheels[wheelIndex];
    const temperatures = frame.player_tire_zone_temperature_c[wheelIndex];
    for (const zoneElement of wheel.querySelectorAll<HTMLElement>("[data-zone]")) {
      const zone = zoneElement.dataset.zone as Zone;
      const temperature = temperatures[zoneIndexes[zone]];
      setText(zoneElement.querySelector("strong")!, readable(temperature));
      setStyle(zoneElement, "--zone-color", temperatureColor(temperature));
      const title = t(zoneTitleKeys[zone], { value: readable(temperature, 1) });
      if (zoneElement.title !== title) zoneElement.title = title;
    }
    const state = frame.player_tire_detached[wheelIndex]
      ? "detached"
      : frame.player_tire_flat[wheelIndex] ? "flat" : "normal";
    if (wheel.dataset.state !== state) wheel.dataset.state = state;

    const brakeTemperature = frame.player_brake_temperature_c[wheelIndex];
    const brake = brakes[wheelIndex];
    setText(brake.querySelector("strong")!, readable(brakeTemperature));
    setStyle(brake, "--brake-color", brakeColor(brakeTemperature));
    const brakeTitle = t("tiretemps.brake", { value: readable(brakeTemperature, 1) });
    if (brake.title !== brakeTitle) brake.title = brakeTitle;
  }
};

const previewFrame = {
  player_tire_zone_temperature_c: [
    [82, 78, 73], [84, 80, 75], [76, 73, 69], [78, 74, 70]
  ],
  player_brake_temperature_c: [575, 605, 438, 462],
  player_tire_flat: [false, false, false, false],
  player_tire_detached: [false, false, false, false]
} as TelemetryFrame;

fitOverlay({ width: 310, height: 184 }, { widthTextRatio: 0.25, heightTextRatio: 0.25 });
bindOverlayTransparency("tiretemps");
bindOverlayInteractionMode();
if (!isTauriRuntime()) render(previewFrame);
void listenTelemetry((frame) => renderPerformance.measure(() => render(frame)));
