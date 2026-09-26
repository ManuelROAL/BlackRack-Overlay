export type DrivingPedalId = "throttle" | "brake" | "clutch";

export interface DrivingSettings {
  graphPosition: "left" | "right";
  graphPedals: Record<DrivingPedalId, boolean>;
  inputPedals: Record<DrivingPedalId, boolean>;
  showGraph: boolean;
  showSteering: boolean;
  showForceFeedback: boolean;
  showSpeed: boolean;
  showGear: boolean;
  showRpmLeds: boolean;
}

export const DRIVING_SETTINGS_KEY = "blackrack-overlay.driving.v1";

export const DRIVING_PEDALS = [
  { id: "throttle", labelKey: "pedal.throttle" }, { id: "brake", labelKey: "pedal.brake" }, { id: "clutch", labelKey: "pedal.clutch" }
] as const;

export const defaultDrivingSettings = (): DrivingSettings => ({
  graphPosition: "right",
  graphPedals: { throttle: true, brake: true, clutch: true },
  inputPedals: { throttle: true, brake: true, clutch: true },
  showGraph: true,
  showSteering: true,
  showForceFeedback: true,
  showSpeed: true,
  showGear: true,
  showRpmLeds: true
});

export const readDrivingSettings = (): DrivingSettings => {
  const settings = defaultDrivingSettings();
  try {
    const stored = JSON.parse(localStorage.getItem(DRIVING_SETTINGS_KEY) ?? "{}") as {
      graphPedals?: Partial<Record<DrivingPedalId, boolean>>;
      inputPedals?: Partial<Record<DrivingPedalId, boolean>>;
      showGraph?: boolean;
      showSteering?: boolean;
      showForceFeedback?: boolean;
      showSpeed?: boolean;
      showGear?: boolean;
      showRpmLeds?: boolean;
      graphPosition?: unknown;
    };
    for (const { id } of DRIVING_PEDALS) {
      if (typeof stored.graphPedals?.[id] === "boolean") {
        settings.graphPedals[id] = stored.graphPedals[id];
      }
      if (typeof stored.inputPedals?.[id] === "boolean") {
        settings.inputPedals[id] = stored.inputPedals[id];
      }
    }
    if (typeof stored.showGraph === "boolean") settings.showGraph = stored.showGraph;
    if (typeof stored.showSteering === "boolean") settings.showSteering = stored.showSteering;
    if (typeof stored.showForceFeedback === "boolean") {
      settings.showForceFeedback = stored.showForceFeedback;
    }
    if (typeof stored.showSpeed === "boolean") settings.showSpeed = stored.showSpeed;
    if (typeof stored.showGear === "boolean") settings.showGear = stored.showGear;
    if (typeof stored.showRpmLeds === "boolean") settings.showRpmLeds = stored.showRpmLeds;
    if (stored.graphPosition === "left" || stored.graphPosition === "right") {
      settings.graphPosition = stored.graphPosition;
    }
  } catch {
    localStorage.removeItem(DRIVING_SETTINGS_KEY);
  }
  return settings;
};
