import "./styles.css";
import "./minimap.css";
import { fitOverlay } from "./overlay-fit";
import { bindOverlayInteractionMode } from "./overlay-interaction";
import { bindOverlayTransparency } from "./overlay-appearance";
import { createOverlayPerformanceTracker } from "./overlay-performance";
import { listenRuntimeEvent, listenTelemetry } from "./runtime-events";
import {
  classColor,
  classPositions,
  fetchTrackMapGeometry,
  isUsableTrackMapGeometry,
  type GeometryPoint
} from "./track-map-common";
import { normalizeTrackMapSettings, readTrackMapSettings, type TrackMapSettings } from "./trackmap-settings";
import type { TelemetryFrame, TrackMapVehicle } from "./telemetry-types";
import { t } from "./i18n";

interface MarkerView {
  root: HTMLDivElement;
  disc: HTMLDivElement;
  label: HTMLSpanElement;
  isPlayer: boolean;
  transform: string;
  visible: boolean;
  color: string;
  customColor: string | null;
  icon: string;
  labelValue: string;
  inPits: boolean;
  causingYellow: boolean;
}

interface WorldPosition {
  x: number;
  y: number;
}

const SIZE = 260;
const CENTER = SIZE / 2;
/** Metres of circuit between the player and the edge of the disc. */
const VIEW_RADIUS_METERS = 190;
const SCALE = CENTER / VIEW_RADIUS_METERS;
/** Cars this far past the rim are dropped rather than drawn clipped. */
const MARKER_MARGIN = 12;
/** Movement needed before the travel direction is trusted. */
const HEADING_MIN_TRAVEL_METERS = 2;
/** Anything longer between two frames is a teleport, not a direction. */
const HEADING_MAX_TRAVEL_METERS = 80;
const HEADING_SMOOTHING_SECONDS = 0.12;

fitOverlay({ width: 276, height: 276 });
bindOverlayTransparency("minimap");
bindOverlayInteractionMode();
const renderPerformance = createOverlayPerformanceTracker("minimap");

const world = document.querySelector<SVGGElement>("#minimap-world")!;
const outline = document.querySelector<SVGPathElement>("#minimap-track-outline")!;
const line = document.querySelector<SVGPathElement>("#minimap-track-line")!;
const pitOutline = document.querySelector<SVGPathElement>("#minimap-pit-outline")!;
const pitLine = document.querySelector<SVGPathElement>("#minimap-pit-line")!;
const vehicleLayer = document.querySelector<HTMLDivElement>("#minimap-vehicle-layer")!;
const status = document.getElementById("minimap-status") as HTMLElement;

let mapKey = "";
let geometryRequestKey = "";
let geometryLoadedKey = "";
let geometryRetryAfter = 0;
let geometryRevision = -1;
let hasGeometry = false;
let worldTransform = "";
const markers = new Map<number, MarkerView>();
let trackMapSettings = readTrackMapSettings();

// Rotation, in radians, that turns the player's direction of travel to the top
// of the disc. It follows the movement itself because the frame carries no yaw.
let rotation = 0;
let targetRotation: number | null = null;
let headingAnchor: WorldPosition | null = null;
let lastRenderAt = 0;

const wrapAngle = (angle: number): number => {
  const turn = Math.PI * 2;
  return ((angle + Math.PI) % turn + turn) % turn - Math.PI;
};

const resetHeading = (): void => {
  targetRotation = null;
  headingAnchor = null;
};

const updateHeading = (player: WorldPosition, now: number): void => {
  if (!headingAnchor) {
    headingAnchor = player;
  } else {
    const dx = player.x - headingAnchor.x;
    const dy = player.y - headingAnchor.y;
    const travel = Math.hypot(dx, dy);
    if (travel > HEADING_MAX_TRAVEL_METERS) {
      headingAnchor = player;
    } else if (travel >= HEADING_MIN_TRAVEL_METERS) {
      const next = -Math.PI / 2 - Math.atan2(dy, dx);
      if (targetRotation === null) rotation = next;
      targetRotation = next;
      headingAnchor = player;
    }
  }
  if (targetRotation !== null) {
    const elapsed = lastRenderAt ? Math.min(Math.max((now - lastRenderAt) / 1000, 0), 0.5) : 0;
    const blend = 1 - Math.exp(-elapsed / HEADING_SMOOTHING_SECONDS);
    rotation = wrapAngle(rotation + wrapAngle(targetRotation - rotation) * blend);
  }
  lastRenderAt = now;
};

const openPath = (points: readonly GeometryPoint[]): string => points
  .map((point, index) => `${index ? "L" : "M"}${point.x.toFixed(1)} ${point.y.toFixed(1)}`)
  .join(" ");

const clearTrack = (): void => {
  for (const path of [outline, line, pitOutline, pitLine]) path.removeAttribute("d");
  hasGeometry = false;
};

const requestGeometry = (key: string): void => {
  if (!key || geometryLoadedKey === key || geometryRequestKey === key || Date.now() < geometryRetryAfter) return;
  geometryRequestKey = key;
  void fetchTrackMapGeometry(key)
    .then((geometry) => {
      if (mapKey !== key) return;
      if (!isUsableTrackMapGeometry(geometry)) throw new Error("Geometria del circuito incompleta");
      const main = `${openPath(geometry.mainPath)} Z`;
      outline.setAttribute("d", main);
      line.setAttribute("d", main);
      // Only the official geometry carries the pitlane; learned maps never invent one.
      const pit = geometry.source === "official" && geometry.pitPath.length >= 2
        ? openPath(geometry.pitPath)
        : "";
      for (const path of [pitOutline, pitLine]) {
        if (pit) path.setAttribute("d", pit);
        else path.removeAttribute("d");
      }
      hasGeometry = true;
      geometryLoadedKey = key;
    })
    .catch(() => {
      if (mapKey === key) geometryRetryAfter = Date.now() + 10_000;
    })
    .finally(() => {
      if (geometryRequestKey === key) geometryRequestKey = "";
    });
};

const toScreen = (x: number, y: number, origin: WorldPosition): [number, number] => {
  const dx = (x - origin.x) * SCALE;
  const dy = (y - origin.y) * SCALE;
  const cos = Math.cos(rotation);
  const sin = Math.sin(rotation);
  return [CENTER + dx * cos - dy * sin, CENTER + dx * sin + dy * cos];
};

const renderWorld = (origin: WorldPosition | null): void => {
  const next = origin
    ? `translate(${CENTER} ${CENTER}) rotate(${(rotation * 180 / Math.PI).toFixed(2)}) scale(${SCALE.toFixed(5)}) translate(${(-origin.x).toFixed(2)} ${(-origin.y).toFixed(2)})`
    : "";
  if (next === worldTransform) return;
  worldTransform = next;
  if (next) {
    world.setAttribute("transform", next);
    world.removeAttribute("visibility");
  } else {
    world.setAttribute("visibility", "hidden");
  }
};

const setStatus = (key: "minimap.learning" | "minimap.waiting" | null): void => {
  if (!key) {
    status.hidden = true;
    return;
  }
  const text = t(key);
  if (status.textContent !== text) status.textContent = text;
  status.hidden = false;
};

const createMarker = (vehicle: TrackMapVehicle): MarkerView => {
  const root = document.createElement("div");
  root.classList.add("minimap-marker");
  if (vehicle.is_player) root.classList.add("is-player");
  const disc = document.createElement("div");
  disc.classList.add("minimap-disc");
  const label = document.createElement("span");
  label.classList.add("minimap-label");
  disc.appendChild(label);
  root.appendChild(disc);
  vehicleLayer.appendChild(root);
  const marker: MarkerView = {
    root,
    disc,
    label,
    isPlayer: vehicle.is_player,
    transform: "",
    visible: true,
    color: "",
    customColor: null,
    icon: "",
    labelValue: "",
    inPits: false,
    causingYellow: false
  };
  markers.set(vehicle.vehicle_id, marker);
  return marker;
};

const setMarkerVisible = (marker: MarkerView, visible: boolean): void => {
  if (marker.visible === visible) return;
  marker.visible = visible;
  marker.root.hidden = !visible;
};

const applyPlayerIcon = (marker: MarkerView, icon: string): void => {
  if (marker.icon === icon) return;
  marker.icon = icon;
  const image = marker.disc.querySelector<HTMLImageElement>(".minimap-player-icon");
  if (icon) {
    const playerImage = image ?? document.createElement("img");
    playerImage.className = "minimap-player-icon";
    playerImage.alt = "";
    playerImage.draggable = false;
    playerImage.src = icon;
    if (!image) marker.disc.prepend(playerImage);
  } else {
    image?.remove();
  }
  marker.root.classList.toggle("has-custom-icon", icon !== "");
};

const renderVehicles = (vehicles: TrackMapVehicle[], origin: WorldPosition | null): void => {
  const active = new Set<number>();
  const positions = classPositions(vehicles);
  const limit = CENTER + MARKER_MARGIN;
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
    // The player is pinned to the centre; everyone else is placed around them
    // and only while the rim can still show them.
    let x = CENTER;
    let y = CENTER;
    if (!vehicle.is_player) {
      if (!origin || !vehicle.world_position_available) {
        setMarkerVisible(marker, false);
        continue;
      }
      [x, y] = toScreen(vehicle.world_x, vehicle.world_y, origin);
      if (Math.hypot(x - CENTER, y - CENTER) > limit) {
        setMarkerVisible(marker, false);
        continue;
      }
    } else if (!origin) {
      setMarkerVisible(marker, false);
      continue;
    }
    setMarkerVisible(marker, true);
    const transform = `translate3d(${x.toFixed(1)}px, ${y.toFixed(1)}px, 0)`;
    if (marker.transform !== transform) {
      marker.transform = transform;
      marker.root.style.transform = transform;
    }
    if (marker.inPits !== vehicle.in_pits) {
      marker.inPits = vehicle.in_pits;
      marker.root.classList.toggle("is-pit", vehicle.in_pits);
    }
    if (marker.causingYellow !== vehicle.causing_yellow) {
      marker.causingYellow = vehicle.causing_yellow;
      marker.root.classList.toggle("is-causing-yellow", vehicle.causing_yellow);
    }
    const customColor = marker.isPlayer ? trackMapSettings.playerColor : null;
    const color = customColor ?? classColor(vehicle.vehicle_class);
    if (marker.color !== color) {
      marker.color = color;
      marker.disc.style.backgroundColor = color;
    }
    if (marker.customColor !== customColor) {
      marker.customColor = customColor;
      marker.root.classList.toggle("has-custom-color", customColor !== null);
    }
    if (marker.isPlayer) applyPlayerIcon(marker, trackMapSettings.playerIconDataUrl ?? "");
    const labelValue = String(positions.get(vehicle.vehicle_id) ?? vehicle.overall_position);
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
  const nextKey = frame.track_map_model.cache_key;
  if (nextKey !== mapKey) {
    mapKey = nextKey;
    geometryRetryAfter = 0;
    geometryLoadedKey = "";
    geometryRevision = frame.track_map_model.geometry_revision;
    clearTrack();
    resetHeading();
  } else if (frame.track_map_model.geometry_revision !== geometryRevision) {
    geometryRevision = frame.track_map_model.geometry_revision;
    geometryRetryAfter = 0;
    geometryLoadedKey = "";
    geometryRequestKey = "";
  }
  requestGeometry(mapKey);
  const player = frame.track_map_vehicles.find((vehicle) => vehicle.is_player && !vehicle.in_garage);
  const origin = player?.world_position_available ? { x: player.world_x, y: player.world_y } : null;
  if (origin) updateHeading(origin, performance.now());
  else resetHeading();
  renderWorld(hasGeometry ? origin : null);
  renderVehicles(frame.track_map_vehicles, origin);
  setStatus(!origin ? "minimap.waiting" : hasGeometry ? null : "minimap.learning");
};

setStatus("minimap.waiting");
renderWorld(null);
void listenTelemetry((frame) => renderPerformance.measure(
  () => render(frame), frame.track_map_vehicles.length
));
void listenRuntimeEvent<TrackMapSettings>("trackmap://settings", (settings) => {
  trackMapSettings = normalizeTrackMapSettings(settings);
});

if (import.meta.env.DEV && new URLSearchParams(window.location.search).has("preview")) {
  const trackLength = 5_000;
  const trackPoints = Array.from({ length: 360 }, (_, index) => {
    const angle = index / 360 * Math.PI * 2;
    const radius = 1_000 + Math.sin(angle * 3) * 210 + Math.cos(angle * 5) * 90;
    return { x: Math.cos(angle) * radius, y: Math.sin(angle) * radius * 0.62, distance: index / 360 * trackLength };
  });
  const main = `${openPath(trackPoints)} Z`;
  outline.setAttribute("d", main);
  line.setAttribute("d", main);
  hasGeometry = true;
  const vehicleAt = (index: number, offset: number, vehicleClass: string, isPlayer: boolean): TrackMapVehicle => {
    const point = trackPoints[(offset + trackPoints.length) % trackPoints.length];
    return {
      vehicle_id: index + 1,
      overall_position: index + 1,
      vehicle_class: vehicleClass,
      world_x: point.x,
      world_y: point.y,
      world_position_available: true,
      lap_distance: point.distance,
      total_laps: 8,
      in_pits: false,
      in_garage: false,
      causing_yellow: index === 3,
      is_player: isPlayer
    };
  };
  const playerIndex = 40;
  const previewVehicles = [
    vehicleAt(0, playerIndex + 6, "HYPERCAR", false),
    vehicleAt(1, playerIndex + 3, "LMP2", false),
    vehicleAt(2, playerIndex, "HYPERCAR", true),
    vehicleAt(3, playerIndex - 4, "LMGT3", false),
    vehicleAt(4, playerIndex - 9, "LMP2", false),
    vehicleAt(5, playerIndex + 60, "LMGT3", false)
  ];
  // Two samples along the lap settle the heading before the first paint.
  const previous = trackPoints[playerIndex - 1];
  updateHeading({ x: previous.x, y: previous.y }, 0);
  const current = trackPoints[playerIndex];
  headingAnchor = { x: previous.x, y: previous.y };
  updateHeading({ x: current.x, y: current.y }, 0);
  const origin = { x: current.x, y: current.y };
  renderWorld(origin);
  renderVehicles(previewVehicles, origin);
  setStatus(null);
}
