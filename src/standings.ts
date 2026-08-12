import "./styles.css";
import "./standings.css";
import { fitOverlay } from "./overlay-fit";
import { bindOverlayInteractionMode } from "./overlay-interaction";
import { createOverlayPerformanceTracker } from "./overlay-performance";
import { bindOverlayTransparency } from "./overlay-appearance";
import type { StandingEntry, TelemetryFrame } from "./telemetry-types";
import {
  readStandingsSettings,
  STANDINGS_COLUMNS,
  visibleStandingsColumns,
  type StandingsColumnId,
  type StandingsSettings
} from "./standings-settings";
import { listenRuntimeEvent, listenTelemetry } from "./runtime-events";
import {
  airTemperatureIconUrl,
  compoundIconUrl,
  profileIconUrl,
  timingIconUrl,
  tiresIconUrl,
  trackTemperatureIconUrl
} from "./lmu-icons";
import { applyTrackLimitTone, formatTrackLimitPoints } from "./track-limit-tone";

let settings = readStandingsSettings();
const updateOverlayFit = fitOverlay({ width: 948, height: 450 });
let fittedOverlayHeight = 450;
bindOverlayTransparency("standings");
const renderPerformance = createOverlayPerformanceTracker("standings");

let lastFrame: TelemetryFrame | null = null;
const initialClassCarCounts = new Map<string, number>();
let countSession: {
  sessionType: number;
  trackName: string;
  playerLaps: number;
  timeRemaining: number;
} | null = null;

const activeColumns = () => visibleStandingsColumns(settings);

const applyColumnLayout = (): void => {
  const columns = activeColumns();
  const shell = document.querySelector<HTMLElement>(".standings-shell");
  shell?.style.setProperty(
    "--standings-grid-columns",
    columns.map(({ width }) => `${width}px`).join(" ")
  );
  const totalWidth = columns.reduce((total, { width }) => total + width, 0);
  const signalsVisible = columns.some(({ id }) => id === "signals");
  const signalsLast = columns.at(-1)?.id === "signals";
  const signalsWidth = columns.find(({ id }) => id === "signals")?.width ?? 0;
  const dataWidth = signalsLast ? totalWidth - signalsWidth : totalWidth;
  shell?.style.setProperty("--standings-content-width", `${dataWidth + 8}px`);
  shell?.style.setProperty(
    "--standings-row-background-width",
    `${dataWidth + (signalsVisible && signalsLast ? 4 : 8)}px`
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
  if (column.id === "tire") label.append(icon(tiresIconUrl, "column-label-icon", "Neumáticos"));
  return label;
};

const decimal = (value: number, digits = 2): string =>
  value.toFixed(digits).replace(".", ",");

const shortDriverName = (value: string): string => {
  const parts = value.trim().split(/\s+/).filter(Boolean);
  if (parts.length <= 2) return parts.join(" ");
  const surname = parts.slice(1).find((part) => part.replace(/\W/g, "").length > 1);
  return surname ? `${parts[0]} ${surname}` : parts.slice(0, 2).join(" ");
};

const formatLapTime = (seconds: number): string => {
  if (!Number.isFinite(seconds) || seconds <= 0) return "--";
  const minutes = Math.floor(seconds / 60);
  return `${minutes}:${(seconds % 60).toFixed(3).padStart(6, "0")}`;
};

const formatDifference = (laps: number, seconds: number): string => {
  if (laps > 0) return `+${laps} V`;
  return `+${decimal(Math.max(seconds, 0), 1)}`;
};

const formatClock = (seconds: number): string => {
  if (!Number.isFinite(seconds) || seconds <= 0) return "--:--";
  const total = Math.ceil(seconds);
  const hours = Math.floor(total / 3600);
  const minutes = Math.floor((total % 3600) / 60);
  const remainder = total % 60;
  return hours > 0
    ? `${hours}:${minutes.toString().padStart(2, "0")}:${remainder.toString().padStart(2, "0")}`
    : `${minutes}:${remainder.toString().padStart(2, "0")}`;
};

const sessionLabel = (sessionType: number): string => {
  if (sessionType >= 10 && sessionType <= 13) {
    return sessionType === 10 ? "CARRERA" : `CARRERA ${sessionType - 9}`;
  }
  if (sessionType >= 5 && sessionType <= 8) {
    return sessionType === 5 ? "CLASIFICACIÓN" : `CLASIFICACIÓN ${sessionType - 4}`;
  }
  if (sessionType === 9) return "WARMUP";
  return sessionType === 0 ? "PRÁCTICA" : `PRÁCTICA ${sessionType}`;
};

const isPracticeSession = (sessionType: number): boolean => sessionType >= 0 && sessionType <= 4;

const classPriority = (vehicleClass: string): number => {
  const tone = classTone(vehicleClass);
  return { hypercar: 0, lmp2: 1, lmp3: 2, lmgt3: 3, other: 4 }[tone] ?? 4;
};

const countryFlagModules = import.meta.glob<string>(
  "./assets/countries/*.{svg,png}",
  { eager: true, query: "?url", import: "default" }
);

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
  const source = countryFlagModules[`./assets/countries/${code}.svg`]
    ?? countryFlagModules[`./assets/countries/${code}.png`];
  if (!code || !source) return undefined;
  const flag = node("img", "country-flag");
  flag.src = source;
  flag.alt = code;
  flag.title = nationality;
  return flag;
};

const displayClass = (vehicleClass: string): string => {
  const value = vehicleClass.toUpperCase();
  if (value.includes("HYPER") || value.includes("GTP")) return "HYPERCAR";
  if (value.includes("LMGT3") || value.includes("GT3")) return "LMGT3";
  return vehicleClass || "SIN CLASE";
};

const classTone = (vehicleClass: string): string => {
  const value = vehicleClass.toUpperCase();
  if (value.includes("HYPER") || value.includes("GTP")) return "hypercar";
  if (value.includes("LMP2")) return "lmp2";
  if (value.includes("LMP3")) return "lmp3";
  if (value.includes("LMGT3") || value.includes("GT3")) return "lmgt3";
  return "other";
};

const manufacturerLogoModules = import.meta.glob<string>(
  "./assets/manufacturers/*.{svg,png}",
  { eager: true, query: "?url", import: "default" }
);

const driverBadgeModules = import.meta.glob<string>(
  "./assets/driver-badges/*.svg",
  { eager: true, query: "?url", import: "default" }
);

const DRIVER_BADGE_LABELS: Record<string, string> = {
  "sr-noob": "Novato",
  "sr-rookie": "Novato",
  "sr-probation": "En prueba",
  "sr-warning": "Advertencia",
  "sr-danger": "Peligro",
  "sr-clean": "Buen piloto",
  "sr-saint": "Piloto de confianza",
  "s397": "Studio 397",
  "content-creator": "Creador de contenido",
  "irl-driver": "Piloto real",
  "early-access": "Acceso anticipado",
  "test-driver": "Piloto de pruebas"
};

const driverBadge = (name: string): HTMLElement => {
  const cell = node("span", "standing-driver-badge");
  const normalized = name.trim().toLowerCase();
  if (!normalized) return cell;
  const source = driverBadgeModules[`./assets/driver-badges/${normalized}.svg`];
  const label = DRIVER_BADGE_LABELS[normalized] ?? normalized;
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
  const source = manufacturerLogoModules[`./assets/manufacturers/${name}.svg`]
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
  if (entry.finish_status === 1) {
    const checkered = node("span", "race-flag checkered-flag");
    checkered.title = "Finalizado";
    checkered.setAttribute("aria-label", "Bandera a cuadros");
    container.append(checkered);
    return container;
  }
  if (entry.finish_status === 3) container.append(node("span", "race-flag dq-flag", "DQ"));
  else if (entry.finish_status === 2) container.append(node("span", "race-flag dnf-flag", "DNF"));
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
  if (seconds < 60) return seconds.toFixed(1);
  const minutes = Math.floor(seconds / 60);
  return `${minutes}:${(seconds % 60).toFixed(1).padStart(4, "0")}`;
};

const cellSignature = (entry: StandingEntry, column: StandingsColumnId, trackLimit: number): string => {
  switch (column) {
    case "position": return `${entry.position}|${entry.position_change}`;
    case "number": return liveCarNumbers.get(entry.vehicle_id) || entry.car_number || "--";
    case "badge": return entry.driver_badge;
    case "driver": return `${entry.driver_name}|${entry.nationality}`;
    case "manufacturer": return `${entry.team_name}|${entry.vehicle_name}`;
    case "ranks": return `${entry.driver_rank}|${Math.round(entry.driver_rank_progress)}|${Math.round(entry.estimated_driver_rank_gain)}|${entry.estimated_driver_rank_gain_available}|${entry.safety_rank}`;
    case "gap": return entry.position === 1 ? `V ${entry.total_laps}` : formatDifference(entry.laps_behind_leader, entry.time_behind_leader);
    case "interval": return entry.position === 1 ? `V ${entry.total_laps}` : formatDifference(entry.laps_behind_next, entry.interval);
    case "best": return `${formatLapTime(entry.best_lap_seconds)}|${entry.best_lap_seconds > 0}|${entry.has_fastest_lap}`;
    case "last": return `${entry.is_out_lap ? "OUT" : formatLapTime(entry.last_lap_seconds)}|${formatLapTime(entry.best_lap_seconds)}|${entry.has_fastest_lap}|${entry.last_lap_valid}`;
    case "average": return formatLapTime(entry.average_lap_seconds);
    case "energy": return entry.virtual_energy_active && entry.virtual_energy_percent > 0
      ? `${entry.virtual_energy_percent >= 99.95 ? "100" : decimal(entry.virtual_energy_percent, 1)}|${entry.virtual_energy_per_lap > 0 ? decimal(entry.virtual_energy_per_lap, 2) : ""}`
      : "--";
    case "damage": return Math.round(entry.damage_percent).toString();
    case "trackLimits": return entry.track_limits_steps === null ? "--" : `${entry.track_limits_steps}|${trackLimit}`;
    case "pitStops": return `${entry.pit_stops}|${entry.pit_stop_requested}|${entry.pit_stop_time_seconds === null ? "" : pitTimeLabel(entry.pit_stop_time_seconds)}`;
    case "tire": return entry.tire_compounds.join("/");
    case "signals": {
      const penalties = livePenalties.get(entry.vehicle_id);
      return `${entry.finish_status}|${entry.in_garage}|${entry.in_pits}|${entry.causing_yellow}|${entry.penalty_count}|${entry.flag}|${penalties?.DT ?? 0}|${penalties?.SG ?? 0}`;
    }
  }
};

const createCell = (entry: StandingEntry, column: StandingsColumnId, trackLimit: number): HTMLElement => {
  switch (column) {
    case "position": {
      const cell = node("div", "standing-position-cell");
      cell.append(node("strong", "standing-position", entry.position.toString()));
      cell.append(positionChange(entry.position_change));
      return cell;
    }
    case "number": return node("span", "car-number", liveCarNumbers.get(entry.vehicle_id) || entry.car_number || "--");
    case "badge": return driverBadge(entry.driver_badge);
    case "driver": {
      const cell = node("div", "standing-driver");
      const flag = countryFlag(entry.nationality);
      if (flag) cell.append(flag);
      const fullName = entry.driver_name || "—";
      const name = node("b", "driver-name", shortDriverName(fullName));
      name.title = fullName;
      cell.append(name);
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
      const personalBest = entry.last_lap_seconds > 0 && entry.best_lap_seconds > 0 && Math.abs(entry.last_lap_seconds - entry.best_lap_seconds) <= 0.001;
      if (personalBest) cell.classList.add(entry.has_fastest_lap ? "session-fastest" : "personal-best");
      if (entry.last_lap_seconds > 0 && !entry.last_lap_valid) cell.classList.add("invalid-lap");
      return cell;
    }
    case "average": return node("b", "lap-time", formatLapTime(entry.average_lap_seconds));
    case "energy": {
      const cell = node("div", "standing-energy");
      if (entry.virtual_energy_active && entry.virtual_energy_percent > 0) {
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
        ? "Cortes no disponibles"
        : `${value} puntos de límites de pista`;
      if (entry.track_limits_steps !== null) applyTrackLimitTone(cell, entry.track_limits_steps, trackLimit);
      return cell;
    }
    case "pitStops": {
      const timing = entry.pit_stop_time_seconds !== null;
      const value = timing ? pitTimeLabel(entry.pit_stop_time_seconds ?? 0) : entry.pit_stops.toString();
      const cell = node("strong", "standing-pit-stops", value);
      cell.classList.toggle("timing", timing);
      cell.classList.toggle("requested", entry.pit_stop_requested && !timing);
      cell.title = timing ? `Tiempo de pit: ${value} s · ${entry.pit_stops} paradas` : entry.pit_stop_requested ? `${entry.pit_stops} paradas · Petición de parada activa` : `${entry.pit_stops} paradas`;
      return cell;
    }
    case "tire": {
      const compounds = entry.tire_compounds.length === 4 ? entry.tire_compounds : [entry.tire_compound];
      const unique = [...new Set(compounds)];
      if (unique.length === 1) {
        const compound = unique[0] || "?";
        const cell = node("span", "tire-compound");
        cell.dataset.compound = compound.toLowerCase();
        cell.title = `Compuesto ${compound}`;
        cell.append(icon(compoundIconUrl(compound), "tire-compound-icon"));
        return cell;
      }
      const grid = node("span", "tire-compound-grid");
      for (const compound of compounds) {
        const wheel = node("span", "tire-wheel");
        wheel.dataset.compound = compound.toLowerCase();
        wheel.title = `Compuesto ${compound || "?"}`;
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

const continuousDriverRank = (entry: StandingEntry): number | null => {
  if (!Number.isFinite(entry.driver_rank_progress) || entry.driver_rank_progress < 0) return null;
  const match = entry.driver_rank.trim().toUpperCase().match(/^([BSGP])([1-3])$/);
  if (!match) return null;
  const level = { B: 0, S: 3, G: 6, P: 9 }[match[1] as "B" | "S" | "G" | "P"];
  return (level + Number(match[2])) * 100 + Math.min(entry.driver_rank_progress, 100);
};

const formatStrengthOfField = (entries: StandingEntry[]): HTMLElement => {
  const ratings = entries
    .map(continuousDriverRank)
    .filter((rating): rating is number => rating !== null);
  const sof = node("span", "class-sof", "SOF --");
  if (ratings.length === 0) {
    sof.title = "SOF no disponible: no hay perfiles DR resueltos";
    return sof;
  }

  const average = ratings.reduce((total, rating) => total + rating, 0) / ratings.length;
  const capped = Math.max(100, Math.min(average, 1300));
  const terminalRank = capped >= 1300;
  const band = terminalRank ? 12 : Math.floor(capped / 100);
  const rankIndex = Math.max(0, Math.min(band - 1, 11));
  const levels = ["B", "S", "G", "P"];
  const rank = `${levels[Math.floor(rankIndex / 3)]}${rankIndex % 3 + 1}`;
  const progress = terminalRank ? 100 : Math.round(capped - band * 100);
  sof.textContent = `SOF ${rank} ${progress}%`;
  sof.title = `Fuerza media de la categoría · ${ratings.length}/${entries.length} perfiles DR`;
  sof.dataset.coverage = ratings.length === entries.length ? "complete" : "partial";
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
    sessionGroup.append(node("span", "standings-session-split", `SPLIT ${split}`));
  }
  if (settings.header.remainingTime) {
    const clock = node("b", "standings-session-clock");
    clock.append(icon(timingIconUrl, "standings-session-timing-icon"), formatClock(frame.session_time_remaining));
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
      `${frame.player_total_laps + 1}/${totalLabel}`
    ));
  }
  if (frame.rest_weather_available && settings.header.airTemperature) {
    const ambient = node("span", "standings-header-temperature");
    ambient.append(
      icon(airTemperatureIconUrl, "standings-header-icon", "Temperatura ambiente"),
      `${Math.round(frame.ambient_temperature_c)}°C`
    );
    ambient.title = `Temperatura ambiente ${frame.ambient_temperature_c.toFixed(1)} °C`;
    dataGroup.append(ambient);
  }
  if (frame.rest_weather_available && settings.header.trackTemperature) {
    const track = node("span", "standings-header-temperature");
    track.append(
      icon(trackTemperatureIconUrl, "standings-header-icon", "Temperatura de pista"),
      `${Math.round(frame.track_temperature_c)}°C`
    );
    track.title = `Temperatura de pista ${frame.track_temperature_c.toFixed(1)} °C`;
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
  if (settings.header.realTimeClock) {
    const localTime = node("time", "standings-header-local-time");
    localTime.append(
      icon(timingIconUrl, "standings-header-icon", "Hora real"),
      new Date().toLocaleTimeString("es-ES", { hour: "2-digit", minute: "2-digit" })
    );
    dataGroup.append(localTime);
  }
  header.append(sessionGroup, dataGroup);
  return header;
};

const classTableHeader = (
  vehicleClass: string,
  entries: StandingEntry[],
  initialCount: number,
  sessionType: number
): HTMLElement => {
  const header = node("header", "class-table-header");
  const identity = node("div", "class-heading-inline");
  const practice = isPracticeSession(sessionType);
  const currentCount = entries.filter((entry) => entry.finish_status !== 2 && entry.finish_status !== 3).length;
  const retiredCount = Math.max(initialCount - currentCount, 0);
  const carCount = node("span", "class-car-count");
  carCount.append(icon(profileIconUrl, "class-car-icon"), practice ? `${currentCount}` : `${currentCount}/${initialCount}`);
  carCount.dataset.retired = !practice && retiredCount > 0 ? "true" : "false";
  carCount.title = practice
    ? `${currentCount} actuales`
    : retiredCount > 0
      ? `${currentCount} actuales · ${initialCount} iniciales · ${retiredCount} DNF/DQ`
      : `${currentCount} actuales · ${initialCount} iniciales`;
  const columns = activeColumns();
  const identityColumns = columns.filter(({ identity: isIdentity }) => isIdentity);
  identity.append(node("strong", undefined, displayClass(vehicleClass)));
  identity.append(carCount);
  if (!practice) identity.append(formatStrengthOfField(entries));
  identity.style.gridColumn = `span ${identityColumns.length}`;
  header.append(identity);
  for (const column of columns.filter(({ identity: isIdentity }) => !isIdentity)) {
    header.append(columnLabel(column));
  }
  return header;
};

const visibleClassEntries = (entries: StandingEntry[], requestedRows: number): StandingEntry[] => {
  const targetSize = Math.min(Math.max(requestedRows, 3), entries.length);
  const top = entries.slice(0, Math.min(3, targetSize));
  const playerIndex = entries.findIndex((entry) => entry.is_player);
  if (playerIndex < 0) return entries.slice(0, targetSize);

  const selectedIds = new Set(top.map((entry) => entry.vehicle_id));
  const visible = [...top];
  const proximityOrder = entries
    .map((entry, index) => ({ entry, distance: Math.abs(index - playerIndex) }))
    .sort((left, right) => left.distance - right.distance || left.entry.position - right.entry.position);
  for (const { entry } of proximityOrder) {
    if (visible.length >= targetSize) break;
    if (!selectedIds.has(entry.vehicle_id)) {
      selectedIds.add(entry.vehicle_id);
      visible.push(entry);
    }
  }
  return visible.sort((left, right) => left.position - right.position);
};

const updateInitialClassCounts = (
  frame: TelemetryFrame,
  groups: Map<string, StandingEntry[]>
): boolean => {
  const trackName = frame.track_name.trim().toUpperCase();
  const sessionChanged = countSession !== null && (
    countSession.sessionType !== frame.session_type
    || countSession.trackName !== trackName
    || (
      frame.player_total_laps + 1 < countSession.playerLaps
      && frame.session_time_remaining > countSession.timeRemaining + 60
    )
    || (
      frame.player_total_laps <= 1
      && frame.session_time_remaining > countSession.timeRemaining + 120
    )
  );
  const reset = countSession === null || sessionChanged;
  if (reset) initialClassCarCounts.clear();

  if (!isPracticeSession(frame.session_type)) {
    for (const [vehicleClass, entries] of groups) {
      initialClassCarCounts.set(
        vehicleClass,
        Math.max(initialClassCarCounts.get(vehicleClass) ?? 0, entries.length)
      );
    }
  }
  countSession = {
    sessionType: frame.session_type,
    trackName,
    playerLaps: frame.player_total_laps,
    timeRemaining: frame.session_time_remaining
  };
  return reset;
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

const resetRenderCaches = (): void => {
  rowCache.clear();
  classSections.clear();
  classHeaders.clear();
  cachedSessionHeader = null;
};

const cachedSessionHeaderFor = (frame: TelemetryFrame): HTMLElement => {
  const signature = JSON.stringify([
    settings.header,
    frame.session_type,
    Math.ceil(frame.session_time_remaining),
    frame.session_max_laps,
    frame.player_total_laps,
    frame.session_split_number,
    frame.session_split_count,
    Math.round(frame.session_total_laps_estimated * 100),
    frame.rest_weather_available,
    Math.round(frame.ambient_temperature_c),
    Math.round(frame.track_temperature_c),
    Math.round(frame.brake_bias_percent * 10),
    frame.track_limits_steps,
    frame.track_limits_steps_per_penalty,
    new Date().toLocaleTimeString("es-ES", { hour: "2-digit", minute: "2-digit" })
  ]);
  if (!cachedSessionHeader || cachedSessionHeader.signature !== signature) {
    cachedSessionHeader = { signature, element: sessionHeader(frame) };
  }
  return cachedSessionHeader.element;
};

const cachedClassHeaderFor = (
  vehicleClass: string,
  entries: StandingEntry[],
  initialCount: number,
  sessionType: number
): HTMLElement => {
  const signature = JSON.stringify([
    activeColumns().map(({ id }) => id),
    sessionType,
    initialCount,
    entries.map((entry) => [
      entry.vehicle_id,
      entry.finish_status,
      entry.driver_rank,
      entry.driver_rank_progress
    ]),
  ]);
  const cached = classHeaders.get(vehicleClass);
  if (!cached || cached.signature !== signature) {
    const next = { signature, element: classTableHeader(vehicleClass, entries, initialCount, sessionType) };
    classHeaders.set(vehicleClass, next);
    return next.element;
  }
  return cached.element;
};

const render = (frame: TelemetryFrame): void => {
  const list = document.getElementById("standings-list");
  if (!list) return;

  if (frame.standings.length === 0) {
    // LMU puede publicar un frame intermedio sin standings al actualizar la
    // sesión. Conservamos la tabla ya pintada para que ese frame no produzca
    // un destello ni sustituya los datos por el estado de espera.
    if (lastFrame?.standings.length) return;
    list.replaceChildren(node("p", "empty-state", "Esperando datos de la sesión…"));
    return;
  }
  lastFrame = frame;

  const groups = new Map<string, StandingEntry[]>();
  const playerClass = frame.standings.find((entry) => entry.is_player)?.vehicle_class;
  for (const entry of frame.standings) {
    const entries = groups.get(entry.vehicle_class) ?? [];
    entries.push(entry);
    groups.set(entry.vehicle_class, entries);
  }
  const sessionReset = updateInitialClassCounts(frame, groups);
  if (sessionReset) resetRenderCaches();

  const visibleGroups = [...groups.entries()]
    .filter(([vehicleClass]) => !playerClass || vehicleClass === playerClass || settings.showOtherClasses)
    .sort(([left], [right]) => classPriority(left) - classPriority(right));
  const children: HTMLElement[] = [];
  if (settings.showHeader) children.push(cachedSessionHeaderFor(frame));
  const visibleRowCounts: number[] = [];
  for (const [vehicleClass, entries] of visibleGroups) {
    const isPlayerClass = vehicleClass === playerClass;
    const visibleEntries = vehicleClass === playerClass
      ? visibleClassEntries(entries, settings.ownClassRows)
      : entries.slice(0, settings.otherClassRows);
    visibleRowCounts.push(visibleEntries.length);
    let group = classSections.get(vehicleClass);
    if (!group) {
      group = node("section", "standings-class");
      classSections.set(vehicleClass, group);
    }
    group.dataset.classTone = classTone(vehicleClass);
    const initialCount = initialClassCarCounts.get(vehicleClass) ?? entries.length;
    syncChildren(group, [
      cachedClassHeaderFor(vehicleClass, entries, initialCount, frame.session_type),
      ...visibleEntries.map((entry) => renderRow(entry, frame.track_limits_steps_per_penalty))
    ]);
    children.push(group);
  }
  syncChildren(list, children);
  const contentHeight = visibleRowCounts.reduce((height, rowCount, groupIndex) => (
    height
    + 23
    + rowCount * 23
    + (groupIndex < visibleRowCounts.length - 1 ? 6 : 0)
  ), 20 + (settings.showHeader ? 25 : 0));
  const nextOverlayHeight = Math.max(450, contentHeight);
  if (nextOverlayHeight !== fittedOverlayHeight) {
    fittedOverlayHeight = nextOverlayHeight;
    updateOverlayFit({ width: 948, height: nextOverlayHeight });
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
    standings
  } as TelemetryFrame);
}
