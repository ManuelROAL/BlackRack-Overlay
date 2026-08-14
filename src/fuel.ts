import "./styles.css";
import "./fuel.css";
import { fitOverlay } from "./overlay-fit";
import { bindOverlayInteractionMode } from "./overlay-interaction";
import { createOverlayPerformanceTracker } from "./overlay-performance";
import { bindOverlayTransparency } from "./overlay-appearance";
import type { ResourceStrategy, TelemetryFrame } from "./telemetry-types";
import { listenTelemetry } from "./runtime-events";

fitOverlay({ width: 560, height: 230 });
bindOverlayTransparency("fuel");
const renderPerformance = createOverlayPerformanceTracker("fuel");

type ProfileName = "estimated" | "average" | "qualifying" | "last";
type PitLevel = "unknown" | "safe" | "caution" | "warning" | "critical";

interface StintTracker {
  sessionKey: string;
  mode: "energy" | "fuel";
  startResource: number;
  startDistance: number;
  previousResource: number;
  targetConsumption: number;
}

let stintTracker: StintTracker | undefined;

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

const signed = (value: number, decimals = 2): string => {
  if (!Number.isFinite(value)) return "--";
  if (Math.abs(value) < 0.005) return (0).toFixed(decimals);
  return `${value > 0 ? "+" : "−"}${Math.abs(value).toFixed(decimals)}`;
};

const consumptionReference = (...values: number[]): number | undefined =>
  values.find((value) => Number.isFinite(value) && value > 0);

const pitLevel = (playerActive: boolean, autonomy: number): PitLevel => {
  if (!playerActive || !Number.isFinite(autonomy) || autonomy < 0) return "unknown";
  if (autonomy <= 1) return "critical";
  if (autonomy <= 2) return "warning";
  if (autonomy <= 3) return "caution";
  return "safe";
};

const updateStintDelta = (
  frame: TelemetryFrame,
  mode: "energy" | "fuel",
  current: number,
  targetConsumption: number
): number | undefined => {
  const distance = frame.player_total_laps + frame.lap_progress;
  const sessionKey = `${frame.track_name}|${frame.session_type}|${frame.session_total_laps_estimated}`;
  const reset =
    !stintTracker ||
    stintTracker.sessionKey !== sessionKey ||
    stintTracker.mode !== mode ||
    distance + 0.05 < stintTracker.startDistance ||
    current > stintTracker.previousResource + 0.05;

  if (reset) {
    stintTracker = {
      sessionKey,
      mode,
      startResource: current,
      startDistance: distance,
      previousResource: current,
      targetConsumption
    };
    return undefined;
  }

  const tracker = stintTracker;
  if (!tracker) return undefined;
  tracker.previousResource = current;
  const travelled = Math.max(distance - tracker.startDistance, 0);
  if (travelled < 0.02 || tracker.targetConsumption <= 0) return undefined;
  const actualUsed = Math.max(tracker.startResource - current, 0);
  return actualUsed - tracker.targetConsumption * travelled;
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
  text(`${id}-delta`, plan ? `${signed(plan.autonomy_delta, 1)}V` : "--");
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
  const profileReference = energyMode
    ? frame.virtual_energy_reference_per_lap
    : frame.fuel_reference_per_lap;
  const projected = energyMode ? frame.virtual_energy_projected_lap : frame.fuel_projected_lap;
  const pitCycleConsumption = energyMode
    ? frame.virtual_energy_pit_cycle_consumption
    : frame.fuel_pit_cycle_consumption;
  const pitOutConsumption = energyMode
    ? frame.virtual_energy_pit_out_consumption
    : frame.fuel_pit_out_consumption;
  const reference = consumptionReference(projected, average, last, profileReference, qualifying) ?? 0;
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
  text("race-laps", frame.session_laps_remaining > 0 ? format(frame.session_laps_remaining, 1) : "--");
  text("race-total-laps", frame.session_total_laps_estimated > 0 ? format(frame.session_total_laps_estimated, 0) : "--");

  if (strategy) {
    text("strategy-autonomy", `${format(strategy.autonomy, 1)}V/${format(strategy.minutes, 0)}m`);
    text(
      "pit-window",
      strategy.stops > 0
        ? strategy.earliest_pit_lap === strategy.latest_pit_lap
          ? `V${strategy.latest_pit_lap}`
          : `V${strategy.earliest_pit_lap}–${strategy.latest_pit_lap}`
        : "NO PIT"
    );
    text("stop-plan", strategy.stops > 0 ? `${strategy.stops}→${strategy.target_stops}` : "0");

    const fullAllowed = qualifying > 0 && strategy.target_consumption >= qualifying;
    const displayedTarget = fullAllowed ? qualifying : strategy.target_consumption;
    text("target-label", fullAllowed ? "FULL" : "OBJ/V");
    text("target-consumption", format(displayedTarget));
    text("saving-required", fullAllowed ? "0,0%" : `−${format(strategy.saving_percent, 1)}%`);
    tone("saving-required", fullAllowed || strategy.saving_percent <= 2 ? "good" : strategy.saving_percent <= 7 ? "warn" : "bad");
    const nextFill = frame.fuel_strategies.conservative_next_fill > 0
      ? frame.fuel_strategies.conservative_next_fill
      : strategy.next_fill;
    text("next-fill-label", frame.fuel_strategies.conservative_fill_active ? "CARGA Q" : "CARGA");
    text("next-fill", strategy.stops > 0 ? `${format(nextFill, 1)}${unit}` : "--");

    const delta = updateStintDelta(frame, mode, current, displayedTarget);
    text("stint-delta", delta === undefined ? "--" : `${signed(delta)}${unit}`);
    tone("stint-delta", delta === undefined ? "neutral" : delta <= 0 ? "good" : "bad");

    const pit = document.getElementById("pit-status");
    if (pit) pit.dataset.level = pitLevel(frame.player_active, strategy.autonomy);
  } else {
    for (const [id, value] of [
      ["strategy-autonomy", "--"], ["pit-window", "--"], ["stop-plan", "--"],
      ["target-consumption", "--"], ["saving-required", "--"], ["stint-delta", "--"],
      ["next-fill", "--"]
    ]) text(id, value);
    text("target-label", "OBJ/V");
    text("next-fill-label", "CARGA");
    const pit = document.getElementById("pit-status");
    if (pit) pit.dataset.level = "unknown";
  }

  const lastStrategy = frame.fuel_strategies.last;
  text(
    "autonomy-gain",
    lastStrategy ? `GANANCIA ${signed(lastStrategy.autonomy_delta, 1)}V` : "GANANCIA --"
  );
  tone(
    "autonomy-gain",
    !lastStrategy ? "neutral" : lastStrategy.autonomy_delta >= 0.05 ? "good" : lastStrategy.autonomy_delta <= -0.05 ? "bad" : "neutral"
  );
  const confidence = frame.consumption_profile_samples >= 5
    ? "ALTA"
    : frame.consumption_profile_samples >= 2 ? "MEDIA" : "BAJA";
  text("strategy-confidence", `CONF. ${confidence}`);
  tone("strategy-confidence", confidence === "ALTA" ? "good" : confidence === "MEDIA" ? "warn" : "bad");

  const pitDelta = reference > 0 && pitCycleConsumption > 0
    ? pitCycleConsumption - 2 * reference
    : Number.NaN;
  text("pit-cycle-cost", Number.isFinite(pitDelta) ? `PIT Δ ${signed(pitDelta)}${unit}` : "PIT Δ --");
  const pitService = document.getElementById("pit-service-time");
  const tire = frame.player_tire_remaining_percent >= 0
    ? `N${format(frame.player_tire_remaining_percent, 0)}%`
    : "N--";
  text(
    "pit-service-time",
    frame.pit_stop_estimate_available && frame.pit_stop_estimate_seconds > 0
      ? `PIT ${format(frame.pit_stop_estimate_seconds, 0)}S/${tire}`
      : `PIT --/${tire}`
  );
  if (pitService) {
    pitService.title = frame.pit_stop_estimate_available
      ? `Parada ${format(frame.pit_stop_estimate_seconds, 1)} s · Fuel ${format(frame.pit_stop_fuel_seconds, 1)} s · NRG ${format(frame.pit_stop_energy_seconds, 1)} s · Neumáticos ${format(frame.pit_stop_tire_seconds, 1)} s · Reparaciones ${format(frame.pit_stop_damage_seconds, 1)} s · Penalización ${format(frame.pit_stop_penalty_seconds, 1)} s · Piloto ${format(frame.pit_stop_driver_swap_seconds, 1)} s · Stint ${frame.player_stint || "--"}`
      : "Estimación de parada no disponible";
  }

  const fuelAutonomy = fuelReference > 0
    ? Math.max(frame.fuel_liters, 0) / fuelReference
    : Number.POSITIVE_INFINITY;
  const energyReference = consumptionReference(
    frame.virtual_energy_projected_lap,
    frame.virtual_energy_per_lap,
    frame.virtual_energy_last_lap,
    frame.virtual_energy_reference_per_lap,
    frame.virtual_energy_qualifying_lap
  ) ?? 0;
  const energyAutonomy = energyReference > 0
    ? Math.max(frame.virtual_energy_percent, 0) / energyReference
    : Number.POSITIVE_INFINITY;
  const limiting = energyMode
    ? `NRG ${format(energyAutonomy, 1)}V / FUEL ${format(fuelAutonomy, 1)}V`
    : `FUEL ${format(fuelAutonomy, 1)}V`;
  text("limiting-resource", `LIMITA ${limiting}`);
  text("fuel-current", energyMode ? `${format(frame.fuel_liters, 1)}L` : "--");
  text("fuel-consumption", energyMode ? `${format(fuelReference)}L/V` : "--");
  text("fuel-autonomy", energyMode ? `${format(fuelAutonomy, 1)}V` : "--");
  text("fuel-required", energyMode && fuelStrategy ? `+${format(fuelStrategy.total_additional, 1)}L` : "--");
  text("fuel-stops", energyMode && fuelStrategy ? `${fuelStrategy.stops}` : "--");

  renderProfile("energy", "estimated", reference, frame.fuel_strategies.estimated);
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
