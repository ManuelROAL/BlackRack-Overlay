import "./dashboard.css";
import batteryIconUrl from "./assets/lmu-icons/battery-empty-svgrepo-com.svg";
import { t } from "./i18n";
import type { TelemetryFrame } from "./telemetry-types";
import { fitOverlayToContentBox } from "./overlay-fit";
import { bindOverlayInteractionMode } from "./overlay-interaction";
import { bindOverlayTransparency } from "./overlay-appearance";
import { createOverlayPerformanceTracker } from "./overlay-performance";
import { isTauriRuntime, listenRuntimeEvent, listenTelemetry } from "./runtime-events";
import {
  RPM_LED_CRITICAL,
  RPM_LED_START,
  rpmLedBand,
  rpmLedIsActive,
  rpmLedState
} from "./rpm-leds";
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
const batteryField = fields.battery;
const batteryFill = document.getElementById("dashboard-battery-fill")!;
const batteryOutline = document.querySelector<HTMLImageElement>(".dash-hybrid-outline")!;
batteryOutline.src = batteryIconUrl;
const readout = document.querySelector<HTMLElement>(".dash-readout")!;
const row = value("fields");
const empty = value("empty");
const adjustment = value("adjustment");
const adjustmentLabel = value("adjustment-label");
const adjustmentValue = value("adjustment-value");
const adjustmentFields = (["map", "tc", "tcslip", "tccut", "abs", "bias"] as const).map((id) => ({
  id,
  label: fields[id].querySelector("dt")!,
  value: value(id),
  previous: ""
}));
let adjustmentContext = "";
let adjustmentUntil = 0;
let lastAdjustment: typeof adjustmentFields[number] | undefined;

const REV_LIGHTS = 12;
const lights: HTMLElement[] = [];
for (let index = 0; index < REV_LIGHTS; index += 1) {
  const light = document.createElement("i");
  light.dataset.lit = "false";
  // The band is fixed by the position on the strip, so it is written once.
  light.dataset.band = rpmLedBand(index, REV_LIGHTS);
  revs.append(light);
  lights.push(light);
}

const synchronizeOverlaySize = fitOverlayToContentBox(shell);

let settings = readDashboardSettings();
let lastFrame: TelemetryFrame | null = null;

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

const wiperStateLabel = (state: number): string => {
  switch (state) {
    case 0: return t("dashboard.wipersOff");
    case 1: return t("dashboard.wipersAuto");
    case 2: return t("dashboard.wipersSlow");
    case 3: return t("dashboard.wipersFast");
    default: return UNKNOWN;
  }
};

type HybridVisualState = "off" | "deploy" | "regen";

const hybridVisualState = (frame: TelemetryFrame): HybridVisualState => {
  if (!frame.hybrid_available) return "off";
  if (frame.hybrid_motor_state === 3) return "regen";
  if (frame.hybrid_motor_state === 2) return "deploy";
  return "off";
};

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

/** The gear ring follows the same two thresholds the shift lights use. */
const revLevel = (ratio: number): string =>
  ratio >= RPM_LED_CRITICAL ? "limit" : ratio >= RPM_LED_START ? "high" : "normal";

/**
 * Half a blink period. Read from the clock on each telemetry update rather than
 * kept on a timer of its own: the state still changes only when a frame
 * arrives, so the host keeps repainting from data, and the rate stays the same
 * across performance profiles even though their cadences differ.
 */
const WARNING_BLINK_MS = 450;

/**
 * The limiter wins when both are engaged. They only overlap on the way into the
 * pits, where the limiter is the one with a penalty attached.
 */
const warningKind = (frame: TelemetryFrame): "limiter" | "liftcoast" | "none" => {
  if (settings.visible.limiter && frame.speed_limiter_active) return "limiter";
  if (settings.visible.liftcoast && frame.lift_and_coast_progress > 0) return "liftcoast";
  return "none";
};

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
    case "abs": return frame.car_electronics_available && frame.anti_lock_brakes_max > 0;
    case "battery": return frame.hybrid_available;
    case "headlights": case "wipers": return frame.capabilities.car_electronics;
    case "energy": return frame.virtual_energy_active;
    case "delta": return frame.delta_model.available;
    case "lastlap": case "bestlap": case "predicted": return frame.timing_model.available;
    default: return true;
  }
};

const renderValues = (frame: TelemetryFrame): void => {
  setText(gear, gearLabel(frame.gear));
  const revState = rpmLedState(frame.rpm, frame.max_rpm, REV_LIGHTS);
  for (let index = 0; index < REV_LIGHTS; index += 1) {
    const light = lights[index];
    setState(light, "lit", rpmLedIsActive(index, REV_LIGHTS, revState) ? "true" : "false");
    setState(light, "critical", revState.visible && revState.critical ? "true" : "false");
    setState(light, "overRev", revState.visible && revState.overRev ? "true" : "false");
  }
  setState(gear, "rev", revLevel(revState.ratio));

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
  setText(value("headlights"), frame.headlights_on ? t("dashboard.statusOn") : t("dashboard.statusOff"));
  const range = frame.resource_autonomy.range_laps;
  setText(value("laps"), range == null ? "--" : rounded(range, 1));
  const batteryPercent = Number.isFinite(frame.battery_charge_percent)
    ? Math.max(0, Math.min(100, frame.battery_charge_percent))
    : 0;
  batteryFill.style.width = `${(62.5 * batteryPercent / 100).toFixed(2)}%`;
  const state = hybridVisualState(frame);
  setState(batteryField, "hybridState", state);

  setText(value("map"), level(frame.engine_map, frame.engine_map_max));
  setText(value("tc"), level(frame.traction_control_level, frame.traction_control_max));
  setText(value("tcslip"), level(frame.traction_control_slip, frame.traction_control_slip_max));
  setText(value("tccut"), level(frame.traction_control_cut, frame.traction_control_cut_max));
  setText(value("abs"), level(frame.anti_lock_brakes_level, frame.anti_lock_brakes_max));
  setText(value("bias"), `${frame.brake_bias_percent.toFixed(1)}%`);
  setText(value("air"), rounded(frame.ambient_temperature_c, 0, "°"));
  setText(value("track"), rounded(frame.track_temperature_c, 0, "°"));
  setText(value("wipers"), wiperStateLabel(frame.wiper_state));
};

const renderAdjustment = (frame: TelemetryFrame): void => {
  const context = JSON.stringify([
    frame.source, frame.track_name, frame.player_vehicle_name, frame.session_type
  ]);
  const reset = !frame.player_active || context !== adjustmentContext;
  if (reset) {
    adjustmentUntil = 0;
    lastAdjustment = undefined;
  }
  const now = performance.now();
  for (const field of adjustmentFields) {
    const current = frame.player_active && available(field.id, frame)
      ? field.value.textContent ?? "" : "";
    const valid = current !== "" && !current.includes("NaN") && !current.includes("--")
      && !current.includes("Infinity") && !current.startsWith("-");
    if (!reset && valid && field.previous !== "" && current !== field.previous) {
      lastAdjustment = field;
      adjustmentUntil = now + 3000;
    }
    field.previous = valid ? current : "";
  }
  adjustmentContext = frame.player_active ? context : "";
  const shown = now < adjustmentUntil && lastAdjustment !== undefined;
  if (shown && lastAdjustment) {
    setText(adjustmentLabel, lastAdjustment.label.textContent ?? "");
    setText(adjustmentValue, lastAdjustment.value.textContent ?? "");
  }
  toggle(adjustment, shown);
};

const render = (frame: TelemetryFrame): void => {
  lastFrame = frame;
  const live = frame.player_active;
  if (live) renderValues(frame);
  renderAdjustment(frame);

  let rowFields = 0;
  for (const { id } of DASHBOARD_FIELDS) {
    // The limiter warning has no readout of its own; it lights something that
    // is already on screen, so there is no element to toggle here.
    const element = fields[id];
    if (!element) continue;
    const shown = live && settings.visible[id] && available(id, frame);
    toggle(element, shown);
    if (shown && id !== "gear" && id !== "revs") rowFields += 1;
  }
  const gearShown = live && settings.visible.gear;
  const revsShown = live && settings.visible.revs;

  const warning = live ? warningKind(frame) : "none";
  const lit = warning !== "none"
    && Math.floor(performance.now() / WARNING_BLINK_MS) % 2 === 0;
  setState(gear, "warning", lit && gearShown ? warning : "none");
  const warnWholeOverlay = !gearShown
    || (warning === "limiter" && settings.pitWarningTarget === "overlay");
  setState(shell, "warning", lit && warnWholeOverlay ? warning : "none");
  setState(shell, "gear", gearShown ? "on" : "off");
  toggle(row, rowFields > 0);
  toggle(readout, rowFields > 0 || revsShown);
  toggle(empty, !gearShown && rowFields === 0 && !revsShown);
  synchronizeOverlaySize();
};

const applySettings = (next: DashboardSettings): void => {
  settings = next;
  if (lastFrame) {
    render(lastFrame);
    return;
  }
  for (const { id } of DASHBOARD_FIELDS) {
    if (fields[id]) toggle(fields[id], settings.visible[id]);
  }
  synchronizeOverlaySize();
};

applySettings(settings);

const previewFrame = {
  capabilities: { car_electronics: true },
  player_active: true,
  gear: 6,
  speed_kph: 255,
  rpm: 8464,
  max_rpm: 9200,
  player_position: 12,
  player_class_position: 12,
  player_class_size: 20,
  lap_number: 37,
  session_max_laps: 0,
  session_time_remaining: 3 * 3600 + 42 * 60 + 15,
  fuel_liters: 46.3,
  estimated_fuel_laps: 17.5,
  resource_autonomy: { fuel_laps: 17.5, energy_laps: 15.2, range_laps: 15.2 },
  virtual_energy_active: true,
  virtual_energy_percent: 58,
  headlights_on: true,
  wiper_state: 2,
  estimated_virtual_energy_laps: 15.2,
  hybrid_available: true,
  battery_charge_percent: 62,
  hybrid_regen_kw: 84,
  hybrid_motor_state: 3,
  lift_and_coast_progress: 0,
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
if (!isTauriRuntime()) {
  render(previewFrame);
  adjustmentContext = "";
}
void listenTelemetry((frame) => renderPerformance.measure(() => render(frame)));
if (isTauriRuntime()) {
  void listenRuntimeEvent<DashboardSettings>("dashboard://settings", applySettings);
}
