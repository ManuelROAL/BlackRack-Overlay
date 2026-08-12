import "./styles.css";
import "./driving.css";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { fitOverlay } from "./overlay-fit";
import { bindOverlayInteractionMode } from "./overlay-interaction";
import { bindOverlayTransparency } from "./overlay-appearance";
import type { TelemetryFrame } from "./telemetry-types";
import { isCompositeOverlay, isTauriRuntime, listenTelemetry } from "./runtime-events";

const WIDTH = 440;
const HEIGHT = 120;
const HISTORY_SIZE = 80;
const history = {
  throttle: [] as number[],
  brake: [] as number[],
  clutch: [] as number[],
  tc: [] as boolean[],
  abs: [] as boolean[],
};
const canvas = document.getElementById("trailing-canvas") as HTMLCanvasElement;
const context = canvas.getContext("2d");

fitOverlay({ width: WIDTH, height: HEIGHT });
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
  const element = document.getElementById(id);
  if (element && element.textContent !== value) element.textContent = value;
};

const setLevel = (id: string, value: number): void => {
  const element = document.getElementById(id);
  if (element) element.style.height = `${Math.round(Math.max(0, Math.min(1, value)) * 100)}%`;
};

const setForceFeedback = (value: number): void => {
  const element = document.getElementById("ffb-level");
  if (!element) return;
  const force = Number.isFinite(value) ? Math.max(-1, Math.min(1, value)) : 0;
  element.style.left = `${(force < 0 ? 0.5 + force * 0.5 : 0.5) * 100}%`;
  element.style.width = `${Math.abs(force) * 50}%`;
};

const setSteering = (angle: number): void => {
  const safeAngle = Number.isFinite(angle) ? angle : 0;
  const wheel = document.getElementById("steering-wheel");
  if (wheel) wheel.style.transform = `rotate(${safeAngle}deg)`;
  text("steering-angle", `${Math.round(safeAngle)}°`);
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
  context.strokeStyle = "rgba(255,255,255,.08)";
  context.lineWidth = 1;
  for (const y of [0.25, 0.5, 0.75]) {
    context.beginPath();
    context.moveTo(0, canvas.height * y);
    context.lineTo(canvas.width, canvas.height * y);
    context.stroke();
  }
  // Canvas paints later strokes on top: clutch < brake < throttle.
  drawLine(history.clutch, "#62cce9");
  drawLine(history.brake, "#ff5367");
  drawLine(history.throttle, "#55ef93");
  drawActivations(history.brake, history.abs, "#f4f7fa");
  drawActivations(history.throttle, history.tc, "#ffd24a");
};

const render = (frame: TelemetryFrame): void => {
  const throttle = Math.max(0, Math.min(1, frame.throttle));
  const brake = Math.max(0, Math.min(1, frame.brake));
  // The shared-memory contract does not expose clutch input yet.
  const clutch = 0;
  push(history.throttle, throttle);
  push(history.brake, brake);
  push(history.clutch, clutch);
  history.tc.push(frame.tc_active);
  history.abs.push(frame.abs_active);
  if (history.tc.length > HISTORY_SIZE) history.tc.shift();
  if (history.abs.length > HISTORY_SIZE) history.abs.shift();
  drawTrailing();
  text("throttle-value", `${Math.round(throttle * 100)}`);
  text("brake-value", `${Math.round(brake * 100)}`);
  text("clutch-value", `${Math.round(clutch * 100)}`);
  text("speed", `${Math.round(frame.speed_kph)}`);
  text("gear", frame.gear < 0 ? "R" : frame.gear === 0 ? "N" : `${frame.gear}`);
  setLevel("throttle-level", throttle);
  setLevel("brake-level", brake);
  setLevel("clutch-level", clutch);
  setForceFeedback(frame.force_feedback);
  setSteering(frame.steering_angle_degrees);
};

void listenTelemetry(render);
bindOverlayInteractionMode();
