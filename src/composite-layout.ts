import { invoke } from "@tauri-apps/api/core";
import { emit } from "@tauri-apps/api/event";
import type { OverlayId } from "./overlay-appearance";

export interface OverlayDisplay {
  index: number;
  label: string;
  name: string;
  x: number;
  y: number;
  width: number;
  height: number;
  scaleFactor: number;
}

export interface OverlayPlacement {
  overlay: OverlayId;
  monitor: number;
  x: number;
  y: number;
  width: number;
  height: number;
}

export type CompositeLayout = Record<OverlayId, OverlayPlacement>;

export interface MonitorSelectionSettings {
  mode: "global" | "individual";
  globalMonitor: number;
  individualMonitors: Partial<Record<OverlayId, number>>;
}

export const COMPOSITE_LAYOUT_KEY = "lmu-overlay.composite-layout.v1";
export const MONITOR_SELECTION_KEY = "lmu-overlay.monitor-selection.v1";
export const MIN_VISIBLE_PANEL_EDGE = 32;

export const clampPanelCoordinate = (
  coordinate: number,
  panelSize: number,
  monitorSize: number
): number => Math.max(
  MIN_VISIBLE_PANEL_EDGE - panelSize,
  Math.min(coordinate, monitorSize - MIN_VISIBLE_PANEL_EDGE)
);

const overlayIds: OverlayId[] = [
  "dashboard", "delta", "timing", "driving", "tires", "damage", "standings",
  "relative", "fuel", "pitstop", "flags", "rejoin", "trackmap"
];

let layoutPromise: Promise<CompositeLayout> | null = null;
let displaysPromise: Promise<OverlayDisplay[]> | null = null;

const validPlacement = (value: unknown, overlay: OverlayId): value is OverlayPlacement => {
  if (!value || typeof value !== "object") return false;
  const placement = value as Partial<OverlayPlacement>;
  return placement.overlay === overlay
    && Number.isInteger(placement.monitor)
    && [placement.x, placement.y, placement.width, placement.height]
      .every((number) => typeof number === "number" && Number.isFinite(number));
};

const migrateCompactPanels = (placement: OverlayPlacement): OverlayPlacement => {
  if (placement.overlay === "tires"
    && ((placement.width === 260 && placement.height === 260)
      || (placement.width === 190 && placement.height === 180)
      || (placement.width === 236 && placement.height === 188)
      || (placement.width === 184 && placement.height === 148)
      || (placement.width === 174 && placement.height === 148))) {
    return { ...placement, width: 174, height: 130 };
  }
  if (placement.overlay === "damage"
    && (placement.width === 180 || placement.width === 160 || placement.width === 130 || placement.width === 108)
    && (placement.height === 112 || placement.height === 94)) {
    return { ...placement, width: 94, height: 94 };
  }
  if (placement.overlay === "fuel"
    && ((placement.width === 560 && placement.height === 230)
      || (placement.width === 470 && placement.height === 188)
      || (placement.width === 390 && placement.height === 188)
      || (placement.width === 356 && placement.height === 202)
      || (placement.width === 356 && placement.height === 188))) {
    return { ...placement, width: 252, height: 188 };
  }
  return placement;
};

const readStoredLayout = (): Partial<CompositeLayout> | null => {
  try {
    const parsed = JSON.parse(localStorage.getItem(COMPOSITE_LAYOUT_KEY) ?? "null") as
      Partial<Record<OverlayId, unknown>> | null;
    if (!parsed) return null;
    const entries = overlayIds
      .filter((overlay) => parsed[overlay] !== undefined)
      .map((overlay) => [overlay, parsed[overlay]] as const);
    if (!entries.every(([overlay, placement]) => validPlacement(placement, overlay))) return null;
    return Object.fromEntries(entries.map(([overlay, placement]) => [
      overlay,
      migrateCompactPanels(placement as OverlayPlacement)
    ])) as Partial<CompositeLayout>;
  } catch {
    return null;
  }
};

export const getOverlayDisplays = (): Promise<OverlayDisplay[]> => {
  displaysPromise ??= invoke<OverlayDisplay[]>("get_overlay_displays");
  return displaysPromise;
};

export const ensureCompositeLayout = async (): Promise<CompositeLayout> => {
  layoutPromise ??= (async () => {
    const stored = readStoredLayout();
    const seed = await invoke<OverlayPlacement[]>("get_composite_layout_seed");
    const layout = {
      ...Object.fromEntries(seed.map((placement) => [placement.overlay, placement])),
      ...stored
    } as CompositeLayout;
    localStorage.setItem(COMPOSITE_LAYOUT_KEY, JSON.stringify(layout));
    return layout;
  })();
  return layoutPromise;
};

export const readCompositeLayout = (): CompositeLayout | null => {
  const stored = readStoredLayout();
  return stored && overlayIds.every((overlay) => stored[overlay])
    ? stored as CompositeLayout
    : null;
};

export const readMonitorSelection = (): MonitorSelectionSettings => {
  const fallback: MonitorSelectionSettings = {
    mode: "individual",
    globalMonitor: 0,
    individualMonitors: {}
  };
  try {
    const stored = JSON.parse(localStorage.getItem(MONITOR_SELECTION_KEY) ?? "null") as
      Partial<MonitorSelectionSettings> | null;
    if (!stored) return fallback;
    const individualMonitors = Object.fromEntries(
      Object.entries(stored.individualMonitors ?? {}).filter((entry): entry is [string, number] =>
        Number.isInteger(entry[1]) && entry[1] >= 0
      )
    ) as Partial<Record<OverlayId, number>>;
    return {
      mode: stored.mode === "global" ? "global" : "individual",
      globalMonitor: Number.isInteger(stored.globalMonitor) && Number(stored.globalMonitor) >= 0
        ? Number(stored.globalMonitor)
        : 0,
      individualMonitors
    };
  } catch {
    localStorage.removeItem(MONITOR_SELECTION_KEY);
    return fallback;
  }
};

export const saveMonitorSelection = (settings: MonitorSelectionSettings): void => {
  localStorage.setItem(MONITOR_SELECTION_KEY, JSON.stringify(settings));
};

export const saveOverlayPlacement = async (placement: OverlayPlacement): Promise<void> => {
  const initializedLayout = await ensureCompositeLayout();
  const currentLayout = readCompositeLayout() ?? initializedLayout;
  currentLayout[placement.overlay] = placement;
  initializedLayout[placement.overlay] = placement;
  localStorage.setItem(COMPOSITE_LAYOUT_KEY, JSON.stringify(currentLayout));
  await emit("overlay://layout", placement);
};

export const resetOverlayPlacement = async (overlay: OverlayId): Promise<OverlayPlacement> => {
  const initializedLayout = await ensureCompositeLayout();
  const layout = readCompositeLayout() ?? initializedLayout;
  const defaults = await invoke<OverlayPlacement>("get_default_overlay_placement", { label: overlay });
  const placement = { ...defaults, monitor: layout[overlay].monitor };
  await saveOverlayPlacement(placement);
  return placement;
};

export const moveOverlayToMonitor = async (
  overlay: OverlayId,
  monitor: number
): Promise<void> => {
  const [initializedLayout, displays] = await Promise.all([
    ensureCompositeLayout(),
    getOverlayDisplays()
  ]);
  const display = displays.find((candidate) => candidate.index === monitor);
  if (!display) return;
  const layout = readCompositeLayout() ?? initializedLayout;
  const current = layout[overlay];
  const logicalWidth = display.width / Math.max(display.scaleFactor, 0.1);
  const logicalHeight = display.height / Math.max(display.scaleFactor, 0.1);
  await saveOverlayPlacement({
    ...current,
    monitor,
    x: clampPanelCoordinate(current.x, current.width, logicalWidth),
    y: clampPanelCoordinate(current.y, current.height, logicalHeight)
  });
};
