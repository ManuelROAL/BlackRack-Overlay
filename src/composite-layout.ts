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
  x: number;
  y: number;
  width: number;
  height: number;
  scale?: number;
}

export type CompositeLayout = Record<OverlayId, OverlayPlacement>;

export const COMPOSITE_LAYOUT_KEY = "blackrack-overlay.composite-layout.v1";
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
  "delta", "timing", "stinthistory", "driving", "liftcoast", "tires", "tiretemps", "damage", "standings",
  "relative", "fuel", "pitstop", "flags", "rejoin", "trackmap",
  "forecast", "conditions"
];

let layoutPromise: Promise<CompositeLayout> | null = null;
let displaysPromise: Promise<OverlayDisplay[]> | null = null;

const validPlacement = (value: unknown, overlay: OverlayId): value is OverlayPlacement => {
  if (!value || typeof value !== "object") return false;
  const placement = value as Partial<OverlayPlacement>;
  return placement.overlay === overlay
    && [placement.x, placement.y, placement.width, placement.height]
      .every((number) => typeof number === "number" && Number.isFinite(number))
    && (placement.scale === undefined
      || (typeof placement.scale === "number" && Number.isFinite(placement.scale) && placement.scale > 0));
};

const correctedDefaultSizes: Partial<Record<OverlayId, readonly [number, number, number, number]>> = {
  driving: [540, 120, 468, 120],
  standings: [980, 500, 970, 500],
  relative: [980, 300, 344, 255],
  forecast: [352, 118, 366, 112],
  conditions: [480, 96, 390, 90]
};

const migrateCompactPanels = (placement: OverlayPlacement): OverlayPlacement => {
  const correctedDefault = correctedDefaultSizes[placement.overlay];
  if (correctedDefault
    && placement.width === correctedDefault[0]
    && placement.height === correctedDefault[1]) {
    return { ...placement, width: correctedDefault[2], height: correctedDefault[3] };
  }
  if (placement.overlay === "timing" && placement.width === 366) {
    const migratedHeight = placement.height === 150
      ? 130
      : placement.height === 236
        ? 200
        : (placement.height === 202 || placement.height === 210)
          ? 172
          : null;
    if (migratedHeight !== null) return { ...placement, width: 318, height: migratedHeight };
  }
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
    return { ...placement, width: 292, height: 198 };
  }
  if (placement.overlay === "fuel" && placement.width === 252 && placement.height === 188) {
    return { ...placement, width: 292, height: 198 };
  }
  if (placement.overlay === "fuel" && placement.width === 252 && placement.height === 216) {
    return { ...placement, width: 292, height: 198 };
  }
  if (placement.overlay === "fuel" && placement.width === 292 && placement.height === 216) {
    return { ...placement, height: 198 };
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

const LEGACY_MONITOR_SELECTION_KEY = "blackrack-overlay.monitor-selection.v1";

interface LegacyMonitorSelection {
  mode?: "global" | "individual";
  globalMonitor?: number;
  individualMonitors?: Partial<Record<OverlayId, number>>;
}

const readLegacyMonitorSelection = (): number | null => {
  try {
    const stored = JSON.parse(localStorage.getItem(LEGACY_MONITOR_SELECTION_KEY) ?? "null") as
      LegacyMonitorSelection | null;
    return stored && Number.isInteger(stored.globalMonitor) && Number(stored.globalMonitor) >= 0
      ? Number(stored.globalMonitor)
      : null;
  } catch {
    return null;
  }
};

let monitorPromise: Promise<number> | null = null;

export const resolveOverlayMonitor = (): Promise<number> => {
  monitorPromise ??= (async () => {
    const displays = await getOverlayDisplays();
    const available = new Set(displays.map(({ index }) => index));
    const primary = displays[0]?.index ?? 0;
    const legacy = readLegacyMonitorSelection();
    if (legacy !== null) {
      localStorage.removeItem(LEGACY_MONITOR_SELECTION_KEY);
      if (available.has(legacy)) {
        await invoke<number>("set_overlay_monitor", { index: legacy }).catch(() => undefined);
        return legacy;
      }
    }
    const current = await invoke<number>("get_overlay_monitor").catch(() => primary);
    return available.has(current) ? current : primary;
  })();
  return monitorPromise;
};

export const setOverlayMonitor = (index: number): Promise<number> => {
  monitorPromise = null;
  return invoke<number>("set_overlay_monitor", { index });
};

export const ensureCompositeLayout = async (): Promise<CompositeLayout> => {
  layoutPromise ??= (async () => {
    const monitor = await resolveOverlayMonitor();
    const stored = readStoredLayout();
    const seed = await invoke<OverlayPlacement[]>("get_composite_layout_seed", { monitor });
    const layout = {
      ...Object.fromEntries(seed.map((placement) => [placement.overlay, placement])),
      ...stored
    } as CompositeLayout;
    localStorage.setItem(COMPOSITE_LAYOUT_KEY, JSON.stringify(layout));
    return layout;
  })();
  return layoutPromise;
};

export const getDefaultCompositeLayout = async (): Promise<CompositeLayout> => {
  const monitor = await resolveOverlayMonitor();
  const seed = await invoke<OverlayPlacement[]>("get_composite_layout_seed", { monitor });
  return Object.fromEntries(seed.map((placement) => [placement.overlay, placement])) as CompositeLayout;
};

export const readCompositeLayout = (): CompositeLayout | null => {
  const stored = readStoredLayout();
  return stored && overlayIds.every((overlay) => stored[overlay])
    ? stored as CompositeLayout
    : null;
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
  const placement = await invoke<OverlayPlacement>("get_default_overlay_placement", {
    label: overlay
  });
  await saveOverlayPlacement(placement);
  return placement;
};
