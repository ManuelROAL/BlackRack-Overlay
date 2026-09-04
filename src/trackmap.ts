import "./styles.css";
import "./trackmap.css";
import { fitOverlay } from "./overlay-fit";
import { bindOverlayInteractionMode } from "./overlay-interaction";
import { bindOverlayTransparency } from "./overlay-appearance";
import { createOverlayPerformanceTracker } from "./overlay-performance";
import { invokeRuntime, isTauriRuntime, listenRuntimeEvent, listenTelemetry } from "./runtime-events";
import { readTrackMapSettings, type TrackMapSettings } from "./trackmap-settings";
import type { TelemetryFrame, TrackMapVehicle } from "./telemetry-types";
import { t } from "./i18n";

interface MapPoint {
  x: number;
  y: number;
  distance: number;
}

interface Transform {
  x: (worldX: number) => number;
  y: (worldY: number) => number;
}

interface GeometryPoint {
  x: number;
  y: number;
}

interface TrackMapGeometry {
  source: "official" | "learned";
  mainPath: MapPoint[];
  mainLength: number;
  pitPath: GeometryPoint[];
}

interface PreparedGeometry {
  mainPath: MapPoint[];
  mainLength: number;
  pitPath: GeometryPoint[];
}

interface CalibrationObservation {
  rawDistance: number;
  lapDistance: number;
}

interface MarkerView {
  root: HTMLDivElement;
  disc: HTMLDivElement;
  label: HTMLSpanElement;
  halo: HTMLDivElement | null;
  isPlayer: boolean;
  transform: string;
  color: string;
  labelValue: string;
  inPits: boolean;
  causingYellow: boolean;
  isRaceLeader: boolean;
}

const SVG_NS = "http://www.w3.org/2000/svg";
const SIZE = 420;
const MARGIN = 34;

fitOverlay({ width: 436, height: 436 });
bindOverlayTransparency("trackmap");
bindOverlayInteractionMode();
const renderPerformance = createOverlayPerformanceTracker("trackmap");

const outline = document.querySelector<SVGPathElement>("#track-outline")!;
const line = document.querySelector<SVGPathElement>("#track-line")!;
const pitOutline = document.querySelector<SVGPathElement>("#pit-outline")!;
const pitLine = document.querySelector<SVGPathElement>("#pit-line")!;
const startLine = document.querySelector<SVGPathElement>("#start-line")!;
const yellowSectorLines = [1, 2, 3].map((sector) =>
  document.querySelector<SVGPathElement>(`#yellow-sector-${sector}`)!
);
const purpleSectorLines = [1, 2, 3].map((sector) =>
  document.querySelector<SVGPathElement>(`#purple-sector-${sector}`)!
);
const vehicleLayer = document.querySelector<HTMLDivElement>("#vehicle-layer")!;
const status = document.getElementById("map-status") as HTMLElement;

let mapKey = "";
let learnedPoints: MapPoint[] = [];
let officialGeometry: PreparedGeometry | null = null;
let officialDistancePoints: MapPoint[] = [];
let calibrationObservation: CalibrationObservation | null = null;
let geometryRequestKey = "";
let geometryLoadedKey = "";
let geometryRetryAfter = 0;
let geometryRevision = -1;
let transform: Transform | null = null;
const markers = new Map<number, MarkerView>();
let predictionMarker: HTMLDivElement | null = null;
let predictionTransform = "";
let trackMapSettings = readTrackMapSettings();
let latestPerformanceProfile: TelemetryFrame["performance_profile"] = "smooth";
let latestYellowSectors = 0;
let latestPurpleSectors = 0;
let latestSectorBoundaries: [number | null, number | null] = [null, null];
let latestTrackLength = 0;
let sectorHighlightRenderKey = "";

const migrateLegacyLearning = (key: string, trackName: string, trackLength: number): void => {
  if (!isTauriRuntime()) return;
  const pitKey = key.replace(
    "blackrack-overlay.track-map.v1.",
    "blackrack-overlay.track-map.pit-traversal.v1."
  );
  let points: MapPoint[] = [];
  let pitTraversalSamples: number[] = [];
  try {
    const map = JSON.parse(localStorage.getItem(key) ?? "null") as { version?: number; points?: MapPoint[] } | null;
    const pit = JSON.parse(localStorage.getItem(pitKey) ?? "null") as { version?: number; samples?: number[] } | null;
    if (map?.version === 1 && Array.isArray(map.points)) points = map.points;
    if (pit?.version === 1 && Array.isArray(pit.samples)) pitTraversalSamples = pit.samples;
  } catch {
    localStorage.removeItem(key);
    localStorage.removeItem(pitKey);
    return;
  }
  if (!points.length && !pitTraversalSamples.length) return;
  void invokeRuntime<boolean>("migrate_legacy_track_map_learning", {
    cacheKey: key,
    trackName,
    trackLength,
    points,
    pitTraversalSamples
  }).then((handled) => {
    if (handled) {
      localStorage.removeItem(key);
      localStorage.removeItem(pitKey);
    }
  }).catch(() => undefined);
};

const normalizeDistance = (distance: number, length: number): number =>
  length > 0 ? ((distance % length) + length) % length : 0;

const wrappedDelta = (next: number, previous: number, length: number): number => {
  let delta = next - previous;
  if (length > 0 && delta > length / 2) delta -= length;
  if (length > 0 && delta < -length / 2) delta += length;
  return delta;
};

const fetchOfficialGeometry = async (key: string): Promise<TrackMapGeometry> => {
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

const requestOfficialGeometry = (key: string): void => {
  if (!key || geometryLoadedKey === key || geometryRequestKey === key || Date.now() < geometryRetryAfter) return;
  geometryRequestKey = key;
  void fetchOfficialGeometry(key)
    .then((geometry) => {
      if (mapKey !== key) return;
      if (geometry.mainPath.length < 40 || geometry.mainLength < 100) {
        throw new Error("Geometria del circuito incompleta");
      }
      officialGeometry = geometry.source === "official"
        ? { mainPath: geometry.mainPath, mainLength: geometry.mainLength, pitPath: geometry.pitPath }
        : null;
      learnedPoints = geometry.source === "learned" ? geometry.mainPath : [];
      officialDistancePoints = [];
      calibrationObservation = null;
      geometryLoadedKey = key;
      renderTrack();
    })
    .catch(() => {
      if (mapKey === key) geometryRetryAfter = Date.now() + 10_000;
    })
    .finally(() => {
      if (geometryRequestKey === key) geometryRequestKey = "";
    });
};

const makeTransform = (points: MapPoint[]): Transform => {
  const xs = points.map((point) => point.x);
  const ys = points.map((point) => point.y);
  const minX = Math.min(...xs);
  const maxX = Math.max(...xs);
  const minY = Math.min(...ys);
  const maxY = Math.max(...ys);
  const width = Math.max(maxX - minX, 1);
  const height = Math.max(maxY - minY, 1);
  const scale = (SIZE - MARGIN * 2) / Math.max(width, height);
  const offsetX = (SIZE - width * scale) / 2;
  const offsetY = (SIZE - height * scale) / 2;
  return {
    x: (worldX) => offsetX + (worldX - minX) * scale,
    y: (worldY) => offsetY + (worldY - minY) * scale
  };
};

const pointPath = (points: MapPoint[], fit: Transform): string => points
  .map((point, index) => `${index ? "L" : "M"}${fit.x(point.x).toFixed(2)} ${fit.y(point.y).toFixed(2)}`)
  .join(" ") + " Z";

const openPointPath = (points: GeometryPoint[], fit: Transform): string => points
  .map((point, index) => `${index ? "L" : "M"}${fit.x(point.x).toFixed(2)} ${fit.y(point.y).toFixed(2)}`)
  .join(" ");

const clearPitPath = (): void => {
  pitOutline.removeAttribute("d");
  pitLine.removeAttribute("d");
};

const renderTrack = (): void => {
  sectorHighlightRenderKey = "";
  const displayPoints = officialGeometry?.mainPath ?? learnedPoints;
  if (displayPoints.length >= 40) {
    transform = makeTransform(displayPoints);
    const path = pointPath(displayPoints, transform);
    outline.setAttribute("d", path);
    line.setAttribute("d", path);
    if (officialGeometry && officialGeometry.pitPath.length >= 2) {
      const pitPath = openPointPath(officialGeometry.pitPath, transform);
      pitOutline.setAttribute("d", pitPath);
      pitLine.setAttribute("d", pitPath);
    } else {
      clearPitPath();
    }
    const first = officialDistancePoints.length ? officialDistancePoints[0] : displayPoints[0];
    const firstDisplayIndex = officialDistancePoints.length
      ? displayPoints.reduce((bestIndex, point, index) => {
          const best = displayPoints[bestIndex];
          return Math.hypot(point.x - first.x, point.y - first.y)
            < Math.hypot(best.x - first.x, best.y - first.y) ? index : bestIndex;
        }, 0)
      : 0;
    const next = displayPoints[(firstDisplayIndex + 3) % displayPoints.length];
    const x = transform.x(first.x);
    const y = transform.y(first.y);
    const dx = transform.x(next.x) - x;
    const dy = transform.y(next.y) - y;
    const length = Math.hypot(dx, dy) || 1;
    const nx = -dy / length * 8;
    const ny = dx / length * 8;
    startLine.setAttribute("d", `M${x - nx} ${y - ny} L${x + nx} ${y + ny}`);
    status.hidden = true;
  } else {
    transform = null;
    const radius = (SIZE - MARGIN * 2) / 2;
    outline.setAttribute("d", `M${SIZE / 2} ${MARGIN}a${radius} ${radius} 0 1 1 0 ${radius * 2}a${radius} ${radius} 0 1 1 0 ${-radius * 2}`);
    line.setAttribute("d", outline.getAttribute("d") ?? "");
    clearPitPath();
    startLine.setAttribute("d", `M${SIZE / 2 - 8} ${MARGIN} L${SIZE / 2 + 8} ${MARGIN}`);
    status.hidden = false;
  }
  renderSectorHighlights();
};

const nearestMainPoint = (x: number, y: number): { point: MapPoint; distance: number } | null => {
  if (!officialGeometry) return null;
  let bestPoint = officialGeometry.mainPath[0];
  let bestSquared = Number.POSITIVE_INFINITY;
  for (const point of officialGeometry.mainPath) {
    const dx = x - point.x;
    const dy = y - point.y;
    const squared = dx * dx + dy * dy;
    if (squared < bestSquared) {
      bestSquared = squared;
      bestPoint = point;
    }
  }
  return { point: bestPoint, distance: Math.sqrt(bestSquared) };
};

const calibrateOfficialDistances = (
  player: TrackMapVehicle | undefined,
  trackLength: number
): void => {
  if (!officialGeometry || officialDistancePoints.length || !player || player.in_pits || trackLength <= 0) return;
  const nearest = nearestMainPoint(player.world_x, player.world_y);
  if (!nearest || nearest.distance > 25) {
    calibrationObservation = null;
    return;
  }
  const observation = { rawDistance: nearest.point.distance, lapDistance: player.lap_distance };
  const previous = calibrationObservation;
  calibrationObservation = observation;
  if (!previous) return;
  const rawDelta = wrappedDelta(observation.rawDistance, previous.rawDistance, officialGeometry.mainLength);
  const lapDelta = wrappedDelta(observation.lapDistance, previous.lapDistance, trackLength);
  if (Math.abs(rawDelta) < 3 || Math.abs(lapDelta) < 3) return;

  const direction = Math.sign(rawDelta) === Math.sign(lapDelta) ? 1 : -1;
  const scale = trackLength / officialGeometry.mainLength;
  const orientedCurrent = direction > 0
    ? observation.rawDistance
    : officialGeometry.mainLength - observation.rawDistance;
  const offset = normalizeDistance(observation.lapDistance - orientedCurrent * scale, trackLength);
  officialDistancePoints = officialGeometry.mainPath
    .map((point) => {
      const oriented = direction > 0 ? point.distance : officialGeometry!.mainLength - point.distance;
      return {
        x: point.x,
        y: point.y,
        distance: normalizeDistance(oriented * scale + offset, trackLength)
      };
    })
    .sort((a, b) => a.distance - b.distance);
  renderTrack();
};

const markerPosition = (vehicle: TrackMapVehicle, trackLength: number): [number, number] => {
  if (transform) return [transform.x(vehicle.world_x), transform.y(vehicle.world_y)];
  const progress = trackLength > 0 ? vehicle.lap_distance / trackLength : 0;
  const angle = progress * Math.PI * 2 - Math.PI / 2;
  const radius = (SIZE - MARGIN * 2) / 2;
  return [SIZE / 2 + Math.cos(angle) * radius, SIZE / 2 + Math.sin(angle) * radius];
};

const positionAtLapDistance = (lapDistance: number, trackLength: number): [number, number] => {
  const distance = trackLength > 0
    ? ((lapDistance % trackLength) + trackLength) % trackLength
    : 0;
  const distancePoints = officialDistancePoints.length ? officialDistancePoints : learnedPoints;
  if (!transform || distancePoints.length < 2 || trackLength <= 0) {
    const angle = distance / Math.max(trackLength, 1) * Math.PI * 2 - Math.PI / 2;
    const radius = (SIZE - MARGIN * 2) / 2;
    return [SIZE / 2 + Math.cos(angle) * radius, SIZE / 2 + Math.sin(angle) * radius];
  }

  let upperIndex = distancePoints.findIndex((point) => point.distance >= distance);
  let upperDistance: number;
  let lowerDistance: number;
  let upper: MapPoint;
  let lower: MapPoint;
  if (upperIndex < 0) {
    upperIndex = 0;
    lower = distancePoints[distancePoints.length - 1];
    upper = distancePoints[0];
    lowerDistance = lower.distance;
    upperDistance = upper.distance + trackLength;
  } else if (upperIndex === 0) {
    lower = distancePoints[distancePoints.length - 1];
    upper = distancePoints[0];
    lowerDistance = lower.distance - trackLength;
    upperDistance = upper.distance;
  } else {
    lower = distancePoints[upperIndex - 1];
    upper = distancePoints[upperIndex];
    lowerDistance = lower.distance;
    upperDistance = upper.distance;
  }
  const span = Math.max(upperDistance - lowerDistance, 0.001);
  const adjustedDistance = upperIndex === 0 && distance > upperDistance ? distance - trackLength : distance;
  const ratio = Math.max(0, Math.min(1, (adjustedDistance - lowerDistance) / span));
  const worldX = lower.x + (upper.x - lower.x) * ratio;
  const worldY = lower.y + (upper.y - lower.y) * ratio;
  return [transform.x(worldX), transform.y(worldY)];
};

const sectorSegmentPath = (
  start: number,
  end: number,
  trackLength: number,
  points: MapPoint[]
): string => {
  const [startX, startY] = positionAtLapDistance(start, trackLength);
  const [endX, endY] = positionAtLapDistance(end, trackLength);
  const middle = points
    .filter((point) => point.distance > start && point.distance < end)
    .map((point) => `L${transform!.x(point.x).toFixed(2)} ${transform!.y(point.y).toFixed(2)}`)
    .join(" ");
  return `M${startX.toFixed(2)} ${startY.toFixed(2)} ${middle} L${endX.toFixed(2)} ${endY.toFixed(2)}`;
};

const applySectorMask = (
  lines: SVGPathElement[],
  mask: number,
  ranges: Array<[number, number, number]>,
  points: MapPoint[]
): void => {
  lines.forEach((sectorLine, index) => {
    const range = ranges[index];
    if (!range || (mask & (1 << range[2])) === 0) {
      if (sectorLine.hasAttribute("d")) sectorLine.removeAttribute("d");
      return;
    }
    const path = sectorSegmentPath(range[0], range[1], latestTrackLength, points);
    if (sectorLine.getAttribute("d") !== path) sectorLine.setAttribute("d", path);
  });
};

const renderSectorHighlights = (): void => {
  const [sector1End, sector2End] = latestSectorBoundaries;
  const points = officialDistancePoints.length ? officialDistancePoints : learnedPoints;
  const ready = transform
    && points.length >= 40
    && latestTrackLength > 100
    && sector1End !== null
    && sector2End !== null
    && sector1End > 0
    && sector2End > sector1End
    && sector2End < latestTrackLength;
  const ranges: Array<[number, number, number]> = ready
    ? [[0, sector1End, 1], [sector1End, sector2End, 2], [sector2End, latestTrackLength, 0]]
    : [];
  const renderKey = `${ready ? 1 : 0}|${latestYellowSectors}|${latestPurpleSectors}|${sector1End}|${sector2End}|${latestTrackLength}`;
  if (renderKey === sectorHighlightRenderKey) return;
  sectorHighlightRenderKey = renderKey;
  // A yellow warns and a purple only reports, so the flag keeps the segment.
  applySectorMask(purpleSectorLines, latestPurpleSectors & ~latestYellowSectors, ranges, points);
  applySectorMask(yellowSectorLines, latestYellowSectors, ranges, points);
};

const ensurePredictionMarker = (): HTMLDivElement => {
  if (predictionMarker) return predictionMarker;
  const group = document.createElement("div");
  group.classList.add("pit-prediction-marker");
  group.title = t("trackmap.pitPredictionTitle");
  const label = document.createElement("span");
  label.textContent = "P";
  group.appendChild(label);
  vehicleLayer.prepend(group);
  predictionMarker = group;
  return group;
};

const renderPitPrediction = (lapDistance: number | null, trackLength: number): void => {
  if (lapDistance === null || trackLength <= 0) {
    if (predictionMarker) predictionMarker.setAttribute("hidden", "");
    return;
  }
  const [x, y] = positionAtLapDistance(lapDistance, trackLength);
  const marker = ensurePredictionMarker();
  marker.removeAttribute("hidden");
  const nextTransform = `translate3d(${x.toFixed(1)}px, ${y.toFixed(1)}px, 0)`;
  if (predictionTransform !== nextTransform) {
    predictionTransform = nextTransform;
    marker.style.transform = nextTransform;
  }
};

const classColor = (vehicleClass: string): string => {
  const value = vehicleClass.toUpperCase();
  if (value.includes("HYPER") || value.includes("GTP")) return "#e33b3b";
  if (value.includes("LMP2")) return "#3988ed";
  if (value.includes("LMP3")) return "#8065ed";
  if (value.includes("GT3")) return "#35c969";
  return "#d8dde2";
};

const setMarkerPosition = (marker: MarkerView, x: number, y: number): void => {
  const nextTransform = `translate3d(${x.toFixed(1)}px, ${y.toFixed(1)}px, 0)`;
  if (marker.transform === nextTransform) return;
  marker.transform = nextTransform;
  marker.root.style.transform = nextTransform;
};

const createMarkerHalo = (root: HTMLDivElement, before: HTMLDivElement | null = null): HTMLDivElement => {
  const halo = document.createElement("div");
  halo.classList.add("marker-halo");
  root.insertBefore(halo, before);
  return halo;
};

const createMarker = (vehicle: TrackMapVehicle): MarkerView => {
  const group = document.createElement("div");
  group.classList.add("vehicle-marker");
  const leaderStar = document.createElementNS(SVG_NS, "svg");
  leaderStar.classList.add("race-leader-star");
  leaderStar.setAttribute("viewBox", "-7 -26 14 15");
  const leaderStarPath = document.createElementNS(SVG_NS, "path");
  leaderStarPath.setAttribute("d", "M0-25 1.9-20.9 6.4-20.4 3.1-17.2 4-12.8 0-15 -4-12.8 -3.1-17.2 -6.4-20.4 -1.9-20.9Z");
  leaderStar.appendChild(leaderStarPath);
  group.appendChild(leaderStar);
  let halo: HTMLDivElement | null = null;
  if (vehicle.is_player) {
    group.classList.add("is-player");
    halo = createMarkerHalo(group);
  }
  const disc = document.createElement("div");
  disc.classList.add("vehicle-disc");
  const label = document.createElement("span");
  label.classList.add("vehicle-label");
  disc.appendChild(label);
  group.appendChild(disc);
  vehicleLayer.appendChild(group);
  const marker: MarkerView = {
    root: group,
    disc,
    label,
    halo,
    isPlayer: vehicle.is_player,
    transform: "",
    color: "",
    labelValue: "",
    inPits: false,
    causingYellow: false,
    isRaceLeader: false
  };
  markers.set(vehicle.vehicle_id, marker);
  return marker;
};

const renderVehicles = (vehicles: TrackMapVehicle[], trackLength: number): void => {
  const active = new Set<number>();
  const classPositions = new Map<number, number>();
  const classCounts = new Map<string, number>();
  const pulseEnabled = latestPerformanceProfile !== "efficiency";
  const pulsePhase = pulseEnabled ? performance.now() % 900 / 900 * Math.PI * 2 : 0;
  const markerHaloOpacity = pulseEnabled
    ? (0.65 - Math.cos(pulsePhase) * 0.35).toFixed(2)
    : "0.45";
  [...vehicles].sort((a, b) => a.overall_position - b.overall_position).forEach((vehicle) => {
    const key = vehicle.vehicle_class.toUpperCase();
    const position = (classCounts.get(key) ?? 0) + 1;
    classCounts.set(key, position);
    classPositions.set(vehicle.vehicle_id, position);
  });
  for (const vehicle of vehicles) {
    if (vehicle.in_garage) continue;
    active.add(vehicle.vehicle_id);
    let marker = markers.get(vehicle.vehicle_id);
    if (marker && marker.isPlayer !== vehicle.is_player) {
      marker.root.remove();
      markers.delete(vehicle.vehicle_id);
      marker = undefined;
    }
    marker ??= createMarker(vehicle);
    const classPosition = classPositions.get(vehicle.vehicle_id) ?? vehicle.overall_position;
    const isRaceLeader = vehicle.overall_position === 1;
    if (marker.isRaceLeader !== isRaceLeader) {
      marker.isRaceLeader = isRaceLeader;
      marker.root.classList.toggle("is-race-leader", isRaceLeader);
    }
    if (marker.inPits !== vehicle.in_pits) {
      marker.inPits = vehicle.in_pits;
      marker.root.classList.toggle("is-pit", vehicle.in_pits);
    }
    if (marker.causingYellow !== vehicle.causing_yellow) {
      marker.causingYellow = vehicle.causing_yellow;
      marker.root.classList.toggle("is-causing-yellow", vehicle.causing_yellow);
    }
    if (vehicle.causing_yellow && !marker.halo) {
      marker.halo = createMarkerHalo(marker.root, marker.disc);
    }
    if (marker.halo && (marker.isPlayer || vehicle.causing_yellow)) {
      marker.halo.style.opacity = markerHaloOpacity;
    }
    const [x, y] = markerPosition(vehicle, trackLength);
    setMarkerPosition(marker, x, y);
    const color = classColor(vehicle.vehicle_class);
    if (marker.color !== color) {
      marker.color = color;
      marker.disc.style.backgroundColor = color;
    }
    const labelValue = String(classPosition);
    if (marker.labelValue !== labelValue) {
      marker.labelValue = labelValue;
      marker.label.textContent = labelValue;
    }
  }
  for (const [id, marker] of markers) {
    if (!active.has(id)) {
      marker.root.remove();
      markers.delete(id);
    }
  }
};

const render = (frame: TelemetryFrame): void => {
  latestPerformanceProfile = frame.performance_profile;
  latestYellowSectors = frame.track_map_model.yellow_sectors;
  latestPurpleSectors = frame.track_map_model.purple_sectors;
  latestSectorBoundaries = frame.track_map_model.sector_boundaries;
  latestTrackLength = frame.track_length_meters;
  const nextKey = frame.track_map_model.cache_key;
  if (nextKey !== mapKey) {
    mapKey = nextKey;
    learnedPoints = [];
    officialGeometry = null;
    officialDistancePoints = [];
    calibrationObservation = null;
    geometryRetryAfter = 0;
    geometryLoadedKey = "";
    geometryRevision = frame.track_map_model.geometry_revision;
    migrateLegacyLearning(mapKey, frame.track_name, frame.track_length_meters);
    renderTrack();
  } else if (frame.track_map_model.geometry_revision !== geometryRevision) {
    geometryRevision = frame.track_map_model.geometry_revision;
    geometryRetryAfter = 0;
    geometryLoadedKey = "";
    geometryRequestKey = "";
  }
  requestOfficialGeometry(mapKey);
  const player = frame.track_map_vehicles.find((vehicle) => vehicle.is_player);
  calibrateOfficialDistances(player, frame.track_length_meters);
  renderSectorHighlights();
  renderVehicles(frame.track_map_vehicles, frame.track_length_meters);
  renderPitPrediction(
    trackMapSettings.showPitPrediction
      ? frame.track_map_model.pit_prediction_lap_distance
      : null,
    frame.track_length_meters
  );
};

renderTrack();
void listenTelemetry((frame) => renderPerformance.measure(
  () => render(frame), frame.track_map_vehicles.length
));
void listenRuntimeEvent<TrackMapSettings>("trackmap://settings", (settings) => {
  trackMapSettings = settings;
  if (!settings.showPitPrediction) renderPitPrediction(null, 0);
});

if (import.meta.env.DEV && new URLSearchParams(window.location.search).has("preview")) {
  learnedPoints = Array.from({ length: 180 }, (_, index) => {
    const angle = index / 180 * Math.PI * 2;
    const radius = 1_000 + Math.sin(angle * 3) * 210 + Math.cos(angle * 5) * 90;
    return {
      x: Math.cos(angle) * radius,
      y: Math.sin(angle) * radius * 0.62,
      distance: index / 180 * 5_000
    };
  });
  renderTrack();
  const previewVehicles = Array.from({ length: 16 }, (_, index) => {
    const point = learnedPoints[(index * 11 + 8) % learnedPoints.length];
    return {
      vehicle_id: index + 1,
      overall_position: index + 1,
      vehicle_class: ["HYPERCAR", "LMP2", "LMP3", "LMGT3"][index % 4],
      world_x: point.x,
      world_y: point.y,
      lap_distance: point.distance,
      total_laps: 8,
      in_pits: index === 12,
      in_garage: false,
      causing_yellow: index === 4,
      is_player: index === 6
    };
  });
  renderVehicles(previewVehicles, 5_000);
  renderPitPrediction(1_900, 5_000);
}
