import "./tiretemps.css";
import type { TelemetryFrame } from "./telemetry-types";
import { fitOverlay } from "./overlay-fit";
import { bindOverlayInteractionMode } from "./overlay-interaction";
import { bindOverlayTransparency } from "./overlay-appearance";
import { createOverlayPerformanceTracker } from "./overlay-performance";
import { isTauriRuntime, listenTelemetry } from "./runtime-events";
import { formatNumber, t, type TranslationKey } from "./i18n";
import {
  brakeTemperatureColor,
  tireTemperatureColor,
  tireTemperatureTextColor
} from "./temperature-colors";

type Zone = "left" | "center" | "right";
const zoneIndexes: Record<Zone, number> = { left: 0, center: 1, right: 2 };
const zoneTitleKeys: Record<Zone, TranslationKey> = {
  left: "tiretemps.left",
  center: "tiretemps.center",
  right: "tiretemps.right"
};
const wheels = Array.from(document.querySelectorAll<HTMLElement>("[data-wheel]"));
const brakes = Array.from(document.querySelectorAll<HTMLElement>("[data-brake]"));
const contactPatches = Array.from(document.querySelectorAll<HTMLElement>(".contact-patch"));
const renderPerformance = createOverlayPerformanceTracker("tiretemps");

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
    const compound = frame.player_tire_compounds[wheelIndex] ?? "";
    const rawSlidingFraction = frame.player_tire_sliding_fraction[wheelIndex];
    const slidingFraction = Number.isFinite(rawSlidingFraction)
      ? Math.max(0, Math.min(1, rawSlidingFraction))
      : 0;
    setStyle(contactPatches[wheelIndex], "--contact-fraction", String(slidingFraction));
    for (const zoneElement of wheel.querySelectorAll<HTMLElement>("[data-zone]")) {
      const zone = zoneElement.dataset.zone as Zone;
      const temperature = temperatures[zoneIndexes[zone]];
      const colorTemperature = Math.round(temperature);
      setText(zoneElement.querySelector("strong")!, readable(temperature));
      setStyle(zoneElement, "--zone-color", tireTemperatureColor(colorTemperature, compound));
      setStyle(zoneElement, "--zone-text-color", tireTemperatureTextColor(colorTemperature, compound));
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
    setStyle(brake, "--brake-color", brakeTemperatureColor(colorBrakeTemperature));
    const brakeTitle = t("tiretemps.brake", { value: readable(brakeTemperature, 1) });
    if (brake.title !== brakeTitle) brake.title = brakeTitle;
  }
};

const previewFrame = {
  player_tire_zone_temperature_c: [
    [82, 78, 73], [84, 80, 75], [76, 73, 69], [78, 74, 70]
  ],
  player_brake_temperature_c: [575, 605, 438, 462],
  player_tire_sliding_fraction: [0.38, 0.42, 0.3, 0.35],
  player_tire_compounds: ["M", "M", "M", "M"],
  player_tire_flat: [false, false, false, false],
  player_tire_detached: [false, false, false, false]
} as TelemetryFrame;

fitOverlay({ width: 310, height: 184 }, { widthTextRatio: 0.25, heightTextRatio: 0.25 });
bindOverlayTransparency("tiretemps");
bindOverlayInteractionMode();
if (!isTauriRuntime()) render(previewFrame);
void listenTelemetry((frame) => renderPerformance.measure(() => render(frame)));
