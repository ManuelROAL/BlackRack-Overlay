import {
  DEFAULT_DRIVER_NAME_FORMAT,
  isDriverNameFormat,
  type DriverNameFormat
} from "./driver-name-format";

export type StandingsColumnId =
  | "position"
  | "number"
  | "badge"
  | "driver"
  | "manufacturer"
  | "ranks"
  | "gap"
  | "interval"
  | "best"
  | "last"
  | "average"
  | "energy"
  | "damage"
  | "trackLimits"
  | "pitStops"
  | "tire"
  | "signals";

export interface StandingsColumnDefinition {
  id: StandingsColumnId;
  label: string;
  header: string;
  width: number;
  configurable: boolean;
  identity: boolean;
}

export interface StandingsSettings {
  columns: Record<StandingsColumnId, boolean>;
  columnOrder: StandingsColumnId[];
  showHeader: boolean;
  header: Record<StandingsHeaderOptionId, boolean>;
  ownClassRows: number;
  otherClassRows: number;
  showOtherClasses: boolean;
  driverNameFormat: DriverNameFormat;
}

export type StandingsHeaderOptionId =
  | "sessionType"
  | "eventSplit"
  | "remainingTime"
  | "laps"
  | "airTemperature"
  | "trackTemperature"
  | "brakeBias"
  | "trackLimits"
  | "realTimeClock";

export interface StandingsHeaderOptionDefinition {
  id: StandingsHeaderOptionId;
  label: string;
}

export const STANDINGS_SETTINGS_KEY = "lmu-overlay.standings.v1";

export const STANDINGS_HEADER_OPTIONS: StandingsHeaderOptionDefinition[] = [
  { id: "sessionType", label: "Tipo de sesión" },
  { id: "eventSplit", label: "Split del evento" },
  { id: "remainingTime", label: "Tiempo restante / total" },
  { id: "laps", label: "Vuelta actual / restantes" },
  { id: "airTemperature", label: "Temperatura ambiente" },
  { id: "trackTemperature", label: "Temperatura de pista" },
  { id: "brakeBias", label: "Reparto de frenada" },
  { id: "trackLimits", label: "Cortes de circuito" },
  { id: "realTimeClock", label: "Hora real" }
];

export const STANDINGS_COLUMNS: StandingsColumnDefinition[] = [
  { id: "position", label: "Posición", header: "", width: 36, configurable: false, identity: true },
  { id: "number", label: "Dorsal", header: "", width: 28, configurable: true, identity: true },
  { id: "manufacturer", label: "Marca", header: "", width: 28, configurable: true, identity: true },
  { id: "badge", label: "Insignia", header: "", width: 27, configurable: true, identity: true },
  { id: "driver", label: "Piloto", header: "", width: 130, configurable: false, identity: true },
  { id: "ranks", label: "DR / SR", header: "", width: 95, configurable: true, identity: true },
  { id: "gap", label: "Gap", header: "GAP", width: 50, configurable: true, identity: false },
  { id: "interval", label: "Intervalo", header: "INT", width: 50, configurable: true, identity: false },
  { id: "best", label: "Mejor vuelta", header: "BEST", width: 68, configurable: true, identity: false },
  { id: "last", label: "Última vuelta", header: "LAST", width: 68, configurable: true, identity: false },
  { id: "average", label: "Media 5", header: "AVG 5", width: 68, configurable: true, identity: false },
  { id: "energy", label: "Energía", header: "NRG", width: 90, configurable: true, identity: false },
  { id: "damage", label: "Daño", header: "DMG", width: 42, configurable: true, identity: false },
  { id: "trackLimits", label: "Cortes de circuito", header: "TL", width: 30, configurable: true, identity: false },
  { id: "pitStops", label: "Paradas / tiempo", header: "PIT", width: 42, configurable: true, identity: false },
  { id: "tire", label: "Neumático", header: "NEU", width: 30, configurable: true, identity: false },
  { id: "signals", label: "Banderas / estados", header: "", width: 115, configurable: true, identity: false }
];

export const defaultStandingsSettings = (): StandingsSettings => {
  const columns = Object.fromEntries(STANDINGS_COLUMNS.map(({ id }) => [id, true])) as Record<
    StandingsColumnId,
    boolean
  >;
  return {
    columns,
    columnOrder: STANDINGS_COLUMNS.map(({ id }) => id),
    showHeader: true,
    header: Object.fromEntries(STANDINGS_HEADER_OPTIONS.map(({ id }) => [id, true])) as Record<
      StandingsHeaderOptionId,
      boolean
    >,
    ownClassRows: 10,
    otherClassRows: 3,
    showOtherClasses: true,
    driverNameFormat: DEFAULT_DRIVER_NAME_FORMAT
  };
};

const integerInRange = (value: unknown, fallback: number, minimum: number, maximum: number): number =>
  Number.isFinite(value) ? Math.max(minimum, Math.min(Math.round(Number(value)), maximum)) : fallback;

export const readStandingsSettings = (): StandingsSettings => {
  const settings = defaultStandingsSettings();
  try {
    const stored = JSON.parse(localStorage.getItem(STANDINGS_SETTINGS_KEY) ?? "{}") as {
      columns?: Partial<Record<StandingsColumnId, boolean>>;
      columnOrder?: unknown[];
      showHeader?: boolean;
      header?: Partial<Record<StandingsHeaderOptionId, boolean>>;
      ownClassRows?: number;
      otherClassRows?: number;
      showOtherClasses?: boolean;
      driverNameFormat?: unknown;
    };
    for (const column of STANDINGS_COLUMNS) {
      if (column.configurable && typeof stored.columns?.[column.id] === "boolean") {
        settings.columns[column.id] = stored.columns[column.id] as boolean;
      }
    }
    if (Array.isArray(stored.columnOrder)) {
      const validIds = new Set(STANDINGS_COLUMNS.map(({ id }) => id));
      const storedOrder = stored.columnOrder.filter(
        (id): id is StandingsColumnId => typeof id === "string" && validIds.has(id as StandingsColumnId)
      );
      settings.columnOrder = [
        ...new Set(storedOrder),
        ...settings.columnOrder.filter((id) => !storedOrder.includes(id))
      ];
      settings.columnOrder = [
        ...settings.columnOrder.filter((id) => id !== "signals"),
        "signals"
      ];
    }
    if (typeof stored.showHeader === "boolean") settings.showHeader = stored.showHeader;
    for (const option of STANDINGS_HEADER_OPTIONS) {
      if (typeof stored.header?.[option.id] === "boolean") {
        settings.header[option.id] = stored.header[option.id] as boolean;
      }
    }
    settings.ownClassRows = integerInRange(stored.ownClassRows, settings.ownClassRows, 3, 30);
    settings.otherClassRows = integerInRange(stored.otherClassRows, settings.otherClassRows, 1, 15);
    if (typeof stored.showOtherClasses === "boolean") {
      settings.showOtherClasses = stored.showOtherClasses;
    }
    if (isDriverNameFormat(stored.driverNameFormat)) {
      settings.driverNameFormat = stored.driverNameFormat;
    }
  } catch {
    localStorage.removeItem(STANDINGS_SETTINGS_KEY);
  }
  return settings;
};

export const visibleStandingsColumns = (
  settings: Pick<StandingsSettings, "columns" | "columnOrder">
): StandingsColumnDefinition[] => {
  const columns = new Map(STANDINGS_COLUMNS.map((column) => [column.id, column]));
  const order = [...settings.columnOrder.filter((id) => id !== "signals"), "signals" as const];
  return order
    .map((id) => columns.get(id))
    .filter((column): column is StandingsColumnDefinition =>
      column !== undefined && (!column.configurable || settings.columns[column.id])
    );
};
