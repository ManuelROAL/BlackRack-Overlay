import "./styles.css";
import "./trackmap.css";
import { fitOverlay } from "./overlay-fit";
import { bindOverlayInteractionMode } from "./overlay-interaction";
import { bindOverlayTransparency } from "./overlay-appearance";
import { createOverlayPerformanceTracker } from "./overlay-performance";
import { invokeRuntime, isTauriRuntime, listenTelemetry } from "./runtime-events";
import type { TelemetryFrame, TrackMapVehicle } from "./telemetry-types";

interface MapPoint {
  x: number;
  y: number;
  distance: number;
}

interface StoredMap {
  version: 1;
  trackName: string;
  trackLength: number;
  points: MapPoint[];
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
  mainPath: GeometryPoint[];
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

interface PitTraversalStore {
  version: 1;
  samples: number[];
}

interface PitVehicleState {
  inPits: boolean;
  eligible: boolean;
  lastDistance: number;
  movingSeconds: number;
  pendingSeconds: number;
  pitStartProgress: number | null;
  maxPitProgressDelta: number;
}

type PitPredictionFrame = Pick<TelemetryFrame,
  "pit_stop_estimate_available" | "pit_stop_estimate_seconds" |
  "last_lap_seconds" | "best_lap_seconds" | "track_length_meters" |
  "player_in_pits">;

const SVG_NS = "http://www.w3.org/2000/svg";
const SIZE = 420;
const MARGIN = 34;
const MIN_SAMPLE_DISTANCE = 3;
const STORAGE_PREFIX = "lmu-overlay.track-map.v1.";
const PIT_STORAGE_PREFIX = "lmu-overlay.track-map.pit-traversal.v1.";
const MAX_PIT_SAMPLES = 7;

fitOverlay({ width: 436, height: 436 });
bindOverlayTransparency("trackmap");
bindOverlayInteractionMode();
const renderPerformance = createOverlayPerformanceTracker("trackmap");

const outline = document.querySelector<SVGPathElement>("#track-outline")!;
const line = document.querySelector<SVGPathElement>("#track-line")!;
const startLine = document.querySelector<SVGPathElement>("#start-line")!;
const vehicleLayer = document.querySelector<SVGGElement>("#vehicle-layer")!;
const status = document.getElementById("map-status") as HTMLElement;

let mapKey = "";
let learnedPoints: MapPoint[] = [];
let officialGeometry: PreparedGeometry | null = null;
let officialDistancePoints: MapPoint[] = [];
let calibrationObservation: CalibrationObservation | null = null;
let geometryRequestKey = "";
let geometryRetryAfter = 0;
let transform: Transform | null = null;
let recordingLap: number | null = null;
let recordingValid = true;
let samples: MapPoint[] = [];
let lastSampleDistance = -Infinity;
const markers = new Map<number, SVGGElement>();
const pitVehicleStates = new Map<number, PitVehicleState>();
let pitTraversalSamples: number[] = [];
let pitTraversalSeconds = 0;
let previousPitSampleTime: number | null = null;
let predictionMarker: SVGGElement | null = null;

const safeKey = (trackName: string, trackLength: number): string =>
  `${STORAGE_PREFIX}${trackName.trim().toLowerCase().replace(/[^a-z0-9]+/g, "-")}.${Math.round(trackLength)}`;

const pitStorageKey = (trackName: string, trackLength: number): string =>
  `${PIT_STORAGE_PREFIX}${trackName.trim().toLowerCase().replace(/[^a-z0-9]+/g, "-")}.${Math.round(trackLength)}`;

const median = (values: number[]): number => {
  if (!values.length) return 0;
  const sorted = [...values].sort((a, b) => a - b);
  const middle = Math.floor(sorted.length / 2);
  return sorted.length % 2 ? sorted[middle] : (sorted[middle - 1] + sorted[middle]) / 2;
};

const normalizeDistance = (distance: number, length: number): number =>
  length > 0 ? ((distance % length) + length) % length : 0;

const wrappedDelta = (next: number, previous: number, length: number): number => {
  let delta = next - previous;
  if (length > 0 && delta > length / 2) delta -= length;
  if (length > 0 && delta < -length / 2) delta += length;
  return delta;
};

const prepareGeometry = (geometry: TrackMapGeometry): PreparedGeometry | null => {
  if (geometry.mainPath.length < 40 || geometry.pitPath.length < 2) return null;
  const mainPath: MapPoint[] = [];
  let distance = 0;
  for (let index = 0; index < geometry.mainPath.length; index += 1) {
    const point = geometry.mainPath[index];
    if (!Number.isFinite(point.x) || !Number.isFinite(point.y)) return null;
    if (index) {
      const previous = geometry.mainPath[index - 1];
      distance += Math.hypot(point.x - previous.x, point.y - previous.y);
    }
    mainPath.push({ x: point.x, y: point.y, distance });
  }
  const first = mainPath[0];
  const last = mainPath[mainPath.length - 1];
  const mainLength = distance + Math.hypot(first.x - last.x, first.y - last.y);
  if (!Number.isFinite(mainLength) || mainLength < 100) return null;
  const pitPath = geometry.pitPath.filter((point) => Number.isFinite(point.x) && Number.isFinite(point.y));
  return pitPath.length >= 2 ? { mainPath, mainLength, pitPath } : null;
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
  if (!key || geometryRequestKey === key || Date.now() < geometryRetryAfter) return;
  geometryRequestKey = key;
  void fetchOfficialGeometry(key)
    .then((geometry) => {
      if (mapKey !== key) return;
      const prepared = prepareGeometry(geometry);
      if (!prepared) throw new Error("Geometria oficial incompleta");
      officialGeometry = prepared;
      officialDistancePoints = [];
      calibrationObservation = null;
      pitVehicleStates.clear();
      renderTrack();
    })
    .catch(() => {
      if (mapKey === key) geometryRetryAfter = Date.now() + 10_000;
    })
    .finally(() => {
      if (geometryRequestKey === key) geometryRequestKey = "";
    });
};

const loadPitTraversal = (trackName: string, trackLength: number): void => {
  try {
    const stored = JSON.parse(localStorage.getItem(pitStorageKey(trackName, trackLength)) ?? "null") as PitTraversalStore | null;
    pitTraversalSamples = stored?.version === 1
      ? stored.samples.filter((value) => Number.isFinite(value) && value >= 5 && value <= 180).slice(-MAX_PIT_SAMPLES)
      : [];
  } catch {
    pitTraversalSamples = [];
  }
  pitTraversalSeconds = median(pitTraversalSamples);
  pitVehicleStates.clear();
  previousPitSampleTime = null;
};

const savePitTraversal = (trackName: string, trackLength: number, seconds: number): void => {
  if (!Number.isFinite(seconds) || seconds < 5 || seconds > 180) return;
  pitTraversalSamples.push(seconds);
  pitTraversalSamples = pitTraversalSamples.slice(-MAX_PIT_SAMPLES);
  pitTraversalSeconds = median(pitTraversalSamples);
  const stored: PitTraversalStore = { version: 1, samples: pitTraversalSamples };
  localStorage.setItem(pitStorageKey(trackName, trackLength), JSON.stringify(stored));
};

const loadMap = (key: string): MapPoint[] => {
  try {
    const value = JSON.parse(localStorage.getItem(key) ?? "null") as StoredMap | null;
    return value?.version === 1 && value.points.length >= 40 ? value.points : [];
  } catch {
    localStorage.removeItem(key);
    return [];
  }
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

const renderTrack = (): void => {
  const displayPoints = officialGeometry?.mainPath ?? learnedPoints;
  if (displayPoints.length >= 40) {
    transform = makeTransform(displayPoints);
    const path = pointPath(displayPoints, transform);
    outline.setAttribute("d", path);
    line.setAttribute("d", path);
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
    startLine.setAttribute("d", `M${SIZE / 2 - 8} ${MARGIN} L${SIZE / 2 + 8} ${MARGIN}`);
    status.hidden = false;
  }
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

const saveLearnedMap = (frame: TelemetryFrame): void => {
  learnedPoints = samples.slice();
  const stored: StoredMap = {
    version: 1,
    trackName: frame.track_name,
    trackLength: frame.track_length_meters,
    points: learnedPoints
  };
  localStorage.setItem(mapKey, JSON.stringify(stored));
  renderTrack();
};

const updateRecorder = (frame: TelemetryFrame, player: TrackMapVehicle | undefined): void => {
  if (!player || officialGeometry || learnedPoints.length || frame.track_length_meters <= 100) return;
  if (recordingLap === null) recordingLap = player.total_laps;
  if (player.total_laps !== recordingLap) {
    const coverage = samples.length > 1
      ? samples[samples.length - 1].distance - samples[0].distance
      : 0;
    if (recordingValid && frame.last_lap_seconds > 0
      && samples.length >= 40 && coverage >= frame.track_length_meters * 0.88) {
      saveLearnedMap(frame);
    }
    recordingLap = player.total_laps;
    recordingValid = true;
    samples = [];
    lastSampleDistance = -Infinity;
  }
  recordingValid &&= frame.player_lap_valid && !player.in_pits;
  if (player.lap_distance >= lastSampleDistance + MIN_SAMPLE_DISTANCE) {
    samples.push({ x: player.world_x, y: player.world_y, distance: player.lap_distance });
    lastSampleDistance = player.lap_distance;
  }
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

const pitProgress = (vehicle: TrackMapVehicle): number | null => {
  const path = officialGeometry?.pitPath;
  if (!path || path.length < 2) return null;
  let bestIndex = 0;
  let bestSquared = Number.POSITIVE_INFINITY;
  for (let index = 0; index < path.length; index += 1) {
    const dx = vehicle.world_x - path[index].x;
    const dy = vehicle.world_y - path[index].y;
    const squared = dx * dx + dy * dy;
    if (squared < bestSquared) {
      bestSquared = squared;
      bestIndex = index;
    }
  }
  return bestSquared <= 30 * 30 ? bestIndex / (path.length - 1) : null;
};

const completedOfficialPitPassage = (
  state: PitVehicleState,
  exitProgress: number | null
): boolean => {
  if (!officialGeometry) return true;
  if (state.pitStartProgress === null || exitProgress === null) return false;
  const startNearEndpoint = Math.min(state.pitStartProgress, 1 - state.pitStartProgress) <= 0.25;
  const exitNearEndpoint = Math.min(exitProgress, 1 - exitProgress) <= 0.25;
  return startNearEndpoint && exitNearEndpoint
    && Math.abs(exitProgress - state.pitStartProgress) >= 0.5
    && state.maxPitProgressDelta >= 0.5;
};

const updatePitTraversal = (frame: TelemetryFrame): void => {
  const now = frame.session_elapsed_seconds;
  if (previousPitSampleTime !== null && now < previousPitSampleTime) pitVehicleStates.clear();
  const deltaSeconds = previousPitSampleTime !== null && now >= previousPitSampleTime
    ? Math.min(now - previousPitSampleTime, 0.25)
    : 0;
  previousPitSampleTime = now;
  const active = new Set<number>();

  for (const vehicle of frame.track_map_vehicles) {
    active.add(vehicle.vehicle_id);
    const previous = pitVehicleStates.get(vehicle.vehicle_id);
    if (!previous) {
      pitVehicleStates.set(vehicle.vehicle_id, {
        inPits: vehicle.in_pits,
        eligible: !vehicle.in_pits,
        lastDistance: vehicle.lap_distance,
        movingSeconds: 0,
        pendingSeconds: 0,
        pitStartProgress: null,
        maxPitProgressDelta: 0
      });
      continue;
    }

    if (!previous.inPits && vehicle.in_pits) {
      previous.eligible = true;
      previous.movingSeconds = 0;
      previous.pendingSeconds = 0;
      previous.pitStartProgress = pitProgress(vehicle);
      previous.maxPitProgressDelta = 0;
    } else if (previous.inPits && vehicle.in_pits && previous.eligible && deltaSeconds > 0) {
      const progress = pitProgress(vehicle);
      if (previous.pitStartProgress !== null && progress !== null) {
        previous.maxPitProgressDelta = Math.max(
          previous.maxPitProgressDelta,
          Math.abs(progress - previous.pitStartProgress)
        );
      }
      previous.pendingSeconds = Math.min(previous.pendingSeconds + deltaSeconds, 0.5);
      const rawDelta = vehicle.lap_distance - previous.lastDistance;
      const distanceDelta = frame.track_length_meters > 0
        ? ((rawDelta % frame.track_length_meters) + frame.track_length_meters) % frame.track_length_meters
        : Math.max(rawDelta, 0);
      if (distanceDelta >= 0.1 && distanceDelta <= 8) {
        previous.movingSeconds += previous.pendingSeconds;
        previous.pendingSeconds = 0;
      }
    } else if (previous.inPits && !vehicle.in_pits) {
      if (previous.eligible && completedOfficialPitPassage(previous, pitProgress(vehicle))) {
        savePitTraversal(
          frame.track_name,
          frame.track_length_meters,
          previous.movingSeconds + Math.min(previous.pendingSeconds, 0.5)
        );
      }
      previous.eligible = true;
      previous.movingSeconds = 0;
      previous.pendingSeconds = 0;
      previous.pitStartProgress = null;
      previous.maxPitProgressDelta = 0;
    }
    previous.inPits = vehicle.in_pits;
    previous.lastDistance = vehicle.lap_distance;
  }

  for (const id of pitVehicleStates.keys()) {
    if (!active.has(id)) pitVehicleStates.delete(id);
  }
};

const ensurePredictionMarker = (): SVGGElement => {
  if (predictionMarker) return predictionMarker;
  const group = document.createElementNS(SVG_NS, "g");
  group.classList.add("pit-prediction-marker");
  const title = document.createElementNS(SVG_NS, "title");
  title.textContent = "Salida estimada tras la parada";
  group.appendChild(title);
  const disc = document.createElementNS(SVG_NS, "circle");
  disc.setAttribute("r", "9");
  group.appendChild(disc);
  const label = document.createElementNS(SVG_NS, "text");
  label.textContent = "P";
  group.appendChild(label);
  vehicleLayer.prepend(group);
  predictionMarker = group;
  return group;
};

const renderPitPrediction = (frame: PitPredictionFrame, player: TrackMapVehicle | undefined): void => {
  const pace = frame.last_lap_seconds > 0 ? frame.last_lap_seconds : frame.best_lap_seconds;
  const available = Boolean(player)
    && !frame.player_in_pits
    && frame.pit_stop_estimate_available
    && frame.pit_stop_estimate_seconds >= 0
    && pitTraversalSeconds > 0
    && pace > 0
    && frame.track_length_meters > 0;
  if (!available || !player) {
    if (predictionMarker) predictionMarker.setAttribute("hidden", "");
    return;
  }
  const totalSeconds = frame.pit_stop_estimate_seconds + pitTraversalSeconds;
  const predictedDistance = player.lap_distance
    - totalSeconds / pace * frame.track_length_meters;
  const [x, y] = positionAtLapDistance(predictedDistance, frame.track_length_meters);
  const marker = ensurePredictionMarker();
  marker.removeAttribute("hidden");
  marker.style.transform = `translate(${x.toFixed(2)}px, ${y.toFixed(2)}px)`;
};

const classColor = (vehicleClass: string): string => {
  const value = vehicleClass.toUpperCase();
  if (value.includes("HYPER") || value.includes("GTP")) return "#e33b3b";
  if (value.includes("LMP2")) return "#3988ed";
  if (value.includes("LMP3")) return "#8065ed";
  if (value.includes("GT3")) return "#35c969";
  return "#d8dde2";
};

const createMarker = (vehicle: TrackMapVehicle): SVGGElement => {
  const group = document.createElementNS(SVG_NS, "g");
  group.classList.add("vehicle-marker");
  const leaderStar = document.createElementNS(SVG_NS, "path");
  leaderStar.classList.add("race-leader-star");
  leaderStar.setAttribute("d", "M0-25 1.9-20.9 6.4-20.4 3.1-17.2 4-12.8 0-15 -4-12.8 -3.1-17.2 -6.4-20.4 -1.9-20.9Z");
  group.appendChild(leaderStar);
  if (vehicle.is_player) {
    group.classList.add("is-player");
    const halo = document.createElementNS(SVG_NS, "circle");
    halo.classList.add("player-halo");
    halo.setAttribute("r", "17");
    group.appendChild(halo);
  }
  const disc = document.createElementNS(SVG_NS, "circle");
  disc.classList.add("vehicle-disc");
  disc.setAttribute("r", vehicle.is_player ? "12" : "10");
  group.appendChild(disc);
  const label = document.createElementNS(SVG_NS, "text");
  label.classList.add("vehicle-label");
  group.appendChild(label);
  vehicleLayer.appendChild(group);
  markers.set(vehicle.vehicle_id, group);
  return group;
};

const renderVehicles = (vehicles: TrackMapVehicle[], trackLength: number): void => {
  const active = new Set<number>();
  const classPositions = new Map<number, number>();
  const classCounts = new Map<string, number>();
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
    if (marker && marker.classList.contains("is-player") !== vehicle.is_player) {
      marker.remove();
      markers.delete(vehicle.vehicle_id);
      marker = undefined;
    }
    marker ??= createMarker(vehicle);
    const classPosition = classPositions.get(vehicle.vehicle_id) ?? vehicle.overall_position;
    marker.classList.toggle("is-race-leader", vehicle.overall_position === 1);
    const pitState = vehicle.in_pits ? "1" : "0";
    if (marker.dataset.pit !== pitState) {
      marker.dataset.pit = pitState;
      marker.classList.toggle("is-pit", vehicle.in_pits);
    }
    const [x, y] = markerPosition(vehicle, trackLength);
    marker.style.transform = `translate(${x.toFixed(2)}px, ${y.toFixed(2)}px)`;
    const disc = marker.querySelector<SVGCircleElement>(".vehicle-disc");
    const color = classColor(vehicle.vehicle_class);
    if (disc && disc.style.fill !== color) disc.style.fill = color;
    const label = marker.querySelector<SVGTextElement>(".vehicle-label");
    const labelValue = String(classPosition);
    if (label && label.textContent !== labelValue) label.textContent = labelValue;
  }
  for (const [id, marker] of markers) {
    if (!active.has(id)) {
      marker.remove();
      markers.delete(id);
    }
  }
};

const render = (frame: TelemetryFrame): void => {
  const nextKey = safeKey(frame.track_name, frame.track_length_meters);
  if (nextKey !== mapKey) {
    mapKey = nextKey;
    learnedPoints = loadMap(mapKey);
    officialGeometry = null;
    officialDistancePoints = [];
    calibrationObservation = null;
    geometryRetryAfter = 0;
    loadPitTraversal(frame.track_name, frame.track_length_meters);
    recordingLap = null;
    recordingValid = true;
    samples = [];
    lastSampleDistance = -Infinity;
    renderTrack();
  }
  requestOfficialGeometry(mapKey);
  const player = frame.track_map_vehicles.find((vehicle) => vehicle.is_player);
  calibrateOfficialDistances(player, frame.track_length_meters);
  updatePitTraversal(frame);
  updateRecorder(frame, player);
  renderVehicles(frame.track_map_vehicles, frame.track_length_meters);
  renderPitPrediction(frame, player);
};

renderTrack();
void listenTelemetry((frame) => renderPerformance.measure(
  () => render(frame), frame.track_map_vehicles.length
));

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
      is_player: index === 6
    };
  });
  renderVehicles(previewVehicles, 5_000);
  pitTraversalSeconds = 29;
  renderPitPrediction({
    pit_stop_estimate_available: true,
    pit_stop_estimate_seconds: 18,
    last_lap_seconds: 98,
    best_lap_seconds: 96,
    track_length_meters: 5_000,
    player_in_pits: false
  }, previewVehicles.find((vehicle) => vehicle.is_player));
}
