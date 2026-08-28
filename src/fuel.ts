import "./styles.css";
import "./fuel.css";
import { fitOverlay } from "./overlay-fit";
import { bindOverlayInteractionMode } from "./overlay-interaction";
import { createOverlayPerformanceTracker } from "./overlay-performance";
import { bindOverlayTransparency } from "./overlay-appearance";
import type { ResourceStrategy, TelemetryFrame } from "./telemetry-types";
import { listenTelemetry } from "./runtime-events";
import { formatNumber, t } from "./i18n";
import { energyIconUrl, fuelIconUrl } from "./lmu-icons";

fitOverlay(
  { width: 292, height: 198 },
  { widthTextRatio: 0.5, heightTextRatio: 0.3 }
);
bindOverlayTransparency("fuel");
const renderPerformance = createOverlayPerformanceTracker("fuel");

type ProfileName = "average" | "qualifying" | "last";
type PitLevel = "unknown" | "safe" | "caution" | "warning" | "critical";

const text = (id: string, value: string): void => {
  const element = document.getElementById(id);
  if (element && element.textContent !== value) element.textContent = value;
};

const renderResourceIcon = (containerId: string, imageId: string, energyMode: boolean): void => {
  const label = energyMode ? "NRG" : "FUEL";
  const container = document.getElementById(containerId);
  const image = document.getElementById(imageId) as HTMLImageElement | null;
  if (container && container.getAttribute("aria-label") !== label) {
    container.setAttribute("aria-label", label);
  }
  const url = energyMode ? energyIconUrl : fuelIconUrl;
  if (image && image.getAttribute("src") !== url) image.src = url;
};

const tone = (id: string, value: "good" | "warn" | "bad" | "neutral"): void => {
  const element = document.getElementById(id);
  if (element && element.dataset.tone !== value) element.dataset.tone = value;
};

const format = (value: number, decimals = 2): string =>
  Number.isFinite(value) && value >= 0 ? formatNumber(value, decimals) : "--";

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
  current: number,
  consumption: number,
  plan: ResourceStrategy | null
): void => {
  const id = `${resource}-${name}`;
  text(`${id}-consumption`, plan ? format(consumption) : "--");
  text(`${id}-autonomy`, plan ? format(plan.autonomy) : "--");
  const totalRequired = plan
    ? Math.max(current, 0) + plan.total_additional - plan.end_remaining
    : null;
  text(`${id}-required`, totalRequired === null ? "--" : format(totalRequired));
};

const renderStatus = (frame: TelemetryFrame): void => {
  const connection = document.getElementById("source-status")?.parentElement;
  if (!frame.connected) {
    text("source-status", t("fuel.waiting"));
    connection?.setAttribute("data-state", "offline");
  } else if (!frame.player_active) {
    text("source-status", t("fuel.noCar"));
    connection?.setAttribute("data-state", "standby");
  } else {
    text("source-status", t("fuel.live"));
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
  const strategy = frame.fuel_strategies.active;

  const shell = document.querySelector<HTMLElement>(".fuel-shell");
  shell?.setAttribute("data-resource-mode", mode);
  renderResourceIcon("active-table-label", "active-table-icon", energyMode);
  text("resource-current", frame.player_active ? format(current, 1) : "--");
  text("resource-unit", unit);
  if (strategy) {
    text("strategy-autonomy", t("fuel.lapsValue", { value: format(strategy.autonomy, 1) }));
    text(
      "pit-window",
      strategy.stops > 0
        ? strategy.earliest_pit_lap === strategy.latest_pit_lap
          ? t("timing.lap", { number: strategy.latest_pit_lap })
          : t("fuel.lapRange", { first: strategy.earliest_pit_lap, last: strategy.latest_pit_lap })
        : t("fuel.noPit")
    );
    text("stop-plan", strategy.stops > 0 ? `${strategy.stops}→${strategy.target_stops}` : "0");

    const retainsStops = strategy.target_stops >= strategy.stops;
    const fullAllowed = qualifying > 0 && strategy.target_consumption >= qualifying;
    const displayedTarget = fullAllowed ? qualifying : strategy.target_consumption;
    text("target-label", t(fullAllowed ? "fuel.full" : retainsStops ? "fuel.hold" : "fuel.target"));
    text("target-consumption", format(displayedTarget));
    text("saving-required", fullAllowed ? `${formatNumber(0, 1)}%` : `−${format(strategy.saving_percent, 1)}%`);
    tone("saving-required", fullAllowed || strategy.saving_percent <= 2 ? "good" : strategy.saving_percent <= 7 ? "warn" : "bad");
    const nextFill = frame.fuel_strategies.conservative_next_fill > 0
      ? frame.fuel_strategies.conservative_next_fill
      : strategy.next_fill;
    text("next-fill-label", t(frame.fuel_strategies.conservative_fill_active ? "fuel.qualifyingLoad" : "fuel.load"));
    text("next-fill", strategy.stops > 0 ? `${format(nextFill, 1)}${unit}` : "--");

    const pit = document.getElementById("pit-status");
    if (pit) pit.dataset.level = pitLevel(frame.player_active, strategy.autonomy);
  } else {
    for (const [id, value] of [
      ["strategy-autonomy", "--"], ["pit-window", "--"], ["stop-plan", "--"],
      ["target-consumption", "--"], ["saving-required", "--"],
      ["next-fill", "--"]
    ]) text(id, value);
    text("target-label", t("fuel.target"));
    text("next-fill-label", t("fuel.load"));
    const pit = document.getElementById("pit-status");
    if (pit) pit.dataset.level = "unknown";
  }

  const fuelAutonomy = fuelReference > 0
    ? Math.max(frame.fuel_liters, 0) / fuelReference
    : Number.POSITIVE_INFINITY;
  text("fuel-current", energyMode ? format(frame.fuel_liters, 1) : "--");
  text("fuel-capacity", energyMode ? `/ ${format(frame.fuel_capacity_liters, 1)}` : "/ --");
  text("fuel-autonomy", energyMode ? t("fuel.lapsValue", { value: format(fuelAutonomy, 1) }) : "--");
  text("fuel-ratio-assigned", energyMode ? format(frame.fuel_ratio_assigned) : "--");
  text("fuel-ratio-average", energyMode ? format(frame.fuel_ratio_average) : "--");
  text("fuel-ratio-last", energyMode ? format(frame.fuel_ratio_last) : "--");
  renderProfile("energy", "average", current, average, frame.fuel_strategies.average);
  renderProfile("energy", "qualifying", current, qualifying, frame.fuel_strategies.qualifying);
  renderProfile("energy", "last", current, last, frame.fuel_strategies.last);

  const level = document.getElementById("resource-level");
  if (level) {
    const width = `${Math.round(Math.max(0, Math.min(1, current / Math.max(capacity, 1))) * 1_000) / 10}%`;
    if (level.style.width !== width) level.style.width = width;
  }

  const fuelLevel = document.getElementById("fuel-level");
  if (fuelLevel) {
    const width = `${Math.round(Math.max(0, Math.min(1, frame.fuel_liters / Math.max(frame.fuel_capacity_liters, 1))) * 1_000) / 10}%`;
    if (fuelLevel.style.width !== width) fuelLevel.style.width = width;
  }
};

void listenTelemetry((frame) =>
  renderPerformance.measure(() => render(frame))
);
bindOverlayInteractionMode();
