import { invoke } from "@tauri-apps/api/core";
import { emit } from "@tauri-apps/api/event";
import type { OverlayId } from "./overlay-appearance";
import {
  effectiveOverlayMonitor,
  normalizeOverlayMonitorScope,
  readOverlayMonitorScope,
  saveOverlayMonitorScope,
  type OverlayMonitorScope
} from "./overlay-monitor";

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
  /** Monitor index in the current Tauri monitor ordering. */
  monitor?: number;
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
  "delta", "timing", "stinthistory", "driving", "liftcoast", "tires", "damage", "standings",
  "relative", "fuel", "pitstop", "flags", "rejoin", "trackmap",
  "forecast", "conditions", "dashboard", "sessioninfo", "chat"
];

let layoutPromise: Promise<CompositeLayout> | null = null;
let displaysPromise: Promise<OverlayDisplay[]> | null = null;
let monitorPromise: Promise<number> | null = null;

const validPlacement = (value: unknown, overlay: OverlayId): value is OverlayPlacement => {
  if (!value || typeof value !== "object") return false;
  const placement = value as Partial<OverlayPlacement>;
  return placement.overlay === overlay
    && [placement.x, placement.y, placement.width, placement.height]
      .every((number) => typeof number === "number" && Number.isFinite(number))
    && (placement.monitor === undefined
      || (typeof placement.monitor === "number"
        && Number.isInteger(placement.monitor) && placement.monitor >= 0))
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
      || (placement.width === 174 && placement.height === 148)
      || (placement.width === 174 && placement.height === 130))) {
    return { ...placement, width: 194, height: 130 };
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

const loadOverlayDisplays = (): Promise<OverlayDisplay[]> => {
  const request = invoke<OverlayDisplay[]>("get_overlay_displays");
  displaysPromise = request;
  void request.catch(() => {
    if (displaysPromise === request) displaysPromise = null;
  });
  return request;
};

export const getOverlayDisplays = (): Promise<OverlayDisplay[]> =>
  displaysPromise ?? loadOverlayDisplays();

/** Refresh the native monitor inventory after a display topology change. */
export const refreshOverlayDisplays = (): Promise<OverlayDisplay[]> => {
  monitorPromise = null;
  return loadOverlayDisplays();
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

const resetOverlayMonitorResolution = (): void => {
  monitorPromise = null;
};

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

export const setOverlayMonitor = async (index: number): Promise<number> => {
  const result = await setOverlayMonitorPreference(index);
  saveOverlayMonitorScope({ mode: "global", globalMonitor: result });
  await emit("overlay://layout", { monitor: result });
  await synchronizeOverlayHosts();
  return result;
};

export const setOverlayMonitorScope = async (scope: OverlayMonitorScope): Promise<OverlayMonitorScope> => {
  const displays = await getOverlayDisplays();
  const fallback = displays[0]?.index ?? 0;
  const normalized = normalizeOverlayMonitorScope(scope, { mode: scope.mode, globalMonitor: fallback });
  normalized.globalMonitor = displays.some(({ index }) => index === normalized.globalMonitor)
    ? normalized.globalMonitor
    : fallback;
  saveOverlayMonitorScope(normalized);
  await setOverlayMonitorPreference(normalized.globalMonitor);
  await emit("overlay://layout", normalized);
  await synchronizeOverlayHosts();
  return normalized;
};

export const getEffectiveCompositeLayout = (layout: CompositeLayout): CompositeLayout => {
  const scope = readOverlayMonitorScope();
  return Object.fromEntries(overlayIds.map((overlay) => {
    const placement = layout[overlay];
    return [overlay, placement
      ? { ...placement, monitor: effectiveOverlayMonitor(placement.monitor, scope) }
      : placement];
  })) as CompositeLayout;
};

/** Persist the general monitor without changing per-overlay assignments. */
export const setOverlayMonitorPreference = async (index: number): Promise<number> => {
  resetOverlayMonitorResolution();
  return invoke<number>("set_overlay_monitor", { index });
};

export const ensureCompositeLayout = async (): Promise<CompositeLayout> => {
  layoutPromise ??= (async () => {
    const monitor = await resolveOverlayMonitor();
    const displays = await getOverlayDisplays();
    const available = new Set(displays.map(({ index }) => index));
    const fallback = available.has(monitor) ? monitor : displays[0]?.index ?? 0;
    const stored = readStoredLayout();
    const seed = await invoke<OverlayPlacement[]>("get_composite_layout_seed", { monitor: fallback });
    const layout = {
      ...Object.fromEntries(seed.map((placement) => [placement.overlay, placement])),
      ...stored
    } as CompositeLayout;
    for (const overlay of overlayIds) {
      const placement = layout[overlay];
      if (!placement) continue;
      const assigned = placement.monitor;
      placement.monitor = Number.isInteger(assigned) && assigned !== undefined && available.has(assigned)
        ? assigned
        : fallback;
    }
    localStorage.setItem(COMPOSITE_LAYOUT_KEY, JSON.stringify(layout));
    return layout;
  })();
  return layoutPromise;
};

export const getDefaultCompositeLayout = async (): Promise<CompositeLayout> => {
  const monitor = await resolveOverlayMonitor();
  const seed = await invoke<OverlayPlacement[]>("get_composite_layout_seed", { monitor });
  return Object.fromEntries(seed.map((placement) => [placement.overlay, {
    ...placement,
    monitor
  }])) as CompositeLayout;
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
  const scope = readOverlayMonitorScope();
  const storedPlacement = currentLayout[placement.overlay] ?? initializedLayout[placement.overlay];
  const next = {
    ...placement,
    monitor: scope.mode === "global"
      ? storedPlacement?.monitor ?? await resolveOverlayMonitor()
      : placement.monitor ?? initializedLayout[placement.overlay]?.monitor
      ?? await resolveOverlayMonitor()
  } satisfies OverlayPlacement;
  currentLayout[placement.overlay] = next;
  initializedLayout[placement.overlay] = next;
  localStorage.setItem(COMPOSITE_LAYOUT_KEY, JSON.stringify(currentLayout));
  await emit("overlay://layout", next);
};

interface OverlayVisibilityState {
  label: OverlayId;
  visible: boolean;
}

let overlayHostSyncQueue: Promise<void> = Promise.resolve();

/**
 * Keep native overlay hosts proportional to the monitors that actually have
 * visible panels. The control panel is the only caller: composite hosts only
 * render the layout they receive and must not be able to create windows.
 */
export const synchronizeOverlayHosts = async (providedLayout?: CompositeLayout): Promise<void> => {
  const sync = overlayHostSyncQueue.then(async () => {
    // Read the state after earlier reconciliations have completed. This makes a
    // rapid sequence of visibility/layout edits converge on the latest storage
    // instead of allowing an older queued snapshot to close a needed host.
    const initialLayout = readCompositeLayout() ?? providedLayout ?? await ensureCompositeLayout();
    const states = await invoke<OverlayVisibilityState[]>("get_overlay_states");
    resetOverlayMonitorResolution();
    const [displays, fallbackMonitor] = await Promise.all([
      refreshOverlayDisplays(),
      resolveOverlayMonitor()
    ]);
    // A placement can be edited while the native queries above are pending.
    // Re-read storage before normalizing so a topology refresh cannot overwrite
    // a newer position or monitor assignment.
    const layout = readCompositeLayout() ?? initialLayout;
    const available = new Set(displays.map(({ index }) => index));
    const fallback = available.has(fallbackMonitor) ? fallbackMonitor : displays[0]?.index ?? 0;
    const monitorScope = readOverlayMonitorScope();
    if (!available.has(monitorScope.globalMonitor)) {
      saveOverlayMonitorScope({ ...monitorScope, globalMonitor: fallback });
    }
    let normalized = false;
    if (monitorScope.mode === "individual") {
      for (const overlay of overlayIds) {
        const placement = layout[overlay];
        if (!placement) continue;
        const assigned = placement.monitor;
        if (!Number.isInteger(assigned) || assigned === undefined || !available.has(assigned)) {
          placement.monitor = fallback;
          normalized = true;
        }
      }
    }
    if (normalized) {
      localStorage.setItem(COMPOSITE_LAYOUT_KEY, JSON.stringify(layout));
      layoutPromise = Promise.resolve(layout);
    }
    const effectiveLayout = getEffectiveCompositeLayout(layout);
    const monitors = new Set<number>();
    for (const state of states) {
      if (!state.visible) continue;
      const monitor = effectiveLayout[state.label]?.monitor;
      monitors.add(Number.isInteger(monitor) && monitor !== undefined ? monitor : fallback);
    }
    await invoke("sync_overlay_hosts", { monitors: [...monitors] });
  });
  overlayHostSyncQueue = sync.catch(() => undefined);
  await sync;
};

export const setOverlayPlacementMonitor = async (
  overlay: OverlayId,
  monitor: number
): Promise<OverlayPlacement> => {
  if (readOverlayMonitorScope().mode === "global") {
    throw new Error("monitor_scope_global");
  }
  const displays = await getOverlayDisplays();
  if (!displays.some(({ index }) => index === monitor)) throw new Error("monitor_unavailable");
  const layout = readCompositeLayout() ?? await ensureCompositeLayout();
  const current = layout[overlay];
  if (!current) throw new Error(`Missing placement for ${overlay}`);
  const next = { ...current, monitor } satisfies OverlayPlacement;
  layout[overlay] = next;
  localStorage.setItem(COMPOSITE_LAYOUT_KEY, JSON.stringify(layout));
  layoutPromise = Promise.resolve(layout);
  await emit("overlay://layout", next);
  await synchronizeOverlayHosts(layout);
  return next;
};

export const resetOverlayPlacement = async (overlay: OverlayId): Promise<OverlayPlacement> => {
  const layout = readCompositeLayout() ?? await ensureCompositeLayout();
  const storedMonitor = layout[overlay]?.monitor;
  const monitor = effectiveOverlayMonitor(storedMonitor, readOverlayMonitorScope());
  const placement = await invoke<OverlayPlacement>("get_default_overlay_placement", {
    label: overlay,
    monitor
  });
  placement.monitor = storedMonitor ?? monitor;
  await saveOverlayPlacement(placement);
  return placement;
};
