import "./pitstop.css";
import type { TelemetryFrame } from "./telemetry-types";
import { fitOverlay } from "./overlay-fit";
import { bindOverlayInteractionMode } from "./overlay-interaction";
import { bindOverlayTransparency } from "./overlay-appearance";
import { createOverlayPerformanceTracker } from "./overlay-performance";
import { isTauriRuntime, listenTelemetry } from "./runtime-events";

type PitStopValue = "damage" | "resource" | "tires" | "driver" | "penalty" | "total";

const values = Object.fromEntries(
  Array.from(document.querySelectorAll<HTMLElement>("[data-pitstop-value]")).map((element) => [
    element.dataset.pitstopValue as PitStopValue,
    element
  ])
) as Record<PitStopValue, HTMLElement>;
const resourceLabel = document.querySelector<HTMLElement>("[data-pitstop-resource]")!;
const penaltyRow = document.querySelector<HTMLElement>("[data-pitstop-penalty]")!;
const renderPerformance = createOverlayPerformanceTracker("pitstop");

const formatSeconds = (seconds: number, available = true): string =>
  available && Number.isFinite(seconds) ? `+${Math.max(0, seconds).toFixed(1)}s` : "--.-s";

const setText = (element: HTMLElement, text: string): void => {
  if (element.textContent !== text) element.textContent = text;
};

const render = (frame: TelemetryFrame): void => {
  const available = frame.pit_stop_estimate_available;
  const virtualEnergy = frame.virtual_energy_active;
  const penalty = frame.pit_stop_penalty_seconds;

  setText(resourceLabel, virtualEnergy ? "Energía virtual" : "Combustible");
  setText(values.damage, formatSeconds(frame.pit_stop_damage_seconds, available));
  setText(
    values.resource,
    formatSeconds(virtualEnergy ? frame.pit_stop_energy_seconds : frame.pit_stop_fuel_seconds, available)
  );
  setText(values.tires, formatSeconds(frame.pit_stop_tire_seconds, available));
  setText(values.driver, formatSeconds(frame.pit_stop_driver_swap_seconds, available));
  setText(values.penalty, formatSeconds(penalty, available));
  setText(values.total, formatSeconds(frame.pit_stop_estimate_seconds, available));
  penaltyRow.hidden = !available || penalty <= 0;
  document.body.dataset.available = available ? "true" : "false";
};

const previewFrame = {
  virtual_energy_active: true,
  pit_stop_estimate_available: true,
  pit_stop_estimate_seconds: 8.4,
  pit_stop_fuel_seconds: 0,
  pit_stop_energy_seconds: 8.4,
  pit_stop_tire_seconds: 0,
  pit_stop_damage_seconds: 0,
  pit_stop_penalty_seconds: 0,
  pit_stop_driver_swap_seconds: 0
} as TelemetryFrame;

fitOverlay({ width: 210, height: 150 });
bindOverlayTransparency("pitstop");
bindOverlayInteractionMode();
if (!isTauriRuntime()) render(previewFrame);
void listenTelemetry((frame) => renderPerformance.measure(() => render(frame)));
