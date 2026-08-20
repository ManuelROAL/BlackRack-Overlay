import "./styles.css";
import "./delta.css";
import { fitOverlay } from "./overlay-fit";
import { bindOverlayInteractionMode } from "./overlay-interaction";
import { createOverlayPerformanceTracker } from "./overlay-performance";
import { bindOverlayTransparency } from "./overlay-appearance";
import { readDeltaSettings, type DeltaSettings } from "./delta-settings";
import type { DeltaViewModel } from "./telemetry-types";
import { isTauriRuntime, listenRuntimeEvent, listenTelemetry } from "./runtime-events";

fitOverlay({ width: 420, height: 72 });
bindOverlayTransparency("delta");
bindOverlayInteractionMode();
const performance = createOverlayPerformanceTracker("delta");

let settings = readDeltaSettings();
const card = document.getElementById("delta-card");
const fill = document.getElementById("delta-fill") as HTMLElement | null;
const value = document.getElementById("delta-value");

const setText = (element: HTMLElement | null, text: string): void => {
  if (element && element.textContent !== text) element.textContent = text;
};

const render = (delta: DeltaViewModel): void => {
  const activeMode = delta.mode;
  const available = delta.available && activeMode !== "off";
  const state = activeMode === "off"
    ? "off"
    : !available
      ? "waiting"
      : !delta.current_lap_valid
        ? "invalid"
        : delta.seconds <= 0 ? "gain" : "loss";
  const barTrend = !delta.current_lap_valid ? "absolute" : delta.trend;
  if (card && card.dataset.state !== state) card.dataset.state = state;
  if (card && card.dataset.trend !== barTrend) card.dataset.trend = barTrend;
  setText(value, available ? `${delta.seconds >= 0 ? "+" : "−"}${Math.abs(delta.seconds).toFixed(3)}` : "---.---");
  if (fill) {
    const ratio = available ? Math.min(1, Math.abs(delta.seconds) / settings.displayRange) : 0;
    const width = `${ratio * 50}%`;
    const left = delta.seconds <= 0 ? "50%" : `${50 - ratio * 50}%`;
    if (fill.style.width !== width) fill.style.width = width;
    if (fill.style.left !== left) fill.style.left = left;
  }
};

void listenTelemetry((frame) => performance.measure(() => render(frame.delta_model)));

if (isTauriRuntime()) {
  void listenRuntimeEvent<DeltaSettings>("delta://settings", (next) => {
    settings = next;
  });
}
