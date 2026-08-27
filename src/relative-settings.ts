import {
  DEFAULT_DRIVER_NAME_FORMAT,
  isDriverNameFormat,
  type DriverNameFormat
} from "./driver-name-format";

export type RelativeColumnId =
  | "position"
  | "number"
  | "country"
  | "driver"
  | "badge"
  | "ranks"
  | "relative"
  | "lap"
  | "average"
  | "last"
  | "best"
  | "energy"
  | "damage"
  | "trackLimits"
  | "pitStops"
  | "tire"
  | "signals";

export interface RelativeColumnDefinition {
  id: RelativeColumnId;
  labelKey: import("./i18n").TranslationKey;
  header: string;
  width: number;
  configurable: boolean;
  identity: boolean;
  option?: RelativeOptionId;
}

export type RelativeOptionId =
  | "number"
  | "country"
  | "license"
  | "rating"
  | "tableHeader"
  | "airTemperature"
  | "trackTemperature"
  | "brakeBias"
  | "trackLimits"
  | "gameTimeClock"
  | "realTimeClock"
  | "positionChange"
  | "lap"
  | "average"
  | "last"
  | "best"
  | "energy"
  | "damage"
  | "trackLimitsColumn"
  | "signals"
  | "pitStops"
  | "tire";

export interface RelativeOptionDefinition {
  id: RelativeOptionId;
  labelKey: import("./i18n").TranslationKey;
}

export interface RelativeSettings {
  options: Record<RelativeOptionId, boolean>;
  columnOrder: RelativeColumnId[];
  aheadRows: number;
  behindRows: number;
  driverNameFormat: DriverNameFormat;
}

export const RELATIVE_SETTINGS_KEY = "blackrack-overlay.relative.v3";
export const RELATIVE_HEADER_OPTIONS: RelativeOptionDefinition[] = [
  { id: "airTemperature", labelKey: "header.airTemperature" }, { id: "trackTemperature", labelKey: "header.trackTemperature" },
  { id: "brakeBias", labelKey: "header.brakeBias" }, { id: "trackLimits", labelKey: "header.trackLimits" },
  { id: "gameTimeClock", labelKey: "header.gameTimeClock" }, { id: "realTimeClock", labelKey: "header.realTimeClock" }
];

export const RELATIVE_COLUMN_OPTIONS: RelativeOptionDefinition[] = [
  { id: "number", labelKey: "column.number" }, { id: "country", labelKey: "column.country" }, { id: "license", labelKey: "column.badge" },
  { id: "rating", labelKey: "column.ranks" }, { id: "positionChange", labelKey: "option.positionChange" }, { id: "lap", labelKey: "column.lap" },
  { id: "best", labelKey: "column.best" }, { id: "last", labelKey: "column.last" }, { id: "average", labelKey: "column.average" },
  { id: "energy", labelKey: "column.energy" }, { id: "damage", labelKey: "column.damage" }, { id: "trackLimitsColumn", labelKey: "column.trackLimits" },
  { id: "pitStops", labelKey: "column.pitStops" }, { id: "tire", labelKey: "column.tire" }, { id: "signals", labelKey: "column.signals" }
];

export const RELATIVE_OPTIONS: RelativeOptionDefinition[] = [
  { id: "tableHeader", labelKey: "settings.showHeader" },
  ...RELATIVE_HEADER_OPTIONS,
  ...RELATIVE_COLUMN_OPTIONS
];

const relativeColumn = (
  id: RelativeColumnId,
  header: string,
  width: number,
  identity: boolean,
  option?: RelativeOptionId
): RelativeColumnDefinition => ({
  id,
  labelKey: RELATIVE_COLUMN_LABELS[id],
  header,
  width,
  configurable: option !== undefined,
  identity,
  option
});

const RELATIVE_COLUMN_LABELS: Record<RelativeColumnId, import("./i18n").TranslationKey> = {
  position: "column.position", number: "column.number", country: "column.country", badge: "column.badge", driver: "column.driver", ranks: "column.ranks",
  relative: "column.relative", lap: "column.lap", best: "column.best", last: "column.last", average: "column.average", energy: "column.energy",
  damage: "column.damage", trackLimits: "column.trackLimits", pitStops: "column.pitStops", tire: "column.tire", signals: "column.signals"
};

export const RELATIVE_COLUMNS: RelativeColumnDefinition[] = [
  relativeColumn("position", "POS", 34, true),
  relativeColumn("number", "#", 26, true, "number"),
  relativeColumn("country", "", 23, true, "country"),
  relativeColumn("badge", "LIC", 29, false, "license"),
  relativeColumn("driver", "PILOTO", 95, true),
  relativeColumn("ranks", "ELO", 98, false, "rating"),
  relativeColumn("relative", "REL", 52, false),
  relativeColumn("lap", "V", 28, false, "lap"),
  relativeColumn("best", "BEST", 68, false, "best"),
  relativeColumn("last", "LAST", 68, false, "last"),
  relativeColumn("average", "AVG", 68, false, "average"),
  relativeColumn("energy", "NRG", 90, false, "energy"),
  relativeColumn("damage", "DMG", 42, false, "damage"),
  relativeColumn("trackLimits", "TL", 30, false, "trackLimitsColumn"),
  relativeColumn("pitStops", "PIT", 42, false, "pitStops"),
  relativeColumn("tire", "NEU", 30, false, "tire"),
  relativeColumn("signals", "", 104, false, "signals")
];

export const defaultRelativeSettings = (): RelativeSettings => {
  const compactOptions = new Set<RelativeOptionId>(["number", "signals"]);
  return {
    options: Object.fromEntries(
      RELATIVE_OPTIONS.map(({ id }) => [id, compactOptions.has(id)])
    ) as Record<RelativeOptionId, boolean>,
    columnOrder: RELATIVE_COLUMNS.map(({ id }) => id),
    aheadRows: 4,
    behindRows: 4,
    driverNameFormat: DEFAULT_DRIVER_NAME_FORMAT
  };
};

const integerInRange = (value: unknown, fallback: number): number =>
  Number.isFinite(value) ? Math.max(1, Math.min(Math.round(Number(value)), 10)) : fallback;

export const readRelativeSettings = (): RelativeSettings => {
  const settings = defaultRelativeSettings();
  try {
    const stored = JSON.parse(localStorage.getItem(RELATIVE_SETTINGS_KEY) ?? "{}") as {
      options?: Partial<Record<RelativeOptionId, boolean>>;
      columnOrder?: unknown[];
      aheadRows?: number;
      behindRows?: number;
      driverNameFormat?: unknown;
    };
    for (const option of RELATIVE_OPTIONS) {
      if (typeof stored.options?.[option.id] === "boolean") {
        settings.options[option.id] = stored.options[option.id] as boolean;
      }
    }
    if (Array.isArray(stored.columnOrder)) {
      const validIds = new Set(RELATIVE_COLUMNS.map(({ id }) => id));
      const storedOrder = stored.columnOrder.filter(
        (id): id is RelativeColumnId => typeof id === "string" && validIds.has(id as RelativeColumnId)
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
    settings.aheadRows = integerInRange(stored.aheadRows, settings.aheadRows);
    settings.behindRows = integerInRange(stored.behindRows, settings.behindRows);
    if (isDriverNameFormat(stored.driverNameFormat)) {
      settings.driverNameFormat = stored.driverNameFormat;
    }
  } catch {
    localStorage.removeItem(RELATIVE_SETTINGS_KEY);
  }
  return settings;
};

export const visibleRelativeColumns = (settings: RelativeSettings): RelativeColumnDefinition[] =>
  [...settings.columnOrder.filter((id) => id !== "signals"), "signals" as const]
    .map((id) => RELATIVE_COLUMNS.find((column) => column.id === id))
    .filter((column): column is RelativeColumnDefinition =>
      column !== undefined && (column.option === undefined || settings.options[column.option])
    );
