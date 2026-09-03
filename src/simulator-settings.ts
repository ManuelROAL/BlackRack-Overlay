export const SIMULATOR_PREFERENCE_KEY = "blackrack-overlay.simulator-preference.v1";

/** "auto" or a simulator id; ids are backend data, so no fixed union to check against. */
export const DEFAULT_SIMULATOR_PREFERENCE = "auto";

export const readSimulatorPreference = (): string => {
  try {
    const saved = localStorage.getItem(SIMULATOR_PREFERENCE_KEY);
    if (saved) return saved;
  } catch {
    // Web storage may be unavailable in an isolated browser context.
  }
  return DEFAULT_SIMULATOR_PREFERENCE;
};

export const saveSimulatorPreference = (preference: string): void => {
  localStorage.setItem(SIMULATOR_PREFERENCE_KEY, preference);
};
