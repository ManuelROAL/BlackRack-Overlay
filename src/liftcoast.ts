import "./styles.css";
import "./liftcoast.css";
import { fitOverlay } from "./overlay-fit";
import { bindOverlayInteractionMode } from "./overlay-interaction";
import { createOverlayPerformanceTracker } from "./overlay-performance";
import { bindOverlayTransparency } from "./overlay-appearance";
import type { TelemetryFrame } from "./telemetry-types";
import { listenTelemetry } from "./runtime-events";

const SEGMENT_COUNT = 5;
const card = document.getElementById("liftcoast-card")!;
const segments = Array.from(document.querySelectorAll<HTMLElement>("[data-segment]"));
const renderPerformance = createOverlayPerformanceTracker("liftcoast");

fitOverlay({ width: 190, height: 64 });
bindOverlayTransparency("liftcoast");

const visibleSegments = (rawProgress: number): number => {
  if (!Number.isFinite(rawProgress)) return 0;
  // The official SDK currently exposes only a uint8 progress value and no max.
  // Preserve its discrete 0..5 values; unknown higher values remain an active,
  // fully-lit cue instead of inventing a percentage conversion.
  return Math.max(0, Math.min(SEGMENT_COUNT, Math.trunc(rawProgress)));
};

const render = (frame: TelemetryFrame): void => {
  const lit = visibleSegments(frame.lift_and_coast_progress);
  const active = lit > 0;
  const activeValue = String(active);
  if (card.dataset.active !== activeValue) card.dataset.active = activeValue;
  for (let index = 0; index < segments.length; index += 1) {
    const next = String(index < lit);
    if (segments[index].dataset.lit !== next) segments[index].dataset.lit = next;
  }
};

void listenTelemetry((frame) => renderPerformance.measure(() => render(frame)));
bindOverlayInteractionMode();

if (import.meta.env.DEV) {
  const preview = Number(new URLSearchParams(window.location.search).get("preview"));
  if (Number.isFinite(preview) && preview > 0) {
    render({ lift_and_coast_progress: preview } as TelemetryFrame);
  }
}
