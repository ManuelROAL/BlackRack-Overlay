import "./pitstop.css";
import type { PitStopMenuChange, TelemetryFrame } from "./telemetry-types";
import { fitOverlayToContent } from "./overlay-fit";
import { bindOverlayInteractionMode } from "./overlay-interaction";
import { bindOverlayTransparency } from "./overlay-appearance";
import { createOverlayPerformanceTracker } from "./overlay-performance";
import { isTauriRuntime, listenRuntimeEvent, listenTelemetry } from "./runtime-events";
import { readPitStopSettings, type PitStopSettings } from "./pitstop-settings";
import { t } from "./i18n";

type PitStopValue = "damage" | "resource" | "tires" | "driver" | "penalty" | "total";

const values = Object.fromEntries(
  Array.from(document.querySelectorAll<HTMLElement>("[data-pitstop-value]")).map((element) => [
    element.dataset.pitstopValue as PitStopValue,
    element
  ])
) as Record<PitStopValue, HTMLElement>;
const resourceLabel = document.querySelector<HTMLElement>("[data-pitstop-resource]")!;
const penaltyRow = document.querySelector<HTMLElement>("[data-pitstop-penalty]")!;
const changes = document.querySelector<HTMLElement>("[data-pitstop-changes]")!;
const changeList = document.querySelector<HTMLUListElement>("[data-pitstop-change-list]")!;
const renderPerformance = createOverlayPerformanceTracker("pitstop");
const shell = document.querySelector<HTMLElement>(".pitstop-shell")!;
const synchronizeOverlayHeight = fitOverlayToContent(210, shell);
let renderedChangesKey = "";
let showChanges = readPitStopSettings().showChanges;
let lastMenuChanges: PitStopMenuChange[] = [];

const formatSeconds = (seconds: number, available = true): string =>
  available && Number.isFinite(seconds) ? `+${Math.max(0, seconds).toFixed(1)}s` : "--.-s";

const setText = (element: HTMLElement, text: string): void => {
  if (element.textContent !== text) element.textContent = text;
};

const renderMenuChanges = (menuChanges: PitStopMenuChange[]): void => {
  lastMenuChanges = menuChanges;
  const key = menuChanges.map(({ label, value }) => `${label}\u0000${value}`).join("\u0001");
  if (key !== renderedChangesKey) {
    renderedChangesKey = key;
    changeList.replaceChildren(
      ...menuChanges.map(({ label, value }) => {
        const item = document.createElement("li");
        const labelElement = document.createElement("span");
        const valueElement = document.createElement("strong");
        labelElement.textContent = label;
        valueElement.textContent = value;
        item.append(labelElement, valueElement);
        return item;
      })
    );
  }
  const visible = showChanges && menuChanges.length > 0;
  if (changes.hidden === visible) {
    changes.hidden = !visible;
    synchronizeOverlayHeight();
  }
};

const render = (frame: TelemetryFrame): void => {
  const available = frame.pit_stop_estimate_available;
  const virtualEnergy = frame.virtual_energy_active;
  const penalty = frame.pit_stop_penalty_seconds;

  setText(resourceLabel, t(virtualEnergy ? "pitstop.energy" : "pitstop.fuel"));
  setText(values.damage, formatSeconds(frame.pit_stop_damage_seconds, available));
  setText(
    values.resource,
    formatSeconds(virtualEnergy ? frame.pit_stop_energy_seconds : frame.pit_stop_fuel_seconds, available)
  );
  setText(values.tires, formatSeconds(frame.pit_stop_tire_seconds, available));
  setText(values.driver, formatSeconds(frame.pit_stop_driver_swap_seconds, available));
  setText(values.penalty, formatSeconds(penalty, available));
  setText(values.total, formatSeconds(frame.pit_stop_estimate_seconds, available));
  renderMenuChanges(frame.pit_stop_menu_changes);
  const showPenalty = available && penalty > 0;
  if (penaltyRow.hidden === showPenalty) {
    penaltyRow.hidden = !showPenalty;
    synchronizeOverlayHeight();
  }
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
  pit_stop_driver_swap_seconds: 0,
  pit_stop_menu_changes: []
} as unknown as TelemetryFrame;

bindOverlayTransparency("pitstop");
bindOverlayInteractionMode();
if (!isTauriRuntime()) render(previewFrame);
void listenTelemetry((frame) => renderPerformance.measure(() => render(frame)));
void listenRuntimeEvent<PitStopSettings>("pitstop://settings", (settings) => {
  if (typeof settings?.showChanges !== "boolean" || showChanges === settings.showChanges) return;
  showChanges = settings.showChanges;
  renderMenuChanges(lastMenuChanges);
});
