import "./tiretemps.css";
import type { TelemetryFrame } from "./telemetry-types";
import { fitOverlay } from "./overlay-fit";
import { bindOverlayInteractionMode } from "./overlay-interaction";
import { bindOverlayTransparency } from "./overlay-appearance";
import { createOverlayPerformanceTracker } from "./overlay-performance";
import { isTauriRuntime, listenRuntimeEvent, listenTelemetry } from "./runtime-events";
import { formatNumber, t, type TranslationKey } from "./i18n";
import {
  readTireTempsSettings,
  type TireTempsSettings
} from "./tiretemps-settings";
import {
  brakeTemperatureColor,
  tireTemperatureColor,
  tireTemperatureTextColor
} from "./temperature-colors";

interface WheelElements {
  root: HTMLElement;
  surface: HTMLElement[];
  innerLayer: HTMLElement[];
  carcass: HTMLElement;
}

const wheels: WheelElements[] = Array.from(
  document.querySelectorAll<HTMLElement>("[data-wheel]")
).map((root) => ({
  root,
  surface: Array.from(root.querySelectorAll<HTMLElement>('[data-temperature="surface"] .tread-zone')),
  innerLayer: Array.from(root.querySelectorAll<HTMLElement>('[data-temperature="innerLayer"] .tread-zone')),
  carcass: root.querySelector<HTMLElement>('[data-temperature="carcass"] .tread-zone')!
}));
const brakes = Array.from(document.querySelectorAll<HTMLElement>("[data-brake]"));
const renderPerformance = createOverlayPerformanceTracker("tiretemps");
const resizeOverlay = fitOverlay({ width: 258, height: 136 }, { widthTextRatio: 0.25, heightTextRatio: 0.25 });
let settings = readTireTempsSettings();

const readable = (value: number, digits = 0): string =>
  Number.isFinite(value) && value >= 0 ? `${formatNumber(value, digits)}°` : "--°";

const setText = (element: Element, value: string): void => {
  if (element.textContent !== value) element.textContent = value;
};

const setStyle = (element: HTMLElement, property: string, value: string): void => {
  if (element.style.getPropertyValue(property) !== value) element.style.setProperty(property, value);
};

const renderTemperatureValue = (
  value: HTMLElement,
  temperature: number,
  compound: string,
  titleKey: TranslationKey
): void => {
  const colorTemperature = Math.round(temperature);
  setText(value.querySelector("strong")!, readable(temperature));
  setStyle(value, "--zone-color", tireTemperatureColor(colorTemperature, compound));
  setStyle(value, "--zone-text-color", tireTemperatureTextColor(colorTemperature, compound));
  const title = t(titleKey, { value: readable(temperature, 1) });
  if (value.title !== title) value.title = title;
};

const applySettings = (next: TireTempsSettings): void => {
  settings = next;
  const visibility = {
    surface: settings.showSurface,
    innerLayer: settings.showInnerLayer,
    carcass: settings.showCarcass
  };
  for (const [temperature, visible] of Object.entries(visibility)) {
    for (const layer of document.querySelectorAll<HTMLElement>(`[data-temperature="${temperature}"]`)) {
      layer.hidden = !visible;
    }
  }
  const activeTyreLayers = Object.values(visibility).filter(Boolean).length;
  for (const wheel of wheels) wheel.root.hidden = activeTyreLayers === 0;
  for (const brake of brakes) brake.hidden = !settings.showBrakes;
  resizeOverlay({
    width: 258,
    height: activeTyreLayers === 0 ? 92 : 136
  });
};

const render = (frame: TelemetryFrame): void => {
  for (let wheelIndex = 0; wheelIndex < 4; wheelIndex += 1) {
    const wheel = wheels[wheelIndex];
    const compound = frame.player_tire_compounds[wheelIndex] ?? "";
    const surfaceTemperatures = frame.player_tire_zone_temperature_c[wheelIndex];
    const innerLayerTemperatures = frame.player_tire_inner_layer_temperature_c[wheelIndex];
    for (let zoneIndex = 0; zoneIndex < 3; zoneIndex += 1) {
      renderTemperatureValue(
        wheel.surface[zoneIndex], surfaceTemperatures[zoneIndex], compound, "tiretemps.surface"
      );
      renderTemperatureValue(
        wheel.innerLayer[zoneIndex], innerLayerTemperatures[zoneIndex], compound,
        "tiretemps.innerLayer"
      );
    }
    renderTemperatureValue(
      wheel.carcass, frame.player_tire_carcass_temperature_c[wheelIndex], compound,
      "tiretemps.carcass"
    );
    const state = frame.player_tire_detached[wheelIndex]
      ? "detached"
      : frame.player_tire_flat[wheelIndex] ? "flat" : "normal";
    if (wheel.root.dataset.state !== state) wheel.root.dataset.state = state;

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
    [72, 77, 81], [84, 82, 78], [73, 76, 79], [82, 80, 76]
  ],
  player_tire_inner_layer_temperature_c: [
    [78, 79, 80], [82, 81, 80], [75, 76, 77], [80, 79, 78]
  ],
  player_tire_carcass_temperature_c: [77, 80, 74, 78],
  player_brake_temperature_c: [575, 605, 438, 462],
  player_tire_compounds: ["M", "M", "M", "M"],
  player_tire_flat: [false, false, false, false],
  player_tire_detached: [false, false, false, false]
} as TelemetryFrame;

applySettings(settings);
bindOverlayTransparency("tiretemps");
bindOverlayInteractionMode();
if (!isTauriRuntime()) render(previewFrame);
void listenRuntimeEvent<TireTempsSettings>("tiretemps://settings", applySettings);
void listenTelemetry((frame) => renderPerformance.measure(() => render(frame)));
