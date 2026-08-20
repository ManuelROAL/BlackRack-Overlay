import "./styles.css";
import "./rejoin.css";
import { fitOverlay } from "./overlay-fit";
import { bindOverlayInteractionMode } from "./overlay-interaction";
import { createOverlayPerformanceTracker } from "./overlay-performance";
import { bindOverlayTransparency } from "./overlay-appearance";
import type { RejoinWarning, TelemetryFrame } from "./telemetry-types";
import { listenTelemetry } from "./runtime-events";
import { formatNumber, t } from "./i18n";

fitOverlay({ width: 360, height: 140 });
bindOverlayTransparency("rejoin");
const renderPerformance = createOverlayPerformanceTracker("rejoin");

const text = (id: string, value: string): void => {
  const element = document.getElementById(id);
  if (element && element.textContent !== value) element.textContent = value;
};

const distanceLabel = (meters: number): string => {
  if (meters < 1_000) return `${Math.round(meters)} m`;
  return `${formatNumber(Math.round(meters / 100) / 10)} km`;
};

const classLabel = (vehicleClass: string): string =>
  vehicleClass.replace(/_ELMS$/i, "").replace(/^GT3$/i, "LMGT3") || "--";

const render = (warning: RejoinWarning): void => {
  const card = document.getElementById("rejoin-card");
  if (!card) return;

  const active = String(warning.active);
  if (card.dataset.active !== active) card.dataset.active = active;
  if (card.dataset.safety !== warning.safety) card.dataset.safety = warning.safety;
  text("rejoin-reason", t(warning.reason === "pit_exit" ? "rejoin.reasonPit" : "rejoin.reason"));
  text(
    "rejoin-status",
    warning.safety === "danger"
      ? t("rejoin.danger")
      : warning.safety === "caution"
        ? t("rejoin.caution")
        : t("rejoin.safe")
  );

  if (!warning.rear_car_available) {
    text("rejoin-distance", t("rejoin.clear"));
    text("rejoin-eta", t("rejoin.noCar"));
    text("rejoin-car", "");
    return;
  }

  text("rejoin-distance", distanceLabel(warning.distance_meters));
  text(
    "rejoin-eta",
    warning.time_to_arrival_seconds > 0
      ? `${formatNumber(Math.round(warning.time_to_arrival_seconds * 10) / 10)} s`
      : t("rejoin.notClosing")
  );
  const position = warning.car_position > 0 ? `P${warning.car_position}` : "P--";
  text("rejoin-car", `${position} · ${classLabel(warning.vehicle_class)}`);
};

void listenTelemetry((frame) =>
  renderPerformance.measure(() => render(frame.rejoin_warning))
);
bindOverlayInteractionMode();

if (import.meta.env.DEV) {
  const preview = new URLSearchParams(window.location.search).get("preview");
  if (preview === "safe" || preview === "caution" || preview === "danger") {
    render({
      active: true,
      reason: preview === "safe" ? "pit_exit" : "rejoin",
      safety: preview,
      rear_car_available: true,
      distance_meters: preview === "danger" ? 84 : preview === "caution" ? 184 : 620,
      time_to_arrival_seconds: preview === "danger" ? 3.2 : preview === "caution" ? 7.4 : 18.6,
      car_position: 2,
      vehicle_class: "HYPERCAR"
    });
  }
}
