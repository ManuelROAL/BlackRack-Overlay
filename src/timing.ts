import "./styles.css";
import "./timing.css";
import { bindOverlayTransparency } from "./overlay-appearance";
import { fitOverlay } from "./overlay-fit";
import { bindOverlayInteractionMode } from "./overlay-interaction";
import { createOverlayPerformanceTracker } from "./overlay-performance";
import { isTauriRuntime, listenRuntimeEvent, listenTelemetry } from "./runtime-events";
import { readTimingSettings, TIMING_TIMES, type TimingSettings } from "./timing-settings";
import type { TimingViewModel } from "./telemetry-types";
import { formatNumber, t } from "./i18n";

bindOverlayTransparency("timing");
bindOverlayInteractionMode();
const performance = createOverlayPerformanceTracker("timing");
let settings = readTimingSettings();
const designWidth = 250;
const visibleTimeCount = (): number => TIMING_TIMES.filter(({ id }) => settings.times[id]).length;
const designHeight = (): number => 117 + visibleTimeCount() * 19
  + (settings.historyLaps === 0 ? 0 : settings.historyLaps === 5 ? 70 : 42);
const resizeOverlay = fitOverlay(
  { width: designWidth, height: designHeight() },
  { heightTextRatio: 0.3 }
);
const card = document.getElementById("timing-card");
const times = document.getElementById("timing-times");
const lapNumber = document.getElementById("timing-lap-number");
const current = document.getElementById("timing-current");
const last = document.getElementById("timing-last");
const sessionBest = document.getElementById("timing-session-best");
const personalBest = document.getElementById("timing-personal-best");
const average = document.getElementById("timing-average");
const optimal = document.getElementById("timing-optimal");
const estimated = document.getElementById("timing-estimated");
const history = document.getElementById("timing-history");
const sectorNodes = [...document.querySelectorAll<HTMLElement>("[data-sector]")];

const applyTimingSettings = (): void => {
  for (const { id } of TIMING_TIMES) {
    const row = document.querySelector<HTMLElement>(`[data-time-id="${id}"]`);
    if (row) row.hidden = !settings.times[id];
  }
  if (times) times.hidden = visibleTimeCount() === 0;
  resizeOverlay({ width: designWidth, height: designHeight() });
};
applyTimingSettings();

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
  const totalLaps = Number.isFinite(model.total_laps_estimated) && model.total_laps_estimated > 0
    ? `~${formatNumber(model.total_laps_estimated, 2)}`
    : "~--";
  setText(lapNumber, model.lap_number > 0 ? `${model.lap_number}/${totalLaps}` : `--/${totalLaps}`);
  setText(current, lapTime(model.current_seconds));
  setText(last, lapTime(model.last_seconds));
  last?.parentElement?.toggleAttribute("data-invalid", !model.last_valid);
  setText(sessionBest, lapTime(model.session_personal_best_seconds));
  setText(personalBest, lapTime(model.personal_best_seconds));
  setText(average, lapTime(model.average_seconds));
  setText(optimal, lapTime(model.optimal_seconds));
  setText(estimated, lapTime(model.estimated_seconds));
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
        row.classList.toggle("invalid-lap", !lap.valid);
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
    applyTimingSettings();
  });
}
