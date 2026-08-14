import "./styles.css";
import "./delta.css";
import { fitOverlay } from "./overlay-fit";
import { bindOverlayInteractionMode } from "./overlay-interaction";
import { createOverlayPerformanceTracker } from "./overlay-performance";
import { bindOverlayTransparency } from "./overlay-appearance";
import { readDeltaSettings, type DeltaMode, type DeltaSettings } from "./delta-settings";
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
const mode = document.getElementById("delta-mode");
const sector = document.getElementById("delta-sector");
const reference = document.getElementById("delta-reference");

const labels: Record<DeltaMode, string> = {
  off: "DESACTIVADO",
  overall_best: "MEJOR GLOBAL",
  overall_optimal_lap: "ÓPTIMA GLOBAL",
  overall_optimal_sectors: "SECTORES GLOBAL",
  session_best: "MEJOR SESIÓN",
  session_optimal_lap: "ÓPTIMA SESIÓN",
  session_optimal_sectors: "SECTORES SESIÓN",
  stint_best: "MEJOR STINT",
  last_lap: "ÚLTIMA VUELTA"
};

const setText = (element: HTMLElement | null, text: string): void => {
  if (element && element.textContent !== text) element.textContent = text;
};

const lapTime = (seconds: number): string => {
  if (!Number.isFinite(seconds) || seconds <= 0) return "--:--.---";
  const minutes = Math.floor(seconds / 60);
  return `${minutes}:${(seconds - minutes * 60).toFixed(3).padStart(6, "0")}`;
};

const render = (delta: DeltaViewModel): void => {
  const activeMode = delta.mode in labels ? delta.mode : settings.mode;
  setText(mode, labels[activeMode]);
  setText(reference, lapTime(delta.reference_seconds));
  setText(
    sector,
    delta.sector_count > 0 ? `${delta.sector_index + 1}/${delta.sector_count}` : "--/--"
  );
  const available = delta.available && activeMode !== "off";
  const state = activeMode === "off"
    ? "off"
    : !available
      ? "waiting"
      : !delta.current_lap_valid
        ? "invalid"
        : delta.seconds <= 0 ? "gain" : "loss";
  if (card && card.dataset.state !== state) card.dataset.state = state;
  card?.classList.toggle("frozen", delta.frozen);
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
