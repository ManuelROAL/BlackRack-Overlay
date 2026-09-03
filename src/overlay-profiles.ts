import type { CompositeLayout } from "./composite-layout";
import type { ConditionsSettings } from "./conditions-settings";
import type { DashboardSettings } from "./dashboard-settings";
import type { DeltaSettings } from "./delta-settings";
import type { DrivingSettings } from "./driving-settings";
import type { FuelSettings } from "./fuel-settings";
import type {
  OverlayFontSizeScope,
  OverlayId,
  OverlayTransparencyScope
} from "./overlay-appearance";
import type { RelativeSettings } from "./relative-settings";
import type { StandingsSettings } from "./standings-settings";
import type { TimingSettings } from "./timing-settings";
import type { TiresSettings } from "./tires-settings";
import type { TrackMapSettings } from "./trackmap-settings";

export const OVERLAY_PROFILES_KEY = "blackrack-overlay.profiles.v1";
export const PROFILE_BINDINGS_KEY = "blackrack-overlay.profile-bindings.v1";

export const OVERLAY_MODES = ["game", "spectator", "team"] as const;
export type OverlayMode = (typeof OVERLAY_MODES)[number];

export const MAX_OVERLAY_PROFILES = 10;
export const MAX_PROFILE_NAME_LENGTH = 40;

export const isOverlayMode = (value: unknown): value is OverlayMode =>
  typeof value === "string" && (OVERLAY_MODES as readonly string[]).includes(value);

/**
 * Overlay state a profile owns. Monitor, performance profile, locale, shortcuts
 * and the browser source stay global and are never switched with the mode.
 */
export interface OverlayProfileData {
  visibility: Record<OverlayId, boolean>;
  transparency: { scope: OverlayTransparencyScope; values: Record<OverlayId, number> };
  fontSize: { scope: OverlayFontSizeScope; values: Record<OverlayId, number> };
  layout: CompositeLayout;
  standings: StandingsSettings;
  relative: RelativeSettings;
  driving: DrivingSettings;
  delta: DeltaSettings;
  timing: TimingSettings;
  trackMap: TrackMapSettings;
  fuel: FuelSettings;
  tires: TiresSettings;
  conditions: ConditionsSettings;
  /**
   * Optional so the profiles a user already stored survive this overlay being
   * added: a saved profile written before Dashboard existed stays usable and
   * simply falls back to the default configuration.
   */
  dashboard?: DashboardSettings;
}

export interface OverlayProfile {
  id: string;
  name: string;
  data: OverlayProfileData;
}

export type ProfileBindings = Record<OverlayMode, string>;

export interface ProfileState {
  profiles: OverlayProfile[];
  bindings: ProfileBindings;
}

export const createProfileId = (): string =>
  `p-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 8)}`;

export const sanitizeProfileName = (value: unknown, fallback: string): string => {
  const text = typeof value === "string" ? value.replace(/\s+/g, " ").trim() : "";
  return text ? text.slice(0, MAX_PROFILE_NAME_LENGTH) : fallback;
};

const plainObject = (value: unknown): Record<string, unknown> | null =>
  value !== null && typeof value === "object" && !Array.isArray(value)
    ? value as Record<string, unknown>
    : null;

const PROFILE_DATA_FIELDS = [
  "visibility", "transparency", "fontSize", "layout", "standings", "relative",
  "driving", "delta", "timing", "trackMap", "fuel", "tires", "conditions"
] as const;

/** Structural check only; each overlay still validates its own settings on read. */
const isProfileData = (value: Record<string, unknown>): boolean =>
  PROFILE_DATA_FIELDS.every((field) => plainObject(value[field]) !== null);

/**
 * Accepts a stored or imported profile list, dropping entries that cannot be
 * used instead of rejecting the whole document.
 */
export const normalizeProfiles = (
  value: unknown,
  fallbackName: string
): OverlayProfile[] => {
  if (!Array.isArray(value)) return [];
  const profiles: OverlayProfile[] = [];
  const seen = new Set<string>();
  for (const entry of value) {
    const profile = plainObject(entry);
    const data = profile ? plainObject(profile.data) : null;
    if (!profile || !data || !isProfileData(data)) continue;
    const id = typeof profile.id === "string" && profile.id.trim() && !seen.has(profile.id)
      ? profile.id
      : createProfileId();
    if (seen.has(id)) continue;
    seen.add(id);
    profiles.push({
      id,
      name: sanitizeProfileName(profile.name, `${fallbackName} ${profiles.length + 1}`),
      data: data as unknown as OverlayProfileData
    });
    if (profiles.length >= MAX_OVERLAY_PROFILES) break;
  }
  return profiles;
};

/** Every mode must resolve to an existing profile; unknown ids fall back to the first one. */
export const normalizeBindings = (
  value: unknown,
  profiles: readonly OverlayProfile[]
): ProfileBindings => {
  const available = new Set(profiles.map(({ id }) => id));
  const fallback = profiles[0]?.id ?? "";
  const source = plainObject(value) ?? {};
  return Object.fromEntries(OVERLAY_MODES.map((mode) => {
    const candidate = source[mode];
    return [mode, typeof candidate === "string" && available.has(candidate) ? candidate : fallback];
  })) as ProfileBindings;
};

export const readProfileState = (fallbackName: string): ProfileState | null => {
  try {
    const profiles = normalizeProfiles(
      JSON.parse(localStorage.getItem(OVERLAY_PROFILES_KEY) ?? "null"),
      fallbackName
    );
    if (profiles.length === 0) return null;
    return {
      profiles,
      bindings: normalizeBindings(
        JSON.parse(localStorage.getItem(PROFILE_BINDINGS_KEY) ?? "null"),
        profiles
      )
    };
  } catch {
    return null;
  }
};

export const saveProfileState = (state: ProfileState): void => {
  localStorage.setItem(OVERLAY_PROFILES_KEY, JSON.stringify(state.profiles));
  localStorage.setItem(PROFILE_BINDINGS_KEY, JSON.stringify(state.bindings));
};

export const modeFromFlags = (spectator: boolean, team: boolean): OverlayMode =>
  team ? "team" : spectator ? "spectator" : "game";
