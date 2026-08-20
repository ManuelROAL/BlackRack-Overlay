export interface TrackMapSettings {
  showPitPrediction: boolean;
}

export const TRACK_MAP_SETTINGS_KEY = "lmu-overlay.track-map-settings.v1";

export const defaultTrackMapSettings = (): TrackMapSettings => ({
  showPitPrediction: true
});

export const readTrackMapSettings = (): TrackMapSettings => {
  const defaults = defaultTrackMapSettings();
  try {
    const saved = JSON.parse(localStorage.getItem(TRACK_MAP_SETTINGS_KEY) ?? "null") as Partial<TrackMapSettings> | null;
    return {
      showPitPrediction: typeof saved?.showPitPrediction === "boolean"
        ? saved.showPitPrediction
        : defaults.showPitPrediction
    };
  } catch {
    localStorage.removeItem(TRACK_MAP_SETTINGS_KEY);
    return defaults;
  }
};
