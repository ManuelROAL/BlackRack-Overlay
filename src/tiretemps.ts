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

const optimalTireTemperature = (compound: string): number => {
  const initial = compound.trim().charAt(0).toUpperCase();
  if (initial === "W" || initial === "I") return 50;
  if (initial === "S") return 80;
  if (initial === "H") return 100;
  return 90;
};

const temperatureColor = (temperature: number, optimal: number): string => {
  if (!Number.isFinite(temperature) || temperature < 0) return "#687481";
  if (temperature < optimal - 30) return "#5268e9";
  if (temperature < optimal - 20) return "#4b91ff";
  if (temperature < optimal - 10) return "#4dcff5";
  if (temperature < optimal) return "#55c8be";
  if (temperature < optimal + 10) return "#55d89a";
  if (temperature < optimal + 20) return "#8fe04f";
  if (temperature < optimal + 30) return "#efdb3d";
  if (temperature < optimal + 40) return "#f58a35";
  return "#f05252";
};

const temperatureTextColor = (temperature: number, optimal: number): string =>
  !Number.isFinite(temperature) || temperature < optimal - 20 ? "#f4f7fa" : "#07100f";

const brakeColor = (temperature: number): string => {
  if (!Number.isFinite(temperature) || temperature < 0) return "#69737d";
  if (temperature < 100) return "#5268e9";
  if (temperature < 200) return "#4b91ff";
  if (temperature < 300) return "#4dcff5";
  if (temperature < 400) return "#55c8be";
  if (temperature < 500) return "#55d89a";
  if (temperature < 600) return "#8fe04f";
  if (temperature < 700) return "#efdb3d";
  if (temperature < 800) return "#f58a35";
  return "#f05252";
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
    const optimal = optimalTireTemperature(frame.player_tire_compounds[wheelIndex] ?? "");
    for (const zoneElement of wheel.querySelectorAll<HTMLElement>("[data-zone]")) {
      const zone = zoneElement.dataset.zone as Zone;
      const temperature = temperatures[zoneIndexes[zone]];
      const colorTemperature = Math.round(temperature);
      setText(zoneElement.querySelector("strong")!, readable(temperature));
      setStyle(zoneElement, "--zone-color", temperatureColor(colorTemperature, optimal));
      setStyle(zoneElement, "--zone-text-color", temperatureTextColor(colorTemperature, optimal));
      const title = t(zoneTitleKeys[zone], { value: readable(temperature, 1) });
      if (zoneElement.title !== title) zoneElement.title = title;
    }
    const state = frame.player_tire_detached[wheelIndex]
      ? "detached"
      : frame.player_tire_flat[wheelIndex] ? "flat" : "normal";
    if (wheel.dataset.state !== state) wheel.dataset.state = state;

    const brakeTemperature = frame.player_brake_temperature_c[wheelIndex];
    const colorBrakeTemperature = Math.round(brakeTemperature);
    const brake = brakes[wheelIndex];
    setText(brake.querySelector("strong")!, readable(brakeTemperature));
    setStyle(brake, "--brake-color", brakeColor(colorBrakeTemperature));
    const brakeTitle = t("tiretemps.brake", { value: readable(brakeTemperature, 1) });
    if (brake.title !== brakeTitle) brake.title = brakeTitle;
  }
};

const previewFrame = {
  player_tire_zone_temperature_c: [
    [82, 78, 73], [84, 80, 75], [76, 73, 69], [78, 74, 70]
  ],
  player_brake_temperature_c: [575, 605, 438, 462],
  player_tire_compounds: ["M", "M", "M", "M"],
  player_tire_flat: [false, false, false, false],
  player_tire_detached: [false, false, false, false]
} as TelemetryFrame;

fitOverlay({ width: 310, height: 184 }, { widthTextRatio: 0.25, heightTextRatio: 0.25 });
bindOverlayTransparency("tiretemps");
bindOverlayInteractionMode();
if (!isTauriRuntime()) render(previewFrame);
void listenTelemetry((frame) => renderPerformance.measure(() => render(frame)));
