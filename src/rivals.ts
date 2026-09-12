import "./styles.css";
import "./rivals.css";
import type { StandingEntry, TelemetryFrame } from "./telemetry-types";
import { fitOverlay } from "./overlay-fit";
import { bindOverlayInteractionMode } from "./overlay-interaction";
import { bindOverlayTransparency } from "./overlay-appearance";
import { listenTelemetry } from "./runtime-events";
import { t } from "./i18n";

const list = document.getElementById("rivals-list")!;
const session = document.querySelector<HTMLElement>("[data-rivals-session]")!;
const lap = document.querySelector<HTMLElement>("[data-rivals-lap]")!;
const resize = fitOverlay({ width: 520, height: 122 }, { widthTextRatio: 0.45, heightTextRatio: 0.3 });

const setText = (element: HTMLElement, value: string): void => { if (element.textContent !== value) element.textContent = value; };
const finite = (value: number): boolean => Number.isFinite(value);
const seconds = (value: number): string => finite(value) ? `${value >= 0 ? "+" : "−"}${Math.abs(value).toFixed(1)}s` : "—";
const lapTime = (value: number): string => finite(value) && value > 0 ? `${Math.floor(value / 60)}:${(value % 60).toFixed(1).padStart(4, "0")}` : "—";
const sessionLabel = (value: number): string => {
  if (value >= 10 && value <= 13) return value === 10 ? t("session.race") : t("session.raceNumber", { number: value - 9 });
  if (value >= 5 && value <= 8) return value === 5 ? t("session.qualifying") : t("session.qualifyingNumber", { number: value - 4 });
  if (value === 9) return t("session.warmup");
  return value === 0 ? t("session.practice") : t("session.practiceNumber", { number: value });
};

interface RivalSlot {
  entry: StandingEntry;
  direction: "ahead" | "behind" | "player";
  gap: number;
}

const nearest = (entries: StandingEntry[], player: StandingEntry): RivalSlot[] => {
  const active = entries.filter((entry) => !entry.in_garage && entry.vehicle_id !== player.vehicle_id);
  const ahead = active.filter((entry) => finite(entry.relative_ahead_seconds) && entry.relative_ahead_seconds < -0.05)
    .sort((a, b) => Math.abs(a.relative_ahead_seconds) - Math.abs(b.relative_ahead_seconds))
    .slice(0, 3).reverse()
    .map((entry) => ({ entry, direction: "ahead" as const, gap: entry.relative_ahead_seconds }));
  const behind = active.filter((entry) => finite(entry.relative_behind_seconds) && entry.relative_behind_seconds > 0.05)
    .sort((a, b) => a.relative_behind_seconds - b.relative_behind_seconds).slice(0, 3)
    .map((entry) => ({ entry, direction: "behind" as const, gap: entry.relative_behind_seconds }));
  return [...ahead, { entry: player, direction: "player", gap: 0 }, ...behind];
};

const signal = (className: string, text: string): HTMLSpanElement => {
  const element = document.createElement("span"); element.className = `rival-signal ${className}`; element.textContent = text; return element;
};
const row = ({ entry, direction, gap }: RivalSlot): HTMLElement => {
  const element = document.createElement("article");
  element.className = `rival-row ${direction}`;
  const driver = document.createElement("strong"); driver.className = "rival-driver"; driver.textContent = entry.driver_name;
  const gapCell = document.createElement("span"); gapCell.className = "rival-cell rival-gap"; gapCell.textContent = entry.is_player ? "—" : seconds(gap);
  const lapCell = document.createElement("span"); lapCell.className = "rival-cell rival-lap"; lapCell.textContent = `L${entry.total_laps}`;
  const pace = document.createElement("span"); pace.className = "rival-cell rival-pace"; pace.textContent = lapTime(entry.last_lap_seconds > 0 ? entry.last_lap_seconds : entry.average_lap_seconds);
  const signals = document.createElement("span"); signals.className = "rival-cell rival-signals";
  if (entry.in_pits) signals.append(signal("pit", "PIT"));
  else if (entry.pit_stop_requested) signals.append(signal("pit-requested", "REQ"));
  else if (finite(entry.pit_stop_time_seconds ?? Number.NaN)) {
    signals.append(signal("pit", `P${Math.round(entry.pit_stop_time_seconds ?? 0)}s`));
  }
  if (entry.penalty_count > 0) signals.append(signal("penalty", `!${entry.penalty_count}`));
  if (entry.damage_percent >= 50) signals.append(signal("damage", `D${Math.round(entry.damage_percent)}`));
  else if (entry.damage_percent > 0) signals.append(signal("damage-light", `D${Math.round(entry.damage_percent)}`));
  element.append(driver, gapCell, lapCell, pace, signals); return element;
};

const render = (frame: TelemetryFrame): void => {
  setText(session, sessionLabel(frame.session_type)); setText(lap, `L${frame.lap_number}`);
  const player = frame.standings.find((entry) => entry.is_player);
  if (!player) {
    list.dataset.empty = "true";
    const waiting = document.createElement("p"); waiting.className = "empty-state"; waiting.textContent = t("rivals.waiting");
    list.replaceChildren(waiting); resize({ width: 520, height: 82 }); return;
  }
  const entries = nearest(frame.standings, player); list.dataset.empty = "false";
  const head = document.createElement("div"); head.className = "rivals-table-head";
  for (const label of ["", t("rivals.gap"), t("rivals.lap"), t("rivals.pace"), "ST"]) { const cell = document.createElement("span"); cell.dataset.label = label; head.append(cell); }
  list.replaceChildren(head, ...entries.map(row)); resize({ width: 520, height: 58 + entries.length * 31 });
};

bindOverlayTransparency("rivals");
bindOverlayInteractionMode();
void listenTelemetry(render);
