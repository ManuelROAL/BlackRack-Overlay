import "./dashboard.css";
import type { TelemetryFrame } from "./telemetry-types";
import { fitOverlayToContent } from "./overlay-fit";
import { bindOverlayInteractionMode } from "./overlay-interaction";
import { bindOverlayTransparency } from "./overlay-appearance";
import { createOverlayPerformanceTracker } from "./overlay-performance";
import { isTauriRuntime, listenRuntimeEvent, listenTelemetry } from "./runtime-events";
import { brakeTemperatureColor, tireTemperatureColor } from "./temperature-colors";
import { t } from "./i18n";
import {
  DASHBOARD_OPTIONS,
  readDashboardSettings,
  type DashboardOptionId,
  type DashboardSettings
} from "./dashboard-settings";

type ElectronicsId = "map" | "tc" | "tcslip" | "tccut" | "abs" | "bias" | "migration" | "arb";
type HybridId = "battery" | "regen" | "motor";
type TrackedId = ElectronicsId | HybridId;

bindOverlayTransparency("dashboard");
const renderPerformance = createOverlayPerformanceTracker("dashboard");

const shell = document.querySelector<HTMLElement>(".dashboard-shell")!;
const values = Object.fromEntries(
  Array.from(document.querySelectorAll<HTMLElement>("[data-dashboard-value]")).map((element) => [
    element.dataset.dashboardValue as TrackedId,
    element
  ])
) as Record<TrackedId, HTMLElement>;
const parts = Object.fromEntries(
  Array.from(document.querySelectorAll<HTMLElement>("[data-dashboard-part]")).map((element) => [
    element.dataset.dashboardPart as DashboardOptionId,
    element
  ])
) as Record<DashboardOptionId, HTMLElement>;
const band = (name: string): HTMLElement =>
  document.querySelector<HTMLElement>(`[data-dashboard-band="${name}"]`)!;
const bandTop = band("top");
const bandMain = band("main");
const bandHybrid = band("hybrid");
const bandBottom = band("bottom");
const electronicsRow = band("electronics");

const wheels = Array.from(document.querySelectorAll<HTMLElement>("[data-dashboard-wheel]"));
const wheelValue = (wheel: HTMLElement, name: string): HTMLElement =>
  wheel.querySelector<HTMLElement>(`[data-dashboard-tire="${name}"]`)!;
const wheelPressures = wheels.map((wheel) => wheelValue(wheel, "pressure"));
const wheelTires = wheels.map((wheel) => wheelValue(wheel, "tire"));
const wheelBrakes = wheels.map((wheel) => wheelValue(wheel, "brake"));
const wheelWear = wheels.map((wheel) => wheel.querySelector<HTMLElement>(".dash-tire-wear i")!);
const compound = document.getElementById("dashboard-compound")!;

const gear = document.getElementById("dashboard-gear")!;
const speed = document.getElementById("dashboard-speed")!;
const revFill = document.getElementById("dashboard-rev-fill")!;
const air = document.getElementById("dashboard-air")!;
const track = document.getElementById("dashboard-track")!;
const delta = document.getElementById("dashboard-delta")!;
const sessionTime = document.getElementById("dashboard-session-time")!;
const position = document.getElementById("dashboard-position")!;
const classPosition = document.getElementById("dashboard-class-position")!;
const lap = document.getElementById("dashboard-lap")!;
const predicted = document.getElementById("dashboard-predicted")!;
const last = document.getElementById("dashboard-last")!;
const best = document.getElementById("dashboard-best")!;
const litres = document.getElementById("dashboard-litres")!;
const average = document.getElementById("dashboard-average")!;
const energyCell = document.getElementById("dashboard-energy-cell")!;
const energy = document.getElementById("dashboard-energy")!;
const range = document.getElementById("dashboard-range")!;
const battery = document.getElementById("dashboard-battery-fill")!;
const motorState = document.getElementById("dashboard-motor-state")!;
const limiter = document.getElementById("dashboard-limiter")!;
const headlights = document.getElementById("dashboard-headlights")!;
const wipers = document.getElementById("dashboard-wipers")!;
const empty = document.getElementById("dashboard-empty")!;
// Blocks come and go with the car and with the driver's shortlist, so the panel
// is only ever as tall as what it is actually drawing.
const synchronizeOverlayHeight = fitOverlayToContent(700, shell);

const ELECTRONICS: ElectronicsId[] = [
  "map", "tc", "tcslip", "tccut", "abs", "bias", "migration", "arb"
];
const HYBRID: HybridId[] = ["battery", "regen", "motor"];

let settings = readDashboardSettings();

const setText = (element: HTMLElement, text: string): void => {
  if (element.textContent !== text) element.textContent = text;
};

const setState = (element: HTMLElement, attribute: string, state: string): void => {
  if (element.dataset[attribute] !== state) element.dataset[attribute] = state;
};

const setWidth = (element: HTMLElement, percent: number): void => {
  const width = `${Math.max(0, Math.min(100, percent)).toFixed(1)}%`;
  if (element.style.width !== width) element.style.width = width;
};

const setColor = (element: HTMLElement, property: string, color: string): void => {
  if (element.style.getPropertyValue(property) !== color) {
    element.style.setProperty(property, color);
  }
};

const toggle = (element: HTMLElement, visible: boolean): void => {
  if (element.hidden === !visible) return;
  element.hidden = !visible;
};

/**
 * A rotary the driver has just turned is the value worth finding again, so the
 * cell that changed stays highlighted for a bounded number of telemetry
 * updates. The decay is counted in frames rather than kept on a timer: the
 * native host has to repaint from telemetry, never on its own clock.
 */
const CHANGE_HIGHLIGHT_FRAMES = 100;
const previous = new Map<TrackedId, string>();
const highlighted = new Map<TrackedId, number>();

const publish = (id: TrackedId, text: string): void => {
  const before = previous.get(id);
  previous.set(id, text);
  setText(values[id], text);
  if (before !== undefined && before !== text && text !== "--") {
    highlighted.set(id, CHANGE_HIGHLIGHT_FRAMES);
    setState(parts[id], "changed", "true");
  }
};

const decayHighlights = (): void => {
  for (const [id, remaining] of highlighted) {
    if (remaining > 1) {
      highlighted.set(id, remaining - 1);
      continue;
    }
    highlighted.delete(id);
    setState(parts[id], "changed", "false");
  }
};

const applySettings = (next: DashboardSettings): void => {
  settings = next;
  for (const { id } of DASHBOARD_OPTIONS) toggle(parts[id], settings.visible[id]);
};

applySettings(settings);

const UNKNOWN = "--";
const UNKNOWN_LAP = "--:--.---";

const level = (value: number, max: number): string =>
  max > 0 ? `${value}/${max}` : `${value}`;

const rounded = (value: number, digits = 0, suffix = ""): string =>
  Number.isFinite(value) && value >= 0 ? `${value.toFixed(digits)}${suffix}` : UNKNOWN;

const KPA_TO_PSI = 0.1450377;

const lapTime = (seconds: number): string => {
  if (!Number.isFinite(seconds) || seconds <= 0) return UNKNOWN_LAP;
  const minutes = Math.floor(seconds / 60);
  return `${minutes}:${(seconds - minutes * 60).toFixed(3).padStart(6, "0")}`;
};

const clock = (seconds: number): string => {
  if (!Number.isFinite(seconds) || seconds <= 0) return "--:--:--";
  const whole = Math.floor(seconds);
  const hours = Math.floor(whole / 3600);
  const minutes = Math.floor((whole % 3600) / 60);
  return `${hours}:${String(minutes).padStart(2, "0")}:${String(whole % 60).padStart(2, "0")}`;
};

const gearLabel = (value: number): string =>
  value < 0 ? "R" : value === 0 ? "N" : String(value);

const MOTOR_STATES = ["inactive", "inactive", "propulsion", "regeneration"] as const;
const MOTOR_STATE_KEYS = [
  "dashboard.stateInactive",
  "dashboard.stateInactive",
  "dashboard.statePropulsion",
  "dashboard.stateRegeneration"
] as const;

const chargeLevel = (percent: number): string =>
  percent < 15 ? "low" : percent < 40 ? "medium" : "ok";

const wearLevel = (percent: number): string =>
  percent < 15 ? "low" : percent < 35 ? "medium" : "ok";

const revLevel = (fraction: number): string =>
  fraction >= 0.97 ? "limit" : fraction >= 0.9 ? "high" : "normal";

const renderTires = (frame: TelemetryFrame): void => {
  for (let index = 0; index < wheels.length; index += 1) {
    const pressure = frame.player_tire_pressure_kpa[index];
    const temperature = frame.player_tire_temperature_c[index];
    const brake = frame.player_brake_temperature_c[index];
    const remaining = frame.player_tire_remaining_by_wheel_percent[index];
    const rawCompound = frame.player_tire_compounds[index] ?? "";
    setText(
      wheelPressures[index],
      pressure > 0 ? (pressure * KPA_TO_PSI).toFixed(1) : UNKNOWN
    );
    setText(wheelTires[index], rounded(temperature, 0, "°"));
    setText(wheelBrakes[index], rounded(brake, 0, "°"));
    setColor(wheels[index], "--tire-color", tireTemperatureColor(
      Math.round(temperature),
      frame.player_tire_optimal_temperature_c[index],
      rawCompound
    ));
    setColor(wheels[index], "--brake-color", brakeTemperatureColor(Math.round(brake)));
    setWidth(wheelWear[index], remaining);
    setState(wheels[index], "wear", remaining >= 0 ? wearLevel(remaining) : "ok");
  }
  const fitted = frame.player_tire_compounds.find((name) => name.trim().length > 0) ?? "";
  setText(compound, fitted.trim().toLocaleUpperCase() || UNKNOWN);
};

const renderCore = (frame: TelemetryFrame): void => {
  setText(gear, gearLabel(frame.gear));
  setText(speed, String(Math.max(0, Math.round(frame.speed_kph))));
  const fraction = frame.max_rpm > 0 ? frame.rpm / frame.max_rpm : 0;
  setWidth(revFill, fraction * 100);
  setState(parts.core, "rev", revLevel(fraction));
  setText(air, rounded(frame.ambient_temperature_c, 0, "°"));
  setText(track, rounded(frame.track_temperature_c, 0, "°"));
};

const renderSession = (frame: TelemetryFrame): void => {
  setText(sessionTime, clock(frame.session_time_remaining));
  setText(position, frame.player_position > 0 ? String(frame.player_position) : UNKNOWN);
  setText(
    classPosition,
    frame.player_class_position > 0 && frame.player_class_size > 0
      ? `${frame.player_class_position}/${frame.player_class_size}`
      : UNKNOWN
  );
  setText(
    lap,
    frame.lap_number > 0
      ? frame.session_max_laps > 0
        ? `${frame.lap_number}/${frame.session_max_laps}`
        : String(frame.lap_number)
      : UNKNOWN
  );
};

const renderFuel = (frame: TelemetryFrame): void => {
  setText(litres, rounded(frame.fuel_liters, 1));
  setText(average, rounded(frame.fuel_per_lap, 2));
  toggle(energyCell, frame.virtual_energy_active);
  setText(energy, rounded(frame.virtual_energy_percent, 0, "%"));
  // Virtual energy runs out before the tank on LMU's hybrid classes, so the
  // range is whichever budget ends first rather than the fuel one alone.
  const fuelLaps = frame.estimated_fuel_laps;
  const energyLaps = frame.estimated_virtual_energy_laps;
  const remaining = frame.virtual_energy_active && energyLaps > 0
    ? Math.min(fuelLaps, energyLaps)
    : fuelLaps;
  setText(range, rounded(remaining, 1));
};

const render = (frame: TelemetryFrame): void => {
  const live = frame.player_active;
  const electronicsAvailable = live && frame.car_electronics_available;
  const hybridAvailable = live && frame.hybrid_available;

  if (live) {
    renderTires(frame);
    renderCore(frame);
    renderSession(frame);
    renderFuel(frame);
    const deltaModel = frame.delta_model;
    setText(delta, deltaModel.available
      ? `${deltaModel.seconds >= 0 ? "+" : "−"}${Math.abs(deltaModel.seconds).toFixed(3)}`
      : "--.---");
    setState(delta, "trend", deltaModel.available ? deltaModel.trend : "neutral");
    const timing = frame.timing_model;
    setText(predicted, timing.available ? lapTime(timing.estimated_seconds) : UNKNOWN_LAP);
    setText(last, timing.available ? lapTime(timing.last_seconds) : UNKNOWN_LAP);
    setText(best, timing.available ? lapTime(timing.personal_best_seconds) : UNKNOWN_LAP);
    setState(limiter, "state", frame.speed_limiter_active ? "on" : "off");
    setState(headlights, "state", frame.headlights_on ? "on" : "off");
    setState(wipers, "state", frame.wiper_state > 0 ? "on" : "off");
  }

  if (electronicsAvailable) {
    publish("map", level(frame.engine_map, frame.engine_map_max));
    publish("tc", level(frame.traction_control_level, frame.traction_control_max));
    publish("tcslip", level(frame.traction_control_slip, frame.traction_control_slip_max));
    publish("tccut", level(frame.traction_control_cut, frame.traction_control_cut_max));
    publish("abs", level(frame.anti_lock_brakes_level, frame.anti_lock_brakes_max));
    publish("bias", `${frame.brake_bias_percent.toFixed(1)}%`);
    publish("migration", level(frame.brake_migration, frame.brake_migration_max));
    publish("arb", `${frame.front_anti_roll_bar}/${frame.rear_anti_roll_bar}`);
  }

  if (hybridAvailable) {
    const charge = Math.max(0, Math.min(100, frame.battery_charge_percent));
    publish("battery", `${Math.round(charge)}%`);
    setWidth(battery, charge);
    setState(parts.battery, "level", chargeLevel(charge));
    publish("regen", `${Math.round(Math.max(0, frame.hybrid_regen_kw))} kW`);
    publish("motor", `${Math.round(frame.hybrid_motor_temperature_c)}°`);
    const motor = Math.max(0, Math.min(3, Math.round(frame.hybrid_motor_state)));
    setState(motorState, "state", MOTOR_STATES[motor]);
    setText(motorState, t(MOTOR_STATE_KEYS[motor]));
    toggle(motorState, motor > 0);
  }

  const visible = (id: DashboardOptionId): boolean => live && settings.visible[id];
  const topVisible = visible("status") || visible("delta") || visible("session");
  const mainVisible = visible("tires") || visible("core") || visible("laptimes");
  const electronicsVisible = electronicsAvailable
    && ELECTRONICS.some((id) => settings.visible[id]);
  const hybridVisible = hybridAvailable && HYBRID.some((id) => settings.visible[id]);
  const bottomVisible = electronicsVisible || visible("fuel");
  toggle(bandTop, topVisible);
  toggle(bandMain, mainVisible);
  toggle(bandHybrid, hybridVisible);
  toggle(bandBottom, bottomVisible);
  toggle(electronicsRow, electronicsVisible);
  toggle(empty, !topVisible && !mainVisible && !hybridVisible && !bottomVisible);
  decayHighlights();
  synchronizeOverlayHeight();
};

const previewFrame = {
  player_active: true,
  car_electronics_available: true,
  engine_map: 6,
  engine_map_max: 9,
  traction_control_level: 3,
  traction_control_max: 11,
  traction_control_slip: 5,
  traction_control_slip_max: 11,
  traction_control_cut: 4,
  traction_control_cut_max: 11,
  anti_lock_brakes_level: 2,
  anti_lock_brakes_max: 11,
  brake_bias_percent: 56.5,
  brake_migration: 3,
  brake_migration_max: 6,
  front_anti_roll_bar: 5,
  rear_anti_roll_bar: 4,
  speed_limiter_active: false,
  headlights_on: true,
  wiper_state: 0,
  hybrid_available: true,
  battery_charge_percent: 62,
  hybrid_regen_kw: 84,
  hybrid_motor_state: 2,
  hybrid_motor_temperature_c: 63,
  gear: 4,
  speed_kph: 238,
  rpm: 7600,
  max_rpm: 9200,
  ambient_temperature_c: 20,
  track_temperature_c: 32,
  session_time_remaining: 3 * 3600 + 42 * 60 + 15,
  session_max_laps: 0,
  lap_number: 37,
  player_position: 4,
  player_class_position: 2,
  player_class_size: 9,
  fuel_liters: 46.3,
  fuel_per_lap: 2.64,
  estimated_fuel_laps: 17.5,
  virtual_energy_active: true,
  virtual_energy_percent: 58,
  estimated_virtual_energy_laps: 15.2,
  player_tire_pressure_kpa: [190.4, 191.7, 191.2, 192.4],
  player_tire_temperature_c: [82, 91, 85, 92],
  player_brake_temperature_c: [623, 648, 634, 701],
  player_tire_remaining_by_wheel_percent: [88, 86, 74, 71],
  player_tire_optimal_temperature_c: [90, 90, 90, 90],
  player_tire_compounds: ["Dry 2", "Dry 2", "Dry 2", "Dry 2"],
  delta_model: { available: true, seconds: -0.284, trend: "improving" },
  timing_model: {
    available: true,
    estimated_seconds: 213.412,
    last_seconds: 214.006,
    personal_best_seconds: 212.884
  }
} as unknown as TelemetryFrame;

bindOverlayInteractionMode();
if (!isTauriRuntime()) render(previewFrame);
void listenTelemetry((frame) => renderPerformance.measure(() => render(frame)));
if (isTauriRuntime()) {
  void listenRuntimeEvent<DashboardSettings>("dashboard://settings", applySettings);
}
