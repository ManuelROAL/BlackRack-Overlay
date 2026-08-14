import "./tires.css";
import type { TelemetryFrame } from "./telemetry-types";
import { fitOverlay } from "./overlay-fit";
import { bindOverlayInteractionMode } from "./overlay-interaction";
import { bindOverlayTransparency } from "./overlay-appearance";
import { createOverlayPerformanceTracker } from "./overlay-performance";
import { isTauriRuntime, listenTelemetry } from "./runtime-events";

const wheels = Array.from(document.querySelectorAll<HTMLElement>("[data-wheel]"));
const temperatures = wheels.map((wheel) => wheel.querySelector<HTMLElement>(".wheel-temperature")!);
const brakeTemperatures = wheels.map((wheel) => wheel.querySelector<HTMLElement>(".wheel-brake-temperature")!);
const wearValues = wheels.map((wheel) => wheel.querySelector<HTMLElement>(".wheel-wear")!);
const flatSpotValues = wheels.map((wheel) => wheel.querySelector<HTMLElement>(".wheel-flatspot")!);
const damageParts = Array.from(document.querySelectorAll<SVGElement>("[data-damage-part]"));
const aeroWing = document.querySelector<SVGElement>("[data-aero-wing]")!;
const damageSummary = document.querySelector<HTMLElement>(".damage-summary")!;
const damageValue = document.getElementById("damage-value")!;
const renderPerformance = createOverlayPerformanceTracker("tires");

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
  Number.isFinite(value) && value >= 0 ? `${value.toFixed(digits)}${suffix}` : `--${digits ? ".-" : ""}${suffix}`;

const damageNames = [
  "frontal central", "frontal izquierda", "lateral izquierda", "trasera izquierda",
  "trasera central", "trasera derecha", "lateral derecha", "frontal derecha"
];

const render = (frame: TelemetryFrame): void => {
  for (let index = 0; index < 4; index += 1) {
    const temperature = frame.player_tire_temperature_c[index];
    const brakeTemperature = frame.player_brake_temperature_c[index];
    const remaining = frame.player_tire_remaining_by_wheel_percent[index];
    const flatSpot = frame.player_tire_flat_spot_percent[index];
    const suspension = frame.player_suspension_damage_by_wheel_percent[index];
    const detached = frame.player_tire_detached[index];
    const flat = frame.player_tire_flat[index] && !detached;

    setText(temperatures[index], readable(temperature, 1, "°"));
    setText(brakeTemperatures[index], readable(brakeTemperature, 0, "°"));
    setText(wearValues[index], Number.isFinite(remaining) && remaining >= 0
      ? `${Math.round(remaining)}%`
      : "--%");
    setText(flatSpotValues[index], Number.isFinite(flatSpot) && flatSpot >= 0
      ? `${flatSpot.toFixed(2)}%`
      : "--.--%");
    setStyleProperty(
      wheels[index],
      "--tire-color",
      detached ? "#ff244f" : flat ? "#ff8a2b" : tireColor(temperature)
    );
    setStyleProperty(wheels[index], "--brake-color", brakeColor(brakeTemperature));
    setStyleProperty(wheels[index], "--suspension-color", suspensionColor(suspension, detached));
    wheels[index].classList.toggle("flat", flat);
    wheels[index].classList.toggle("detached", detached);
    const compound = shortCompound(frame.player_tire_compounds[index] ?? "");
    const compoundLabel = compound === "–" ? "desconocido" : compound;

    const title = flat || detached
      ? detached ? "Rueda desprendida" : "Neumático pinchado"
      : `Neumático ${readable(temperature, 1, " °C")} · Disco ${readable(brakeTemperature, 0, " °C")} · ${readable(remaining, 1, "% restante")} · Plano ${readable(flatSpot, 2, "%")} · Suspensión ${readable(suspension, 0, "%")} · Compuesto ${compoundLabel}`;
    if (wheels[index].title !== title) wheels[index].title = title;
  }

  for (const part of damageParts) {
    const partIndex = Number(part.dataset.damagePart);
    const severity = Math.min(frame.player_damage_severity[partIndex] ?? 0, 3);
    setData(part, "severity", String(severity));
    setAttribute(part, "aria-label", `Chasis ${damageNames[partIndex]}: nivel ${severity}`);
  }

  const aeroDamage = frame.player_aero_damage_percent;
  const aeroAvailable = Number.isFinite(aeroDamage) && aeroDamage >= 0;
  const normalizedAeroDamage = aeroAvailable ? Math.min(aeroDamage, 100) : -1;
  const rearWingDetached = frame.player_rear_wing_detached;
  setData(aeroWing, "state", rearWingDetached ? "detached" : "mounted");
  const aeroLabel = rearWingDetached
    ? "Alerón desprendido"
    : aeroAvailable
    ? `Daño aerodinámico global ${Math.round(normalizedAeroDamage)}%`
    : "Daño aerodinámico global no disponible";
  setAttribute(aeroWing, "aria-label", aeroLabel);
  setAttribute(aeroWing, "title", aeroLabel);

  const aggregateDamage = Math.round(frame.player_damage_percent);
  setText(damageValue, `${aggregateDamage}%`);
  setData(damageSummary, "state", frame.player_part_detached || aggregateDamage >= 50
    ? "critical"
    : aggregateDamage > 0 ? "warning" : "normal");
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
  player_aero_damage_percent: 18,
  player_damage_percent: 19,
  player_damage_severity: [0, 2, 1, 0, 0, 0, 0, 0],
  player_part_detached: false,
  player_rear_wing_detached: false
} as TelemetryFrame;

fitOverlay({ width: 174, height: 130 });
bindOverlayTransparency("tires");
bindOverlayInteractionMode();
if (!isTauriRuntime()) render(previewFrame);
void listenTelemetry((frame) => renderPerformance.measure(() => render(frame)));
