import "./dashboard.css";
import type { TelemetryFrame } from "./telemetry-types";
import { fitOverlayToContentBox } from "./overlay-fit";
import { bindOverlayInteractionMode } from "./overlay-interaction";
import { bindOverlayTransparency } from "./overlay-appearance";
import { createOverlayPerformanceTracker } from "./overlay-performance";
import { isTauriRuntime, listenRuntimeEvent, listenTelemetry } from "./runtime-events";
import {
  DASHBOARD_FIELDS,
  readDashboardSettings,
  type DashboardFieldId,
  type DashboardSettings
} from "./dashboard-settings";

bindOverlayTransparency("dashboard");
const renderPerformance = createOverlayPerformanceTracker("dashboard");

const shell = document.querySelector<HTMLElement>(".dashboard-shell")!;
const fields = Object.fromEntries(
  Array.from(document.querySelectorAll<HTMLElement>("[data-dashboard-field]")).map((element) => [
    element.dataset.dashboardField as DashboardFieldId,
    element
  ])
) as Record<DashboardFieldId, HTMLElement>;
const value = (id: string): HTMLElement => document.getElementById(`dashboard-${id}`)!;
const gear = value("gear");
const revs = value("revs");
const readout = document.querySelector<HTMLElement>(".dash-readout")!;
const row = value("fields");
const empty = value("empty");

const REV_LIGHTS = 12;
const lights: HTMLElement[] = [];
for (let index = 0; index < REV_LIGHTS; index += 1) {
  const light = document.createElement("i");
  light.dataset.lit = "false";
  revs.append(light);
  lights.push(light);
}

const synchronizeOverlaySize = fitOverlayToContentBox(shell);

let settings = readDashboardSettings();

const setText = (element: HTMLElement, text: string): void => {
  if (element.textContent !== text) element.textContent = text;
};

const setState = (element: HTMLElement, attribute: string, state: string): void => {
  if (element.dataset[attribute] !== state) element.dataset[attribute] = state;
};

const toggle = (element: HTMLElement, visible: boolean): void => {
  if (element.hidden === !visible) return;
  element.hidden = !visible;
};

const UNKNOWN = "--";
const UNKNOWN_LAP = "--:--.---";

const level = (current: number, max: number): string =>
  max > 0 ? `${current}/${max}` : UNKNOWN;

const rounded = (input: number, digits = 0, suffix = ""): string =>
  Number.isFinite(input) && input >= 0 ? `${input.toFixed(digits)}${suffix}` : UNKNOWN;

/// Zero can only mean "not learned yet" for a rate or a range: no car burns
/// nothing per lap, and a range of zero laps would mean an empty tank.
const learned = (input: number, digits: number, suffix = ""): string =>
  Number.isFinite(input) && input > 0 ? `${input.toFixed(digits)}${suffix}` : UNKNOWN;

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

const gearLabel = (current: number): string =>
  current < 0 ? "R" : current === 0 ? "N" : String(current);

const revLevel = (fraction: number): string =>
  fraction >= 0.97 ? "limit" : fraction >= 0.9 ? "high" : "normal";

/**
 * A field the car or the session cannot answer is dropped rather than drawn as
 * a dash: on a strip this short an empty slot costs as much room as a real
 * reading. A maximum of zero means the system is absent and a maximum of one
 * means there is nothing to select, which is how an LMP2 reports ABS and the
 * engine map.
 */
const available = (id: DashboardFieldId, frame: TelemetryFrame): boolean => {
  switch (id) {
    case "map": return frame.car_electronics_available && frame.engine_map_max > 1;
    case "tc": return frame.car_electronics_available && frame.traction_control_max > 0;
    case "tcslip":
      return frame.car_electronics_available && frame.traction_control_slip_max > 0;
    case "tccut":
      return frame.car_electronics_available && frame.traction_control_cut_max > 0;
    // The chip is the warning itself, so it exists only while the limiter does.
    case "limiter": return frame.speed_limiter_active;
    case "abs": return frame.car_electronics_available && frame.anti_lock_brakes_max > 0;
    case "battery": return frame.hybrid_available;
    case "energy": return frame.virtual_energy_active;
    case "delta": return frame.delta_model.available;
    case "lastlap": case "bestlap": case "predicted": return frame.timing_model.available;
    default: return true;
  }
};

const renderValues = (frame: TelemetryFrame): void => {
  setText(gear, gearLabel(frame.gear));
  const fraction = frame.max_rpm > 0 ? Math.max(0, frame.rpm / frame.max_rpm) : 0;
  const lit = Math.min(REV_LIGHTS, Math.round(fraction * REV_LIGHTS));
  for (let index = 0; index < REV_LIGHTS; index += 1) {
    setState(lights[index], "lit", index < lit ? "true" : "false");
  }
  const rev = revLevel(fraction);
  setState(gear, "rev", rev);
  setState(revs, "rev", rev);

  setText(value("speed"), String(Math.max(0, Math.round(frame.speed_kph))));
  setText(value("rpm"), String(Math.max(0, Math.round(frame.rpm))));
  setText(
    value("position"),
    frame.player_class_position > 0
      ? `P${frame.player_class_position}`
      : frame.player_position > 0 ? `P${frame.player_position}` : UNKNOWN
  );
  setText(value("lap"), frame.lap_number > 0
    ? frame.session_max_laps > 0
      ? `${frame.lap_number}/${frame.session_max_laps}`
      : String(frame.lap_number)
    : UNKNOWN);
  setText(value("session"), clock(frame.session_time_remaining));

  const delta = frame.delta_model;
  setText(value("delta"), delta.available
    ? `${delta.seconds >= 0 ? "+" : "−"}${Math.abs(delta.seconds).toFixed(3)}`
    : "--.---");
  setState(value("delta"), "trend", delta.available ? delta.trend : "neutral");

  const timing = frame.timing_model;
  setText(value("lastlap"), lapTime(timing.last_seconds));
  setText(value("bestlap"), lapTime(timing.personal_best_seconds));
  setText(value("predicted"), lapTime(timing.estimated_seconds));

  setText(value("fuel"), rounded(frame.fuel_liters, 1));
  setText(value("energy"), rounded(frame.virtual_energy_percent, 0, "%"));
  const fuelLaps = frame.estimated_fuel_laps;
  const energyLaps = frame.estimated_virtual_energy_laps;
  setText(value("laps"), learned(
    frame.virtual_energy_active && energyLaps > 0 ? Math.min(fuelLaps, energyLaps) : fuelLaps,
    1
  ));
  setText(value("battery"), rounded(frame.battery_charge_percent, 0, "%"));

  setText(value("map"), level(frame.engine_map, frame.engine_map_max));
  setText(value("tc"), level(frame.traction_control_level, frame.traction_control_max));
  setText(value("tcslip"), level(frame.traction_control_slip, frame.traction_control_slip_max));
  setText(value("tccut"), level(frame.traction_control_cut, frame.traction_control_cut_max));
  setText(value("abs"), level(frame.anti_lock_brakes_level, frame.anti_lock_brakes_max));
  setText(value("bias"), `${frame.brake_bias_percent.toFixed(1)}%`);
  setText(value("air"), rounded(frame.ambient_temperature_c, 0, "°"));
  setText(value("track"), rounded(frame.track_temperature_c, 0, "°"));
};

const render = (frame: TelemetryFrame): void => {
  const live = frame.player_active;
  if (live) renderValues(frame);

  let rowFields = 0;
  for (const { id } of DASHBOARD_FIELDS) {
    const shown = live && settings.visible[id] && available(id, frame);
    toggle(fields[id], shown);
    if (shown && id !== "gear" && id !== "revs" && id !== "limiter") rowFields += 1;
  }
  const gearShown = live && settings.visible.gear;
  const revsShown = live && settings.visible.revs;
  toggle(row, rowFields > 0);
  toggle(readout, rowFields > 0 || revsShown);
  toggle(empty, !gearShown && rowFields === 0 && !revsShown);
  synchronizeOverlaySize();
};

const applySettings = (next: DashboardSettings): void => {
  settings = next;
  for (const { id } of DASHBOARD_FIELDS) {
    // The limiter chip is driven by the car, not by the preference alone, so it
    // waits for the next frame rather than flashing on when it is enabled.
    toggle(fields[id], id !== "limiter" && settings.visible[id]);
  }
  synchronizeOverlaySize();
};

applySettings(settings);

const previewFrame = {
  player_active: true,
  gear: 6,
  speed_kph: 255,
  rpm: 6309,
  max_rpm: 9200,
  player_position: 12,
  player_class_position: 12,
  player_class_size: 20,
  lap_number: 37,
  session_max_laps: 0,
  session_time_remaining: 3 * 3600 + 42 * 60 + 15,
  fuel_liters: 46.3,
  estimated_fuel_laps: 17.5,
  virtual_energy_active: true,
  virtual_energy_percent: 58,
  estimated_virtual_energy_laps: 15.2,
  hybrid_available: true,
  battery_charge_percent: 62,
  car_electronics_available: true,
  engine_map: 6,
  engine_map_max: 9,
  traction_control_level: 3,
  traction_control_max: 11,
  traction_control_slip: 5,
  traction_control_slip_max: 11,
  traction_control_cut: 4,
  traction_control_cut_max: 11,
  speed_limiter_active: false,
  anti_lock_brakes_level: 2,
  anti_lock_brakes_max: 11,
  brake_bias_percent: 56.5,
  ambient_temperature_c: 22,
  track_temperature_c: 27,
  delta_model: { available: true, seconds: -0.284, trend: "improving" },
  timing_model: {
    available: true,
    estimated_seconds: 105.412,
    last_seconds: 105.035,
    personal_best_seconds: 104.884
  }
} as unknown as TelemetryFrame;

bindOverlayInteractionMode();
if (!isTauriRuntime()) render(previewFrame);
void listenTelemetry((frame) => renderPerformance.measure(() => render(frame)));
if (isTauriRuntime()) {
  void listenRuntimeEvent<DashboardSettings>("dashboard://settings", applySettings);
}
