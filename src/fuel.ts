import "./styles.css";
import "./fuel.css";
import { fitOverlay } from "./overlay-fit";
import { bindOverlayInteractionMode } from "./overlay-interaction";
import { createOverlayPerformanceTracker } from "./overlay-performance";
import { bindOverlayTransparency } from "./overlay-appearance";
import type { ResourceStrategy, TelemetryFrame } from "./telemetry-types";
import { listenTelemetry } from "./runtime-events";

fitOverlay({ width: 356, height: 188 });
bindOverlayTransparency("fuel");
const renderPerformance = createOverlayPerformanceTracker("fuel");

type ProfileName = "average" | "qualifying" | "last";
type PitLevel = "unknown" | "safe" | "caution" | "warning" | "critical";

const text = (id: string, value: string): void => {
  const element = document.getElementById(id);
  if (element && element.textContent !== value) element.textContent = value;
};

const tone = (id: string, value: "good" | "warn" | "bad" | "neutral"): void => {
  const element = document.getElementById(id);
  if (element && element.dataset.tone !== value) element.dataset.tone = value;
};

const format = (value: number, decimals = 2): string =>
  Number.isFinite(value) && value >= 0 ? value.toFixed(decimals) : "--";

const consumptionReference = (...values: number[]): number | undefined =>
  values.find((value) => Number.isFinite(value) && value > 0);

const pitLevel = (playerActive: boolean, autonomy: number): PitLevel => {
  if (!playerActive || !Number.isFinite(autonomy) || autonomy < 0) return "unknown";
  if (autonomy <= 1) return "critical";
  if (autonomy <= 2) return "warning";
  if (autonomy <= 3) return "caution";
  return "safe";
};

const renderProfile = (
  resource: "energy" | "fuel",
  name: ProfileName,
  consumption: number,
  plan: ResourceStrategy | null
): void => {
  const id = `${resource}-${name}`;
  text(`${id}-consumption`, plan ? format(consumption) : "--");
  text(`${id}-autonomy`, plan ? format(plan.autonomy) : "--");
  text(`${id}-required`, plan ? format(plan.total_additional) : "--");
};

const renderStatus = (frame: TelemetryFrame): void => {
  const connection = document.getElementById("source-status")?.parentElement;
  if (!frame.connected) {
    text("source-status", "ESPERA");
    connection?.setAttribute("data-state", "offline");
  } else if (!frame.player_active) {
    text("source-status", "SIN COCHE");
    connection?.setAttribute("data-state", "standby");
  } else {
    text("source-status", "DIRECTO");
    connection?.setAttribute("data-state", "live");
  }
};

const render = (frame: TelemetryFrame): void => {
  renderStatus(frame);

  const energyMode = frame.virtual_energy_active;
  const mode = energyMode ? "energy" : "fuel";
  const current = energyMode ? frame.virtual_energy_percent : frame.fuel_liters;
  const capacity = energyMode ? 100 : frame.fuel_capacity_liters;
  const unit = energyMode ? "%" : "L";
  const average = energyMode ? frame.virtual_energy_per_lap : frame.fuel_per_lap;
  const qualifying = energyMode ? frame.virtual_energy_qualifying_lap : frame.fuel_qualifying_lap;
  const last = energyMode ? frame.virtual_energy_last_lap : frame.fuel_last_lap;
  const fuelReference = consumptionReference(
    frame.fuel_projected_lap,
    frame.fuel_per_lap,
    frame.fuel_last_lap,
    frame.fuel_reference_per_lap,
    frame.fuel_qualifying_lap
  ) ?? 0;
  const fuelStrategy = frame.fuel_strategies.fuel;
  const strategy = frame.fuel_strategies.active;

  const shell = document.querySelector<HTMLElement>(".fuel-shell");
  shell?.setAttribute("data-resource-mode", mode);
  text("resource-label", energyMode ? "NRG" : "FUEL");
  text("active-table-label", energyMode ? "NRG" : "FUEL");
  text("resource-current", frame.player_active ? format(current, 1) : "--");
  text("resource-unit", unit);
  if (strategy) {
    text("strategy-autonomy", `${format(strategy.autonomy, 1)}V`);
    text(
      "pit-window",
      strategy.stops > 0
        ? strategy.earliest_pit_lap === strategy.latest_pit_lap
          ? `V${strategy.latest_pit_lap}`
          : `V${strategy.earliest_pit_lap}–${strategy.latest_pit_lap}`
        : "NO PIT"
    );
    text("stop-plan", strategy.stops > 0 ? `${strategy.stops}→${strategy.target_stops}` : "0");

    const retainsStops = strategy.target_stops >= strategy.stops;
    const fullAllowed = qualifying > 0 && strategy.target_consumption >= qualifying;
    const displayedTarget = fullAllowed ? qualifying : strategy.target_consumption;
    text("target-label", fullAllowed ? "FULL" : retainsStops ? "MANTÉN" : "OBJ/V");
    text("target-consumption", format(displayedTarget));
    text("saving-required", fullAllowed ? "0,0%" : `−${format(strategy.saving_percent, 1)}%`);
    tone("saving-required", fullAllowed || strategy.saving_percent <= 2 ? "good" : strategy.saving_percent <= 7 ? "warn" : "bad");
    const nextFill = frame.fuel_strategies.conservative_next_fill > 0
      ? frame.fuel_strategies.conservative_next_fill
      : strategy.next_fill;
    text("next-fill-label", frame.fuel_strategies.conservative_fill_active ? "CARGA Q" : "CARGA");
    text("next-fill", strategy.stops > 0 ? `${format(nextFill, 1)}${unit}` : "--");

    const pit = document.getElementById("pit-status");
    if (pit) pit.dataset.level = pitLevel(frame.player_active, strategy.autonomy);
  } else {
    for (const [id, value] of [
      ["strategy-autonomy", "--"], ["pit-window", "--"], ["stop-plan", "--"],
      ["target-consumption", "--"], ["saving-required", "--"],
      ["next-fill", "--"]
    ]) text(id, value);
    text("target-label", "OBJ/V");
    text("next-fill-label", "CARGA");
    const pit = document.getElementById("pit-status");
    if (pit) pit.dataset.level = "unknown";
  }

  const fuelAutonomy = fuelReference > 0
    ? Math.max(frame.fuel_liters, 0) / fuelReference
    : Number.POSITIVE_INFINITY;
  text("fuel-current", energyMode ? `${format(frame.fuel_liters, 1)}L` : "--");
  text("fuel-consumption", energyMode ? `${format(fuelReference)}L/V` : "--");
  text("fuel-autonomy", energyMode ? `${format(fuelAutonomy, 1)}V` : "--");
  text("fuel-required", energyMode && fuelStrategy ? `+${format(fuelStrategy.total_additional, 1)}L` : "--");
  text("fuel-stops", energyMode && fuelStrategy ? `${fuelStrategy.stops}` : "--");

  renderProfile("energy", "average", average, frame.fuel_strategies.average);
  renderProfile("energy", "qualifying", qualifying, frame.fuel_strategies.qualifying);
  renderProfile("energy", "last", last, frame.fuel_strategies.last);

  const level = document.getElementById("resource-level");
  if (level) {
    const width = `${Math.round(Math.max(0, Math.min(1, current / Math.max(capacity, 1))) * 1_000) / 10}%`;
    if (level.style.width !== width) level.style.width = width;
  }
};

void listenTelemetry((frame) =>
  renderPerformance.measure(() => render(frame))
);
bindOverlayInteractionMode();
