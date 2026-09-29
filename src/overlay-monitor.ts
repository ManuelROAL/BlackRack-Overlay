export type OverlayMonitorScopeMode = "global" | "individual";

export interface OverlayMonitorScope {
  mode: OverlayMonitorScopeMode;
  globalMonitor: number;
  /**
   * The monitor the user chose while it is not connected. `globalMonitor` then
   * holds the fallback that renders the overlays, and the choice comes back as
   * soon as the monitor does, e.g. a DisplayPort screen that wakes up after
   * the app has started.
   */
  displacedMonitor?: number;
}

export const OVERLAY_MONITOR_SCOPE_KEY = "blackrack-overlay.monitor-scope.v1";

export const defaultOverlayMonitorScope = (): OverlayMonitorScope => ({
  mode: "individual",
  globalMonitor: 0
});

const isMonitorIndex = (value: unknown): value is number =>
  Number.isInteger(value) && Number(value) >= 0;

export const normalizeOverlayMonitorScope = (
  value: unknown,
  fallback = defaultOverlayMonitorScope()
): OverlayMonitorScope => {
  if (!value || typeof value !== "object") return { ...fallback };
  const source = value as Partial<OverlayMonitorScope>;
  return {
    mode: source.mode === "global" ? "global" : "individual",
    globalMonitor: isMonitorIndex(source.globalMonitor)
      ? Number(source.globalMonitor)
      : fallback.globalMonitor,
    ...(isMonitorIndex(source.displacedMonitor) ? { displacedMonitor: Number(source.displacedMonitor) } : {})
  };
};

/**
 * Point the scope at a connected monitor without forgetting the user's choice:
 * a missing monitor falls back for rendering and is restored once it returns.
 */
export const reconcileOverlayMonitorScope = (
  scope: OverlayMonitorScope,
  available: ReadonlySet<number>,
  fallback: number
): OverlayMonitorScope => {
  const preferred = scope.displacedMonitor ?? scope.globalMonitor;
  return available.has(preferred)
    ? { mode: scope.mode, globalMonitor: preferred }
    : { mode: scope.mode, globalMonitor: fallback, displacedMonitor: preferred };
};

export const sameOverlayMonitorScope = (left: OverlayMonitorScope, right: OverlayMonitorScope): boolean =>
  left.mode === right.mode
  && left.globalMonitor === right.globalMonitor
  && left.displacedMonitor === right.displacedMonitor;

export const readOverlayMonitorScope = (): OverlayMonitorScope => {
  try {
    const raw = localStorage.getItem(OVERLAY_MONITOR_SCOPE_KEY);
    return normalizeOverlayMonitorScope(raw ? JSON.parse(raw) : null);
  } catch {
    return defaultOverlayMonitorScope();
  }
};

export const saveOverlayMonitorScope = (scope: OverlayMonitorScope): void => {
  localStorage.setItem(OVERLAY_MONITOR_SCOPE_KEY, JSON.stringify(normalizeOverlayMonitorScope(scope)));
};

export const effectiveOverlayMonitor = (
  storedMonitor: number | undefined,
  scope: OverlayMonitorScope
): number => scope.mode === "global" ? scope.globalMonitor : storedMonitor ?? scope.globalMonitor;
