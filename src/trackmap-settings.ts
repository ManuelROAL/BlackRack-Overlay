export interface TrackMapSettings {
  showPitPrediction: boolean;
  playerColor: string | null;
  playerIconDataUrl: string | null;
}

export const TRACK_MAP_SETTINGS_KEY = "blackrack-overlay.track-map-settings.v1";

export const defaultTrackMapSettings = (): TrackMapSettings => ({
  showPitPrediction: true,
  playerColor: null,
  playerIconDataUrl: null
});

export const MAX_PLAYER_ICON_BYTES = 96 * 1024;
export const MAX_PLAYER_ICON_DATA_URL_LENGTH = 128 * 1024;

const normalizeIcon = (value: unknown): string | null => {
  if (typeof value !== "string" || value.length > MAX_PLAYER_ICON_DATA_URL_LENGTH
    || !/^data:image\/png;base64,[A-Za-z0-9+/]+={0,2}$/.test(value)) return null;
  const encoded = value.slice("data:image/png;base64,".length);
  const size = Math.floor(encoded.length * 3 / 4) - (encoded.endsWith("==") ? 2 : encoded.endsWith("=") ? 1 : 0);
  if (size < 33 || size > MAX_PLAYER_ICON_BYTES) return null;
  try {
    const bytes = Uint8Array.from(atob(encoded), (char) => char.charCodeAt(0));
    if (bytes.length < 33 || bytes[0] !== 137 || bytes[1] !== 80 || bytes[2] !== 78 || bytes[3] !== 71
      || bytes[4] !== 13 || bytes[5] !== 10 || bytes[6] !== 26 || bytes[7] !== 10) return null;
    const view = new DataView(bytes.buffer);
    if (view.getUint32(8) !== 13 || String.fromCharCode(...bytes.slice(12, 16)) !== "IHDR"
      || view.getUint32(16) < 1 || view.getUint32(20) < 1
      || view.getUint32(16) > 128 || view.getUint32(20) > 128) return null;
    let offset = 8;
    let chunkCount = 0;
    let hasImageData = false;
    let hasEnd = false;
    while (offset + 12 <= bytes.length) {
      if (++chunkCount > 1024) return null;
      const length = view.getUint32(offset);
      if (length > bytes.length - offset - 12) return null;
      const type = String.fromCharCode(...bytes.slice(offset + 4, offset + 8));
      if (type === "acTL" || type === "fcTL" || type === "fdAT") return null;
      if (type === "IDAT") hasImageData = true;
      offset += length + 12;
      if (type === "IEND") {
        hasEnd = length === 0 && offset === bytes.length;
        break;
      }
    }
    return hasImageData && hasEnd ? value : null;
  } catch { return null; }
};

export const normalizeTrackMapSettings = (value: unknown): TrackMapSettings => {
  const saved = value && typeof value === "object" ? value as Partial<TrackMapSettings> : {};
  return {
    showPitPrediction: typeof saved.showPitPrediction === "boolean" ? saved.showPitPrediction : true,
    playerColor: typeof saved.playerColor === "string" && /^#[0-9a-fA-F]{6}$/.test(saved.playerColor)
      ? saved.playerColor.toLowerCase() : null,
    playerIconDataUrl: normalizeIcon(saved.playerIconDataUrl)
  };
};

export const readTrackMapSettings = (): TrackMapSettings => {
  const defaults = defaultTrackMapSettings();
  try {
    return normalizeTrackMapSettings(JSON.parse(localStorage.getItem(TRACK_MAP_SETTINGS_KEY) ?? "null"));
  } catch {
    localStorage.removeItem(TRACK_MAP_SETTINGS_KEY);
    return defaults;
  }
};
