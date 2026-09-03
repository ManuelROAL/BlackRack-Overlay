import "./dashboard.css";
import type { TelemetryFrame } from "./telemetry-types";
import { fitOverlayToContent } from "./overlay-fit";
import { bindOverlayInteractionMode } from "./overlay-interaction";
import { bindOverlayTransparency } from "./overlay-appearance";
import { createOverlayPerformanceTracker } from "./overlay-performance";
import { isTauriRuntime, listenRuntimeEvent, listenTelemetry } from "./runtime-events";
import { t } from "./i18n";
import {
  DASHBOARD_OPTIONS,
  readDashboardSettings,
  type DashboardOptionId,
  type DashboardSettings
} from "./dashboard-settings";

type DashboardValueId = Exclude<DashboardOptionId, "status">;

bindOverlayTransparency("dashboard");
const renderPerformance = createOverlayPerformanceTracker("dashboard");

const values = Object.fromEntries(
  Array.from(document.querySelectorAll<HTMLElement>("[data-dashboard-value]")).map((element) => [
    element.dataset.dashboardValue as DashboardValueId,
    element
  ])
) as Record<DashboardValueId, HTMLElement>;
const cells = Object.fromEntries(
  Array.from(document.querySelectorAll<HTMLElement>("[data-dashboard-part]")).map((element) => [
    element.dataset.dashboardPart as DashboardOptionId,
    element
  ])
) as Record<DashboardOptionId, HTMLElement>;
const shell = document.querySelector<HTMLElement>(".dashboard-shell")!;
const electronics = document.querySelector<HTMLElement>("[data-dashboard-section='electronics']")!;
const hybrid = document.querySelector<HTMLElement>("[data-dashboard-section='hybrid']")!;
const status = document.querySelector<HTMLElement>("[data-dashboard-section='status']")!;
const battery = document.querySelector<HTMLElement>("[data-dashboard-part='battery']")!;
const batteryFill = document.getElementById("dashboard-battery-fill")!;
const motorState = document.getElementById("dashboard-motor-state")!;
const limiter = document.getElementById("dashboard-limiter")!;
const headlights = document.getElementById("dashboard-headlights")!;
const wipers = document.getElementById("dashboard-wipers")!;
const motorValues = document.querySelector<HTMLElement>(".dashboard-hybrid-values")!;
const empty = document.getElementById("dashboard-empty")!;
// Sections come and go with the car and with the driver's shortlist, so the
// panel is only as tall as what it is actually drawing.
const synchronizeOverlayHeight = fitOverlayToContent(300, shell);

const ELECTRONICS_VALUES: DashboardValueId[] = [
  "map", "tc", "tcslip", "tccut", "abs", "bias", "migration", "arb"
];
const HYBRID_VALUES: DashboardValueId[] = ["battery", "regen", "motor"];
const MOTOR_VALUES: DashboardValueId[] = ["regen", "motor"];
const MAXIMUM_COLUMNS = 4;

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

/**
 * A rotary the driver has just turned is the value worth finding again, so the
 * cell that changed stays highlighted for a bounded number of telemetry
 * updates. The decay is counted in frames rather than kept on a timer: the
 * native host has to repaint from telemetry, never on its own clock.
 */
const CHANGE_HIGHLIGHT_FRAMES = 40;
const previous = new Map<DashboardValueId, string>();
const highlighted = new Map<DashboardValueId, number>();

const publish = (id: DashboardValueId, text: string): void => {
  const element = values[id];
  const cell = cells[id];
  const before = previous.get(id);
  previous.set(id, text);
  setText(element, text);
  if (before !== undefined && before !== text && text !== "--") {
    highlighted.set(id, CHANGE_HIGHLIGHT_FRAMES);
    setState(cell, "changed", "true");
  }
};

const decayHighlights = (): void => {
  for (const [id, remaining] of highlighted) {
    if (remaining > 1) {
      highlighted.set(id, remaining - 1);
      continue;
    }
    highlighted.delete(id);
    setState(cells[id], "changed", "false");
  }
};

/**
 * The grid wraps, so the column count decides how the rows break. Spreading the
 * visible values over as few rows as fit keeps the last row the same width as
 * the one above instead of leaving one value stretched across the panel.
 */
const applyColumns = (): void => {
  const visible = ELECTRONICS_VALUES.filter((id) => settings.visible[id]).length;
  if (visible === 0) return;
  const rows = Math.ceil(visible / MAXIMUM_COLUMNS);
  electronics.style.setProperty("--dashboard-columns", String(Math.ceil(visible / rows)));
};

const applySettings = (next: DashboardSettings): void => {
  settings = next;
  for (const { id } of DASHBOARD_OPTIONS) toggle(cells[id], settings.visible[id]);
  toggle(motorValues, MOTOR_VALUES.some((id) => settings.visible[id]));
  applyColumns();
};

applySettings(settings);

const level = (value: number, max: number): string =>
  max > 0 ? `${value}/${max}` : `${value}`;

const MOTOR_STATES = ["inactive", "inactive", "propulsion", "regeneration"] as const;
const MOTOR_STATE_KEYS = [
  "dashboard.stateInactive",
  "dashboard.stateInactive",
  "dashboard.statePropulsion",
  "dashboard.stateRegeneration"
] as const;

const batteryLevel = (percent: number): string =>
  percent < 15 ? "low" : percent < 40 ? "medium" : "ok";

const render = (frame: TelemetryFrame): void => {
  // A source without these systems never sets the availability flags, so they
  // are the only gate the panel needs; the control panel is what refuses the
  // overlay outright on a simulator missing the capability.
  const electronicsAvailable = frame.car_electronics_available;
  const hybridAvailable = frame.hybrid_available;

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
    const width = `${charge.toFixed(1)}%`;
    if (batteryFill.style.width !== width) batteryFill.style.width = width;
    setState(battery, "level", batteryLevel(charge));
    publish("regen", `${Math.round(Math.max(0, frame.hybrid_regen_kw))} kW`);
    publish("motor", `${Math.round(frame.hybrid_motor_temperature_c)}°`);
    const motor = Math.max(0, Math.min(3, Math.round(frame.hybrid_motor_state)));
    setState(motorState, "state", MOTOR_STATES[motor]);
    setText(motorState, t(MOTOR_STATE_KEYS[motor]));
    toggle(motorState, motor > 0);
  }

  setState(limiter, "state", frame.speed_limiter_active ? "on" : "off");
  setState(headlights, "state", frame.headlights_on ? "on" : "off");
  setState(wipers, "state", frame.wiper_state > 0 ? "on" : "off");

  const electronicsVisible = electronicsAvailable
    && ELECTRONICS_VALUES.some((id) => settings.visible[id]);
  const hybridVisible = hybridAvailable && HYBRID_VALUES.some((id) => settings.visible[id]);
  const statusVisible = electronicsAvailable && settings.visible.status;
  toggle(electronics, electronicsVisible);
  toggle(hybrid, hybridVisible);
  toggle(status, statusVisible);
  toggle(empty, !electronicsVisible && !hybridVisible && !statusVisible);
  decayHighlights();
  synchronizeOverlayHeight();
};

const previewFrame = {
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
  hybrid_motor_temperature_c: 63
} as TelemetryFrame;

bindOverlayInteractionMode();
if (!isTauriRuntime()) render(previewFrame);
void listenTelemetry((frame) => renderPerformance.measure(() => render(frame)));
if (isTauriRuntime()) {
  void listenRuntimeEvent<DashboardSettings>("dashboard://settings", applySettings);
}
