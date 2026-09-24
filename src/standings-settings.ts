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
  | "delta"
  | "average"
  | "energy"
  | "damage"
  | "trackLimits"
  | "pitStops"
  | "pitTime"
  | "pitLap"
  | "tire"
  | "signals";

export interface StandingsColumnDefinition {
  id: StandingsColumnId;
  labelKey: import("./i18n").TranslationKey;
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
  pitInformationLayout: "inline" | "above" | "column";
  driverNameFormat: DriverNameFormat;
  combineLapTimes: boolean;
  deltaLapCount: number;
  deltaReference: "last_lap" | "best_lap";
  invertDeltaLayout: boolean;
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
  | "gameTimeClock"
  | "realTimeClock";

export interface StandingsHeaderOptionDefinition {
  id: StandingsHeaderOptionId;
  labelKey: import("./i18n").TranslationKey;
}

export const STANDINGS_SETTINGS_KEY = "blackrack-overlay.standings.v1";

export const STANDINGS_HEADER_OPTIONS: StandingsHeaderOptionDefinition[] = [
  { id: "sessionType", labelKey: "header.sessionType" }, { id: "eventSplit", labelKey: "header.eventSplit" },
  { id: "remainingTime", labelKey: "header.remainingTime" }, { id: "laps", labelKey: "header.laps" },
  { id: "airTemperature", labelKey: "header.airTemperature" }, { id: "trackTemperature", labelKey: "header.trackTemperature" },
  { id: "brakeBias", labelKey: "header.brakeBias" }, { id: "trackLimits", labelKey: "header.trackLimits" },
  { id: "gameTimeClock", labelKey: "header.gameTimeClock" }, { id: "realTimeClock", labelKey: "header.realTimeClock" }
];

export const STANDINGS_COLUMNS: StandingsColumnDefinition[] = [
  { id: "position", labelKey: "column.position", header: "", width: 36, configurable: false, identity: true },
  { id: "number", labelKey: "column.number", header: "", width: 28, configurable: true, identity: true },
  { id: "manufacturer", labelKey: "column.manufacturer", header: "", width: 28, configurable: true, identity: true },
  { id: "badge", labelKey: "column.badge", header: "", width: 27, configurable: true, identity: true },
  { id: "driver", labelKey: "column.driver", header: "", width: 145, configurable: false, identity: true },
  { id: "ranks", labelKey: "column.ranks", header: "", width: 95, configurable: true, identity: true },
  { id: "gap", labelKey: "column.gap", header: "GAP", width: 46, configurable: true, identity: false },
  { id: "interval", labelKey: "column.interval", header: "INT", width: 46, configurable: true, identity: false },
  { id: "best", labelKey: "column.best", header: "BEST", width: 60, configurable: true, identity: false },
  { id: "last", labelKey: "column.last", header: "LAST", width: 60, configurable: true, identity: false },
  { id: "delta", labelKey: "column.delta", header: "DELTA", width: 72, configurable: true, identity: false },
  { id: "average", labelKey: "column.average", header: "AVG 5", width: 60, configurable: true, identity: false },
  { id: "energy", labelKey: "column.energy", header: "NRG", width: 62, configurable: true, identity: false },
  { id: "damage", labelKey: "column.damage", header: "DMG", width: 34, configurable: true, identity: false },
  { id: "trackLimits", labelKey: "column.trackLimits", header: "TL", width: 30, configurable: true, identity: false },
  { id: "pitStops", labelKey: "column.pitStops", header: "PIT", width: 34, configurable: true, identity: false },
  { id: "pitTime", labelKey: "column.pitTime", header: "", width: 0, configurable: true, identity: false },
  { id: "pitLap", labelKey: "column.pitLap", header: "", width: 0, configurable: true, identity: false },
  { id: "tire", labelKey: "column.tire", header: "NEU", width: 30, configurable: true, identity: false },
  { id: "signals", labelKey: "column.signals", header: "", width: 96, configurable: true, identity: false }
];

export const normalizeStandingsColumnOrder = (
  order: ReadonlyArray<StandingsColumnId>
): StandingsColumnId[] => {
  const completeOrder = [
    ...new Set(order),
    ...STANDINGS_COLUMNS.map(({ id }) => id).filter((id) => !order.includes(id))
  ];
  const identityIds = new Set(
    STANDINGS_COLUMNS.filter(({ identity }) => identity).map(({ id }) => id)
  );
  return [
    ...completeOrder.filter((id) => identityIds.has(id)),
    ...completeOrder.filter((id) => !identityIds.has(id) && id !== "signals"),
    "signals"
  ];
};

export const defaultStandingsSettings = (): StandingsSettings => {
  const columns = Object.fromEntries(STANDINGS_COLUMNS.map(({ id }) => [id, true])) as Record<
    StandingsColumnId,
    boolean
  >;
  columns.delta = false;
  return {
    columns,
    columnOrder: normalizeStandingsColumnOrder(STANDINGS_COLUMNS.map(({ id }) => id)),
    showHeader: true,
    header: Object.fromEntries(STANDINGS_HEADER_OPTIONS.map(({ id }) => [id, true])) as Record<
      StandingsHeaderOptionId,
      boolean
    >,
    ownClassRows: 10,
    otherClassRows: 3,
    showOtherClasses: true,
    pitInformationLayout: "inline",
    driverNameFormat: DEFAULT_DRIVER_NAME_FORMAT,
    combineLapTimes: false,
    deltaLapCount: 3,
    deltaReference: "last_lap",
    invertDeltaLayout: false
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
      pitInformationLayout?: unknown;
      driverNameFormat?: unknown;
      combineLapTimes?: boolean;
      deltaLapCount?: number;
      deltaReference?: unknown;
      invertDeltaLayout?: boolean;
    };
    for (const column of STANDINGS_COLUMNS) {
      if (column.configurable && typeof stored.columns?.[column.id] === "boolean") {
        settings.columns[column.id] = stored.columns[column.id] as boolean;
      }
    }
    for (const id of ["pitTime", "pitLap"] as const) {
      if (stored.columns?.[id] === undefined && typeof stored.columns?.pitStops === "boolean") {
        settings.columns[id] = stored.columns.pitStops;
      }
    }
    if (Array.isArray(stored.columnOrder)) {
      const validIds = new Set(STANDINGS_COLUMNS.map(({ id }) => id));
      const storedOrder = stored.columnOrder.filter(
        (id): id is StandingsColumnId => typeof id === "string" && validIds.has(id as StandingsColumnId)
      );
      settings.columnOrder = normalizeStandingsColumnOrder([
        ...new Set(storedOrder),
        ...settings.columnOrder.filter((id) => !storedOrder.includes(id))
      ]);
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
    if (stored.pitInformationLayout === "above" || stored.pitInformationLayout === "column") {
      settings.pitInformationLayout = stored.pitInformationLayout;
    }
    if (isDriverNameFormat(stored.driverNameFormat)) {
      settings.driverNameFormat = stored.driverNameFormat;
    }
    if (typeof stored.combineLapTimes === "boolean") settings.combineLapTimes = stored.combineLapTimes;
    settings.deltaLapCount = integerInRange(stored.deltaLapCount, settings.deltaLapCount, 1, 5);
    if (stored.deltaReference === "last_lap" || stored.deltaReference === "best_lap") {
      settings.deltaReference = stored.deltaReference;
    }
    if (typeof stored.invertDeltaLayout === "boolean") settings.invertDeltaLayout = stored.invertDeltaLayout;
  } catch {
    localStorage.removeItem(STANDINGS_SETTINGS_KEY);
  }
  return settings;
};

export const visibleStandingsColumns = (
  settings: Pick<StandingsSettings, "columns" | "columnOrder" | "pitInformationLayout" | "combineLapTimes">
): StandingsColumnDefinition[] => {
  const columns = new Map(STANDINGS_COLUMNS.map((column) => [column.id, column]));
  const order = [...settings.columnOrder.filter((id) => id !== "signals"), "signals" as const];
  return order
    .map((id) => columns.get(id))
    .filter((column): column is StandingsColumnDefinition =>
      column !== undefined
        && !["pitTime", "pitLap"].includes(column.id)
        && (column.id === "best" && settings.combineLapTimes
          ? settings.columns.best || settings.columns.last
          : column.id === "last" && settings.combineLapTimes
            ? false
            : column.id === "pitStops"
              ? settings.pitInformationLayout === "column" && (settings.columns.pitStops || settings.columns.pitTime || settings.columns.pitLap)
              : !column.configurable || settings.columns[column.id])
    ).map((column) => column.id === "pitStops"
      ? { ...column, width: 112 }
      : column.id === "best" && settings.combineLapTimes
        ? { ...column, header: "BEST/LAST" }
        : column);
};
