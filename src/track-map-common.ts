import { invokeRuntime, isTauriRuntime } from "./runtime-events";

export interface MapPoint {
  x: number;
  y: number;
  distance: number;
}

export interface GeometryPoint {
  x: number;
  y: number;
}

export interface TrackMapGeometry {
  source: "official" | "learned";
  mainPath: MapPoint[];
  mainLength: number;
  pitPath: GeometryPoint[];
}

/** Track Map and Minimap read the same cached geometry, official or learned. */
export const fetchTrackMapGeometry = async (key: string): Promise<TrackMapGeometry> => {
  if (isTauriRuntime()) {
    return invokeRuntime<TrackMapGeometry>("get_track_map_geometry", { cacheKey: key });
  }
  if (document.documentElement.dataset.browserSource === "true") {
    const response = await fetch(`/api/trackmap?key=${encodeURIComponent(key)}`);
    if (!response.ok) throw new Error(`HTTP ${response.status}`);
    return response.json() as Promise<TrackMapGeometry>;
  }
  throw new Error("Geometria oficial no disponible en la vista previa");
};

export const isUsableTrackMapGeometry = (geometry: TrackMapGeometry): boolean =>
  geometry.mainPath.length >= 40 && geometry.mainLength >= 100;

export const classColor = (vehicleClass: string): string => {
  const value = vehicleClass.toUpperCase();
  if (value.includes("HYPER") || value.includes("GTP")) return "#e33b3b";
  if (value.includes("LMP2")) return "#3988ed";
  if (value.includes("LMP3")) return "#8065ed";
  if (value.includes("GT3")) return "#35c969";
  if (value.includes("GTE")) return "#e2b93b";
  return "#d8dde2";
};

/** Position within each car's class, ordered by overall position. */
export const classPositions = (
  vehicles: readonly { vehicle_id: number; overall_position: number; vehicle_class: string }[]
): Map<number, number> => {
  const positions = new Map<number, number>();
  const counts = new Map<string, number>();
  [...vehicles].sort((a, b) => a.overall_position - b.overall_position).forEach((vehicle) => {
    const key = vehicle.vehicle_class.toUpperCase();
    const position = (counts.get(key) ?? 0) + 1;
    counts.set(key, position);
    positions.set(vehicle.vehicle_id, position);
  });
  return positions;
};
