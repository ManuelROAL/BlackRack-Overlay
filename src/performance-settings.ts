export const PERFORMANCE_PROFILE_KEY = "blackrack-overlay.performance-profile.v1";

export const PERFORMANCE_PROFILES = ["smooth", "balanced", "efficiency"] as const;
export type PerformanceProfile = typeof PERFORMANCE_PROFILES[number];

export const DEFAULT_PERFORMANCE_PROFILE: PerformanceProfile = "smooth";

export const isPerformanceProfile = (value: unknown): value is PerformanceProfile =>
  typeof value === "string" && PERFORMANCE_PROFILES.includes(value as PerformanceProfile);

export const readPerformanceProfile = (): PerformanceProfile => {
  try {
    const saved = localStorage.getItem(PERFORMANCE_PROFILE_KEY);
    if (isPerformanceProfile(saved)) return saved;
  } catch {
    // Web storage may be unavailable in an isolated browser context.
  }
  return DEFAULT_PERFORMANCE_PROFILE;
};

export const savePerformanceProfile = (profile: PerformanceProfile): void => {
  localStorage.setItem(PERFORMANCE_PROFILE_KEY, profile);
};
