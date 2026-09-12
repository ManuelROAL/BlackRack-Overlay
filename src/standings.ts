import "./styles.css";
import "./standings.css";
import { fitOverlay } from "./overlay-fit";
import { bindOverlayInteractionMode } from "./overlay-interaction";
import { createOverlayPerformanceTracker } from "./overlay-performance";
import { bindOverlayTransparency } from "./overlay-appearance";
import type { StandingEntry, StandingsClassModel, TelemetryFrame } from "./telemetry-types";
import {
  readStandingsSettings,
  visibleStandingsColumns,
  type StandingsColumnId,
  type StandingsSettings
} from "./standings-settings";
import { listenRuntimeEvent, listenTelemetry } from "./runtime-events";
import {
  airTemperatureIconUrl,
  clockIconUrl,
  compoundIconUrl,
  profileIconUrl,
  timingIconUrl,
  tiresIconUrl,
  trackTemperatureIconUrl,
  worldIconUrl
} from "./lmu-icons";
import { applyTrackLimitTone, formatTrackLimitPoints } from "./track-limit-tone";
import { formatDriverName } from "./driver-name-format";
import { isPracticeSession, isRaceSession } from "./session-phase";
import { formatClock as formatRealClock, formatNumber, formatTimeOfDay, t, type TranslationKey } from "./i18n";

let settings = readStandingsSettings();
const STANDINGS_EMPTY_HEIGHT = 72;
// GAP e INT miden progreso físico en pista contra una tabla que fuera de
// carrera está ordenada por mejor vuelta, así que solo se muestran en carrera.
// La configuración del usuario se conserva y vuelve sola al empezar la carrera.
const RACE_ONLY_COLUMNS = new Set<StandingsColumnId>(["gap", "interval"]);
let raceSession = true;
// Ancho de la pista del cambio de posición más su separación dentro de la
// celda: sin indicador la columna sobra justo eso y dejaría el hueco vacío.
const POSITION_CHANGE_WIDTH = 19;
const activeColumns = () =>
  visibleStandingsColumns(settings)
    .filter(({ id }) => raceSession || !RACE_ONLY_COLUMNS.has(id))
    .map((column) => column.id === "position" && !raceSession
      ? { ...column, width: column.width - POSITION_CHANGE_WIDTH }
      : column.id === "delta"
        ? { ...column, width: Math.max(column.width, settings.deltaLapCount * 22 + 6) }
        : column);
const columnExpansionRatio = (id: StandingsColumnId): number => {
  if (id === "driver" || id === "manufacturer" || id === "badge" || id === "tire") return 0.5;
  if (id === "position") return 0.75;
  if (id === "number") return 0.5;
  return 1;
};
const expandableColumnsWidth = (): number => activeColumns()
  .reduce((total, { id, width }) => total + width * columnExpansionRatio(id), 0);
const standingsBaseWidth = (): number => Math.max(
  760,
  activeColumns().reduce((total, { width }) => total + width, 0) + 8
);
const updateOverlayFit = fitOverlay(
  { width: standingsBaseWidth(), height: STANDINGS_EMPTY_HEIGHT },
  { widthTextRatio: () => expandableColumnsWidth() / standingsBaseWidth() }
);
let fittedOverlayHeight = STANDINGS_EMPTY_HEIGHT;
bindOverlayTransparency("standings");
const renderPerformance = createOverlayPerformanceTracker("standings");

let lastFrame: TelemetryFrame | null = null;

const columnLength = ({ id, width }: { id: StandingsColumnId; width: number }): string => {
  const ratio = columnExpansionRatio(id);
  if (ratio === 0) return `${width}px`;
  if (ratio === 1) return `calc(${width}px * var(--overlay-font-track-expansion, 1))`;
  return `calc(${width * (1 - ratio)}px + ${width * ratio}px * var(--overlay-font-track-expansion, 1))`;
};
const columnsLength = (columns: ReadonlyArray<{ id: StandingsColumnId; width: number }>): string => {
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

const icon = (source: string, className: string, title = ""): HTMLImageElement => {
  const image = node("img", className);
  image.src = source;
  image.alt = "";
  image.setAttribute("aria-hidden", "true");
  if (title) image.title = title;
  return image;
};

const columnLabel = (column: { id: StandingsColumnId; header: string }): HTMLElement => {
  const label = node("span", "column-label", column.id === "tire" ? "" : column.header);
  label.dataset.column = column.id;
  if (column.id === "tire") label.append(icon(tiresIconUrl, "column-label-icon", t("common.tires")));
  return label;
};

const decimal = (value: number, digits = 2): string =>
  formatNumber(value, digits);

const formatLapTime = (seconds: number): string => {
  if (!Number.isFinite(seconds) || seconds <= 0) return "--";
  const minutes = Math.floor(seconds / 60);
  return `${minutes}:${(seconds % 60).toFixed(3).padStart(6, "0")}`;
};

const formatRecentLapDelta = (value: number | null): string => {
  if (value === null || !Number.isFinite(value)) return "-.-";
  return Math.abs(value) > 9.94 ? Math.abs(value).toFixed(0) : Math.abs(value).toFixed(1);
};

const recentLapDeltaCell = (entry: StandingEntry): HTMLElement => {
  const cell = node("span", "recent-lap-deltas");
  const values = (entry.last_lap_delta_seconds ?? []).slice(-settings.deltaLapCount);
  if (settings.invertDeltaLayout) values.reverse();
  for (const value of values) {
    const item = node("span", "recent-lap-delta", formatRecentLapDelta(value));
    item.dataset.tone = value === null || !Number.isFinite(value) ? "unavailable" : entry.is_player || Math.abs(value) < 0.0005 ? "neutral" : value < 0 ? "gain" : "loss";
    cell.append(item);
  }
  if (values.length === 0) cell.append(node("span", "recent-lap-delta", "-.-"));
  return cell;
};

const formatDifference = (laps: number, seconds: number): string => {
  if (laps > 0) return t("common.lapsBehind", { count: laps });
  return `+${decimal(Math.max(seconds, 0), 1)}`;
};

const formatClock = (seconds: number): string => {
  if (!Number.isFinite(seconds) || seconds < 0) return "--:--";
  const total = Math.ceil(seconds);
  const hours = Math.floor(total / 3600);
  const minutes = Math.floor((total % 3600) / 60);
  const remainder = total % 60;
  return hours > 0
    ? `${hours}:${minutes.toString().padStart(2, "0")}:${remainder.toString().padStart(2, "0")}`
    : `${minutes}:${remainder.toString().padStart(2, "0")}`;
};

const formatSessionDuration = (seconds: number): string => {
  if (!Number.isFinite(seconds) || seconds <= 0) return "--";
  const totalMinutes = Math.max(1, Math.round(seconds / 60));
  const hours = Math.floor(totalMinutes / 60);
  const minutes = totalMinutes % 60;
  return hours > 0
    ? `${hours}h ${minutes.toString().padStart(2, "0")}m`
    : `${minutes}m`;
};

const sessionLabel = (sessionType: number): string => {
  if (sessionType >= 10 && sessionType <= 13) {
    return sessionType === 10 ? t("session.race") : t("session.raceNumber", { number: sessionType - 9 });
  }
  if (sessionType >= 5 && sessionType <= 8) {
    return sessionType === 5 ? t("session.qualifying") : t("session.qualifyingNumber", { number: sessionType - 4 });
  }
  if (sessionType === 9) return t("session.warmup");
  return sessionType === 0 ? t("session.practice") : t("session.practiceNumber", { number: sessionType });
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
  const flag = node("img", "country-flag");
  flag.src = source;
  flag.alt = code;
  flag.title = nationality;
  return flag;
};

const classTone = (vehicleClass: string): string => {
  const value = vehicleClass.toUpperCase();
  if (value.includes("HYPER") || value.includes("GTP")) return "hypercar";
  if (value.includes("LMP2")) return "lmp2";
  if (value.includes("LMP3")) return "lmp3";
  if (value.includes("LMGT3") || value.includes("GT3")) return "lmgt3";
  if (value.includes("GTE")) return "gte";
  return "other";
};

const manufacturerLogoRasterModules = import.meta.glob<string>(
  "./assets/manufacturers-raster/*.png",
  { eager: true, query: "?url", import: "default" }
);

const manufacturerLogoModules = import.meta.glob<string>(
  "./assets/manufacturers/*.{svg,png}",
  { eager: true, query: "?url", import: "default" }
);

const driverBadgeModules = import.meta.glob<string>(
  "./assets/driver-badges/*.svg",
  { eager: true, query: "?url", import: "default" }
);

const DRIVER_BADGE_LABELS: Record<string, TranslationKey> = {
  "sr-noob": "badge.noob", "sr-rookie": "badge.noob", "sr-probation": "badge.probation",
  "sr-warning": "badge.warning", "sr-danger": "badge.danger", "sr-clean": "badge.clean", "sr-saint": "badge.saint",
  "content-creator": "badge.creator", "irl-driver": "badge.realDriver",
  "early-access": "badge.earlyAccess", "test-driver": "badge.testDriver"
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

const manufacturer = (entry: StandingEntry): string | undefined => {
  const value = `${entry.team_name} ${entry.vehicle_name}`.toUpperCase();
  const manufacturers: Array<[string[], string]> = [
    [["MCLAREN", "720S", "720 GT3", "MP4-12C"], "McLaren"],
    [["PORSCHE"], "Porsche"], [["FERRARI"], "Ferrari"], [["BMW"], "BMW"],
    [["TOYOTA"], "Toyota"], [["CADILLAC"], "Cadillac"], [["ALPINE"], "Alpine"],
    [["PEUGEOT"], "Peugeot"], [["ASTON"], "Aston"], [["CORVETTE"], "Corvette"],
    [["FORD"], "Ford"], [["LEXUS"], "Lexus"], [["LAMBORGHINI"], "Lamborghini"],
    [["MERCEDES"], "Mercedes"], [["GINETTA"], "Ginetta"], [["LIGIER"], "Ligier"],
    [["ORECA"], "Oreca"], [["DUQUEINE"], "Duqueine"], [["GENESIS"], "Genesis"],
    [["ACURA"], "Acura"], [["GLICKENHAUS"], "Glickenhaus"], [["ISOTTA"], "Isotta"],
    [["VANWALL"], "Vanwall"], [["ADESS"], "Addes"]
  ];
  return manufacturers.find(([tokens]) => tokens.some((token) => value.includes(token)))?.[1];
};

const manufacturerLogo = (entry: StandingEntry): HTMLImageElement | undefined => {
  const name = manufacturer(entry);
  if (!name) return undefined;
  const source = manufacturerLogoRasterModules[`./assets/manufacturers-raster/${name}.png`]
    ?? manufacturerLogoModules[`./assets/manufacturers/${name}.svg`]
    ?? manufacturerLogoModules[`./assets/manufacturers/${name}.png`];
  if (!source) return undefined;
  const logo = node("img", "manufacturer-logo");
  logo.src = source;
  logo.alt = name;
  logo.title = name;
  return logo;
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
  // Terminado, DNF y DQ son estados de clasificación de carrera. Fuera de ella
  // no hay nada de lo que retirarse y el early return taparía PIT, GAR y las
  // penalizaciones, que sí siguen siendo válidas en clasificación.
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
  else if (entry.in_pits) container.append(node("span", "race-flag pit-flag", "PIT"));
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
  cells: Map<StandingsColumnId, CachedCell>;
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
  const { pitStops, pitTime, pitLap } = settings.columns;
  if (!pitStops && !pitTime && !pitLap) return;
  const summary = node("span", "driver-pit-summary");
  if (entry.in_pits) {
    if (pitTime && entry.pit_stop_time_seconds !== null) {
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
    if (pitLap) summary.append(node("b", "driver-pit-lap", `L${entry.pit_stop_lap}`));
    if (pitTime) summary.append(node("b", "driver-pit-time", time));
    if (pitStops) summary.append(node("b", "driver-pit-count", entry.pit_stops.toString()));
    summary.title = t("standings.pitSummary", {
      count: entry.pit_stops,
      lap: entry.pit_stop_lap,
      time
    });
  }
  if (summary.childElementCount > 0) cell.append(summary);
};

const cellSignature = (entry: StandingEntry, column: StandingsColumnId, trackLimit: number): string => {
  switch (column) {
    case "position": return `${entry.position}|${raceSession ? entry.position_change : ""}`;
    case "number": return liveCarNumbers.get(entry.vehicle_id) || entry.car_number || "--";
    case "badge": return entry.driver_badge;
    case "pitTime":
    case "pitLap":
    case "pitStops":
    case "driver": return `${entry.driver_name}|${entry.nationality}|${settings.driverNameFormat}|${settings.pitInformationLayout}|${settings.columns.pitStops}|${settings.columns.pitTime}|${settings.columns.pitLap}|${raceSession}|${entry.in_pits}|${entry.pit_stop_requested}|${entry.pit_stops}|${entry.pit_stop_lap ?? ""}|${entry.pit_stop_time_seconds === null ? "" : pitTimeLabel(entry.pit_stop_time_seconds)}`;
    case "manufacturer": return `${entry.team_name}|${entry.vehicle_name}`;
    case "ranks": return `${entry.driver_rank}|${Math.round(entry.driver_rank_progress)}|${Math.round(entry.estimated_driver_rank_gain)}|${entry.estimated_driver_rank_gain_available}|${entry.safety_rank}`;
    case "gap": return entry.position === 1 ? `V ${entry.total_laps}` : formatDifference(entry.laps_behind_leader, entry.time_behind_leader);
    case "interval": return entry.position === 1 ? `V ${entry.total_laps}` : formatDifference(entry.laps_behind_next, entry.interval);
    case "best": return `${formatLapTime(entry.best_lap_seconds)}|${entry.best_lap_seconds > 0}|${entry.has_fastest_lap}`;
    case "last": return `${entry.is_out_lap ? "OUT" : formatLapTime(entry.last_lap_seconds)}|${formatLapTime(entry.best_lap_seconds)}|${entry.has_fastest_lap}|${entry.last_lap_valid}`;
    case "average": return formatLapTime(entry.average_lap_seconds);
    case "delta": return `${JSON.stringify(entry.last_lap_delta_seconds)}|${settings.deltaLapCount}|${settings.invertDeltaLayout}|${entry.is_player}`;
    case "energy": return raceSession && entry.virtual_energy_active && entry.virtual_energy_percent > 0
      ? `${entry.virtual_energy_percent >= 99.95 ? "100" : decimal(entry.virtual_energy_percent, 1)}|${entry.virtual_energy_per_lap > 0 ? decimal(entry.virtual_energy_per_lap, 2) : ""}`
      : "--";
    case "damage": return Math.round(entry.damage_percent).toString();
    case "trackLimits": return entry.track_limits_steps === null ? "--" : `${entry.track_limits_steps}|${trackLimit}`;
    case "tire": return entry.tire_compounds.join("/");
    case "signals": {
      const penalties = livePenalties.get(entry.vehicle_id);
      return `${raceSession ? entry.finish_status : 0}|${entry.in_garage}|${entry.in_pits}|${entry.causing_yellow}|${entry.penalty_count}|${entry.flag}|${penalties?.DT ?? 0}|${penalties?.SG ?? 0}`;
    }
  }
};

const createCell = (entry: StandingEntry, column: StandingsColumnId, trackLimit: number): HTMLElement => {
  switch (column) {
    case "position": {
      const cell = node("div", "standing-position-cell");
      cell.append(node("strong", "standing-position", entry.position.toString()));
      // Sin parrilla no hay referencia para el cambio de posición y la celda
      // solo pintaría un guion fijo en todas las filas.
      if (raceSession) cell.append(positionChange(entry.position_change));
      return cell;
    }
    case "number": return node("span", "car-number", liveCarNumbers.get(entry.vehicle_id) || entry.car_number || "--");
    case "badge": return driverBadge(entry.driver_badge);
    case "driver": {
      const cell = node("div", "standing-driver");
      const flag = countryFlag(entry.nationality);
      if (flag) cell.append(flag);
      const fullName = entry.driver_name || "—";
      const name = node("b", "driver-name", formatDriverName(fullName, settings.driverNameFormat));
      name.title = fullName;
      cell.append(name);
      if (settings.pitInformationLayout !== "column") {
        cell.classList.toggle("pit-above", settings.pitInformationLayout === "above" && (settings.columns.pitStops || settings.columns.pitTime || settings.columns.pitLap));
        appendDriverPitStatus(cell, entry);
      }
      return cell;
    }
    case "manufacturer": {
      const cell = node("div", "standing-team");
      const logo = manufacturerLogo(entry);
      if (logo) cell.append(logo);
      return cell;
    }
    case "ranks": {
      const cell = node("span", "driver-ranks");
      if (entry.driver_rank) cell.append(rankBadge("DR", entry.driver_rank, entry.driver_rank_progress, entry.estimated_driver_rank_gain, entry.estimated_driver_rank_gain_available));
      if (entry.safety_rank) cell.append(rankBadge("SR", entry.safety_rank));
      return cell;
    }
    case "gap": return node("b", "standing-gap", entry.position === 1 ? `V ${entry.total_laps}` : formatDifference(entry.laps_behind_leader, entry.time_behind_leader));
    case "interval": return node("b", "standing-interval", entry.position === 1 ? `V ${entry.total_laps}` : formatDifference(entry.laps_behind_next, entry.interval));
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
    case "delta": return recentLapDeltaCell(entry);
    case "energy": {
      const cell = node("div", "standing-energy");
      // Comparar la energía de los rivales solo tiene sentido con una
      // estrategia común: fuera de carrera cada coche lleva la carga que quiere.
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
    case "pitTime":
    case "pitLap":
    case "pitStops": {
      const cell = node("span", "standing-pit-information");
      appendDriverPitStatus(cell, entry);
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

const renderRow = (entry: StandingEntry, trackLimit: number, instanceKey = `vehicle-${entry.vehicle_id}`): HTMLElement => {
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
  cached.element.dataset.classTone = classTone(entry.vehicle_class);

  for (const column of columns) {
    const signature = cellSignature(entry, column, trackLimit);
    const previous = cached.cells.get(column);
    if (!previous || previous.signature !== signature) {
      const element = createCell(entry, column, trackLimit);
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

const strengthOfField = (model: StandingsClassModel): HTMLElement => {
  const sof = node("span", "class-sof", "SOF --");
  if (!model.strength_of_field) {
    sof.title = t("standings.sofUnavailable");
    return sof;
  }
  const strength = model.strength_of_field;
  sof.textContent = strength.label;
  sof.title = t("standings.sofTitle", { resolved: strength.resolved_profiles, total: strength.total_profiles });
  sof.dataset.coverage = strength.resolved_profiles === strength.total_profiles ? "complete" : "partial";
  return sof;
};

const sessionHeader = (frame: TelemetryFrame): HTMLElement => {
  const header = node("header", "standings-session-header");
  const sessionGroup = node("div", "standings-session-group");
  const dataGroup = node("div", "standings-header-data-group");
  if (settings.header.sessionType) {
    sessionGroup.append(node("strong", "standings-session-type", sessionLabel(frame.session_type)));
  }
  if (settings.header.eventSplit) {
    const currentSplit = frame.session_split_number > 0 ? `${frame.session_split_number}` : "--";
    const totalSplits = frame.session_split_count > 0 ? `${frame.session_split_count}` : "--";
    const split = `${currentSplit}/${totalSplits}`;
    sessionGroup.append(node("span", "standings-session-split", t("standings.split", { split })));
  }
  if (settings.header.remainingTime) {
    const clock = node("b", "standings-session-clock");
    clock.append(
      icon(timingIconUrl, "standings-session-timing-icon"),
      `${formatClock(frame.session_time_remaining)} / ${formatSessionDuration(frame.session_max_time_seconds)}`
    );
    sessionGroup.append(clock);
  }
  if (settings.header.laps) {
    const total = frame.session_total_laps_estimated;
    const totalLabel = Number.isFinite(total) && total > 0
      ? `~${decimal(total, 2)}`
      : "~--";
    sessionGroup.append(node(
      "b",
      "standings-session-lap",
      `${frame.player_total_laps + 1}/${totalLabel}${frame.session_extra_laps_estimated == null ? "" : ` (${frame.session_extra_laps_estimated >= 0 ? "+" : ""}${frame.session_extra_laps_estimated}${frame.session_extra_laps_approximate ? "~" : ""})`}`
    ));
  }
  if (frame.rest_weather_available && settings.header.airTemperature) {
    const ambient = node("span", "standings-header-temperature");
    ambient.append(
      icon(airTemperatureIconUrl, "standings-header-icon", t("standings.airTemperature")),
      `${Math.round(frame.ambient_temperature_c)}°C`
    );
    ambient.title = t("standings.airTemperatureTitle", { value: formatNumber(frame.ambient_temperature_c, 1) });
    dataGroup.append(ambient);
  }
  if (frame.rest_weather_available && settings.header.trackTemperature) {
    const track = node("span", "standings-header-temperature");
    track.append(
      icon(trackTemperatureIconUrl, "standings-header-icon", t("standings.trackTemperature")),
      `${Math.round(frame.track_temperature_c)}°C`
    );
    track.title = t("standings.trackTemperatureTitle", { value: formatNumber(frame.track_temperature_c, 1) });
    dataGroup.append(track);
  }
  if (settings.header.brakeBias) {
    dataGroup.append(node("strong", "standings-header-brake-bias", `BB ${decimal(frame.brake_bias_percent, 1)}%`));
  }
  if (settings.header.trackLimits) {
    const currentTrackLimitPoints = formatTrackLimitPoints(frame.track_limits_steps);
    const penaltyTrackLimitPoints = frame.track_limits_steps_per_penalty > 0
      ? formatTrackLimitPoints(frame.track_limits_steps_per_penalty)
      : "-";
    const trackLimits = node(
      "strong",
      "standings-header-track-limits",
      `× ${currentTrackLimitPoints}/${penaltyTrackLimitPoints}`
    );
    applyTrackLimitTone(trackLimits, frame.track_limits_steps, frame.track_limits_steps_per_penalty);
    dataGroup.append(trackLimits);
  }
  if (settings.header.gameTimeClock) {
    const gameTime = node("time", "standings-header-game-time");
    gameTime.append(
      icon(clockIconUrl, "standings-header-icon", t("standings.gameTime")),
      formatTimeOfDay(frame.game_time_of_day_seconds)
    );
    dataGroup.append(gameTime);
  }
  if (settings.header.realTimeClock) {
    const localTime = node("time", "standings-header-local-time");
    localTime.append(
      icon(worldIconUrl, "standings-header-icon", t("standings.localTime")),
      formatRealClock(new Date())
    );
    dataGroup.append(localTime);
  }
  header.append(sessionGroup, dataGroup);
  return header;
};

const classTableHeader = (
  model: StandingsClassModel,
  sessionType: number
): HTMLElement => {
  const header = node("header", "class-table-header");
  const identity = node("div", "class-heading-inline");
  const practice = isPracticeSession(sessionType);
  const currentCount = model.current_count;
  const initialCount = model.initial_count;
  const retiredCount = model.retired_count;
  const carCount = node("span", "class-car-count");
  carCount.append(icon(profileIconUrl, "class-car-icon"), practice ? `${currentCount}` : `${currentCount}/${initialCount}`);
  carCount.dataset.retired = !practice && retiredCount > 0 ? "true" : "false";
  carCount.title = practice
    ? t("standings.currentCars", { current: currentCount })
    : retiredCount > 0
      ? t("standings.retiredCars", { current: currentCount, initial: initialCount, retired: retiredCount })
      : t("standings.initialCars", { current: currentCount, initial: initialCount });
  const columns = activeColumns();
  const identityColumns = columns.filter(({ identity: isIdentity }) => isIdentity);
  identity.append(node("strong", undefined, model.display_class));
  identity.append(carCount);
  if (!practice) identity.append(strengthOfField(model));
  identity.style.gridColumn = `span ${identityColumns.length}`;
  header.append(identity);
  for (const column of columns.filter(({ identity: isIdentity }) => !isIdentity)) {
    header.append(columnLabel(column));
  }
  return header;
};

interface CachedNode {
  signature: string;
  element: HTMLElement;
}

const classSections = new Map<string, HTMLElement>();
const classHeaders = new Map<string, CachedNode>();
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

const cachedSessionHeaderFor = (frame: TelemetryFrame): HTMLElement => {
  const signature = JSON.stringify([
    settings.header,
    frame.session_type,
    Math.ceil(frame.session_time_remaining),
    Math.round(frame.session_max_time_seconds / 60),
    frame.session_max_laps,
    frame.player_total_laps,
    frame.session_split_number,
    frame.session_split_count,
    Math.round(frame.session_total_laps_estimated * 100),
    frame.session_extra_laps_estimated,
    frame.session_extra_laps_approximate,
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

const cachedClassHeaderFor = (
  model: StandingsClassModel,
  sessionType: number
): HTMLElement => {
  const signature = JSON.stringify([
    activeColumns().map(({ id }) => id),
    sessionType,
    model.current_count,
    model.initial_count,
    model.retired_count,
    model.strength_of_field
  ]);
  const cached = classHeaders.get(model.vehicle_class);
  if (!cached || cached.signature !== signature) {
    const next = { signature, element: classTableHeader(model, sessionType) };
    classHeaders.set(model.vehicle_class, next);
    return next.element;
  }
  return cached.element;
};

// Entrar o salir de carrera cambia el juego de columnas visibles, así que hay
// que rehacer la rejilla y el ancho de diseño antes de pintar las filas.
const applySessionPhase = (sessionType: number): void => {
  const nextRaceSession = isRaceSession(sessionType);
  if (nextRaceSession === raceSession) return;
  raceSession = nextRaceSession;
  applyColumnLayout();
  updateOverlayFit({ width: standingsBaseWidth(), height: fittedOverlayHeight });
};

const render = (frame: TelemetryFrame): void => {
  const list = document.getElementById("standings-list");
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

  const entriesById = new Map(frame.standings.map((entry) => [entry.vehicle_id, entry]));
  const children: HTMLElement[] = [];
  if (settings.showHeader) children.push(cachedSessionHeaderFor(frame));
  for (const model of frame.standings_model.groups) {
    const visibleEntries = model.visible_vehicle_ids
      .map((vehicleId) => entriesById.get(vehicleId))
      .filter((entry): entry is StandingEntry => entry !== undefined);
    let group = classSections.get(model.vehicle_class);
    if (!group) {
      group = node("section", "standings-class");
      classSections.set(model.vehicle_class, group);
    }
    group.dataset.classTone = model.class_tone;
    syncChildren(group, [
      cachedClassHeaderFor(model, frame.session_type),
      ...visibleEntries.map((entry) => renderRow(entry, frame.track_limits_steps_per_penalty))
    ]);
    children.push(group);
  }
  syncChildren(list, children);
  synchronizeOverlayHeight();
};

const synchronizeOverlayHeight = (): void => {
  const list = document.getElementById("standings-list");
  const shell = document.querySelector<HTMLElement>(".standings-shell");
  if (!list || !shell) return;
  const listStyle = getComputedStyle(list);
  const childrenHeight = Array.from(list.children).reduce((height, child) => {
    const element = child as HTMLElement;
    const style = getComputedStyle(element);
    return height
      + element.offsetHeight
      + (Number.parseFloat(style.marginTop) || 0)
      + (Number.parseFloat(style.marginBottom) || 0);
  }, (Number.parseFloat(listStyle.paddingTop) || 0) + (Number.parseFloat(listStyle.paddingBottom) || 0));
  const layoutChrome = document.body.offsetHeight - list.offsetHeight;
  const nextOverlayHeight = Math.max(
    STANDINGS_EMPTY_HEIGHT,
    Math.ceil(childrenHeight + layoutChrome) + 1
  );
  if (nextOverlayHeight !== fittedOverlayHeight) {
    fittedOverlayHeight = nextOverlayHeight;
    updateOverlayFit({ width: standingsBaseWidth(), height: nextOverlayHeight });
  }
};

const standingsRuntime = window as Window & {
  __lmuStandingsLastTelemetryRenderAt?: number;
};
const telemetryListener = listenTelemetry((payload) => {
  const now = performance.now();
  // Protección adicional para desarrollo/HMR y para ráfagas de eventos. El
  // standings no necesita repintarse por encima de la frecuencia de 5 Hz del
  // backend y reconstruirlo varias veces en el mismo instante causa destellos.
  if (now - (standingsRuntime.__lmuStandingsLastTelemetryRenderAt ?? 0) < 150) return;
  standingsRuntime.__lmuStandingsLastTelemetryRenderAt = now;
  renderPerformance.measure(() => render(payload), payload.standings.length);
});
const settingsListener = listenRuntimeEvent<StandingsSettings>("standings://settings", (payload) => {
  settings = payload;
  applyColumnLayout();
  updateOverlayFit({ width: standingsBaseWidth(), height: fittedOverlayHeight });
  if (lastFrame) render(lastFrame);
});

if (import.meta.hot) {
  import.meta.hot.dispose(() => {
    void telemetryListener.then((unlisten) => unlisten());
    void settingsListener.then((unlisten) => unlisten());
  });
}
bindOverlayInteractionMode();
window.addEventListener("overlay-font-size-change", () => {
  window.requestAnimationFrame(synchronizeOverlayHeight);
});
void document.fonts.ready.then(synchronizeOverlayHeight);

if (import.meta.env.DEV && new URLSearchParams(window.location.search).has("preview")) {
  settings = {
    ...settings,
    columns: { ...settings.columns, pitStops: true, pitTime: true, pitLap: true }
  };
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
      last_lap_delta_seconds: index % 4 === 2 ? null : [-0.342, 0.118, null, -0.041, index % 3 === 0 ? 0 : 0.205],
      average_lap_seconds: 105.4 + classIndex * 15 + index * 0.35,
      virtual_energy_active: true,
      virtual_energy_percent: classIndex === 1 ? 45.9 + index * 4.8 : 70.0 + index * 3.0,
      virtual_energy_per_lap: classIndex === 1 ? 3.1 : 0,
      damage_percent: index === 3 ? 16 : 0,
      track_limits_steps: index * 2,
      pit_stops: index === 2 ? 1 : Math.floor(index / 3),
      pit_stop_requested: index === 2,
      pit_stop_lap: index === count - 1 || index === 2 ? 19 + index : null,
      pit_stop_time_seconds: index === count - 1 || index === 2 ? 31.7 : null,
      tire_compound: index % 5 === 4 ? "W" : index % 5 === 3 ? "S/M/H/W" : "M",
      tire_compounds: index % 5 === 4
        ? ["W", "W", "W", "W"]
        : index % 5 === 3 ? ["S", "M", "H", "W"] : ["M", "M", "M", "M"],
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
    track_name: "Circuit de la Sarthe",
    rest_weather_available: true,
    ambient_temperature_c: 18,
    track_temperature_c: 22,
    track_wetness_percent: 84,
    track_wetness_min_percent: 72,
    track_wetness_max_percent: 91,
    rain_percent: 69,
    standings_model: {
      groups: [
        ["LMP2_ELMS", "LMP2", "lmp2", [0, 1, 2]],
        ["LMP3", "LMP3", "lmp3", [40, 41, 42]],
        ["LMGT3", "LMGT3", "lmgt3", [20, 21, 22, 23, 24, 25, 26, 27, 28, 29]]
      ].map(([vehicleClass, displayClass, classTone, vehicleIds]) => ({
        vehicle_class: vehicleClass,
        display_class: displayClass,
        class_tone: classTone,
        current_count: (vehicleIds as number[]).length,
        initial_count: (vehicleIds as number[]).length,
        retired_count: 0,
        strength_of_field: null,
        visible_vehicle_ids: vehicleIds
      }))
    },
    relative_model: { rows: [] },
    standings
  } as unknown as TelemetryFrame);
}
