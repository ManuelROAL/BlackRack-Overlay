export type TrackLimitTone = "normal" | "warning" | "critical";

const TRACK_LIMIT_STEPS_PER_POINT = 4;

export const trackLimitPoints = (steps: number): number => steps / TRACK_LIMIT_STEPS_PER_POINT;

export const formatTrackLimitPoints = (steps: number): string => {
  const points = trackLimitPoints(steps);
  if (!Number.isFinite(points)) return "--";
  if (Number.isInteger(points)) return points.toFixed(0);
  return points.toFixed(2).replace(/0$/, "").replace(".", ",");
};

export const trackLimitTone = (steps: number, stepsPerPenalty: number): TrackLimitTone | null => {
  if (!Number.isFinite(steps) || !Number.isFinite(stepsPerPenalty) || stepsPerPenalty <= 0) return null;
  const progress = Math.max(steps, 0) / stepsPerPenalty;
  if (progress >= 0.8) return "critical";
  if (progress >= 0.6) return "warning";
  return "normal";
};

export const applyTrackLimitTone = (
  element: HTMLElement,
  steps: number,
  stepsPerPenalty: number
): void => {
  const tone = trackLimitTone(steps, stepsPerPenalty);
  if (tone) element.dataset.trackLimitTone = tone;
};
