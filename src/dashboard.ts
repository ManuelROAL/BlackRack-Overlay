import "./styles.css";
import "./dashboard.css";
import { fitOverlay } from "./overlay-fit";
import { bindOverlayInteractionMode } from "./overlay-interaction";
import { createOverlayPerformanceTracker } from "./overlay-performance";
import { bindOverlayTransparency } from "./overlay-appearance";
import type { InteractionMode, TelemetryFrame } from "./telemetry-types";
import { listenTelemetry } from "./runtime-events";

fitOverlay({ width: 780, height: 340 });
bindOverlayTransparency("dashboard");
const renderPerformance = createOverlayPerformanceTracker("dashboard");

const text = (id: string, value: string): void => {
  const element = document.getElementById(id);
  if (element && element.textContent !== value) element.textContent = value;
};

const width = (id: string, ratio: number): void => {
  const element = document.getElementById(id);
  if (element) {
    const safeRatio = Math.max(0, Math.min(1, ratio));
    const value = `${Math.round(safeRatio * 1_000) / 10}%`;
    if (element.style.width !== value) element.style.width = value;
  }
};

const formatLapTime = (seconds: number): string => {
  const minutes = Math.floor(seconds / 60);
  const remaining = seconds - minutes * 60;
  return `${minutes.toString().padStart(2, "0")}:${remaining
    .toFixed(3)
    .padStart(6, "0")}`;
};

const render = (frame: TelemetryFrame): void => {
  document.querySelector(".fuel-widget")?.classList.toggle("energy-mode", frame.virtual_energy_active);
  text("speed", Math.round(frame.speed_kph).toString().padStart(3, "0"));
  text("gear", frame.gear < 0 ? "R" : frame.gear === 0 ? "N" : frame.gear.toString());
  text("rpm", `${Math.round(frame.rpm).toLocaleString("es-ES")} RPM`);
  text("throttle", `${Math.round(frame.throttle * 100)}%`);
  text("brake", `${Math.round(frame.brake * 100)}%`);
  if (frame.virtual_energy_active) {
    text("dashboard-resource-label", "ENERGÍA VIRTUAL");
    text("dashboard-resource-unit", "%");
    text("dashboard-range-label", "AUTONOMÍA NRG");
    text("fuel", frame.virtual_energy_percent.toFixed(1));
    text(
      "fuel-laps",
      frame.estimated_virtual_energy_laps > 0
        ? frame.estimated_virtual_energy_laps.toFixed(1)
        : "--"
    );
  } else {
    text("dashboard-resource-label", "COMBUSTIBLE");
    text("dashboard-resource-unit", "L");
    text("dashboard-range-label", "VUELTAS EST.");
    text("fuel", frame.fuel_liters.toFixed(1));
    text("fuel-laps", frame.estimated_fuel_laps > 0 ? frame.estimated_fuel_laps.toFixed(1) : "--");
  }
  text("lap-time", formatLapTime(frame.current_lap_seconds));
  text("best-lap", formatLapTime(frame.best_lap_seconds));
  text(
    "lap-delta",
    `${frame.lap_delta_seconds >= 0 ? "+" : "−"}${Math.abs(frame.lap_delta_seconds).toFixed(3)}`
  );
  text("source-name", frame.source.toUpperCase());

  const sourceStatus = document.getElementById("source-status");
  const connection = sourceStatus?.parentElement;
  if (frame.source === "mock") {
    text("source-status", "SIMULACIÓN");
    connection?.setAttribute("data-state", "mock");
  } else if (!frame.connected) {
    text("source-status", "ESPERANDO LMU");
    connection?.setAttribute("data-state", "offline");
  } else if (!frame.player_active) {
    text("source-status", "LMU · SIN COCHE");
    connection?.setAttribute("data-state", "standby");
  } else {
    text("source-status", "TELEMETRÍA LMU");
    connection?.setAttribute("data-state", "live");
  }

  width("rpm-bar", frame.rpm / frame.max_rpm);
  width("throttle-bar", frame.throttle);
  width("brake-bar", frame.brake);
  width(
    "fuel-bar",
    frame.virtual_energy_active
      ? frame.virtual_energy_percent / 100
      : frame.fuel_liters / frame.fuel_capacity_liters
  );

  const delta = document.getElementById("lap-delta");
  delta?.classList.toggle("positive", frame.lap_delta_seconds <= 0);
  delta?.classList.toggle("negative", frame.lap_delta_seconds > 0);
};

const updateInteractionMode = ({ click_through }: InteractionMode): void => {
  text(
    "interaction-mode",
    click_through ? "MODO JUEGO" : "MODO EDICIÓN"
  );
};

void listenTelemetry((frame) =>
  renderPerformance.measure(() => render(frame))
);
bindOverlayInteractionMode(updateInteractionMode);
