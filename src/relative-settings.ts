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
  label: string;
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
  label: string;
}

export interface RelativeSettings {
  options: Record<RelativeOptionId, boolean>;
  columnOrder: RelativeColumnId[];
  aheadRows: number;
  behindRows: number;
}

export const RELATIVE_SETTINGS_KEY = "lmu-overlay.relative.v3";
export const RELATIVE_HEADER_OPTIONS: RelativeOptionDefinition[] = [
  { id: "airTemperature", label: "Temperatura ambiente" },
  { id: "trackTemperature", label: "Temperatura de pista" },
  { id: "brakeBias", label: "Reparto de frenada" },
  { id: "trackLimits", label: "Cortes de circuito" },
  { id: "realTimeClock", label: "Hora real" }
];

export const RELATIVE_COLUMN_OPTIONS: RelativeOptionDefinition[] = [
  { id: "number", label: "Dorsal" },
  { id: "country", label: "Bandera del país" },
  { id: "license", label: "Insignia" },
  { id: "rating", label: "DR / SR" },
  { id: "positionChange", label: "Cambio de posición" },
  { id: "lap", label: "Número de vuelta" },
  { id: "best", label: "Mejor vuelta" },
  { id: "last", label: "Última vuelta" },
  { id: "average", label: "Media 5" },
  { id: "energy", label: "Energía" },
  { id: "damage", label: "Daño" },
  { id: "trackLimitsColumn", label: "Cortes de circuito" },
  { id: "pitStops", label: "Paradas / tiempo" },
  { id: "tire", label: "Neumático" },
  { id: "signals", label: "Banderas / estados" }
];

export const RELATIVE_OPTIONS: RelativeOptionDefinition[] = [
  { id: "tableHeader", label: "Mostrar cabecera" },
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
  label: RELATIVE_COLUMN_LABELS[id],
  header,
  width,
  configurable: option !== undefined,
  identity,
  option
});

const RELATIVE_COLUMN_LABELS: Record<RelativeColumnId, string> = {
  position: "Posición",
  number: "Dorsal",
  country: "Bandera del país",
  badge: "Insignia",
  driver: "Piloto",
  ranks: "DR / SR",
  relative: "Tiempo relativo",
  lap: "Número de vuelta",
  best: "Mejor vuelta",
  last: "Última vuelta",
  average: "Media 5",
  energy: "Energía",
  damage: "Daño",
  trackLimits: "Cortes de circuito",
  pitStops: "Paradas / tiempo",
  tire: "Neumático",
  signals: "Banderas / estados"
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
  relativeColumn("signals", "", 82, false, "signals")
];

export const defaultRelativeSettings = (): RelativeSettings => ({
  options: Object.fromEntries(RELATIVE_OPTIONS.map(({ id }) => [id, true])) as Record<RelativeOptionId, boolean>,
  columnOrder: RELATIVE_COLUMNS.map(({ id }) => id),
  aheadRows: 4,
  behindRows: 4
});

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
