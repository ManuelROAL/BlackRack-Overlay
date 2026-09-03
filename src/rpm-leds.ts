/**
 * Shift lights are read the same way wherever they appear, so the thresholds,
 * the fill and the colour bands live here instead of being restated by each
 * overlay that draws them. Two panels disagreeing about when to shift is worse
 * than either of them being slightly wrong.
 */
export const RPM_LED_START = 0.84;
export const RPM_LED_CRITICAL = 0.96;
export const RPM_LED_OVER_REV = 0.9999;

/**
 * Half a blink period at the critical band. Callers read the clock on each
 * telemetry update rather than running a timer, so the state still changes only
 * when a frame arrives and the rate stays the same across performance profiles.
 */
const CRITICAL_BLINK_MS = 125;

export interface RpmLedState {
  ratio: number;
  /** Lit pairs, counted inwards from both ends of the strip. */
  activePairs: number;
  critical: boolean;
  overRev: boolean;
  /** False on the dark half of the critical blink. */
  visible: boolean;
}

export const rpmLedState = (rpm: number, maxRpm: number, count: number): RpmLedState => {
  const ratio = Number.isFinite(rpm) && Number.isFinite(maxRpm) && maxRpm > 0
    ? Math.max(0, Math.min(1, rpm / maxRpm))
    : 0;
  const critical = ratio >= RPM_LED_CRITICAL;
  const pairCount = Math.ceil(count / 2);
  return {
    ratio,
    critical,
    overRev: ratio >= RPM_LED_OVER_REV,
    visible: !critical || Math.floor(performance.now() / CRITICAL_BLINK_MS) % 2 === 0,
    activePairs: critical
      ? pairCount
      : Math.ceil(
        Math.max(0, ratio - RPM_LED_START) / (RPM_LED_CRITICAL - RPM_LED_START) * pairCount
      )
  };
};

/** The strip fills from both ends towards the middle, as a wheel's own does. */
export const rpmLedIsActive = (index: number, count: number, state: RpmLedState): boolean =>
  state.visible && Math.min(index, count - 1 - index) < state.activePairs;

/**
 * Colour follows the position on the strip, not the current revs: the middle is
 * always the red band, so the shape of the lit strip is what reads, and it
 * reads the same at a glance in every overlay.
 */
export const rpmLedBand = (index: number, count: number): "low" | "mid" | "high" => {
  const pairCount = Math.ceil(count / 2);
  const distanceFromEdge = Math.min(index, count - 1 - index);
  const band = Math.floor(distanceFromEdge / (pairCount / 3));
  return band <= 0 ? "low" : band === 1 ? "mid" : "high";
};
