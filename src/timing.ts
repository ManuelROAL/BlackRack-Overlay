import "./styles.css";
import "./timing.css";
import { bindOverlayTransparency } from "./overlay-appearance";
import { fitOverlay } from "./overlay-fit";
import { bindOverlayInteractionMode } from "./overlay-interaction";
import { createOverlayPerformanceTracker } from "./overlay-performance";
import { isTauriRuntime, listenRuntimeEvent, listenTelemetry } from "./runtime-events";
import { readTimingSettings, type TimingSettings } from "./timing-settings";
import type { TimingViewModel } from "./telemetry-types";
import { t } from "./i18n";

bindOverlayTransparency("timing");
bindOverlayInteractionMode();
const performance = createOverlayPerformanceTracker("timing");
let settings = readTimingSettings();
const designHeight = (rows: number): number => rows === 0 ? 150 : rows === 5 ? 236 : 202;
const resizeOverlay = fitOverlay({ width: 366, height: designHeight(settings.historyLaps) });
const card = document.getElementById("timing-card");
const delta = document.getElementById("timing-delta");
const current = document.getElementById("timing-current");
const last = document.getElementById("timing-last");
const best = document.getElementById("timing-best");
const history = document.getElementById("timing-history");
const sectorNodes = [...document.querySelectorAll<HTMLElement>("[data-sector]")];

const setText = (element: Element | null, value: string): void => {
  if (element && element.textContent !== value) element.textContent = value;
};
const lapTime = (seconds: number): string => {
  if (!Number.isFinite(seconds) || seconds <= 0) return "--:--.---";
  const minutes = Math.floor(seconds / 60);
  return `${minutes}:${(seconds - minutes * 60).toFixed(3).padStart(6, "0")}`;
};
const sectorTime = (seconds: number): string =>
  Number.isFinite(seconds) && seconds > 0 ? seconds.toFixed(3) : "--.---";

const render = (model: TimingViewModel): void => {
  card?.setAttribute("data-state", model.available ? "active" : "waiting");
  card?.classList.toggle("frozen", model.delta_frozen);
  setText(delta, model.delta_available
    ? `${model.delta_seconds >= 0 ? "+" : "−"}${Math.abs(model.delta_seconds).toFixed(3)}`
    : "---.---");
  delta?.classList.toggle("gain", model.delta_available && model.delta_seconds <= 0);
  delta?.classList.toggle("loss", model.delta_available && model.delta_seconds > 0);
  setText(current, lapTime(model.current_seconds));
  setText(last, lapTime(model.last_seconds));
  setText(best, lapTime(model.best_seconds));
  for (const [index, node] of sectorNodes.entries()) {
    const sector = model.sectors[index];
    node.dataset.state = sector?.state ?? "pending";
    node.classList.toggle("active", index === model.active_sector);
    setText(node.querySelector("b"), sectorTime(sector?.seconds ?? 0));
  }
  if (history) {
    const rows = model.history.slice(0, settings.historyLaps);
    history.hidden = settings.historyLaps === 0;
    const signature = rows.map((lap) => `${lap.number}:${lap.seconds}:${lap.valid}:${lap.state}`).join("|");
    if (history.dataset.signature !== signature) {
      history.dataset.signature = signature;
      history.replaceChildren(...rows.map((lap) => {
        const row = document.createElement("li");
        row.dataset.state = lap.state;
        const label = document.createElement("span");
        label.textContent = t("timing.lap", { number: lap.number });
        const value = document.createElement("b");
        value.textContent = lap.valid ? lapTime(lap.seconds) : t("timing.invalid", { time: lapTime(lap.seconds) });
        row.append(label, value);
        return row;
      }));
    }
  }
};

void listenTelemetry((frame) => performance.measure(() => render(frame.timing_model)));
if (isTauriRuntime()) {
  void listenRuntimeEvent<TimingSettings>("timing://settings", (next) => {
    settings = next;
    resizeOverlay({ width: 366, height: designHeight(settings.historyLaps) });
  });
}
