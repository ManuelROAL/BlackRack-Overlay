import "./styles.css";
import "./fuel.css";
import { fitOverlay } from "./overlay-fit";
import { bindOverlayInteractionMode } from "./overlay-interaction";
import { createOverlayPerformanceTracker } from "./overlay-performance";
import { bindOverlayTransparency } from "./overlay-appearance";
import type { ResourceStrategy, TelemetryFrame } from "./telemetry-types";
import { isTauriRuntime, listenRuntimeEvent, listenTelemetry } from "./runtime-events";
import { formatNumber, t } from "./i18n";
import { energyIconUrl, fuelIconUrl } from "./lmu-icons";
import { normalizeFuelSettings, readFuelSettings, type FuelField, type FuelSettings } from "./fuel-settings";

const resizeOverlay = fitOverlay(
  { width: 292, height: 198 },
  { widthTextRatio: 0.5, heightTextRatio: 0.3 }
);
bindOverlayTransparency("fuel");
const renderPerformance = createOverlayPerformanceTracker("fuel");
let settings = readFuelSettings();
let energyLayout = true;
const shell = document.querySelector<HTMLElement>(".fuel-shell")!;
const summary = shell.querySelector<HTMLElement>(".strategy-summary")!;
const table = shell.querySelector<HTMLElement>(".strategy-table")!;
const fieldSelectors: Partial<Record<FuelField, string>> = {
  current: ".accent-stat",
  autonomy: ".summary-stat:nth-child(2)",
  pitWindow: ".wide-stat > span, #pit-window",
  postPit: "#next-stint-range",
  pitStatus: "#pit-status",
  level: ".strategy-level",
  targets: ".stint-targets",
  average: ".average-row",
  qualifying: ".qualify-row",
  last: ".last-row",
  consumption: ".strategy-columns > :nth-child(2), .strategy-row > :nth-child(2)",
  scenarioAutonomy: ".strategy-columns > :nth-child(3), .strategy-row > :nth-child(3)",
  scenarioValue: ".strategy-columns > :nth-child(4), .strategy-row > :nth-child(4)",
  fuelCard: ".fuel-card",
  ratios: ".fuel-card-values"
};
const fieldNodes = Object.entries(fieldSelectors).map(([id, selector]) =>
  [id as FuelField, Array.from(shell.querySelectorAll<HTMLElement>(selector))] as const
);
const wideStat = shell.querySelector<HTMLElement>(".wide-stat")!;
const applySettings = (): void => {
  const v = settings.visible;
  for (const [id, nodes] of fieldNodes) {
    for (const node of nodes) node.hidden = !v[id];
  }
  wideStat.hidden = !v.pitWindow && !v.postPit;
  const summaryTracks = [
    v.current ? "52fr" : "", v.autonomy ? "52fr" : "",
    v.pitWindow || v.postPit ? "98fr" : "", v.pitStatus ? "28fr" : ""
  ].filter(Boolean);
  summary.hidden = summaryTracks.length === 0 && !v.level;
  summary.style.gridTemplateColumns = summaryTracks.join(" ") || "1fr";
  summary.classList.toggle("level-only", summaryTracks.length === 0);
  const rows = Number(v.average) + Number(v.qualifying) + Number(v.last);
  const columns = Number(v.consumption) + Number(v.scenarioAutonomy) + Number(v.scenarioValue);
  table.hidden = rows === 0 || columns === 0;
  table.style.gridTemplateRows = `max(12px, calc(11px * var(--overlay-font-scale, 1))) repeat(${Math.max(rows, 1)}, 1fr)`;
  shell.style.setProperty("--fuel-scenario-columns", `56fr repeat(${Math.max(columns, 1)}, 58fr)`);
  shell.classList.toggle("hide-ratios", !v.ratios);
  const summaryHeight = summary.hidden ? 0 : summaryTracks.length ? 39 : 10;
  const tableHeight = table.hidden ? 0 : 16 + rows * 18;
  const cardHeight = v.fuelCard && energyLayout ? (v.ratios ? 52 : 34) : 0;
  resizeOverlay({ width: 292, height: Math.max(32, 6 + summaryHeight + (v.targets ? 31 : 0) + tableHeight + cardHeight) });
  text("scenario-value-label", t(settings.scenarioMode === "refuel" ? "fuel.refuel" : "fuel.totalAdd"));
};

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

const renderStintTargets = (frame: TelemetryFrame, unit: string): void => {
  for (let index = 0; index < 3; index += 1) {
    const target = frame.fuel_strategies.stint_targets[index];
    const number = index + 1;
    text(`stint-target-${number}`, target ? `${format(target.target_consumption)}${unit}` : "--");
    text(
      `stint-target-${number}-meta`,
      target ? `+${target.extra_laps}` : `+${number}`
    );
    tone(
      `stint-target-${number}`,
      target?.net_time_seconds == null
        ? "neutral"
        : target.net_time_seconds > 0.5
          ? "good"
          : target.net_time_seconds < -0.5
            ? "bad"
            : "warn"
    );
  }
};

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
    ? Math.max(current, 0) + plan.total_additional
    : null;
  const displayedValue = settings.scenarioMode === "refuel" ? plan?.next_fill : totalRequired;
  text(`${id}-required`, displayedValue == null ? "--" : format(displayedValue));
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
  const range = frame.resource_autonomy.range_laps;
  const strategy = frame.fuel_strategies.active;
  const nextStintLaps = frame.fuel_strategies.next_stint_laps;
  text("scenario-value-label", t(settings.scenarioMode === "refuel" ? "fuel.refuel" : "fuel.totalAdd"));
  text(
    "next-stint-range",
    nextStintLaps != null && Number.isFinite(nextStintLaps)
      ? t("fuel.postPit", { value: format(nextStintLaps, 1) })
      : ""
  );

  if (energyLayout !== energyMode) {
    energyLayout = energyMode;
    applySettings();
  }
  shell?.setAttribute("data-resource-mode", mode);
  renderResourceIcon("active-table-label", "active-table-icon", energyMode);
  renderStintTargets(frame, unit);
  text("resource-current", frame.player_active ? format(current, 1) : "--");
  text("resource-unit", unit);
  if (strategy) {
    text(
      "pit-window",
      strategy.stops > 0
        ? strategy.earliest_pit_lap === strategy.latest_pit_lap
          ? t("timing.lap", { number: strategy.latest_pit_lap })
          : t("fuel.lapRange", { first: strategy.earliest_pit_lap, last: strategy.latest_pit_lap })
        : t("fuel.noPit")
    );
  } else {
    text("pit-window", "--");
  }

  const pit = document.getElementById("pit-status");
  if (pit) pit.dataset.level = range == null ? "unknown" : pitLevel(frame.player_active, range);
  text("strategy-autonomy", t("fuel.lapsValue", { value: format(range ?? NaN, 1) }));
  const fuelAutonomy = frame.resource_autonomy.fuel_laps;
  text("fuel-current", energyMode ? format(frame.fuel_liters, 1) : "--");
  text("fuel-capacity", energyMode ? `/ ${format(frame.fuel_capacity_liters, 1)}` : "/ --");
  text("fuel-autonomy", energyMode ? t("fuel.lapsValue", { value: format(fuelAutonomy ?? NaN, 1) }) : "--");
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

applySettings();
void listenTelemetry((frame) =>
  renderPerformance.measure(() => render(frame))
);
if (isTauriRuntime()) {
  void listenRuntimeEvent<FuelSettings>("fuel://settings", (next) => {
    settings = normalizeFuelSettings(next);
    applySettings();
  });
}
bindOverlayInteractionMode();
