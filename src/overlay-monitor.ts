export type OverlayMonitorScopeMode = "global" | "individual";

export interface OverlayMonitorScope {
  mode: OverlayMonitorScopeMode;
  globalMonitor: number;
}

export const OVERLAY_MONITOR_SCOPE_KEY = "blackrack-overlay.monitor-scope.v1";

export const defaultOverlayMonitorScope = (): OverlayMonitorScope => ({
  mode: "individual",
  globalMonitor: 0
});

export const normalizeOverlayMonitorScope = (
  value: unknown,
  fallback = defaultOverlayMonitorScope()
): OverlayMonitorScope => {
  if (!value || typeof value !== "object") return { ...fallback };
  const source = value as Partial<OverlayMonitorScope>;
  return {
    mode: source.mode === "global" ? "global" : "individual",
    globalMonitor: Number.isInteger(source.globalMonitor) && Number(source.globalMonitor) >= 0
      ? Number(source.globalMonitor)
      : fallback.globalMonitor
  };
};

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
