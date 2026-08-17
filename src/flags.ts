import "./styles.css";
import "./flags.css";
import { fitOverlay } from "./overlay-fit";
import { bindOverlayInteractionMode } from "./overlay-interaction";
import { createOverlayPerformanceTracker } from "./overlay-performance";
import { bindOverlayTransparency } from "./overlay-appearance";
import type { FlagWarning, TelemetryFrame } from "./telemetry-types";
import { listenTelemetry } from "./runtime-events";

fitOverlay({ width: 360, height: 120 });
bindOverlayTransparency("flags");
const renderPerformance = createOverlayPerformanceTracker("flags");

const distanceLabel = (meters: number): string => {
  if (!Number.isFinite(meters)) return "-- m";
  meters = Math.abs(meters);
  if (meters < 1_000) return `${Math.round(meters)} m`;
  return `${(meters / 1_000).toFixed(1).replace(".", ",")} km`;
};

const yellowDirection = (meters: number): string => meters < 0 ? "↓" : "↑";

const classLabel = (vehicleClass: string): string =>
  vehicleClass.replace(/_ELMS$/i, "").replace(/^GT3$/i, "LMGT3") || "--";

const render = (warning: FlagWarning): void => {
  const card = document.getElementById("flag-card");
  if (!card) return;

  const active = warning.active && (
    warning.kind === "yellow" || warning.kind === "blue" || warning.kind === "checkered"
  );
  const state = active ? warning.kind : "green";
  if (card.dataset.state !== state) card.dataset.state = state;

  const distance = document.getElementById("flag-distance");
  const car = document.getElementById("flag-car");
  const count = document.getElementById("flag-count");
  if (distance) {
    const value = warning.kind === "checkered"
      ? ""
      : warning.kind === "yellow"
        ? `${yellowDirection(warning.distance_meters)} ${distanceLabel(warning.distance_meters)}`
        : distanceLabel(warning.distance_meters);
    if (distance.textContent !== value) distance.textContent = value;
  }
  if (car) {
    if (warning.kind === "checkered") {
      if (car.textContent !== "") car.textContent = "";
    } else {
      const position = warning.car_position > 0 ? `P${warning.car_position}` : "P--";
      const value = `${position} · ${classLabel(warning.vehicle_class)}`;
      if (car.textContent !== value) car.textContent = value;
    }
  }
  if (count) {
    const visible = warning.kind === "blue" && warning.car_count > 1;
    const value = visible ? `${warning.car_count} COCHES` : "";
    if (count.textContent !== value) count.textContent = value;
    if (count.hidden === visible) count.hidden = !visible;
  }
};

void listenTelemetry((frame) =>
  renderPerformance.measure(() => render(frame.flag_warning))
);
bindOverlayInteractionMode();

if (import.meta.env.DEV) {
  const preview = new URLSearchParams(window.location.search).get("preview");
  if (preview === "yellow" || preview === "blue" || preview === "checkered") {
    render({
      kind: preview,
      active: true,
      distance_meters: preview === "yellow" ? 428 : 164,
      car_position: preview === "yellow" ? 7 : 2,
      vehicle_class: preview === "yellow" ? "LMGT3" : "HYPERCAR",
      car_count: preview === "blue" ? 3 : preview === "yellow" ? 1 : 0
    });
  }
}
