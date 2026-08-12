import { invokeRuntime, isTauriRuntime, listenRuntimeEvent } from "./runtime-events";

interface LoggingState {
  enabled: boolean;
}

export const createOverlayPerformanceTracker = (overlay: string) => {
  let enabled = false;
  let periodStartedAt = performance.now();
  let renderCount = 0;
  let totalRenderMs = 0;
  let maxRenderMs = 0;
  let maxItems = 0;

  const reset = (): void => {
    periodStartedAt = performance.now();
    renderCount = 0;
    totalRenderMs = 0;
    maxRenderMs = 0;
    maxItems = 0;
  };

  const setEnabled = (next: boolean): void => {
    if (enabled !== next) reset();
    enabled = next;
  };

  if (isTauriRuntime()) {
    void listenRuntimeEvent<LoggingState>("performance://logging", ({ enabled }) => setEnabled(enabled));
    void invokeRuntime<LoggingState>("get_telemetry_logging")
      .then(({ enabled: active }) => setEnabled(active))
      .catch(() => undefined);
  }

  window.setInterval(() => {
    const now = performance.now();
    if (isTauriRuntime() && enabled && renderCount > 0) {
      void invokeRuntime("record_frontend_performance", {
        sample: {
          overlay,
          period_ms: Math.round(now - periodStartedAt),
          render_count: renderCount,
          average_render_ms: totalRenderMs / renderCount,
          max_render_ms: maxRenderMs,
          max_items: maxItems
        }
      }).catch(() => undefined);
    }
    reset();
  }, 5_000);

  return {
    measure(render: () => void, items = 1): void {
      if (!enabled) {
        render();
        return;
      }
      const startedAt = performance.now();
      render();
      const elapsed = performance.now() - startedAt;
      renderCount += 1;
      totalRenderMs += elapsed;
      maxRenderMs = Math.max(maxRenderMs, elapsed);
      maxItems = Math.max(maxItems, items);
    }
  };
};
