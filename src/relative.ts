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
import { airTemperatureIconUrl, compoundIconUrl, timingIconUrl, trackTemperatureIconUrl } from "./lmu-icons";
import { applyTrackLimitTone, formatTrackLimitPoints } from "./track-limit-tone";

let relativeSettings = readRelativeSettings();
const relativeBaseHeight = (): number => Math.max(
  220,
  48 + (relativeSettings.aheadRows + relativeSettings.behindRows + 1) * 23
);
const relativeBaseWidth = (): number => Math.max(
  760,
  visibleRelativeColumns(relativeSettings).reduce((total, { width }) => total + width, 0) + 32
);
const updateOverlayFit = fitOverlay({
  width: relativeBaseWidth(),
  height: relativeBaseHeight()
});
bindOverlayTransparency("relative");
const renderPerformance = createOverlayPerformanceTracker("relative");

let lastFrame: TelemetryFrame | null = null;

const activeColumns = () => visibleRelativeColumns(relativeSettings);

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

const timingIcon = (): HTMLImageElement => {
  const image = node("img", "relative-timing-icon");
  image.src = timingIconUrl;
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
  cells: Map<RelativeColumnId, CachedCell>;
  columns: string;
}

const rowCache = new Map<string, CachedRow>();

const pitTimeLabel = (seconds: number): string => {
  if (seconds < 60) return seconds.toFixed(1);
  const minutes = Math.floor(seconds / 60);
  return `${minutes}:${(seconds % 60).toFixed(1).padStart(4, "0")}`;
};

const cellSignature = (entry: StandingEntry, column: RelativeColumnId, trackLimit: number): string => {
  switch (column) {
    case "position": return `${entry.position}|${entry.position_change}|${relativeSettings.options.positionChange}`;
    case "number": return liveCarNumbers.get(entry.vehicle_id) || entry.car_number || "--";
    case "country": return entry.nationality;
    case "badge": return entry.driver_badge;
    case "driver": return `${entry.driver_name}|${entry.nationality}`;
    case "ranks": return `${entry.driver_rank}|${Math.round(entry.driver_rank_progress)}|${Math.round(entry.estimated_driver_rank_gain)}|${entry.estimated_driver_rank_gain_available}|${entry.safety_rank}`;
    case "relative": return entry.relative_gap_seconds.toFixed(2);
    case "lap": return entry.total_laps.toString();
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

const createCell = (entry: StandingEntry, column: RelativeColumnId, trackLimit: number): HTMLElement => {
  switch (column) {
    case "position": {
      const cell = node("div", "standing-position-cell");
      cell.append(node("strong", "standing-position", entry.position.toString()));
      if (relativeSettings.options.positionChange) {
        cell.append(positionChange(entry.position_change));
      }
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
      const name = node("b", "driver-name", shortDriverName(fullName));
      name.title = fullName;
      cell.append(name);
      return cell;
    }
    case "ranks": {
      const cell = node("span", "driver-ranks");
      if (entry.driver_rank) cell.append(rankBadge("DR", entry.driver_rank, entry.driver_rank_progress, entry.estimated_driver_rank_gain, entry.estimated_driver_rank_gain_available));
      if (entry.safety_rank) cell.append(rankBadge("SR", entry.safety_rank));
      return cell;
    }
    case "relative": {
      const gap = entry.is_player ? "0,0" : `${entry.relative_gap_seconds > 0 ? "+" : ""}${decimal(entry.relative_gap_seconds, 1)}`;
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

const renderRow = (entry: StandingEntry, trackLimit: number, instanceKey: string): HTMLElement => {
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


const sessionHeader = (frame: TelemetryFrame): HTMLElement => {
  const header = node("header", "relative-table-header");
  if (frame.rest_weather_available && relativeSettings.options.airTemperature) {
    const ambient = node("span", "relative-temperature relative-temperature-air");
    ambient.append(temperatureIcon(airTemperatureIconUrl), `${Math.round(frame.ambient_temperature_c)}°C`);
    ambient.title = `Temperatura ambiente ${frame.ambient_temperature_c.toFixed(1)} °C`;
    header.append(ambient);
  }
  if (frame.rest_weather_available && relativeSettings.options.trackTemperature) {
    const track = node("span", "relative-temperature relative-temperature-track");
    track.append(temperatureIcon(trackTemperatureIconUrl), `${Math.round(frame.track_temperature_c)}°C`);
    track.title = `Temperatura de pista ${frame.track_temperature_c.toFixed(1)} °C`;
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
  if (relativeSettings.options.realTimeClock) {
    const localTime = node("time", "relative-local-time");
    localTime.append(timingIcon(), new Date().toLocaleTimeString("es-ES", {
      hour: "2-digit",
      minute: "2-digit"
    }));
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
    relativeSettings.options.realTimeClock,
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

const relativeEntries = (entries: StandingEntry[]): {
  ahead: StandingEntry[];
  behind: StandingEntry[];
  visible: StandingEntry[];
} => {
  const player = entries.find((entry) => entry.is_player);
  if (!player) return { ahead: [], behind: [], visible: [] };

  const aheadCandidates = entries
    .filter((entry) => !entry.is_player && !entry.in_garage && Number.isFinite(entry.relative_ahead_seconds) && entry.relative_ahead_seconds < -0.05)
    .map((entry) => ({ ...entry, relative_gap_seconds: entry.relative_ahead_seconds }))
    .sort((left, right) => Math.abs(left.relative_gap_seconds) - Math.abs(right.relative_gap_seconds))
    .slice(0, relativeSettings.aheadRows);
  const behindCandidates = entries
    .filter((entry) => !entry.is_player && !entry.in_garage && Number.isFinite(entry.relative_behind_seconds) && entry.relative_behind_seconds > 0.05)
    .map((entry) => ({ ...entry, relative_gap_seconds: entry.relative_behind_seconds }))
    .sort((left, right) => left.relative_gap_seconds - right.relative_gap_seconds)
    .slice(0, relativeSettings.behindRows);
  // Igual que el relativo de LMU, un coche puede representar el tráfico más
  // próximo una vez por delante y otra por detrás al cerrar la vuelta, pero no
  // se rellena repetidamente una misma dirección con ese único coche.
  const ahead = aheadCandidates.reverse();
  const behind = behindCandidates;
  return { ahead, behind, visible: [...ahead, player, ...behind] };
};

const render = (frame: TelemetryFrame): void => {
  const list = document.getElementById("relative-list");
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

  {
    const selected = relativeEntries(frame.standings);
    if (selected.visible.length === 0) {
      list.replaceChildren(node("p", "empty-state", "Esperando al jugador…"));
      return;
    }
    let group = classSections.get("__relative");
    if (!group) {
      group = node("section", "standings-class");
      group.dataset.classTone = "relative";
      classSections.set("__relative", group);
    }
    const groupChildren: HTMLElement[] = [];
    groupChildren.push(...selected.visible.map((entry, index) => renderRow(
      entry,
      frame.track_limits_steps_per_penalty,
      `relative-${index}`
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
    standings
  } as TelemetryFrame);
}
