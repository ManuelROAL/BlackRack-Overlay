import "./damage.css";
import type { TelemetryFrame } from "./telemetry-types";
import { fitOverlay } from "./overlay-fit";
import { bindOverlayInteractionMode } from "./overlay-interaction";
import { bindOverlayTransparency } from "./overlay-appearance";
import { createOverlayPerformanceTracker } from "./overlay-performance";
import { isTauriRuntime, listenTelemetry } from "./runtime-events";

const rows = {
  aero: document.querySelector<HTMLElement>('[data-damage-kind="aero"]')!,
  suspension: document.querySelector<HTMLElement>('[data-damage-kind="suspension"]')!,
  body: document.querySelector<HTMLElement>('[data-damage-kind="body"]')!,
  tires: document.querySelector<HTMLElement>('[data-damage-kind="tires"]')!
};
const values = Object.fromEntries(
  Object.entries(rows).map(([key, row]) => [key, row.querySelector<HTMLElement>("dd")!])
) as Record<keyof typeof rows, HTMLElement>;
const renderPerformance = createOverlayPerformanceTracker("damage");

const updateValue = (kind: keyof typeof rows, damage: number): void => {
  const available = Number.isFinite(damage) && damage >= 0;
  const value = available ? Math.max(0, Math.min(100, damage)) : -1;
  values[kind].textContent = available ? `${Math.round(value)}%` : "--%";
  rows[kind].dataset.state = !available
    ? "unavailable"
    : value >= 50 ? "critical" : value >= 15 ? "heavy" : value > 0 ? "warning" : "normal";
};

const updateTireWear = (wear: number): void => {
  const available = Number.isFinite(wear) && wear >= 0;
  const value = available ? Math.max(0, Math.min(100, wear)) : -1;
  values.tires.textContent = available ? `${Math.round(value)}%` : "--%";
  rows.tires.dataset.state = !available
    ? "unavailable"
    : value >= 75 ? "critical" : value >= 50 ? "heavy" : value >= 30 ? "warning" : "normal";
};

const render = (frame: TelemetryFrame): void => {
  updateValue("aero", frame.player_aero_damage_percent);
  updateValue("body", frame.player_body_damage_percent);
  updateValue("suspension", frame.player_suspension_damage_percent);

  const remainingByWheel = frame.player_tire_remaining_by_wheel_percent;
  const validRemaining = remainingByWheel.filter((remaining) => Number.isFinite(remaining) && remaining >= 0);
  const worstRemaining = validRemaining.length > 0 ? Math.min(...validRemaining) : -1;
  updateTireWear(worstRemaining >= 0 ? 100 - worstRemaining : -1);
};

const previewFrame = {
  player_aero_damage_percent: 4,
  player_suspension_damage_percent: 18,
  player_body_damage_percent: 7,
  player_tire_remaining_by_wheel_percent: [94, 94, 95, 95],
  player_tire_flat: [false, false, false, false],
  player_tire_detached: [false, false, false, false]
} as TelemetryFrame;

fitOverlay({ width: 94, height: 94 });
bindOverlayTransparency("damage");
bindOverlayInteractionMode();
if (!isTauriRuntime()) render(previewFrame);
void listenTelemetry((frame) => renderPerformance.measure(() => render(frame)));
