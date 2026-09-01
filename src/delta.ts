import "./styles.css";
import "./delta.css";
import { fitOverlay } from "./overlay-fit";
import { bindOverlayInteractionMode } from "./overlay-interaction";
import { createOverlayPerformanceTracker } from "./overlay-performance";
import { bindOverlayTransparency } from "./overlay-appearance";
import { DELTA_MODES, readDeltaSettings, type DeltaMode, type DeltaSettings } from "./delta-settings";
import type { DeltaViewModel } from "./telemetry-types";
import { isTauriRuntime, listenRuntimeEvent, listenTelemetry } from "./runtime-events";
import { t } from "./i18n";

fitOverlay({ width: 420, height: 72 });
bindOverlayTransparency("delta");
bindOverlayInteractionMode();
const performance = createOverlayPerformanceTracker("delta");

let settings = readDeltaSettings();
const card = document.getElementById("delta-card");
const fill = document.getElementById("delta-fill") as HTMLElement | null;
const value = document.getElementById("delta-value");
const modeNotice = document.getElementById("delta-mode-notice");
let previousMode: DeltaMode | null = null;
let modeNoticeTimeout: number | null = null;

const VALUE_NEUTRAL_SECONDS = 0.001;
const VALUE_COLOR_RANGE_SECONDS = 0.04;
const VALUE_COLORS = {
  gain: [0, 255, 0],
  neutral: [240, 248, 255],
  loss: [220, 50, 50],
} as const;

const interpolateColor = (from: readonly number[], to: readonly number[], ratio: number): string => {
  const clamped = Math.min(1, Math.max(0, ratio));
  const channels = from.map((channel, index) => Math.round(channel + (to[index] - channel) * clamped));
  return `rgb(${channels.join(" ")})`;
};

const deltaValueColor = (seconds: number): string => {
  const value = Math.abs(seconds) < VALUE_NEUTRAL_SECONDS ? 0 : seconds;
  if (value <= 0) {
    return interpolateColor(
      VALUE_COLORS.gain,
      VALUE_COLORS.neutral,
      (value + VALUE_COLOR_RANGE_SECONDS) / VALUE_COLOR_RANGE_SECONDS,
    );
  }
  return interpolateColor(
    VALUE_COLORS.neutral,
    VALUE_COLORS.loss,
    value / VALUE_COLOR_RANGE_SECONDS,
  );
};

const setText = (element: HTMLElement | null, text: string): void => {
  if (element && element.textContent !== text) element.textContent = text;
};

const showModeNotice = (mode: DeltaMode): void => {
  const option = DELTA_MODES.find(({ value }) => value === mode);
  if (!modeNotice || !option) return;
  setText(modeNotice, t(option.labelKey));
  modeNotice.hidden = false;
  if (modeNoticeTimeout !== null) window.clearTimeout(modeNoticeTimeout);
  modeNoticeTimeout = window.setTimeout(() => {
    modeNotice.hidden = true;
    modeNoticeTimeout = null;
  }, 1_800);
};

const render = (delta: DeltaViewModel): void => {
  const activeMode = delta.mode;
  if (previousMode !== null && activeMode !== previousMode) showModeNotice(activeMode);
  previousMode = activeMode;
  const available = delta.available && activeMode !== "off";
  const state = activeMode === "off"
    ? "off"
    : !available
      ? "waiting"
      : !delta.current_lap_valid
        ? "invalid"
        : delta.seconds <= 0 ? "gain" : "loss";
  const barTrend = !delta.current_lap_valid ? "absolute" : delta.trend;
  if (card && card.dataset.state !== state) card.dataset.state = state;
  if (card && card.dataset.trend !== barTrend) card.dataset.trend = barTrend;
  if (card) {
    const color = deltaValueColor(delta.seconds);
    if (card.style.getPropertyValue("--delta-value-color") !== color) {
      card.style.setProperty("--delta-value-color", color);
    }
  }
  setText(value, available ? `${delta.seconds >= 0 ? "+" : "−"}${Math.abs(delta.seconds).toFixed(3)}` : "---.---");
  if (fill) {
    const ratio = available ? Math.min(1, Math.abs(delta.seconds) / settings.displayRange) : 0;
    const width = `${ratio * 50}%`;
    const left = delta.seconds <= 0 ? "50%" : `${50 - ratio * 50}%`;
    if (fill.style.width !== width) fill.style.width = width;
    if (fill.style.left !== left) fill.style.left = left;
  }
};

void listenTelemetry((frame) => performance.measure(() => render(frame.delta_model)));

if (isTauriRuntime()) {
  void listenRuntimeEvent<DeltaSettings>("delta://settings", (next) => {
    settings = next;
  });
}
