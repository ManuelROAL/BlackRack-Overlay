import "./tires.css";
import type { TelemetryFrame } from "./telemetry-types";
import { fitOverlay } from "./overlay-fit";
import { bindOverlayInteractionMode } from "./overlay-interaction";
import { bindOverlayTransparency } from "./overlay-appearance";
import { createOverlayPerformanceTracker } from "./overlay-performance";
import { isTauriRuntime, listenRuntimeEvent, listenTelemetry } from "./runtime-events";
import { formatNumber, t, type TranslationKey } from "./i18n";
import { brakeTemperatureColor, tireTemperatureColor } from "./temperature-colors";
import { readTiresSettings, type TiresSettings } from "./tires-settings";
import {
  applyDisplayUnits,
  formatTemperature,
  temperatureUnit,
  type DisplayUnits
} from "./display-units";

const wheels = Array.from(document.querySelectorAll<HTMLElement>("[data-wheel]"));
const temperatures = wheels.map((wheel) => wheel.querySelector<HTMLElement>(".wheel-temperature")!);
const brakeTemperatures = wheels.map((wheel) => wheel.querySelector<HTMLElement>(".wheel-brake-temperature")!);
const wearValues = wheels.map((wheel) => wheel.querySelector<HTMLElement>(".wheel-wear")!);
const flatSpotValues = wheels.map((wheel) => wheel.querySelector<HTMLElement>(".wheel-flatspot")!);
const damageParts = Array.from(document.querySelectorAll<SVGElement>("[data-damage-part]"));
const aeroWing = document.querySelector<SVGElement>("[data-aero-wing]")!;
const engineStatus = document.querySelector<HTMLElement>("[data-engine-status]")!;
const damageSummary = document.querySelector<HTMLElement>(".damage-summary")!;
const damageValue = document.getElementById("damage-value")!;
const oilTemperature = document.querySelector<HTMLElement>('[data-engine-temperature="oil"]')!;
const waterTemperature = document.querySelector<HTMLElement>('[data-engine-temperature="water"]')!;
const oilTemperatureValue = oilTemperature.querySelector<HTMLElement>("strong")!;
const waterTemperatureValue = waterTemperature.querySelector<HTMLElement>("strong")!;
const renderPerformance = createOverlayPerformanceTracker("tires");
let settings = readTiresSettings();
let latestFrame: TelemetryFrame | null = null;

const setText = (element: HTMLElement, value: string): void => {
  if (element.textContent !== value) element.textContent = value;
};

const setStyleProperty = (element: HTMLElement, property: string, value: string): void => {
  if (element.style.getPropertyValue(property) !== value) element.style.setProperty(property, value);
};

const setData = (element: HTMLElement | SVGElement, key: string, value: string): void => {
  if (element.dataset[key] !== value) element.dataset[key] = value;
};

const setAttribute = (element: Element, name: string, value: string): void => {
  if (element.getAttribute(name) !== value) element.setAttribute(name, value);
};

const suspensionColor = (damage: number, detached: boolean): string => {
  if (detached) return "#ff244f";
  if (!Number.isFinite(damage) || damage < 0) return "#687481";
  if (damage >= 80) return "#ff244f";
  if (damage >= 40) return "#f05a42";
  if (damage >= 15) return "#f57835";
  if (damage >= 2) return "#ebc13c";
  return "#8995a2";
};

const shortCompound = (value: string): string => {
  const first = value.trim().charAt(0).toUpperCase();
  return ["S", "M", "H", "I", "W"].includes(first) ? first : "–";
};

const readable = (value: number, digits: number, suffix: string): string =>
  Number.isFinite(value) && value >= 0 ? `${formatNumber(value, digits)}${suffix}` : `--${digits ? ".-" : ""}${suffix}`;

const readableTemperature = (value: number, digits: number): string =>
  Number.isFinite(value) && value >= 0
    ? formatTemperature(value, digits)
    : `--${digits ? ".-" : ""}${temperatureUnit()}`;

const damageNameKeys: TranslationKey[] = [
  "tires.part.frontCenter", "tires.part.frontLeft", "tires.part.left", "tires.part.rearLeft",
  "tires.part.rearCenter", "tires.part.rearRight", "tires.part.right", "tires.part.frontRight"
];

const render = (frame: TelemetryFrame): void => {
  const brakeTemperatureAvailable = frame.capabilities?.brake_temperatures !== false;
  for (const element of brakeTemperatures) {
    element.hidden = !settings.showBrakeTemperature || !brakeTemperatureAvailable;
  }
  for (let index = 0; index < 4; index += 1) {
    const temperature = frame.player_tire_temperature_c[index];
    const zoneTemperatures = frame.player_tire_temperature_by_zone_c[index]
      ?? [temperature, temperature, temperature];
    const brakeTemperature = frame.player_brake_temperature_c[index];
    const remaining = frame.player_tire_remaining_by_wheel_percent[index];
    const flatSpot = frame.player_tire_flat_spot_percent[index];
    const suspension = frame.player_suspension_damage_by_wheel_percent[index];
    const detached = frame.player_tire_detached[index];
    const flat = frame.player_tire_flat[index] && !detached;
    const rawCompound = frame.player_tire_compounds[index] ?? "";
    // The wheel body follows its own bands, so its glow and text cannot
    // disagree with the rubber drawn underneath them. The printed number stays
    // the carcass-and-inner composite the MFD shows.
    const bandTemperature = zoneTemperatures.every(
      (value) => Number.isFinite(value) && value >= 0
    )
      ? zoneTemperatures.reduce((total, value) => total + value, 0) / zoneTemperatures.length
      : temperature;

    setText(temperatures[index], readableTemperature(temperature, 1));
    setText(brakeTemperatures[index], readableTemperature(brakeTemperature, 0));
    setText(wearValues[index], Number.isFinite(remaining) && remaining >= 0
      ? `${Math.round(remaining)}%`
      : "--%");
    setText(flatSpotValues[index], Number.isFinite(flatSpot) && flatSpot >= 0
      ? `${formatNumber(flatSpot, 2)}%`
      : "--.--%");
    setStyleProperty(
      wheels[index],
      "--tire-color",
      detached
        ? "#ff244f"
        : flat
        ? "#ff8a2b"
        : tireTemperatureColor(Math.round(bandTemperature))
    );
    for (let zoneIndex = 0; zoneIndex < 3; zoneIndex += 1) {
      const zoneColor = detached
        ? "#ff244f"
        : flat
        ? "#ff8a2b"
        : tireTemperatureColor(Math.round(zoneTemperatures[zoneIndex]));
      setStyleProperty(wheels[index], `--tire-zone-${zoneIndex}`, zoneColor);
    }
    setStyleProperty(wheels[index], "--brake-color", brakeTemperatureColor(Math.round(brakeTemperature)));
    setStyleProperty(wheels[index], "--suspension-color", suspensionColor(suspension, detached));
    wheels[index].classList.toggle("flat", flat);
    wheels[index].classList.toggle("detached", detached);
    const compound = shortCompound(rawCompound);
    // The optimum LMU publishes for the fitted compound stays out of the colour,
    // which follows the game's own fixed scale, but it is the number that says
    // where this rubber wants to be, so the tooltip carries it.
    const optimalTemperature = frame.player_tire_optimal_temperature_c?.[index] ?? -1;
    const compoundLabel = compound === "–" ? t("tires.unknown") : compound;

    const title = flat || detached
      ? detached ? t("tires.detached") : t("tires.flat")
      : t("tires.tooltip", {
          tire: readableTemperature(temperature, 1), brake: readableTemperature(brakeTemperature, 0),
          remaining: readable(remaining, 1, "%"), flat: readable(flatSpot, 2, "%"),
          suspension: readable(suspension, 0, "%"), compound: compoundLabel,
          optimal: readableTemperature(optimalTemperature, 0)
        });
    if (wheels[index].title !== title) wheels[index].title = title;
  }

  for (const part of damageParts) {
    const partIndex = Number(part.dataset.damagePart);
    const severity = Math.min(frame.player_damage_severity[partIndex] ?? 0, 3);
    setData(part, "severity", String(severity));
    setAttribute(part, "aria-label", t("tires.chassisPart", { part: t(damageNameKeys[partIndex]), level: severity }));
  }

  const aeroDamage = frame.player_aero_damage_percent;
  const aeroAvailable = Number.isFinite(aeroDamage) && aeroDamage >= 0;
  const normalizedAeroDamage = aeroAvailable ? Math.min(aeroDamage, 100) : -1;
  const rearWingDetached = frame.player_rear_wing_detached;
  setData(aeroWing, "state", rearWingDetached ? "detached" : "mounted");
  const aeroLabel = rearWingDetached
    ? t("tires.wingDetached")
    : aeroAvailable
    ? t("tires.aeroDamage", { value: Math.round(normalizedAeroDamage) })
    : t("tires.aeroUnavailable");
  setAttribute(aeroWing, "aria-label", aeroLabel);
  setAttribute(aeroWing, "title", aeroLabel);

  const engineLabel = t(frame.player_engine_overheating ? "tires.engineFailure" : "tires.engineNormal");
  setData(engineStatus, "state", frame.player_engine_overheating ? "failure" : "normal");
  setAttribute(engineStatus, "aria-label", engineLabel);
  setAttribute(engineStatus, "title", engineLabel);

  setText(oilTemperatureValue, readableTemperature(frame.player_engine_oil_temperature_c, 0));
  setText(waterTemperatureValue, readableTemperature(frame.player_engine_water_temperature_c, 0));
  setAttribute(oilTemperature, "title", t("tires.oilTemperature", {
    value: readableTemperature(frame.player_engine_oil_temperature_c, 0)
  }));
  setAttribute(waterTemperature, "title", t("tires.waterTemperature", {
    value: readableTemperature(frame.player_engine_water_temperature_c, 0)
  }));

  const aggregateDamageAvailable = Number.isFinite(frame.player_damage_percent)
    && frame.player_damage_percent >= 0;
  const aggregateDamage = aggregateDamageAvailable ? Math.round(frame.player_damage_percent) : 0;
  setText(damageValue, aggregateDamageAvailable ? `${aggregateDamage}%` : "--");
  setData(damageSummary, "state", frame.player_part_detached || (aggregateDamageAvailable && aggregateDamage >= 50)
    ? "critical"
    : aggregateDamageAvailable && aggregateDamage > 0 ? "warning" : "normal");

};

const previewFrame = {
  player_tire_temperature_c: [76.2, 83.3, 76.7, 81.1],
  player_tire_temperature_by_zone_c: [
    [70, 76, 82], [77, 83, 89], [71, 77, 83], [75, 81, 87]
  ],
  player_brake_temperature_c: [540, 575, 420, 445],
  player_tire_remaining_by_wheel_percent: [94, 94, 95, 95],
  player_tire_flat_spot_percent: [0.08, 0, 0.15, 0.03],
  player_suspension_damage_by_wheel_percent: [2, 18, 4, 0],
  player_tire_compounds: ["M", "M", "M", "M"],
  player_tire_optimal_temperature_c: [89, 89, 89, 89],
  player_tire_flat: [false, false, false, false],
  player_tire_detached: [false, false, false, false],
  player_aero_damage_percent: 18,
  player_damage_percent: 19,
  player_damage_severity: [0, 2, 1, 0, 0, 0, 0, 0],
  player_engine_overheating: false,
  player_engine_oil_temperature_c: 101,
  player_engine_water_temperature_c: 86,
  player_part_detached: false,
  player_rear_wing_detached: false
} as TelemetryFrame;

const applySettings = (next: TiresSettings): void => {
  settings = next;
  for (const element of temperatures) element.hidden = !settings.showTireTemperature;
  for (const element of brakeTemperatures) element.hidden = !settings.showBrakeTemperature;
  for (const element of flatSpotValues) element.hidden = !settings.showFlatSpot;
  for (const element of wearValues) element.hidden = !settings.showTireWear;
  oilTemperature.hidden = !settings.showOilTemperature;
  waterTemperature.hidden = !settings.showWaterTemperature;
};

fitOverlay(
  { width: 194, height: 130 },
  { widthTextRatio: 0.65, heightTextRatio: 0.6 }
);
bindOverlayTransparency("tires");
bindOverlayInteractionMode();
applySettings(settings);
if (!isTauriRuntime()) render(previewFrame);
void listenTelemetry((frame) => {
  latestFrame = frame;
  renderPerformance.measure(() => render(frame));
});
void listenRuntimeEvent<TiresSettings>("tires://settings", applySettings);
void listenRuntimeEvent<DisplayUnits>("display-units://change", (next) => {
  applyDisplayUnits(next);
  if (latestFrame) render(latestFrame);
});
