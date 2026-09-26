import "./styles.css";
import "./driving.css";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { fitOverlay } from "./overlay-fit";
import { bindOverlayInteractionMode } from "./overlay-interaction";
import { bindOverlayTransparency } from "./overlay-appearance";
import {
  DRIVING_PEDALS,
  readDrivingSettings,
  type DrivingPedalId,
  type DrivingSettings
} from "./driving-settings";
import type { TelemetryFrame } from "./telemetry-types";
import {
  isCompositeOverlay,
  isTauriRuntime,
  listenRuntimeEvent,
  listenTelemetry
} from "./runtime-events";
import { rpmLedIsActive, rpmLedState } from "./rpm-leds";
import { applyDisplayUnits, formatSpeedValue, speedUnit, type DisplayUnits } from "./display-units";

const BASE_HEIGHT = 120;
const RPM_LEDS_HEIGHT = 14;
const TELEMETRY_RATE_HZ = 50;
const HISTORY_SECONDS = 5;
const HISTORY_SIZE = TELEMETRY_RATE_HZ * HISTORY_SECONDS;
const history = {
  throttle: [] as number[],
  brake: [] as number[],
  clutch: [] as number[],
  tc: [] as boolean[],
  abs: [] as boolean[],
};
const canvas = document.getElementById("trailing-canvas") as HTMLCanvasElement;
const context = canvas.getContext("2d");
const shell = document.querySelector<HTMLElement>(".driving-shell");
const driveDial = document.querySelector<HTMLElement>(".drive-dial");
const speedReadout = document.getElementById("speed")!;
const trailingPanel = document.querySelector<HTMLElement>(".trailing-panel");
const pedalPanel = document.querySelector<HTMLElement>(".pedal-panel");
const rpmLeds = Array.from(document.querySelectorAll<HTMLElement>(".rpm-leds i"));
const elements = new Map<string, HTMLElement>();
for (const element of document.querySelectorAll<HTMLElement>("[id]")) {
  elements.set(element.id, element);
}
let settings = readDrivingSettings();
let graphRenderPhase = 0;
let latestSpeedKph: number | null = null;

const visiblePedalCount = (values: Record<DrivingPedalId, boolean>): number =>
  DRIVING_PEDALS.filter(({ id }) => values[id]).length;

const drivingWidth = (): number => {
  const graphWidth = settings.showGraph && visiblePedalCount(settings.graphPedals) > 0 ? 278 : 0;
  const inputCount = visiblePedalCount(settings.inputPedals);
  const inputWidth = inputCount > 0 ? 13 + inputCount * 25 : 0;
  const dialWidth = settings.showSteering || settings.showGear
    || settings.showSpeed || settings.showForceFeedback ? 88 : 0;
  const widths = [dialWidth, inputWidth, graphWidth].filter((width) => width > 0);
  return Math.max(120, 14 + widths.reduce((total, width) => total + width, 0));
};

const drivingHeight = (): number => BASE_HEIGHT + (settings.showRpmLeds ? RPM_LEDS_HEIGHT : 0);
const updateOverlayFit = fitOverlay({ width: drivingWidth(), height: drivingHeight() });
bindOverlayTransparency("driving");

const resizeHandle = document.querySelector<HTMLElement>("[data-resize-handle]");
resizeHandle?.addEventListener("mousedown", (event) => {
  event.preventDefault();
  event.stopPropagation();
  if (isTauriRuntime() && !isCompositeOverlay()) {
    void getCurrentWindow().startResizeDragging("SouthEast").catch((error) => {
      console.error("No se pudo iniciar el redimensionado del overlay:", error);
    });
  }
});

const text = (id: string, value: string): void => {
  const element = elements.get(id);
  if (element && element.textContent !== value) element.textContent = value;
};

const setLevel = (id: string, value: number): void => {
  const element = elements.get(id);
  if (element) {
    const height = `${Math.round(Math.max(0, Math.min(1, value)) * 100)}%`;
    if (element.style.height !== height) element.style.height = height;
  }
};

const setForceFeedback = (value: number): void => {
  const element = elements.get("ffb-level");
  if (!element) return;
  const force = Number.isFinite(value) ? Math.max(-1, Math.min(1, value)) : 0;
  const left = `${Math.round((force < 0 ? 50 + force * 50 : 50) * 10) / 10}%`;
  const width = `${Math.round(Math.abs(force) * 500) / 10}%`;
  if (element.style.left !== left) element.style.left = left;
  if (element.style.width !== width) element.style.width = width;
};

const setSteering = (angle: number): void => {
  const safeAngle = Number.isFinite(angle) ? angle : 0;
  const wheel = elements.get("steering-wheel");
  if (wheel) {
    const transform = `rotate(${Math.round(safeAngle * 10) / 10}deg)`;
    if (wheel.style.transform !== transform) wheel.style.transform = transform;
  }
};

const setRpmLeds = (rpm: number, maxRpm: number): void => {
  if (!settings.showRpmLeds) return;
  const state = rpmLedState(rpm, maxRpm, rpmLeds.length);
  rpmLeds.forEach((led, index) => {
    led.classList.toggle("active", rpmLedIsActive(index, rpmLeds.length, state));
    led.classList.toggle("critical", state.visible && state.critical);
    led.classList.toggle("over-rev", state.visible && state.overRev);
  });
};

const push = (values: number[], value: number): void => {
  values.push(Math.max(0, Math.min(1, value)));
  if (values.length > HISTORY_SIZE) values.shift();
};

const drawLine = (values: number[], color: string): void => {
  if (!context || values.length < 2) return;
  context.beginPath();
  values.forEach((value, index) => {
    const x = (index / (HISTORY_SIZE - 1)) * canvas.width;
    const y = canvas.height - value * (canvas.height - 4) - 2;
    if (index === 0) context.moveTo(x, y);
    else context.lineTo(x, y);
  });
  context.strokeStyle = color;
  context.lineWidth = 2;
  context.stroke();
};

const drawActivations = (values: number[], active: boolean[], color: string): void => {
  if (!context) return;
  context.fillStyle = color;
  active.forEach((isActive, index) => {
    if (!isActive || index >= values.length) return;
    const x = (index / (HISTORY_SIZE - 1)) * canvas.width;
    const y = canvas.height - values[index] * (canvas.height - 4) - 2;
    context.beginPath();
    context.arc(x, y, 2.6, 0, Math.PI * 2);
    context.fill();
  });
};

const drawTrailing = (): void => {
  if (!context) return;
  context.clearRect(0, 0, canvas.width, canvas.height);
  if (!settings.showGraph || visiblePedalCount(settings.graphPedals) === 0) return;
  context.strokeStyle = "rgba(255,255,255,.08)";
  context.lineWidth = 1;
  for (const y of [0.25, 0.5, 0.75]) {
    context.beginPath();
    context.moveTo(0, canvas.height * y);
    context.lineTo(canvas.width, canvas.height * y);
    context.stroke();
  }
  // Canvas paints later strokes on top: clutch < brake < throttle.
  if (settings.graphPedals.clutch) drawLine(history.clutch, "#62cce9");
  if (settings.graphPedals.brake) drawLine(history.brake, "#ff5367");
  if (settings.graphPedals.throttle) drawLine(history.throttle, "#55ef93");
  if (settings.graphPedals.brake) drawActivations(history.brake, history.abs, "#ffd400");
  if (settings.graphPedals.throttle) drawActivations(history.throttle, history.tc, "#2448ff");
};

const applySettings = (next: DrivingSettings): void => {
  settings = next;
  const graphCount = visiblePedalCount(settings.graphPedals);
  const inputCount = visiblePedalCount(settings.inputPedals);
  if (trailingPanel) trailingPanel.hidden = !settings.showGraph || graphCount === 0;
  const order = (position: DrivingSettings["graphPosition"]): string =>
    String(({ left: 1, center: 2, right: 3 })[position]);
  if (trailingPanel) trailingPanel.style.order = order(settings.graphPosition);
  if (pedalPanel) pedalPanel.style.order = order(settings.inputPosition);
  if (driveDial) driveDial.style.order = order(settings.dataPosition);
  for (const { id } of DRIVING_PEDALS) {
    const input = document.querySelector<HTMLElement>(`[data-input-pedal="${id}"]`);
    if (input) input.hidden = !settings.inputPedals[id];
    const label = input?.querySelector<HTMLElement>("span");
    if (label) label.hidden = !settings.showPedalLabels;
  }
  if (pedalPanel) {
    pedalPanel.hidden = inputCount === 0;
    pedalPanel.style.width = `${13 + inputCount * 25}px`;
  }
  for (const element of document.querySelectorAll<HTMLElement>('[data-driving-item="gear"]')) {
    element.hidden = !settings.showGear;
  }
  for (const element of document.querySelectorAll<HTMLElement>('[data-driving-item="speed"]')) {
    element.hidden = !settings.showSpeed;
  }
  for (const element of document.querySelectorAll<HTMLElement>('[data-driving-item="ffb"]')) {
    element.hidden = !settings.showForceFeedback;
  }
  for (const element of document.querySelectorAll<HTMLElement>('[data-driving-item="steering"]')) {
    element.hidden = !settings.showSteering;
  }
  if (driveDial) {
    driveDial.hidden = !settings.showGear && !settings.showSteering
      && !settings.showSpeed && !settings.showForceFeedback;
  }
  const orderedPanels = [driveDial, pedalPanel, trailingPanel]
    .filter((panel): panel is HTMLElement => panel !== null && !panel.hidden)
    .sort((left, right) => Number(left.style.order || 0) - Number(right.style.order || 0));
  for (const [index, panel] of orderedPanels.entries()) {
    panel.style.borderLeft = index === 0 ? "0" : "1px solid rgb(255 255 255 / 11%)";
  }
  const rpmLedStrip = elements.get("rpm-leds");
  if (rpmLedStrip) rpmLedStrip.hidden = !settings.showRpmLeds;
  shell?.classList.toggle("has-rpm-leds", settings.showRpmLeds);
  const width = drivingWidth();
  const height = drivingHeight();
  if (shell) {
    shell.style.width = `${width}px`;
    shell.style.height = `${height}px`;
  }
  updateOverlayFit({ width, height });
  drawTrailing();
};

const render = (frame: TelemetryFrame): void => {
  latestSpeedKph = frame.speed_kph;
  const throttle = Math.max(0, Math.min(1, frame.throttle));
  const brake = Math.max(0, Math.min(1, frame.brake));
  const clutch = Math.max(0, Math.min(1, frame.clutch));
  push(history.throttle, throttle);
  push(history.brake, brake);
  push(history.clutch, clutch);
  history.tc.push(frame.tc_active);
  history.abs.push(frame.abs_active);
  if (history.tc.length > HISTORY_SIZE) history.tc.shift();
  if (history.abs.length > HISTORY_SIZE) history.abs.shift();
  graphRenderPhase = (graphRenderPhase + 1) % 2;
  if (graphRenderPhase === 0) drawTrailing();
  text("throttle-value", `${Math.round(throttle * 100)}`);
  text("brake-value", `${Math.round(brake * 100)}`);
  text("clutch-value", `${Math.round(clutch * 100)}`);
  text("speed", formatSpeedValue(frame.speed_kph));
  speedReadout.dataset.unit = speedUnit();
  text("gear", frame.gear < 0 ? "R" : frame.gear === 0 ? "N" : `${frame.gear}`);
  setLevel("throttle-level", throttle);
  setLevel("brake-level", brake);
  setLevel("clutch-level", clutch);
  setForceFeedback(frame.force_feedback);
  setSteering(frame.steering_angle_degrees);
  setRpmLeds(frame.rpm, frame.max_rpm);
};

void listenTelemetry(render);
void listenRuntimeEvent<DrivingSettings>("driving://settings", applySettings);
void listenRuntimeEvent<DisplayUnits>("display-units://change", (next) => {
  applyDisplayUnits(next);
  speedReadout.dataset.unit = speedUnit();
  if (latestSpeedKph !== null) text("speed", formatSpeedValue(latestSpeedKph));
});
applySettings(settings);
bindOverlayInteractionMode();
