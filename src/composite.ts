import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import "./composite.css";
import { applyTranslations, getLocale, t } from "./i18n";
import { installFrontendDiagnostics } from "./frontend-diagnostics";
import {
  COMPOSITE_LAYOUT_KEY,
  clampPanelCoordinate,
  ensureCompositeLayout,
  readCompositeLayout,
  saveOverlayPlacement,
  type CompositeLayout,
  type OverlayPlacement
} from "./composite-layout";
import type { OverlayId } from "./overlay-appearance";
import type { InteractionMode, TelemetryFrame } from "./telemetry-types";

installFrontendDiagnostics("composite", (diagnostic) =>
  invoke("record_frontend_error", { ...diagnostic })
);
applyTranslations();

interface OverlayState {
  label: OverlayId;
  visible: boolean;
}

interface RuntimeMessage {
  source: "blackrack-overlay-composite";
  kind: "event" | "invoke" | "fit";
  event?: string;
  payload?: unknown;
  command?: string;
  args?: Record<string, unknown>;
  requestId?: string;
}

interface OverlayDesignSize {
  width: number;
  height: number;
}

interface OverlayInteractionRegion {
  x: number;
  y: number;
  width: number;
  height: number;
}

interface OverlayHostViewport {
  width: number;
  height: number;
  scaleFactor: number;
}

interface TelemetryBatch {
  targets: OverlayId[];
  frame: TelemetryFrame;
}

const overlayIds: OverlayId[] = [
  "delta", "timing", "stinthistory", "driving", "liftcoast", "tires", "damage", "standings",
  "relative", "fuel", "pitstop", "flags", "rejoin", "trackmap",
  "forecast", "conditions", "dashboard"
];
// Projected into every overlay on top of its own allowlist: an overlay has to
// know what the active simulator can report before it decides what to draw.
const sharedTelemetryFields: readonly (keyof TelemetryFrame)[] = ["capabilities"];

const telemetryFields: Record<OverlayId, readonly (keyof TelemetryFrame)[]> = {
  delta: ["delta_model"],
  timing: ["timing_model"],
  stinthistory: ["stint_history_model"],
  driving: [
    "speed_kph", "gear", "throttle", "brake", "tc_active", "abs_active",
    "steering_angle_degrees", "force_feedback", "rpm", "max_rpm"
  ],
  liftcoast: ["lift_and_coast_progress"],
  tires: [
    "player_damage_percent", "player_aero_damage_percent", "player_damage_severity",
    "player_engine_overheating", "player_engine_oil_temperature_c",
    "player_engine_water_temperature_c",
    "player_part_detached", "player_rear_wing_detached", "player_tire_temperature_c",
    "player_tire_temperature_by_zone_c",
    "player_brake_temperature_c", "player_tire_remaining_by_wheel_percent",
    "player_tire_flat_spot_percent", "player_tire_compounds",
    "player_tire_optimal_temperature_c", "player_tire_flat",
    "player_tire_detached", "player_suspension_damage_by_wheel_percent"
  ],
  damage: [
    "player_aero_damage_percent", "player_body_damage_percent",
    "player_suspension_damage_percent", "player_tire_remaining_by_wheel_percent"
  ],
  standings: [
    "session_type", "session_max_laps", "session_time_remaining", "game_time_of_day_seconds", "session_max_time_seconds",
    "session_split_number", "session_split_count", "rest_weather_available", "ambient_temperature_c",
    "track_temperature_c", "player_total_laps", "brake_bias_percent", "track_limits_steps",
    "track_limits_steps_per_penalty", "session_total_laps_estimated", "standings_model",
    "standings"
  ],
  relative: [
    "session_type",
    "game_time_of_day_seconds", "rest_weather_available", "ambient_temperature_c", "track_temperature_c",
    "brake_bias_percent", "track_limits_steps", "track_limits_steps_per_penalty",
    "relative_model", "standings"
  ],
  fuel: [
    "connected", "player_active",
    "fuel_liters", "fuel_capacity_liters", "fuel_per_lap", "fuel_last_lap",
    "fuel_qualifying_lap", "fuel_reference_per_lap", "fuel_projected_lap",
    "fuel_ratio_assigned", "fuel_ratio_average", "fuel_ratio_last",
    "virtual_energy_active", "virtual_energy_percent",
    "virtual_energy_per_lap", "virtual_energy_last_lap", "virtual_energy_qualifying_lap",
    "fuel_strategies"
  ],
  pitstop: [
    "virtual_energy_active", "pit_stop_estimate_available", "pit_stop_estimate_seconds",
    "pit_stop_fuel_seconds", "pit_stop_energy_seconds", "pit_stop_tire_seconds",
    "pit_stop_damage_seconds", "pit_stop_penalty_seconds", "pit_stop_driver_swap_seconds"
  ],
  flags: ["flag_warning"],
  rejoin: ["rejoin_warning"],
  trackmap: ["performance_profile", "track_name", "track_length_meters", "track_map_vehicles", "track_map_model"],
  forecast: ["rest_weather_available", "ambient_temperature_c", "rain_percent", "cloud_coverage", "weather_forecast"],
  conditions: [
    "rest_weather_available", "ambient_temperature_c", "track_temperature_c",
    "rain_percent", "track_wetness_percent", "wind_speed_ms", "wind_direction_degrees",
    "wind_relative_direction_degrees",
    "player_grip_percent", "track_rubber_percent", "track_grip_state", "cloud_coverage", "current_humidity_percent",
    "weather_forecast"
  ],
  dashboard: [
    "player_active", "gear", "speed_kph", "rpm", "max_rpm",
    "player_position", "player_class_position",
    "lap_number", "session_max_laps", "session_time_remaining",
    "delta_model", "timing_model",
    "fuel_liters", "estimated_fuel_laps",
    "virtual_energy_active", "virtual_energy_percent", "estimated_virtual_energy_laps",
    "hybrid_available", "battery_charge_percent",
    "car_electronics_available", "engine_map", "engine_map_max",
    "traction_control_level", "traction_control_max",
    "anti_lock_brakes_level", "anti_lock_brakes_max", "brake_bias_percent",
    "ambient_temperature_c", "track_temperature_c"
  ]
};
const overlayTitleKeys: Record<OverlayId, import("./i18n").TranslationKey> = {
  delta: "card.delta", timing: "card.timing", stinthistory: "card.stintHistory", driving: "card.driving", liftcoast: "card.liftcoast", tires: "card.tires",
  damage: "card.damage", standings: "card.standings", relative: "card.relative", fuel: "card.fuel",
  pitstop: "card.pitstop", flags: "card.flags", rejoin: "card.rejoin", trackmap: "card.trackmap",
  forecast: "card.forecast", conditions: "card.conditions", dashboard: "card.dashboard"
};
const overlayTitle = (overlay: OverlayId): string => t(overlayTitleKeys[overlay]).toLocaleUpperCase(getLocale());

// Overlay iframes reach the Tauri backend only through this host. Restrict the
// bridge to the read-only/diagnostic commands the overlays actually use so a
// single compromised overlay document cannot drive the whole command surface.
const BRIDGE_COMMANDS: ReadonlySet<string> = new Set([
  "record_frontend_error",
  "record_frontend_performance",
  "get_interaction_mode",
  "get_telemetry_logging",
  "get_track_map_geometry",
  "migrate_legacy_track_map_learning"
]);

const stage = document.getElementById("overlay-stage") as HTMLElement;
const panels = new Map<OverlayId, HTMLElement>();
const frames = new Map<OverlayId, HTMLIFrameElement>();
const latestFrames = new Map<OverlayId, TelemetryFrame>();
const projectedFrames = new Map<OverlayId, TelemetryFrame>();
const designSizes = new Map<OverlayId, OverlayDesignSize>();
const visible = new Set<OverlayId>();
const placements = new Map<OverlayId, OverlayPlacement>();
let clickThrough = false;
// The monitor in CSS pixels. While the host covers the display this is its own
// viewport, but once the host shrinks to the panels the viewport stops
// describing the surface the layout is placed on, so the backend owns it.
let monitorViewport: OverlayDesignSize = {
  width: window.innerWidth,
  height: window.innerHeight
};
// Margin kept around the panels so panel shadows and the edit chrome border are
// never clipped by the host edge.
const HOST_BOUNDS_MARGIN = 24;
let appliedHostBounds: OverlayInteractionRegion | null = null;
let hostBoundsPending: Promise<unknown> | null = null;
let interactionRegionFrame: number | undefined;
let interactionRegionSyncing = false;
let interactionRegionDirty = false;

const synchronizeInteractionRegions = (): void => {
  interactionRegionFrame = undefined;
  if (interactionRegionSyncing) return;
  interactionRegionDirty = false;
  // Convert the CSS-pixel panel boxes to physical pixels with the WebView's own
  // devicePixelRatio so the native hit-test matches what is actually rendered
  // regardless of the display scale factor.
  const dpr = window.devicePixelRatio || 1;
  const regions: OverlayInteractionRegion[] = Array.from(panels.values(), (panel) => {
    const bounds = panel.getBoundingClientRect();
    return {
      x: bounds.x * dpr,
      y: bounds.y * dpr,
      width: bounds.width * dpr,
      height: bounds.height * dpr
    };
  });
  interactionRegionSyncing = true;
  void invoke("set_overlay_interaction_regions", { regions })
    .catch(() => undefined)
    .finally(() => {
      interactionRegionSyncing = false;
      if (interactionRegionDirty) {
        interactionRegionFrame = requestAnimationFrame(synchronizeInteractionRegions);
      }
    });
};

const scheduleInteractionRegionSync = (): void => {
  interactionRegionDirty = true;
  if (interactionRegionFrame === undefined && !interactionRegionSyncing) {
    interactionRegionFrame = requestAnimationFrame(synchronizeInteractionRegions);
  }
};

// The rectangle the host must cover, in CSS pixels relative to the monitor, or
// null for the whole display. Editing keeps the full monitor because panels are
// dragged anywhere on it; in game mode the transparent surface the compositor
// puts over the game shrinks to what the overlays actually occupy.
const computeHostBounds = (): OverlayInteractionRegion | null => {
  if (!clickThrough || panels.size === 0) return null;
  let left = Number.POSITIVE_INFINITY;
  let top = Number.POSITIVE_INFINITY;
  let right = Number.NEGATIVE_INFINITY;
  let bottom = Number.NEGATIVE_INFINITY;
  for (const overlay of panels.keys()) {
    const placement = placements.get(overlay);
    if (!placement) return null;
    left = Math.min(left, placement.x);
    top = Math.min(top, placement.y);
    right = Math.max(right, placement.x + placement.width);
    bottom = Math.max(bottom, placement.y + placement.height);
  }
  if (!Number.isFinite(left) || !Number.isFinite(top)) return null;
  left = Math.max(0, Math.floor(left - HOST_BOUNDS_MARGIN));
  top = Math.max(0, Math.floor(top - HOST_BOUNDS_MARGIN));
  right = Math.min(monitorViewport.width, Math.ceil(right + HOST_BOUNDS_MARGIN));
  bottom = Math.min(monitorViewport.height, Math.ceil(bottom + HOST_BOUNDS_MARGIN));
  if (right <= left || bottom <= top) return null;
  if (left === 0 && top === 0
    && right >= monitorViewport.width && bottom >= monitorViewport.height) return null;
  return { x: left, y: top, width: right - left, height: bottom - top };
};

const sameHostBounds = (
  left: OverlayInteractionRegion | null,
  right: OverlayInteractionRegion | null
): boolean => left === right
  || (left !== null && right !== null
    && left.x === right.x && left.y === right.y
    && left.width === right.width && left.height === right.height);

const applyStageOffset = (bounds: OverlayInteractionRegion | null, dpr: number): void => {
  // Mirror the backend's own flooring so the panels sit on exactly the pixel the
  // host was moved to instead of drifting by a fraction at fractional scales.
  const left = bounds ? Math.floor(bounds.x * dpr) / dpr : 0;
  const top = bounds ? Math.floor(bounds.y * dpr) / dpr : 0;
  stage.style.left = `${-left}px`;
  stage.style.top = `${-top}px`;
  stage.style.width = `${monitorViewport.width}px`;
  stage.style.height = `${monitorViewport.height}px`;
};

const synchronizeHostBounds = (): void => {
  // A change while a resize is in flight is picked up by the recomputation
  // that runs when it settles.
  if (hostBoundsPending) return;
  const bounds = computeHostBounds();
  if (sameHostBounds(bounds, appliedHostBounds)) return;
  const dpr = window.devicePixelRatio || 1;
  const physical = bounds && {
    x: bounds.x * dpr,
    y: bounds.y * dpr,
    width: bounds.width * dpr,
    height: bounds.height * dpr
  };
  // Never leave a frame with panels outside the host: grow the window before
  // moving the stage into it, and move the stage before shrinking the window.
  const growing = !bounds
    || !appliedHostBounds
    || (bounds.x <= appliedHostBounds.x && bounds.y <= appliedHostBounds.y);
  if (!growing) applyStageOffset(bounds, dpr);
  hostBoundsPending = invoke<OverlayHostViewport>("set_overlay_host_bounds", { bounds: physical })
    .then((viewport) => {
      appliedHostBounds = bounds;
      const monitorChanged = viewport.width !== monitorViewport.width
        || viewport.height !== monitorViewport.height;
      if (monitorChanged) monitorViewport = { width: viewport.width, height: viewport.height };
      if (growing || monitorChanged) applyStageOffset(bounds, dpr);
      scheduleInteractionRegionSync();
      return monitorChanged;
    })
    .catch(() => false)
    .then((monitorChanged) => {
      hostBoundsPending = null;
      if (monitorChanged) void synchronizePanels();
      else synchronizeHostBounds();
    });
};

const suppressBrowserInteraction = (event: Event): void => {
  event.preventDefault();
  event.stopPropagation();
};

for (const eventName of ["contextmenu", "dragstart", "selectstart", "auxclick"] as const) {
  stage.addEventListener(eventName, suppressBrowserInteraction);
}

const postEvent = (overlay: OverlayId, event: string, payload: unknown): void => {
  frames.get(overlay)?.contentWindow?.postMessage({
    source: "blackrack-overlay-composite",
    kind: "event",
    event,
    payload
  } satisfies RuntimeMessage, window.location.origin);
};

const projectTelemetryFrame = (overlay: OverlayId, frame: TelemetryFrame): TelemetryFrame => {
  let projected = projectedFrames.get(overlay);
  if (!projected) {
    projected = {} as TelemetryFrame;
    projectedFrames.set(overlay, projected);
  }
  const target = projected as unknown as Record<string, unknown>;
  const source = frame as unknown as Record<string, unknown>;
  for (const field of sharedTelemetryFields) target[field] = source[field];
  for (const field of telemetryFields[overlay]) target[field] = source[field];
  return projected;
};

const applyPlacement = (panel: HTMLElement, placement: OverlayPlacement): void => {
  placements.set(placement.overlay, placement);
  panel.style.left = `${placement.x}px`;
  panel.style.top = `${placement.y}px`;
  panel.style.width = `${placement.width}px`;
  panel.style.height = `${placement.height}px`;
};

const minimumPanelWidth = (overlay: OverlayId): number => overlay === "damage" ? 90 : 120;

/**
 * El suelo de tamaño mantiene el panel agarrable mientras el usuario lo
 * redimensiona, así que solo se aplica ahí. Restaurar una escala guardada no
 * vuelve a aplicarlo: el diseño que informa el iframe cambia con el contenido
 * (el standings vacío mide MINIMUM_PANEL_HEIGHT de alto) y evaluar el suelo
 * contra ese tamaño transitorio subía la escala elegida y la persistía en cada
 * arranque.
 */
const MINIMUM_PANEL_HEIGHT = 32;
const MINIMUM_PANEL_SCALE = 0.1;

const fitPlacementToMonitor = (placement: OverlayPlacement): OverlayPlacement => {
  const designSize = designSizes.get(placement.overlay);
  let width = Math.max(minimumPanelWidth(placement.overlay), placement.width);
  let height = Math.max(MINIMUM_PANEL_HEIGHT, placement.height);
  let fittedScale = placement.scale;
  if (designSize) {
    const requestedScale = fittedScale ?? Math.min(
      width / designSize.width,
      height / designSize.height
    );
    const maximumScale = Math.min(
      monitorViewport.width / designSize.width,
      monitorViewport.height / designSize.height
    );
    fittedScale = Math.min(maximumScale, Math.max(MINIMUM_PANEL_SCALE, requestedScale));
    width = designSize.width * fittedScale;
    height = designSize.height * fittedScale;
  } else {
    width = Math.min(width, monitorViewport.width);
    height = Math.min(height, monitorViewport.height);
  }
  return {
    ...placement,
    width,
    height,
    ...(fittedScale === undefined ? {} : { scale: fittedScale }),
    x: clampPanelCoordinate(placement.x, width, monitorViewport.width),
    y: clampPanelCoordinate(placement.y, height, monitorViewport.height)
  };
};

const bindPointerMove = (
  event: PointerEvent,
  overlay: OverlayId,
  mode: "move" | "resize"
): void => {
  if (clickThrough || event.button !== 0) return;
  event.preventDefault();
  event.stopPropagation();
  const target = event.currentTarget as HTMLElement;
  target.setPointerCapture(event.pointerId);
  const panel = panels.get(overlay);
  const layout = readCompositeLayout();
  if (!panel || !layout) return;
  const initial = { ...layout[overlay] };
  const designSize = designSizes.get(overlay) ?? {
    width: initial.width,
    height: initial.height
  };
  const startX = event.clientX;
  const startY = event.clientY;

  const move = (next: PointerEvent): void => {
    const dx = next.clientX - startX;
    const dy = next.clientY - startY;
    const placement = mode === "move"
      ? {
          ...initial,
          x: clampPanelCoordinate(initial.x + dx, initial.width, monitorViewport.width),
          y: clampPanelCoordinate(initial.y + dy, initial.height, monitorViewport.height)
        }
      : {
          ...initial,
          ...(() => {
            const widthScale = (initial.width + dx) / designSize.width;
            const heightScale = (initial.height + dy) / designSize.height;
            const initialScale = initial.width / designSize.width;
            const requestedScale = Math.abs(widthScale - initialScale)
              >= Math.abs(heightScale - initialScale)
              ? widthScale
              : heightScale;
            const minimumScale = Math.max(
              minimumPanelWidth(overlay) / designSize.width,
              MINIMUM_PANEL_HEIGHT / designSize.height,
              MINIMUM_PANEL_SCALE
            );
            const maximumScale = Math.max(minimumScale, Math.min(
              (monitorViewport.width - initial.x) / designSize.width,
              (monitorViewport.height - initial.y) / designSize.height
            ));
            const scale = Math.max(minimumScale, Math.min(requestedScale, maximumScale));
            return {
              width: designSize.width * scale,
              height: designSize.height * scale,
              scale
            };
          })()
        };
    layout[overlay] = placement;
    applyPlacement(panel, placement);
    scheduleInteractionRegionSync();
  };
  const finish = (): void => {
    target.removeEventListener("pointermove", move);
    target.removeEventListener("pointerup", finish);
    target.removeEventListener("pointercancel", finish);
    void saveOverlayPlacement(layout[overlay]);
  };
  target.addEventListener("pointermove", move);
  target.addEventListener("pointerup", finish);
  target.addEventListener("pointercancel", finish);
};

const removePanel = (overlay: OverlayId): void => {
  panels.get(overlay)?.remove();
  panels.delete(overlay);
  frames.delete(overlay);
  placements.delete(overlay);
};

const createPanel = (overlay: OverlayId, placement: OverlayPlacement): void => {
  if (panels.has(overlay)) {
    applyPlacement(panels.get(overlay)!, placement);
    return;
  }
  const panel = document.createElement("section");
  panel.className = "composite-panel";
  panel.dataset.overlay = overlay;
  applyPlacement(panel, placement);

  const iframe = document.createElement("iframe");
  iframe.src = `${overlay}.html?composite=1`;
  iframe.title = overlayTitle(overlay);
  iframe.allow = "none";
  iframe.tabIndex = -1;
  iframe.draggable = false;

  const chrome = document.createElement("div");
  chrome.className = "composite-panel-chrome";
  chrome.addEventListener("pointerdown", (event) => bindPointerMove(event, overlay, "move"));
  const label = document.createElement("span");
  label.className = "composite-panel-label";
  label.textContent = overlayTitle(overlay);
  const resize = document.createElement("button");
  resize.className = "composite-resize";
  resize.type = "button";
  resize.ariaLabel = t("composite.resize", { overlay: overlayTitle(overlay) });
  resize.tabIndex = -1;
  resize.addEventListener("pointerdown", (event) => bindPointerMove(event, overlay, "resize"));
  chrome.append(label, resize);
  panel.append(iframe, chrome);
  stage.append(panel);
  panels.set(overlay, panel);
  frames.set(overlay, iframe);

  iframe.addEventListener("load", () => {
    postEvent(overlay, "overlay://interaction-mode", { click_through: clickThrough });
    const frame = latestFrames.get(overlay);
    if (frame) postEvent(overlay, "telemetry://frame", frame);
  });
};

const synchronizePanels = async (): Promise<void> => {
  const layout = readCompositeLayout() ?? await ensureCompositeLayout();
  let normalized = false;
  // A stored layout written before an overlay existed has no placement for it.
  // Seeding the missing one here keeps a single new overlay from throwing out
  // of this loop and leaving the whole host without panels.
  let seeded: CompositeLayout | null = null;
  for (const overlay of overlayIds) {
    let placement = layout[overlay];
    if (!placement) {
      seeded ??= await ensureCompositeLayout();
      placement = seeded[overlay];
      if (!placement) continue;
      layout[overlay] = placement;
      normalized = true;
    }
    const fitted = fitPlacementToMonitor(placement);
    normalized ||= fitted.x !== placement.x || fitted.y !== placement.y
      || fitted.width !== placement.width || fitted.height !== placement.height
      || fitted.scale !== placement.scale;
    layout[overlay] = fitted;
    if (visible.has(overlay)) createPanel(overlay, fitted);
    else removePanel(overlay);
  }
  if (normalized) localStorage.setItem(COMPOSITE_LAYOUT_KEY, JSON.stringify(layout));
  scheduleInteractionRegionSync();
  synchronizeHostBounds();
};

const applyInteractionMode = (mode: InteractionMode): void => {
  clickThrough = mode.click_through;
  stage.classList.toggle("editing", !clickThrough);
  for (const overlay of frames.keys()) {
    postEvent(overlay, "overlay://interaction-mode", mode);
  }
  scheduleInteractionRegionSync();
  synchronizeHostBounds();
};

void listen<TelemetryBatch>("telemetry://batch", ({ payload }) => {
  for (const overlay of payload.targets) {
    if (!frames.has(overlay)) continue;
    const frame = projectTelemetryFrame(overlay, payload.frame);
    latestFrames.set(overlay, frame);
    postEvent(overlay, "telemetry://frame", frame);
  }
});

for (const event of [
  "standings://settings",
  "relative://settings",
  "driving://settings",
  "delta://settings",
  "timing://settings",
  "fuel://settings",
  "trackmap://settings",
  "tires://settings",
  "conditions://settings",
  "dashboard://settings",
  "overlay://background-transparency",
  "overlay://font-size",
  "performance://logging"
]) {
  void listen<unknown>(event, ({ payload }) => {
    for (const overlay of frames.keys()) postEvent(overlay, event, payload);
  });
}

void listen<OverlayState>("overlay://visibility", ({ payload }) => {
  if (payload.visible) visible.add(payload.label);
  else visible.delete(payload.label);
  void synchronizePanels();
});
void listen<OverlayPlacement>("overlay://layout", () => void synchronizePanels());
void listen<InteractionMode>("overlay://interaction-mode", ({ payload }) => applyInteractionMode(payload));
void listen("locale://change", () => window.location.reload());

window.addEventListener("storage", (event) => {
  if (event.key === COMPOSITE_LAYOUT_KEY) void synchronizePanels();
});
window.addEventListener("resize", () => {
  void synchronizePanels();
  scheduleInteractionRegionSync();
});
void getCurrentWindow().onMoved(() => {
  void synchronizePanels();
  scheduleInteractionRegionSync();
}).catch(() => undefined);

window.addEventListener("message", (event: MessageEvent<RuntimeMessage>) => {
  if (event.origin !== window.location.origin || event.data?.source !== "blackrack-overlay-composite") return;
  if (event.data.kind === "fit") {
    const overlay = Array.from(frames.entries())
      .find(([, frame]) => frame.contentWindow === event.source)?.[0];
    const size = event.data.payload as Partial<OverlayDesignSize> | undefined;
    if (!overlay || !size || !Number.isFinite(size.width) || !Number.isFinite(size.height)
      || Number(size.width) <= 1 || Number(size.height) <= 1) return;
    const nextSize = { width: Number(size.width), height: Number(size.height) };
    const previousSize = designSizes.get(overlay);
    designSizes.set(overlay, nextSize);
    const layout = readCompositeLayout();
    const panel = panels.get(overlay);
    if (!layout || !panel) return;
    const current = layout[overlay];
    const scale = current.scale ?? (previousSize
      ? Math.min(current.width / previousSize.width, current.height / previousSize.height)
      : overlay === "timing"
        ? current.width / nextSize.width
        : Math.min(current.width / nextSize.width, current.height / nextSize.height));
    const fitted = fitPlacementToMonitor({
      ...current,
      width: nextSize.width * scale,
      height: nextSize.height * scale,
      scale
    });
    const changed = Math.abs(fitted.width - current.width) > 0.5
      || Math.abs(fitted.height - current.height) > 0.5
      || fitted.scale !== current.scale;
    if (changed) {
      layout[overlay] = fitted;
      applyPlacement(panel, fitted);
      scheduleInteractionRegionSync();
      synchronizeHostBounds();
      void saveOverlayPlacement(fitted);
    }
    return;
  }
  if (event.data.kind !== "invoke" || !event.data.command || !event.data.requestId) return;
  const source = event.source as WindowProxy | null;
  if (!BRIDGE_COMMANDS.has(event.data.command)) {
    source?.postMessage({
      source: "blackrack-overlay-composite",
      kind: "event",
      event: `invoke:${event.data.requestId}:error`,
      payload: `Comando no permitido: ${event.data.command}`
    } satisfies RuntimeMessage, event.origin);
    return;
  }
  void invoke(event.data.command, event.data.args)
    .then((payload) => source?.postMessage({
      source: "blackrack-overlay-composite",
      kind: "event",
      event: `invoke:${event.data.requestId}:ok`,
      payload
    } satisfies RuntimeMessage, event.origin))
    .catch((error) => source?.postMessage({
      source: "blackrack-overlay-composite",
      kind: "event",
      event: `invoke:${event.data.requestId}:error`,
      payload: String(error)
    } satisfies RuntimeMessage, event.origin));
});

void Promise.all([
  invoke<OverlayState[]>("get_overlay_states"),
  invoke<InteractionMode>("get_interaction_mode"),
  ensureCompositeLayout()
]).then(([states, mode]) => {
  for (const state of states) if (state.visible) visible.add(state.label);
  applyInteractionMode(mode);
  void synchronizePanels();
});
