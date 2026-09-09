import type { TranslationKey } from "./i18n";
import type { OverlayId } from "./overlay-appearance";

export interface OverlayGuideEntry {
  icon: string;
  title: TranslationKey;
  purpose: TranslationKey;
  reading: TranslationKey;
  tip: TranslationKey;
  /**
   * An overlay whose cues appear only while something is happening needs them
   * explained where the driver will not meet them by accident. Optional: the
   * section is left out entirely for the overlays that only ever show readouts.
   */
  warnings?: TranslationKey;
}

export const OVERLAY_GUIDE: Record<OverlayId, OverlayGuideEntry> = {
  standings: {
    icon: "P",
    title: "card.standings",
    purpose: "guide.standings.purpose",
    reading: "guide.standings.reading",
    tip: "guide.standings.tip"
  },
  relative: {
    icon: "R",
    title: "card.relative",
    purpose: "guide.relative.purpose",
    reading: "guide.relative.reading",
    tip: "guide.relative.tip"
  },
  fuel: {
    icon: "F",
    title: "card.fuel",
    purpose: "guide.fuel.purpose",
    reading: "guide.fuel.reading",
    tip: "guide.fuel.tip"
  },
  driving: {
    icon: "T",
    title: "card.driving",
    purpose: "guide.driving.purpose",
    reading: "guide.driving.reading",
    tip: "guide.driving.tip"
  },
  liftcoast: {
    icon: "L",
    title: "card.liftcoast",
    purpose: "guide.liftcoast.purpose",
    reading: "guide.liftcoast.reading",
    warnings: "guide.liftcoast.warnings",
    tip: "guide.liftcoast.tip"
  },
  tires: {
    icon: "N",
    title: "card.tires",
    purpose: "guide.tires.purpose",
    reading: "guide.tires.reading",
    tip: "guide.tires.tip"
  },
  damage: {
    icon: "D",
    title: "card.damage",
    purpose: "guide.damage.purpose",
    reading: "guide.damage.reading",
    tip: "guide.damage.tip"
  },
  pitstop: {
    icon: "P",
    title: "card.pitstop",
    purpose: "guide.pitstop.purpose",
    reading: "guide.pitstop.reading",
    tip: "guide.pitstop.tip"
  },
  flags: {
    icon: "Y",
    title: "card.flags",
    purpose: "guide.flags.purpose",
    reading: "guide.flags.reading",
    warnings: "guide.flags.warnings",
    tip: "guide.flags.tip"
  },
  rejoin: {
    icon: "R",
    title: "card.rejoin",
    purpose: "guide.rejoin.purpose",
    reading: "guide.rejoin.reading",
    warnings: "guide.rejoin.warnings",
    tip: "guide.rejoin.tip"
  },
  delta: {
    icon: "Δ",
    title: "card.delta",
    purpose: "guide.delta.purpose",
    reading: "guide.delta.reading",
    tip: "guide.delta.tip"
  },
  timing: {
    icon: "T",
    title: "card.timing",
    purpose: "guide.timing.purpose",
    reading: "guide.timing.reading",
    tip: "guide.timing.tip"
  },
  stinthistory: {
    icon: "S",
    title: "card.stintHistory",
    purpose: "guide.stintHistory.purpose",
    reading: "guide.stintHistory.reading",
    tip: "guide.stintHistory.tip"
  },
  trackmap: {
    icon: "M",
    title: "card.trackmap",
    purpose: "guide.trackmap.purpose",
    reading: "guide.trackmap.reading",
    tip: "guide.trackmap.tip"
  },
  forecast: {
    icon: "☁",
    title: "card.forecast",
    purpose: "guide.forecast.purpose",
    reading: "guide.forecast.reading",
    tip: "guide.forecast.tip"
  },
  conditions: {
    icon: "⛅",
    title: "card.conditions",
    purpose: "guide.conditions.purpose",
    reading: "guide.conditions.reading",
    tip: "guide.conditions.tip"
  },
  chat: {
    icon: "C",
    title: "card.chat",
    purpose: "guide.chat.purpose",
    reading: "guide.chat.reading",
    tip: "guide.chat.tip"
  },
  dashboard: {
    icon: "⚙",
    title: "card.dashboard",
    purpose: "guide.dashboard.purpose",
    reading: "guide.dashboard.reading",
    warnings: "guide.dashboard.warnings",
    tip: "guide.dashboard.tip"
  }
};

export const OVERLAY_GUIDE_ORDER = [
  "standings",
  "relative",
  "fuel",
  "driving",
  "liftcoast",
  "tires",
  "damage",
  "pitstop",
  "flags",
  "rejoin",
  "delta",
  "timing",
  "stinthistory",
  "trackmap",
  "forecast",
  "conditions",
  "chat",
  "dashboard"
] as const satisfies readonly OverlayId[];
