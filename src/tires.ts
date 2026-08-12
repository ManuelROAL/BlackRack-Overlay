import "./tires.css";
import type { TelemetryFrame } from "./telemetry-types";
import { fitOverlay } from "./overlay-fit";
import { bindOverlayInteractionMode } from "./overlay-interaction";
import { bindOverlayTransparency } from "./overlay-appearance";
import { createOverlayPerformanceTracker } from "./overlay-performance";
import { isTauriRuntime, listenTelemetry } from "./runtime-events";
import { compoundIconUrl } from "./lmu-icons";

const wheels = Array.from(document.querySelectorAll<HTMLElement>("[data-wheel]"));
const temperatures = wheels.map((wheel) => wheel.querySelector<HTMLElement>(".wheel-temperature")!);
const brakeTemperatures = wheels.map((wheel) => wheel.querySelector<HTMLElement>(".wheel-brake-temperature")!);
const wearValues = wheels.map((wheel) => wheel.querySelector<HTMLElement>(".wheel-wear")!);
const flatSpotValues = wheels.map((wheel) => wheel.querySelector<HTMLElement>(".wheel-flatspot")!);
const suspensionValues = wheels.map((wheel) => wheel.querySelector<HTMLElement>(".wheel-suspension")!);
const compounds = wheels.map((wheel) => wheel.querySelector<HTMLImageElement>("[data-compound]")!);
const damageParts = Array.from(document.querySelectorAll<SVGElement>("[data-damage-part]"));
const damageSummary = document.querySelector<HTMLElement>(".damage-summary")!;
const damageValue = document.getElementById("damage-value")!;
const renderPerformance = createOverlayPerformanceTracker("tires");

const tireColor = (temperature: number): string => {
  if (!Number.isFinite(temperature) || temperature < 0) return "#687481";
  if (temperature < 60) return "#4b91ff";
  if (temperature < 75) return "#55c8be";
  if (temperature <= 105) return "#55d89a";
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

const suspensionColor = (damage: number, detached: boolean): string => {
  if (detached) return "#ff244f";
  if (!Number.isFinite(damage) || damage < 0) return "#687481";
  if (damage >= 75) return "#ff244f";
  if (damage >= 50) return "#f05a42";
  if (damage >= 15) return "#ebc13c";
  return "#8995a2";
};

const shortCompound = (value: string): string => {
  const first = value.trim().charAt(0).toUpperCase();
  return ["S", "M", "H", "I", "W"].includes(first) ? first : "–";
};

const readable = (value: number, digits: number, suffix: string): string =>
  Number.isFinite(value) && value >= 0 ? `${value.toFixed(digits)}${suffix}` : `--${digits ? ".-" : ""}${suffix}`;

const damageNames = [
  "frontal", "delantera derecha", "lateral derecha", "trasera derecha",
  "trasera", "trasera izquierda", "lateral izquierda", "delantera izquierda"
];

const render = (frame: TelemetryFrame): void => {
  for (let index = 0; index < 4; index += 1) {
    const temperature = frame.player_tire_temperature_c[index];
    const brakeTemperature = frame.player_brake_temperature_c[index];
    const remaining = frame.player_tire_remaining_by_wheel_percent[index];
    const flatSpot = frame.player_tire_flat_spot_percent[index];
    const suspension = frame.player_suspension_damage_by_wheel_percent[index];
    const detached = frame.player_tire_detached[index];
    const critical = frame.player_tire_flat[index] || detached;

    temperatures[index].textContent = readable(temperature, 1, "°");
    brakeTemperatures[index].textContent = readable(brakeTemperature, 0, "°");
    wearValues[index].textContent = Number.isFinite(remaining) && remaining >= 0
      ? `${Math.round(remaining)}%`
      : "--%";
    flatSpotValues[index].textContent = Number.isFinite(flatSpot) && flatSpot >= 0
      ? `${flatSpot.toFixed(2)}%`
      : "--.--%";
    suspensionValues[index].textContent = Number.isFinite(suspension) && suspension >= 0
      ? `${Math.round(suspension)}%`
      : "--%";

    wheels[index].style.setProperty("--tire-color", critical ? "#e7314f" : tireColor(temperature));
    wheels[index].style.setProperty("--brake-color", brakeColor(brakeTemperature));
    wheels[index].style.setProperty("--suspension-color", suspensionColor(suspension, detached));
    wheels[index].classList.toggle("critical", critical);
    suspensionValues[index].dataset.state = detached || suspension >= 75
      ? "critical"
      : suspension >= 50 ? "heavy" : suspension >= 15 ? "warning" : "normal";

    const compound = shortCompound(frame.player_tire_compounds[index] ?? "");
    const iconUrl = compoundIconUrl(compound);
    if (compounds[index].getAttribute("src") !== iconUrl) compounds[index].src = iconUrl;
    compounds[index].title = `Compuesto ${compound === "–" ? "desconocido" : compound}`;
    compounds[index].dataset.kind = compound.toLowerCase();

    wheels[index].title = critical
      ? detached ? "Rueda desprendida" : "Neumático pinchado"
      : `Neumático ${readable(temperature, 1, " °C")} · Disco ${readable(brakeTemperature, 0, " °C")} · ${readable(remaining, 1, "% restante")} · Plano ${readable(flatSpot, 2, "%")} · Suspensión ${readable(suspension, 0, "%")}`;
  }

  for (const part of damageParts) {
    const partIndex = Number(part.dataset.damagePart);
    const severity = Math.min(frame.player_damage_severity[partIndex] ?? 0, 3);
    part.dataset.severity = frame.player_part_detached && severity > 0 ? "critical" : String(severity);
    part.setAttribute("aria-label", `Chasis ${damageNames[partIndex]}: nivel ${severity}`);
  }

  const aggregateDamage = Math.round(frame.player_damage_percent);
  damageValue.textContent = `${aggregateDamage}%`;
  damageSummary.dataset.state = frame.player_part_detached || aggregateDamage >= 50
    ? "critical"
    : aggregateDamage > 0 ? "warning" : "normal";
};

const previewFrame = {
  player_tire_temperature_c: [76.2, 83.3, 76.7, 81.1],
  player_brake_temperature_c: [540, 575, 420, 445],
  player_tire_remaining_by_wheel_percent: [94, 94, 95, 95],
  player_tire_flat_spot_percent: [0.08, 0, 0.15, 0.03],
  player_suspension_damage_by_wheel_percent: [2, 18, 4, 0],
  player_tire_compounds: ["M", "M", "M", "M"],
  player_tire_flat: [false, false, false, false],
  player_tire_detached: [false, false, false, false],
  player_damage_percent: 12,
  player_damage_severity: [1, 0, 0, 0, 0, 0, 0, 1],
  player_part_detached: false
} as TelemetryFrame;

fitOverlay({ width: 236, height: 188 });
bindOverlayTransparency("tires");
bindOverlayInteractionMode();
if (!isTauriRuntime()) render(previewFrame);
void listenTelemetry((frame) => renderPerformance.measure(() => render(frame)));
