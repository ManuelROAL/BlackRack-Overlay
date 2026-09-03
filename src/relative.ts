import "./styles.css";
import "./relative.css";
import { fitOverlay } from "./overlay-fit";
import { bindOverlayInteractionMode } from "./overlay-interaction";
import { createOverlayPerformanceTracker } from "./overlay-performance";
import { bindOverlayTransparency } from "./overlay-appearance";
import type { StandingEntry, TelemetryFrame } from "./telemetry-types";
import {
  readRelativeSettings,
  visibleRelativeColumns,
  type RelativeColumnId,
  type RelativeSettings
} from "./relative-settings";
import { listenRuntimeEvent, listenTelemetry } from "./runtime-events";
import { airTemperatureIconUrl, clockIconUrl, compoundIconUrl, trackTemperatureIconUrl, worldIconUrl } from "./lmu-icons";
import { applyTrackLimitTone, formatTrackLimitPoints } from "./track-limit-tone";
import { formatDriverName } from "./driver-name-format";
import { isRaceSession } from "./session-phase";
import { formatClock as formatRealClock, formatNumber, formatTimeOfDay, t, type TranslationKey } from "./i18n";

let relativeSettings = readRelativeSettings();
// V es el contador de vueltas de la sesión: fuera de carrera cada piloto lleva
// las suyas desde que entró y el número no relaciona a dos filas contiguas.
// La configuración del usuario se conserva y vuelve sola al empezar la carrera.
const RACE_ONLY_COLUMNS = new Set<RelativeColumnId>(["lap"]);
let raceSession = true;
// Ancho de la pista del cambio de posición más su separación dentro de la
// celda: sin indicador la columna sobra justo eso y dejaría el hueco vacío.
const POSITION_CHANGE_WIDTH = 19;
const positionChangeVisible = () => raceSession && relativeSettings.options.positionChange;
const activeColumns = () =>
  visibleRelativeColumns(relativeSettings)
    .filter(({ id }) => raceSession || !RACE_ONLY_COLUMNS.has(id))
    .map((column) => column.id === "position" && !positionChangeVisible()
      ? { ...column, width: column.width - POSITION_CHANGE_WIDTH }
      : column);
const columnExpansionRatio = (id: RelativeColumnId): number => {
  if (id === "driver") return 0.5;
  if (id === "country" || id === "badge" || id === "tire") return 0.5;
  if (id === "position") return 0.75;
  if (id === "number") return 0.5;
  return 1;
};
const relativeBaseHeight = (): number => Math.max(
  220,
  48 + (relativeSettings.aheadRows + relativeSettings.behindRows + 1) * 23
);
const relativeBaseWidth = (): number => Math.max(
  344,
  activeColumns().reduce((total, { width }) => total + width, 0) + 32
);
const expandableColumnsWidth = (): number => activeColumns()
  .reduce((total, { id, width }) => total + width * columnExpansionRatio(id), 0);
const updateOverlayFit = fitOverlay({
  width: relativeBaseWidth(),
  height: relativeBaseHeight()
}, { widthTextRatio: () => expandableColumnsWidth() / relativeBaseWidth(), heightTextRatio: 0.35 });
bindOverlayTransparency("relative");
const renderPerformance = createOverlayPerformanceTracker("relative");

let lastFrame: TelemetryFrame | null = null;

const columnLength = ({ id, width }: { id: RelativeColumnId; width: number }): string => {
  const ratio = columnExpansionRatio(id);
  if (ratio === 0) return `${width}px`;
  if (ratio === 1) return `calc(${width}px * var(--overlay-font-track-expansion, 1))`;
  return `calc(${width * (1 - ratio)}px + ${width * ratio}px * var(--overlay-font-track-expansion, 1))`;
};
const columnsLength = (columns: ReadonlyArray<{ id: RelativeColumnId; width: number }>): string => {
  const fixedWidth = columns.reduce(
    (total, { id, width }) => total + width * (1 - columnExpansionRatio(id)),
    0
  );
  const expandableWidth = columns
    .reduce((total, { id, width }) => total + width * columnExpansionRatio(id), 0);
  if (fixedWidth === 0) return `calc(${expandableWidth}px * var(--overlay-font-track-expansion, 1))`;
  if (expandableWidth === 0) return `${fixedWidth}px`;
  return `calc(${fixedWidth}px + ${expandableWidth}px * var(--overlay-font-track-expansion, 1))`;
};

const applyColumnLayout = (): void => {
  const columns = activeColumns();
  const shell = document.querySelector<HTMLElement>(".standings-shell");
  shell?.style.setProperty(
    "--standings-grid-columns",
    columns.map(columnLength).join(" ")
  );
  const signalsVisible = columns.some(({ id }) => id === "signals");
  const signalsLast = columns.at(-1)?.id === "signals";
  const dataColumns = signalsLast ? columns.filter(({ id }) => id !== "signals") : columns;
  shell?.style.setProperty(
    "--standings-content-width",
    `calc(${columnsLength(dataColumns)} + 8px)`
  );
  shell?.style.setProperty(
    "--standings-row-background-width",
    `calc(${columnsLength(dataColumns)} + ${signalsVisible && signalsLast ? 4 : 8}px)`
  );
};

applyColumnLayout();

interface LivePenalties {
  DT?: number;
  SG?: number;
  TIME?: number;
}

interface LiveStanding {
  slotID?: number;
  penalties?: LivePenalties;
  carNumber?: string | number;
  vehicleNumber?: string | number;
}

const livePenalties = new Map<number, LivePenalties>();
const liveCarNumbers = new Map<number, string>();

const connectLiveStandings = (): void => {
  const socket = new WebSocket("ws://localhost:6398/websocket/ui");
  socket.addEventListener("open", () => {
    socket.send(JSON.stringify({ messageType: "SUB", topic: "LiveStandings" }));
  });
  socket.addEventListener("message", ({ data }) => {
    if (typeof data !== "string") return;
    try {
      const message = JSON.parse(data) as { topic?: string; body?: LiveStanding[] };
      if (message.topic !== "LiveStandings" || !Array.isArray(message.body)) return;
      livePenalties.clear();
      liveCarNumbers.clear();
      for (const standing of message.body) {
        if (!Number.isInteger(standing.slotID)) continue;
        const slotID = standing.slotID as number;
        if (standing.penalties) {
          livePenalties.set(slotID, standing.penalties);
        }
        const sessionNumber = standing.vehicleNumber ?? standing.carNumber;
        if (sessionNumber !== undefined && String(sessionNumber).trim()) {
          liveCarNumbers.set(slotID, String(sessionNumber).trim());
        }
      }
    } catch {
      // LMU puede cerrar el canal entre sesiones; conservamos la telemetría base.
    }
  });
  socket.addEventListener("close", () => window.setTimeout(connectLiveStandings, 2_000));
  socket.addEventListener("error", () => socket.close());
};

connectLiveStandings();

const node = <K extends keyof HTMLElementTagNameMap>(
  tag: K,
  className?: string,
  content?: string
): HTMLElementTagNameMap[K] => {
  const element = document.createElement(tag);
  if (className) element.className = className;
  if (content !== undefined) element.textContent = content;
  return element;
};

const clockIcon = (source: string): HTMLImageElement => {
  const image = node("img", "relative-timing-icon");
  image.src = source;
  image.width = 12;
  image.height = 12;
  image.alt = "";
  image.setAttribute("aria-hidden", "true");
  return image;
};

const temperatureIcon = (source: string): HTMLImageElement => {
  const image = node("img", "relative-temperature-icon");
  image.src = source;
  image.width = 12;
  image.height = 12;
  image.alt = "";
  image.setAttribute("aria-hidden", "true");
  return image;
};

const icon = (source: string, className: string): HTMLImageElement => {
  const image = node("img", className);
  image.src = source;
  image.alt = "";
  image.setAttribute("aria-hidden", "true");
  return image;
};

const decimal = (value: number, digits = 2): string => formatNumber(value, digits);

const formatLapTime = (seconds: number): string => {
  if (!Number.isFinite(seconds) || seconds <= 0) return "--";
  const minutes = Math.floor(seconds / 60);
  return `${minutes}:${(seconds % 60).toFixed(3).padStart(6, "0")}`;
};

const countryFlagRasterModules = import.meta.glob<string>(
  "./assets/countries-raster/*.png",
  { eager: true, query: "?url", import: "default" }
);

const countryFlagModules = import.meta.glob<string>(
  "./assets/countries/*.{svg,png}",
  { eager: true, query: "?url", import: "default" }
);

// The overlays draw flags into a box of a few pixels. `tools/rasterize-icons.mjs`
// pre-renders them so WebView2 never parses a vector document to fill it; the
// SVG source is only reached for an icon that has not been rasterised yet.
const countryFlagSource = (code: string): string | undefined =>
  countryFlagRasterModules[`./assets/countries-raster/${code}.png`]
    ?? countryFlagModules[`./assets/countries/${code}.svg`]
    ?? countryFlagModules[`./assets/countries/${code}.png`];

const countryCode = (nationality: string): string => {
  const aliases: Record<string, string> = {
    ARG: "AR", AUS: "AU", AUT: "AT", BEL: "BE", BRA: "BR", CAN: "CA",
    CHE: "CH", CHL: "CL", CHN: "CN", COL: "CO", CZE: "CZ", DEU: "DE",
    DNK: "DK", ESP: "ES", FIN: "FI", FRA: "FR", GBR: "GB", GER: "DE",
    HUN: "HU", IRL: "IE", ITA: "IT", JPN: "JP", MEX: "MX", NLD: "NL",
    NOR: "NO", NZL: "NZ", POL: "PL", PRT: "PT", SWE: "SE", USA: "US",
    ARGENTINA: "AR", AUSTRALIA: "AU", AUSTRIA: "AT", BELGIUM: "BE",
    BRAZIL: "BR", CANADA: "CA", CHILE: "CL", CHINA: "CN", COLOMBIA: "CO",
    DENMARK: "DK", ENGLAND: "GB", FINLAND: "FI", FRANCE: "FR",
    GERMANY: "DE", HUNGARY: "HU", IRELAND: "IE", ITALY: "IT", JAPAN: "JP",
    MEXICO: "MX", NETHERLANDS: "NL", NORWAY: "NO", POLAND: "PL",
    PORTUGAL: "PT", SPAIN: "ES", SWEDEN: "SE", SWITZERLAND: "CH",
    "UNITED KINGDOM": "GB", "UNITED STATES": "US"
  };
  const raw = nationality.trim().toUpperCase();
  const code = raw.length === 2 ? raw : aliases[raw];
  return code && /^[A-Z]{2}$/.test(code) ? code : "";
};

const countryFlag = (nationality: string): HTMLImageElement | undefined => {
  const code = countryCode(nationality);
  const source = countryFlagSource(code);
  if (!code || !source) return undefined;
  const fallbackSource = countryFlagSource("XX");
  const flag = node("img", "country-flag");
  flag.src = source;
  flag.alt = code;
  flag.title = nationality;
  let fallbackActive = false;
  flag.addEventListener("error", () => {
    if (!fallbackActive && fallbackSource && source !== fallbackSource) {
      fallbackActive = true;
      flag.src = fallbackSource;
      flag.alt = "XX";
      flag.title = t("relative.flagUnavailable", { nationality });
      return;
    }
    flag.remove();
  });
  return flag;
};

const classTone = (vehicleClass: string): string => {
  const value = vehicleClass.toUpperCase();
  if (value.includes("HYPER") || value.includes("GTP")) return "hypercar";
  if (value.includes("LMP2")) return "lmp2";
  if (value.includes("LMP3")) return "lmp3";
  if (value.includes("LMGT3") || value.includes("GT3")) return "lmgt3";
  return "other";
};

const driverBadgeModules = import.meta.glob<string>(
  "./assets/driver-badges/*.svg",
  { eager: true, query: "?url", import: "default" }
);

const DRIVER_BADGE_LABELS: Record<string, TranslationKey> = {
  "sr-noob": "badge.noob",
  "sr-rookie": "badge.noob",
  "sr-probation": "badge.probation",
  "sr-warning": "badge.warning",
  "sr-danger": "badge.danger",
  "sr-clean": "badge.clean",
  "sr-saint": "badge.saint",
  "content-creator": "badge.creator",
  "irl-driver": "badge.realDriver",
  "early-access": "badge.earlyAccess",
  "test-driver": "badge.testDriver"
};

const driverBadge = (name: string): HTMLElement => {
  const cell = node("span", "standing-driver-badge");
  const normalized = name.trim().toLowerCase();
  if (!normalized) return cell;
  const source = driverBadgeModules[`./assets/driver-badges/${normalized}.svg`];
  const labelKey = DRIVER_BADGE_LABELS[normalized];
  const label = normalized === "s397" ? "Studio 397" : labelKey ? t(labelKey) : normalized;
  cell.title = label;
  cell.setAttribute("aria-label", label);
  cell.dataset.badge = normalized;
  if (source) {
    const image = node("img", "driver-badge-image");
    image.src = source;
    image.alt = "";
    cell.append(image);
  } else {
    cell.append(node("span", "driver-badge-fallback", "◆"));
  }
  return cell;
};

const positionChange = (value: number): HTMLElement => {
  const change = node("span", "position-change");
  if (value > 0) {
    change.classList.add("gained");
    change.textContent = `▲${value}`;
  } else if (value < 0) {
    change.classList.add("lost");
    change.textContent = `▼${Math.abs(value)}`;
  } else {
    change.textContent = "–";
  }
  return change;
};

const rankBadge = (
  label: "DR" | "SR",
  rank: string,
  progress = -1,
  estimatedGain = 0,
  estimatedGainAvailable = false
): HTMLElement => {
  const badge = node("span", `driver-rank driver-rank--${label.toLowerCase()}`);
  badge.append(node("b", "rank-code", rank));
  if (label === "DR" && progress >= 0) {
    badge.append(node("span", "rank-progress", `${Math.round(progress)}%`));
  }
  if (label === "DR" && estimatedGainAvailable) {
    const rounded = Math.round(estimatedGain);
    const gain = node("span", "rank-estimate", `${Math.abs(rounded)}%`);
    gain.dataset.direction = rounded > 0 ? "positive" : rounded < 0 ? "negative" : "neutral";
    badge.append(gain);
  }
  badge.title = `${label} ${rank}`;
  badge.setAttribute("aria-label", `${label} ${rank}`);
  badge.dataset.rank = rank.slice(0, 1).toLowerCase();
  return badge;
};

const signals = (entry: StandingEntry): HTMLElement => {
  const container = node("div", "standing-signals");
  if (raceSession && entry.finish_status === 1) {
    const checkered = node("span", "race-flag checkered-flag");
    checkered.title = t("standings.finished");
    checkered.setAttribute("aria-label", t("standings.checkeredAria"));
    container.append(checkered);
    return container;
  }
  if (raceSession && entry.finish_status === 3) container.append(node("span", "race-flag dq-flag", "DQ"));
  else if (raceSession && entry.finish_status === 2) container.append(node("span", "race-flag dnf-flag", "DNF"));
  if (entry.in_garage) container.append(node("span", "race-flag garage-flag", "GAR"));
  else if (entry.is_out_lap) container.append(node("span", "race-flag out-lap-flag", "OUT"));
  if (entry.causing_yellow) container.append(node("span", "race-flag yellow-flag", "Y"));
  const penalties = livePenalties.get(entry.vehicle_id);
  if ((penalties?.DT ?? 0) > 0) container.append(node("span", "race-flag penalty-flag", "DT"));
  if ((penalties?.SG ?? 0) > 0) container.append(node("span", "race-flag penalty-flag", "SG"));
  if (!penalties && entry.penalty_count > 0) {
    container.append(node("span", "race-flag penalty-flag", "PEN"));
  }
  if (entry.flag > 0 && entry.flag !== 6) {
    container.append(node("span", "race-flag red-flag", "F"));
  }
  return container;
};

interface CachedCell {
  element: HTMLElement;
  signature: string;
}

interface CachedRow {
  element: HTMLElement;
  cells: Map<RelativeColumnId, CachedCell>;
  columns: string;
}

const rowCache = new Map<string, CachedRow>();

const pitTimeLabel = (seconds: number): string => {
  const rounded = Math.round(Math.max(0, seconds));
  const minutes = Math.floor(rounded / 60);
  const remainder = rounded % 60;
  return minutes > 0 ? `${minutes}m${remainder.toString().padStart(2, "0")}s` : `${remainder}s`;
};

const appendDriverPitStatus = (cell: HTMLElement, entry: StandingEntry): void => {
  if (!relativeSettings.options.pitStops) return;
  const summary = node("span", "driver-pit-summary");
  if (entry.in_pits) {
    if (entry.pit_stop_time_seconds !== null) {
      const time = pitTimeLabel(entry.pit_stop_time_seconds);
      summary.classList.add("timing");
      summary.append(node("b", "driver-pit-time", time));
      summary.title = t("standings.pitTimer", { time });
    }
  } else if (entry.pit_stop_requested) {
    summary.classList.add("requested");
    summary.append(node("b", "driver-pit-requested", "PIT"));
    summary.title = t("standings.pitRequested", { count: entry.pit_stops });
  } else if (raceSession && entry.pit_stop_time_seconds !== null && entry.pit_stop_lap !== null && entry.pit_stops > 0) {
    const time = pitTimeLabel(entry.pit_stop_time_seconds);
    summary.append(
      node("b", "driver-pit-lap", `L${entry.pit_stop_lap}`),
      node("b", "driver-pit-time", time),
      node("b", "driver-pit-count", entry.pit_stops.toString())
    );
    summary.title = t("standings.pitSummary", {
      count: entry.pit_stops,
      lap: entry.pit_stop_lap,
      time
    });
  }
  if (summary.childElementCount > 0) cell.append(summary);
};

const cellSignature = (entry: StandingEntry, column: RelativeColumnId, trackLimit: number, relativeGapSeconds = entry.relative_gap_seconds): string => {
  switch (column) {
    case "position": return `${entry.position}|${raceSession ? entry.position_change : ""}|${relativeSettings.options.positionChange}`;
    case "number": return liveCarNumbers.get(entry.vehicle_id) || entry.car_number || "--";
    case "country": return entry.nationality;
    case "badge": return entry.driver_badge;
    case "driver": return `${entry.driver_name}|${entry.nationality}|${relativeSettings.driverNameFormat}|${relativeSettings.options.pitStops}|${raceSession}|${entry.in_pits}|${entry.pit_stop_requested}|${entry.pit_stops}|${entry.pit_stop_lap ?? ""}|${entry.pit_stop_time_seconds === null ? "" : pitTimeLabel(entry.pit_stop_time_seconds)}`;
    case "ranks": return `${entry.driver_rank}|${Math.round(entry.driver_rank_progress)}|${Math.round(entry.estimated_driver_rank_gain)}|${entry.estimated_driver_rank_gain_available}|${entry.safety_rank}`;
    case "relative": return relativeGapSeconds.toFixed(2);
    case "lap": return entry.total_laps.toString();
    case "best": return `${formatLapTime(entry.best_lap_seconds)}|${entry.best_lap_seconds > 0}|${entry.has_fastest_lap}`;
    case "last": return `${entry.is_out_lap ? "OUT" : formatLapTime(entry.last_lap_seconds)}|${formatLapTime(entry.best_lap_seconds)}|${entry.has_fastest_lap}|${entry.last_lap_valid}`;
    case "average": return formatLapTime(entry.average_lap_seconds);
    case "energy": return raceSession && entry.virtual_energy_active && entry.virtual_energy_percent > 0
      ? `${entry.virtual_energy_percent >= 99.95 ? "100" : decimal(entry.virtual_energy_percent, 1)}|${entry.virtual_energy_per_lap > 0 ? decimal(entry.virtual_energy_per_lap, 2) : ""}`
      : "--";
    case "damage": return Math.round(entry.damage_percent).toString();
    case "trackLimits": return entry.track_limits_steps === null ? "--" : `${entry.track_limits_steps}|${trackLimit}`;
    case "pitStops": return `${entry.pit_stops}|${entry.pit_stop_requested}|${entry.pit_stop_time_seconds === null ? "" : pitTimeLabel(entry.pit_stop_time_seconds)}`;
    case "tire": return entry.tire_compounds.join("/");
    case "signals": {
      const penalties = livePenalties.get(entry.vehicle_id);
      return `${raceSession ? entry.finish_status : 0}|${entry.in_garage}|${entry.is_out_lap}|${entry.causing_yellow}|${entry.penalty_count}|${entry.flag}|${penalties?.DT ?? 0}|${penalties?.SG ?? 0}`;
    }
  }
};

const createCell = (entry: StandingEntry, column: RelativeColumnId, trackLimit: number, relativeGapSeconds = entry.relative_gap_seconds): HTMLElement => {
  switch (column) {
    case "position": {
      const cell = node("div", "standing-position-cell");
      cell.append(node("strong", "standing-position", entry.position.toString()));
      // Sin parrilla no hay referencia para el cambio de posición.
      if (positionChangeVisible()) cell.append(positionChange(entry.position_change));
      return cell;
    }
    case "number": return node("span", "car-number", liveCarNumbers.get(entry.vehicle_id) || entry.car_number || "--");
    case "country": {
      const cell = node("span", "standing-country");
      const flag = countryFlag(entry.nationality);
      if (flag) cell.append(flag);
      else cell.textContent = "--";
      return cell;
    }
    case "badge": return driverBadge(entry.driver_badge);
    case "driver": {
      const cell = node("div", "standing-driver");
      const fullName = entry.driver_name || "—";
      const name = node("b", "driver-name");
      name.append(node("span", "driver-name-text", formatDriverName(fullName, relativeSettings.driverNameFormat)));
      name.title = fullName;
      cell.append(name);
      appendDriverPitStatus(cell, entry);
      return cell;
    }
    case "ranks": {
      const cell = node("span", "driver-ranks");
      if (entry.driver_rank) cell.append(rankBadge("DR", entry.driver_rank, entry.driver_rank_progress, entry.estimated_driver_rank_gain, entry.estimated_driver_rank_gain_available));
      if (entry.safety_rank) cell.append(rankBadge("SR", entry.safety_rank));
      return cell;
    }
    case "relative": {
      const gap = entry.is_player ? decimal(0, 1) : `${relativeGapSeconds > 0 ? "+" : ""}${decimal(relativeGapSeconds, 1)}`;
      return node("b", "standing-relative", gap);
    }
    case "lap": return node("b", "standing-lap-number", entry.total_laps.toString());
    case "best": {
      const cell = node("b", "lap-time best-time", formatLapTime(entry.best_lap_seconds));
      if (entry.best_lap_seconds > 0) cell.classList.add("personal-best");
      if (entry.has_fastest_lap) cell.classList.add("session-fastest");
      return cell;
    }
    case "last": {
      const cell = entry.is_out_lap ? node("b", "lap-time out-lap", "OUT") : node("b", "lap-time", formatLapTime(entry.last_lap_seconds));
      const invalid = !entry.is_out_lap && entry.last_lap_seconds > 0 && !entry.last_lap_valid;
      const personalBest = entry.last_lap_seconds > 0 && entry.best_lap_seconds > 0 && Math.abs(entry.last_lap_seconds - entry.best_lap_seconds) <= 0.001;
      if (invalid) cell.classList.add("invalid-lap");
      else if (!entry.is_out_lap && personalBest) cell.classList.add(entry.has_fastest_lap ? "session-fastest" : "personal-best");
      return cell;
    }
    case "average": return node("b", "lap-time", formatLapTime(entry.average_lap_seconds));
    case "energy": {
      const cell = node("div", "standing-energy");
      if (raceSession && entry.virtual_energy_active && entry.virtual_energy_percent > 0) {
        const percentage = entry.virtual_energy_percent >= 99.95 ? "100%" : `${decimal(entry.virtual_energy_percent, 1)}%`;
        cell.append(node("b", undefined, percentage));
        if (entry.virtual_energy_per_lap > 0) {
          cell.append(node("span", undefined, ` ${decimal(entry.virtual_energy_per_lap, 2)}%`));
        }
      } else cell.textContent = "--";
      return cell;
    }
    case "damage": {
      const cell = node("span", "standing-damage", `${Math.round(entry.damage_percent)}%`);
      if (entry.damage_percent >= 50) cell.classList.add("heavy");
      else if (entry.damage_percent > 0) cell.classList.add("damaged");
      return cell;
    }
    case "trackLimits": {
      const value = entry.track_limits_steps === null ? "--" : formatTrackLimitPoints(entry.track_limits_steps);
      const cell = node("span", "standing-track-limits", value);
      cell.title = entry.track_limits_steps === null
        ? t("standings.trackLimitsUnavailable")
        : t("standings.trackLimitPoints", { value });
      if (entry.track_limits_steps !== null) applyTrackLimitTone(cell, entry.track_limits_steps, trackLimit);
      return cell;
    }
    case "pitStops": {
      const timing = entry.pit_stop_time_seconds !== null;
      const value = timing ? pitTimeLabel(entry.pit_stop_time_seconds ?? 0) : entry.pit_stops.toString();
      const cell = node("strong", "standing-pit-stops", value);
      cell.classList.toggle("timing", timing);
      cell.classList.toggle("requested", entry.pit_stop_requested && !timing);
      return cell;
    }
    case "tire": {
      const compounds = entry.tire_compounds.length === 4 ? entry.tire_compounds : [entry.tire_compound];
      const unique = [...new Set(compounds)];
      if (unique.length === 1) {
        const compound = unique[0] || "?";
        const cell = node("span", "tire-compound");
        cell.dataset.compound = compound.toLowerCase();
        cell.title = t("standings.compound", { compound });
        cell.append(icon(compoundIconUrl(compound), "tire-compound-icon"));
        return cell;
      }
      const grid = node("span", "tire-compound-grid");
      for (const compound of compounds) {
        const wheel = node("span", "tire-wheel");
        wheel.dataset.compound = compound.toLowerCase();
        wheel.title = t("standings.compound", { compound: compound || "?" });
        grid.append(wheel);
      }
      return grid;
    }
    case "signals": return signals(entry);
  }
};

type RelativeLapRelation = TelemetryFrame["relative_model"]["rows"][number]["lap_relation"];
type RelativeRowKind = TelemetryFrame["relative_model"]["rows"][number]["kind"];

const lapRelationLabel = (relation: RelativeLapRelation, kind: RelativeRowKind): string => {
  if (relation === "player_ahead") return t("relative.playerLapped");
  if (relation === "opponent_ahead") {
    return kind === "behind" ? t("relative.lappingPlayer") : t("relative.lappedPlayer");
  }
  return "";
};

const renderRow = (
  entry: StandingEntry,
  trackLimit: number,
  instanceKey: string,
  lapRelation: RelativeLapRelation,
  rowKind: RelativeRowKind,
  relativeGapSeconds = entry.relative_gap_seconds
): HTMLElement => {
  const columns = activeColumns().map(({ id }) => id);
  const columnsKey = columns.join("|");
  let cached = rowCache.get(instanceKey);
  if (!cached) {
    cached = { element: node("div", "standing-row"), cells: new Map(), columns: "" };
    cached.element.dataset.vehicleId = entry.vehicle_id.toString();
    rowCache.set(instanceKey, cached);
  }
  cached.element.classList.toggle("player", entry.is_player);
  cached.element.classList.toggle("in-pits", entry.in_pits);
  cached.element.classList.toggle("out-lap", entry.is_out_lap && !entry.in_pits);
  cached.element.classList.toggle("damaged", entry.damage_percent > 0);
  cached.element.classList.toggle("heavily-damaged", entry.damage_percent >= 50);
  cached.element.dataset.classTone = classTone(entry.vehicle_class);
  cached.element.dataset.lapRelation = lapRelation;
  const relationLabel = lapRelationLabel(lapRelation, rowKind);
  cached.element.title = relationLabel;
  if (relationLabel) cached.element.setAttribute("aria-label", relationLabel);
  else cached.element.removeAttribute("aria-label");

  for (const column of columns) {
    const signature = cellSignature(entry, column, trackLimit, relativeGapSeconds);
    const previous = cached.cells.get(column);
    if (!previous || previous.signature !== signature) {
      const element = createCell(entry, column, trackLimit, relativeGapSeconds);
      if (previous && cached.columns === columnsKey) previous.element.replaceWith(element);
      cached.cells.set(column, { element, signature });
    }
  }
  if (cached.columns !== columnsKey) {
    cached.element.replaceChildren(...columns.map((column) => cached!.cells.get(column)!.element));
    cached.columns = columnsKey;
  }
  return cached.element;
};


const sessionHeader = (frame: TelemetryFrame): HTMLElement => {
  const header = node("header", "relative-table-header");
  if (frame.rest_weather_available && relativeSettings.options.airTemperature) {
    const ambient = node("span", "relative-temperature relative-temperature-air");
    ambient.append(temperatureIcon(airTemperatureIconUrl), `${Math.round(frame.ambient_temperature_c)}°C`);
    ambient.title = t("standings.airTemperatureTitle", { value: decimal(frame.ambient_temperature_c, 1) });
    header.append(ambient);
  }
  if (frame.rest_weather_available && relativeSettings.options.trackTemperature) {
    const track = node("span", "relative-temperature relative-temperature-track");
    track.append(temperatureIcon(trackTemperatureIconUrl), `${Math.round(frame.track_temperature_c)}°C`);
    track.title = t("standings.trackTemperatureTitle", { value: decimal(frame.track_temperature_c, 1) });
    header.append(track);
  }
  if (relativeSettings.options.brakeBias) {
    header.append(node("strong", "relative-brake-bias", `BB ${decimal(frame.brake_bias_percent, 1)}%`));
  }
  if (relativeSettings.options.trackLimits) {
    const currentTrackLimitPoints = formatTrackLimitPoints(frame.track_limits_steps);
    const penaltyTrackLimitPoints = frame.track_limits_steps_per_penalty > 0
      ? formatTrackLimitPoints(frame.track_limits_steps_per_penalty)
      : "-";
    const trackLimits = node(
      "strong",
      "relative-track-limits",
      `× ${currentTrackLimitPoints}/${penaltyTrackLimitPoints}`
    );
    applyTrackLimitTone(trackLimits, frame.track_limits_steps, frame.track_limits_steps_per_penalty);
    header.append(trackLimits);
  }
  if (relativeSettings.options.gameTimeClock) {
    const gameTime = node("time", "relative-game-time");
    gameTime.append(clockIcon(clockIconUrl), formatTimeOfDay(frame.game_time_of_day_seconds));
    gameTime.title = t("standings.gameTime");
    header.append(gameTime);
  }
  if (relativeSettings.options.realTimeClock) {
    const localTime = node("time", "relative-local-time");
    localTime.append(clockIcon(worldIconUrl), formatRealClock(new Date()));
    localTime.title = t("standings.localTime");
    header.append(localTime);
  }
  return header;
};

interface CachedNode {
  signature: string;
  element: HTMLElement;
}

const classSections = new Map<string, HTMLElement>();
let cachedSessionHeader: CachedNode | null = null;

const syncChildren = (parent: HTMLElement, desired: HTMLElement[]): void => {
  desired.forEach((child, index) => {
    const current = parent.children.item(index);
    if (current === child) return;
    if (child.parentElement === parent) parent.insertBefore(child, current);
    else if (current) current.replaceWith(child);
    else parent.append(child);
  });
  while (parent.children.length > desired.length) parent.lastElementChild?.remove();
};

const resetRenderCaches = (): void => {
  rowCache.clear();
  classSections.clear();
  cachedSessionHeader = null;
};

const cachedSessionHeaderFor = (frame: TelemetryFrame): HTMLElement => {
  const signature = JSON.stringify([
    relativeSettings.options.airTemperature,
    relativeSettings.options.trackTemperature,
    relativeSettings.options.brakeBias,
    relativeSettings.options.trackLimits,
    relativeSettings.options.gameTimeClock,
    relativeSettings.options.realTimeClock,
    frame.rest_weather_available,
    Math.round(frame.ambient_temperature_c),
    Math.round(frame.track_temperature_c),
    Math.round(frame.brake_bias_percent * 10),
    frame.track_limits_steps,
    frame.track_limits_steps_per_penalty,
    Math.floor(frame.game_time_of_day_seconds / 60),
    formatRealClock(new Date())
  ]);
  if (!cachedSessionHeader || cachedSessionHeader.signature !== signature) {
    cachedSessionHeader = { signature, element: sessionHeader(frame) };
  }
  return cachedSessionHeader.element;
};

// Entrar o salir de carrera cambia el juego de columnas visibles, así que hay
// que rehacer la rejilla y el ancho de diseño antes de pintar las filas.
const applySessionPhase = (sessionType: number): void => {
  const nextRaceSession = isRaceSession(sessionType);
  if (nextRaceSession === raceSession) return;
  raceSession = nextRaceSession;
  applyColumnLayout();
  updateOverlayFit({ width: relativeBaseWidth(), height: relativeBaseHeight() });
};

const render = (frame: TelemetryFrame): void => {
  const list = document.getElementById("relative-list");
  if (!list) return;

  if (frame.standings.length === 0) {
    // LMU puede publicar un frame intermedio sin standings al actualizar la
    // sesión. Conservamos la tabla ya pintada para que ese frame no produzca
    // un destello ni sustituya los datos por el estado de espera.
    if (lastFrame?.standings.length) return;
    list.replaceChildren(node("p", "empty-state", t("standings.waiting")));
    return;
  }
  lastFrame = frame;
  applySessionPhase(frame.session_type);

  {
    const entriesById = new Map(frame.standings.map((entry) => [entry.vehicle_id, entry]));
    const selected = frame.relative_model.rows
      .map((model) => {
        const entry = entriesById.get(model.vehicle_id);
        return entry ? { model, entry } : null;
      })
      .filter((row): row is { model: typeof frame.relative_model.rows[number]; entry: StandingEntry } => row !== null);
    if (selected.length === 0) {
      list.replaceChildren(node("p", "empty-state", t("relative.waitingPlayer")));
      return;
    }
    let group = classSections.get("__relative");
    if (!group) {
      group = node("section", "standings-class");
      group.dataset.classTone = "relative";
      classSections.set("__relative", group);
    }
    const groupChildren: HTMLElement[] = [];
    groupChildren.push(...selected.map(({ model, entry }) => renderRow(
      entry,
      frame.track_limits_steps_per_penalty,
      `relative-${model.kind}-${model.vehicle_id}`,
      model.lap_relation,
      model.kind,
      model.relative_gap_seconds
    )));
    syncChildren(group, groupChildren);
    const children: HTMLElement[] = [];
    if (relativeSettings.options.tableHeader) children.push(cachedSessionHeaderFor(frame));
    children.push(group);
    syncChildren(list, children);
    return;
  }
};

const relativeRuntime = window as Window & {
  __lmuRelativeLastTelemetryRenderAt?: number;
};
const telemetryListener = listenTelemetry((payload) => {
  const now = performance.now();
  // Relative se publica a 20 Hz. Este margen absorbe únicamente duplicados de
  // desarrollo/HMR sin descartar los frames normales separados por 50 ms.
  if (now - (relativeRuntime.__lmuRelativeLastTelemetryRenderAt ?? 0) < 40) return;
  relativeRuntime.__lmuRelativeLastTelemetryRenderAt = now;
  renderPerformance.measure(() => render(payload), payload.standings.length);
});
const settingsListener = listenRuntimeEvent<RelativeSettings>("relative://settings", (payload) => {
  relativeSettings = payload;
  updateOverlayFit({ width: relativeBaseWidth(), height: relativeBaseHeight() });
  resetRenderCaches();
  applyColumnLayout();
  if (lastFrame) render(lastFrame);
});

if (import.meta.hot) {
  import.meta.hot.dispose(() => {
    void telemetryListener.then((unlisten) => unlisten());
    void settingsListener.then((unlisten) => unlisten());
  });
}
bindOverlayInteractionMode();

if (import.meta.env.DEV && new URLSearchParams(window.location.search).has("preview")) {
  relativeSettings = {
    ...relativeSettings,
    options: { ...relativeSettings.options, pitStops: true }
  };
  updateOverlayFit({ width: relativeBaseWidth(), height: relativeBaseHeight() });
  applyColumnLayout();
  const previewClasses: Array<[string, number]> = [
    ["LMP2_ELMS", 3],
    ["LMGT3", 10],
    ["LMP3", 3]
  ];
  const standings = previewClasses.flatMap(([vehicleClass, count], classIndex) =>
    Array.from({ length: count }, (_, index): StandingEntry => ({
      vehicle_id: classIndex * 20 + index,
      overall_position: classIndex * 20 + index + 1,
      position: index + 1,
      position_change: index % 3 === 0 ? 1 : index % 3 === 1 ? -1 : 0,
      car_number: ["54", "397", "27", "92", "69"][index % 5],
      driver_name: ["Vincent Jeanblanc", "Mario Pardo", "Manuel Rodríguez Álvarez", "Willy Girard"][index % 4],
      driver_rank: ["S2", "B3", "G1", "S1"][index % 4],
      driver_rank_progress: [64, 78, 31, 52][index % 4],
      estimated_driver_rank_gain: [5, -3, 2, -7][index % 4],
      estimated_driver_rank_gain_available: true,
      safety_rank: ["P3", "G2", "G1", "S2"][index % 4],
      safety_rank_progress: [100, 73, 48, 91][index % 4],
      nationality: ["fr", "es", "gb", "de"][index % 4],
      driver_badge: ["sr-saint", "sr-clean", "sr-rookie", "sr-warning"][index % 4],
      team_name: ["Group13 Motorsport", "Group99 Racing", "Proton Competition", "Iron Lynx"][index % 4],
      vehicle_name: ["Ferrari 296 LMGT3", "Porsche 963", "Oreca 07", "Ligier JS P325"][classIndex],
      vehicle_class: vehicleClass,
      total_laps: 7 - Math.floor(index / 4),
      laps_behind_leader: index > 5 ? 1 : 0,
      laps_behind_next: index > 5 ? 1 : 0,
      time_behind_leader: index * 2.41,
      interval: index === 0 ? 0 : 1.27 + index * 0.31,
      relative_gap_seconds: (index - 4) * 2.1,
      relative_ahead_seconds: index < 4 ? (index - 4) * 2.1 : -120 + (index - 4) * 2.1,
      relative_behind_seconds: index < 4 ? 120 + (index - 4) * 2.1 : (index - 4) * 2.1,
      best_lap_seconds: 104.44 + classIndex * 15 + index * 0.31,
      last_lap_seconds: index % 4 === 2 ? 0 : 105.12 + classIndex * 15 + index * 0.44,
      average_lap_seconds: 105.4 + classIndex * 15 + index * 0.35,
      virtual_energy_active: true,
      virtual_energy_percent: classIndex === 1 ? 45.9 + index * 4.8 : 70.0 + index * 3.0,
      virtual_energy_per_lap: classIndex === 1 ? 3.1 : 0,
      damage_percent: index === 3 ? 16 : 0,
      track_limits_steps: index * 2,
      pit_stops: Math.floor(index / 3),
      pit_stop_requested: index === 2,
      pit_stop_lap: index === count - 1 || index === 2 ? 19 + index : null,
      pit_stop_time_seconds: index === count - 1 || index === 2 ? 31.7 : null,
      tire_compound: index % 5 === 4 ? "W" : "M",
      tire_compounds: index % 5 === 4 ? ["W", "W", "W", "W"] : ["M", "M", "M", "M"],
      flag: index === 4 ? 6 : 0,
      causing_yellow: false,
      has_fastest_lap: index === 0,
      in_pits: index === count - 1,
      in_garage: index === count - 2,
      is_out_lap: index === 2,
      last_lap_valid: index % 4 !== 2,
      penalty_count: index === 3 ? 1 : 0,
      finish_status: 0,
      is_player: classIndex === 1 && index === 2
    }))
  );
  render({
    session_type: 1,
    session_time_remaining: 3_220,
    session_elapsed_seconds: 5_179,
    game_time_of_day_seconds: 63_000,
    session_max_time_seconds: 8_400,
    session_max_laps: 0,
    leader_total_laps: 7,
    session_split_number: 2,
    session_split_count: 12,
    player_total_laps: 6,
    session_laps_remaining_estimated: 23.42,
    brake_bias_percent: 56.5,
    track_limits_steps: 4,
    track_limits_steps_per_penalty: 17,
    rest_weather_available: true,
    ambient_temperature_c: 19,
    track_temperature_c: 25,
    track_name: "Circuit de la Sarthe",
    standings_model: { groups: [] },
    relative_model: {
      rows: [
        { vehicle_id: 0, relative_gap_seconds: -8.4, kind: "ahead", lap_relation: "player_ahead" },
        { vehicle_id: 1, relative_gap_seconds: -6.3, kind: "ahead", lap_relation: "same_lap" },
        { vehicle_id: 2, relative_gap_seconds: -4.2, kind: "ahead", lap_relation: "opponent_ahead" },
        { vehicle_id: 3, relative_gap_seconds: -2.1, kind: "ahead", lap_relation: "same_lap" },
        { vehicle_id: 22, relative_gap_seconds: 0, kind: "player", lap_relation: "same_lap" },
        { vehicle_id: 23, relative_gap_seconds: 2.1, kind: "behind", lap_relation: "opponent_ahead" },
        { vehicle_id: 25, relative_gap_seconds: 4.2, kind: "behind", lap_relation: "player_ahead" },
        { vehicle_id: 28, relative_gap_seconds: 6.3, kind: "behind", lap_relation: "same_lap" },
        { vehicle_id: 29, relative_gap_seconds: 8.4, kind: "behind", lap_relation: "same_lap" }
      ]
    },
    standings
  } as unknown as TelemetryFrame);
}
