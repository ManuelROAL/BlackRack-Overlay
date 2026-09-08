import "./styles.css";
import "./stinthistory.css";
import { t } from "./i18n";
import { compoundIconUrl } from "./lmu-icons";
import { bindOverlayTransparency } from "./overlay-appearance";
import { fitOverlayToContentBox } from "./overlay-fit";
import { bindOverlayInteractionMode } from "./overlay-interaction";
import { createOverlayPerformanceTracker } from "./overlay-performance";
import { isTauriRuntime, listenTelemetry } from "./runtime-events";
import type { StintHistoryEntryView, StintHistoryViewModel } from "./telemetry-types";

bindOverlayTransparency("stinthistory");
bindOverlayInteractionMode();
const performance = createOverlayPerformanceTracker("stinthistory");
const card = document.getElementById("stint-history-card");
const rows = document.getElementById("stint-history-rows");
const empty = document.getElementById("stint-history-empty");
const count = document.getElementById("stint-history-count");
const resourceLabel = document.getElementById("stint-history-resource-label");
const resizeOverlay = card ? fitOverlayToContentBox(card) : () => undefined;

const setText = (element: Element | null, value: string): void => {
  if (element && element.textContent !== value) element.textContent = value;
};

const duration = (seconds: number): string => {
  if (!Number.isFinite(seconds) || seconds <= 0) return "--:--";
  const rounded = Math.round(seconds);
  const hours = Math.floor(rounded / 3600);
  const minutes = Math.floor((rounded % 3600) / 60);
  const remainder = rounded % 60;
  return hours > 0
    ? `${hours}:${String(minutes).padStart(2, "0")}:${String(remainder).padStart(2, "0")}`
    : `${minutes}:${String(remainder).padStart(2, "0")}`;
};

const fixed = (value: number | null, digits: number, suffix = ""): string =>
  value !== null && Number.isFinite(value) ? `${value.toFixed(digits)}${suffix}` : "--";

const batteryRange = (entry: StintHistoryEntryView): string => {
  const start = entry.battery_start_percent;
  const end = entry.battery_end_percent;
  if (start !== null && end !== null && Number.isFinite(start) && Number.isFinite(end)) {
    return `${Math.round(start)}→${Math.round(end)}%`;
  }
  return fixed(end, 0, "%");
};

const regeneration = (entry: StintHistoryEntryView): string =>
  entry.regeneration_kwh !== null && Number.isFinite(entry.regeneration_kwh)
    ? `${entry.regeneration_kwh.toFixed(1)} kWh`
    : "--";

const compounds = (entry: StintHistoryEntryView): HTMLElement => {
  const cell = document.createElement("span");
  cell.className = "stint-history-compounds";
  const values = entry.tire_compounds.map((value) => value || "?");
  const unique = [...new Set(values)];
  const visible = unique.length === 1 ? unique : values;
  cell.classList.toggle("mixed", visible.length > 1);
  for (const compound of visible) {
    const icon = document.createElement("img");
    icon.src = compoundIconUrl(compound);
    icon.alt = compound;
    icon.title = t("stinthistory.compound", { compound });
    cell.append(icon);
  }
  return cell;
};

const row = (entry: StintHistoryEntryView, usesVirtualEnergy: boolean): HTMLElement => {
  const element = document.createElement("div");
  element.className = "stint-history-row";
  element.role = "row";
  element.classList.toggle("current", entry.current);
  const values = [
    `S${entry.number}`,
    String(entry.laps),
    duration(entry.time_seconds),
    fixed(entry.resource_used, 1, usesVirtualEnergy ? "%" : " L"),
    batteryRange(entry),
    regeneration(entry)
  ];
  for (const [index, value] of values.entries()) {
    const cell = document.createElement("b");
    cell.role = "cell";
    if (index === 4 || index === 5) cell.className = "stint-history-hybrid-cell";
    cell.textContent = value;
    element.append(cell);
  }
  element.append(compounds(entry));
  for (const value of [
    fixed(entry.tire_wear_percent, 1, "%"),
    fixed(entry.delta_seconds, 3),
    fixed(entry.consistency_percent, 1, "%")
  ]) {
    const cell = document.createElement("b");
    cell.role = "cell";
    cell.textContent = value;
    element.append(cell);
  }
  return element;
};

const render = (model: StintHistoryViewModel): void => {
  card?.setAttribute("data-state", model.available ? "active" : "waiting");
  card?.setAttribute("data-hybrid", model.hybrid_available ? "true" : "false");
  const visibleEntries = model.entries.slice(0, 2);
  setText(count, visibleEntries[0] ? `S${visibleEntries[0].number}` : "--");
  setText(resourceLabel, t(model.uses_virtual_energy ? "stinthistory.energy" : "stinthistory.fuel"));
  empty?.toggleAttribute("hidden", model.available);
  const signature = JSON.stringify([model.uses_virtual_energy, model.hybrid_available, visibleEntries]);
  if (rows && rows.dataset.signature !== signature) {
    rows.dataset.signature = signature;
    rows.replaceChildren(...visibleEntries.map((entry) => row(entry, model.uses_virtual_energy)));
    resizeOverlay();
  }
};

void listenTelemetry((frame) => performance.measure(() => render(frame.stint_history_model)));
if (!isTauriRuntime()) {
  render({
    available: true,
    uses_virtual_energy: true,
    hybrid_available: true,
    entries: [
      {
        number: 3, current: true, laps: 11, time_seconds: 2_371, resource_used: 42.8,
        battery_start_percent: 78, battery_end_percent: 64, regeneration_kwh: 2.4,
        tire_wear_percent: 12.4, tire_compounds: ["M", "M", "M", "M"],
        delta_seconds: 0.684, consistency_percent: 99.7
      },
      {
        number: 2, current: false, laps: 18, time_seconds: 3_902, resource_used: 69.5,
        battery_start_percent: 91, battery_end_percent: 73, regeneration_kwh: 4.1,
        tire_wear_percent: 21.7, tire_compounds: ["H", "H", "H", "H"],
        delta_seconds: 1.142, consistency_percent: 99.5
      }
    ]
  });
}
