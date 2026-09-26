export type DrivingPedalId = "throttle" | "brake" | "clutch";
export type DrivingPanelId = "graph" | "inputs" | "data";
export type DrivingPanelPosition = "left" | "center" | "right";

export interface DrivingSettings {
  graphPosition: DrivingPanelPosition;
  inputPosition: DrivingPanelPosition;
  dataPosition: DrivingPanelPosition;
  graphPedals: Record<DrivingPedalId, boolean>;
  inputPedals: Record<DrivingPedalId, boolean>;
  showGraph: boolean;
  showPedalLabels: boolean;
  showSteering: boolean;
  showForceFeedback: boolean;
  showSpeed: boolean;
  showGear: boolean;
  showRpmLeds: boolean;
}

export const DRIVING_SETTINGS_KEY = "blackrack-overlay.driving.v1";
const PANEL_POSITION_KEYS: Record<DrivingPanelId, keyof Pick<DrivingSettings, "graphPosition" | "inputPosition" | "dataPosition">> = {
  graph: "graphPosition", inputs: "inputPosition", data: "dataPosition"
};
const PANEL_POSITIONS: readonly DrivingPanelPosition[] = ["left", "center", "right"];

export const normalizeDrivingPanelPositions = (
  values: Partial<Pick<DrivingSettings, "graphPosition" | "inputPosition" | "dataPosition">>
): Pick<DrivingSettings, "graphPosition" | "inputPosition" | "dataPosition"> => {
  const defaults = { graphPosition: "right", inputPosition: "center", dataPosition: "left" } as const;
  if (values.graphPosition === "left" && values.inputPosition === undefined && values.dataPosition === undefined) {
    // Before panel positions were configurable, the graph could move left of
    // the dial and inputs. Preserve that complete historical order.
    return { graphPosition: "left", inputPosition: "right", dataPosition: "center" };
  }
  const result = { ...defaults } as Record<typeof PANEL_POSITION_KEYS[DrivingPanelId], DrivingPanelPosition>;
  const used = new Set<DrivingPanelPosition>();
  // Preserve each valid requested location, assigning conflicts to the next free default.
  for (const panel of ["graph", "inputs", "data"] as const) {
    const key = PANEL_POSITION_KEYS[panel];
    const requested = values[key];
    const preferred = PANEL_POSITIONS.includes(requested as DrivingPanelPosition)
      ? requested as DrivingPanelPosition : defaults[key];
    const position = used.has(preferred)
      ? PANEL_POSITIONS.find((candidate) => !used.has(candidate))!
      : preferred;
    result[key] = position;
    used.add(position);
  }
  return result;
};

export const assignDrivingPanelPosition = (
  settings: DrivingSettings, panel: DrivingPanelId, position: DrivingPanelPosition
): DrivingSettings => {
  if (!PANEL_POSITIONS.includes(position)) return settings;
  const positions = normalizeDrivingPanelPositions(settings);
  const key = PANEL_POSITION_KEYS[panel];
  const other = (Object.keys(PANEL_POSITION_KEYS) as DrivingPanelId[])
    .find((candidate) => candidate !== panel && positions[PANEL_POSITION_KEYS[candidate]] === position);
  if (other) positions[PANEL_POSITION_KEYS[other]] = positions[key];
  positions[key] = position;
  return { ...settings, ...positions };
};

export const normalizeDrivingSettings = (value?: Partial<DrivingSettings> | null): DrivingSettings => {
  const settings = defaultDrivingSettings();
  for (const { id } of DRIVING_PEDALS) {
    if (typeof value?.graphPedals?.[id] === "boolean") settings.graphPedals[id] = value.graphPedals[id];
    if (typeof value?.inputPedals?.[id] === "boolean") settings.inputPedals[id] = value.inputPedals[id];
  }
  for (const key of ["showGraph", "showPedalLabels", "showSteering", "showForceFeedback", "showSpeed", "showGear", "showRpmLeds"] as const) {
    if (typeof value?.[key] === "boolean") settings[key] = value[key];
  }
  return { ...settings, ...normalizeDrivingPanelPositions(value ?? {}) };
};

export const DRIVING_PEDALS = [
  { id: "throttle", labelKey: "pedal.throttle" }, { id: "brake", labelKey: "pedal.brake" }, { id: "clutch", labelKey: "pedal.clutch" }
] as const;

export const defaultDrivingSettings = (): DrivingSettings => ({
  graphPosition: "right",
  inputPosition: "center",
  dataPosition: "left",
  graphPedals: { throttle: true, brake: true, clutch: true },
  inputPedals: { throttle: true, brake: true, clutch: true },
  showGraph: true,
  showPedalLabels: true,
  showSteering: true,
  showForceFeedback: true,
  showSpeed: true,
  showGear: true,
  showRpmLeds: true
});

export const readDrivingSettings = (): DrivingSettings => {
  try {
    return normalizeDrivingSettings(JSON.parse(localStorage.getItem(DRIVING_SETTINGS_KEY) ?? "{}"));
  } catch {
    localStorage.removeItem(DRIVING_SETTINGS_KEY);
  }
  return defaultDrivingSettings();
};
