import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import "./composite.css";
import {
  COMPOSITE_LAYOUT_KEY,
  clampPanelCoordinate,
  ensureCompositeLayout,
  readCompositeLayout,
  saveOverlayPlacement,
  type OverlayPlacement
} from "./composite-layout";
import type { OverlayId } from "./overlay-appearance";
import type { InteractionMode, TelemetryFrame } from "./telemetry-types";

interface OverlayState {
  label: OverlayId;
  visible: boolean;
}

interface RuntimeMessage {
  source: "lmu-overlay-composite";
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

interface TelemetryBatch {
  targets: OverlayId[];
  frame: TelemetryFrame;
}

const overlayIds: OverlayId[] = [
  "dashboard", "driving", "tires", "damage", "standings",
  "relative", "fuel", "pitstop", "flags", "rejoin", "trackmap"
];
const telemetryFields: Record<OverlayId, readonly (keyof TelemetryFrame)[]> = {
  dashboard: [
    "source", "connected", "player_active", "speed_kph", "gear", "rpm", "max_rpm",
    "throttle", "brake", "fuel_liters", "fuel_capacity_liters", "estimated_fuel_laps",
    "virtual_energy_active", "virtual_energy_percent", "estimated_virtual_energy_laps",
    "current_lap_seconds", "best_lap_seconds", "lap_delta_seconds"
  ],
  driving: [
    "speed_kph", "gear", "throttle", "brake", "tc_active", "abs_active",
    "steering_angle_degrees", "force_feedback"
  ],
  tires: [
    "player_damage_percent", "player_aero_damage_percent", "player_damage_severity",
    "player_part_detached", "player_rear_wing_detached", "player_tire_temperature_c",
    "player_brake_temperature_c", "player_tire_remaining_by_wheel_percent",
    "player_tire_flat_spot_percent", "player_tire_compounds", "player_tire_flat",
    "player_tire_detached", "player_suspension_damage_by_wheel_percent"
  ],
  damage: [
    "player_aero_damage_percent", "player_body_damage_percent",
    "player_suspension_damage_percent", "player_tire_remaining_by_wheel_percent"
  ],
  standings: [
    "session_type", "session_max_laps", "session_time_remaining", "session_elapsed_seconds",
    "session_split_number", "session_split_count", "rest_weather_available", "ambient_temperature_c",
    "track_temperature_c", "player_total_laps", "brake_bias_percent", "track_limits_steps",
    "track_limits_steps_per_penalty", "session_total_laps_estimated", "standings_model",
    "standings"
  ],
  relative: [
    "rest_weather_available", "ambient_temperature_c", "track_temperature_c",
    "brake_bias_percent", "track_limits_steps", "track_limits_steps_per_penalty",
    "relative_model", "standings"
  ],
  fuel: [
    "connected", "player_active", "session_type", "track_name", "player_total_laps",
    "fuel_liters", "fuel_capacity_liters", "fuel_per_lap", "fuel_last_lap",
    "fuel_qualifying_lap", "fuel_reference_per_lap", "fuel_projected_lap",
    "fuel_pit_cycle_consumption", "fuel_pit_out_consumption", "session_laps_remaining",
    "session_total_laps_estimated", "virtual_energy_active", "virtual_energy_percent",
    "virtual_energy_per_lap", "virtual_energy_last_lap", "virtual_energy_qualifying_lap",
    "virtual_energy_reference_per_lap", "virtual_energy_projected_lap",
    "virtual_energy_pit_cycle_consumption", "virtual_energy_pit_out_consumption",
    "fuel_strategies", "player_tire_remaining_percent", "player_stint",
    "pit_stop_estimate_available", "pit_stop_estimate_seconds", "pit_stop_fuel_seconds",
    "pit_stop_energy_seconds", "pit_stop_tire_seconds", "pit_stop_damage_seconds",
    "pit_stop_penalty_seconds", "pit_stop_driver_swap_seconds", "lap_progress",
    "consumption_profile_samples"
  ],
  pitstop: [
    "virtual_energy_active", "pit_stop_estimate_available", "pit_stop_estimate_seconds",
    "pit_stop_fuel_seconds", "pit_stop_energy_seconds", "pit_stop_tire_seconds",
    "pit_stop_damage_seconds", "pit_stop_penalty_seconds", "pit_stop_driver_swap_seconds"
  ],
  flags: ["flag_warning"],
  rejoin: ["rejoin_warning"],
  trackmap: ["track_name", "track_length_meters", "track_map_vehicles", "track_map_model"]
};
const overlayTitles: Record<OverlayId, string> = {
  dashboard: "DASHBOARD",
  driving: "TRAILING + PEDAL",
  tires: "DAÑOS Y NEUMÁTICOS",
  damage: "DAÑOS DETALLADOS",
  standings: "CLASIFICACIÓN",
  relative: "RELATIVE",
  fuel: "ENERGÍA Y COMBUSTIBLE",
  pitstop: "PARADA ESTIMADA",
  flags: "BANDERAS",
  rejoin: "REJOIN",
  trackmap: "MAPA DEL CIRCUITO"
};

const stage = document.getElementById("overlay-stage") as HTMLElement;
const monitorIndex = Number(getCurrentWindow().label.replace("overlay-monitor-", ""));
const panels = new Map<OverlayId, HTMLElement>();
const frames = new Map<OverlayId, HTMLIFrameElement>();
const latestFrames = new Map<OverlayId, TelemetryFrame>();
const projectedFrames = new Map<OverlayId, TelemetryFrame>();
const designSizes = new Map<OverlayId, OverlayDesignSize>();
const visible = new Set<OverlayId>();
let clickThrough = false;

const suppressBrowserInteraction = (event: Event): void => {
  event.preventDefault();
  event.stopPropagation();
};

for (const eventName of ["contextmenu", "dragstart", "selectstart", "auxclick"] as const) {
  stage.addEventListener(eventName, suppressBrowserInteraction);
}

const postEvent = (overlay: OverlayId, event: string, payload: unknown): void => {
  frames.get(overlay)?.contentWindow?.postMessage({
    source: "lmu-overlay-composite",
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
  for (const field of telemetryFields[overlay]) target[field] = source[field];
  return projected;
};

const applyPlacement = (panel: HTMLElement, placement: OverlayPlacement): void => {
  panel.style.left = `${placement.x}px`;
  panel.style.top = `${placement.y}px`;
  panel.style.width = `${placement.width}px`;
  panel.style.height = `${placement.height}px`;
};

const fitPlacementToMonitor = (placement: OverlayPlacement): OverlayPlacement => {
  const minimumWidth = placement.overlay === "damage" ? 90 : 120;
  const designSize = designSizes.get(placement.overlay);
  let width = Math.max(minimumWidth, placement.width);
  let height = Math.max(72, placement.height);
  if (designSize) {
    const requestedScale = Math.min(
      width / designSize.width,
      height / designSize.height
    );
    const minimumScale = Math.max(
      minimumWidth / designSize.width,
      72 / designSize.height,
      0.1
    );
    const maximumScale = Math.min(
      window.innerWidth / designSize.width,
      window.innerHeight / designSize.height
    );
    const scale = Math.min(maximumScale, Math.max(minimumScale, requestedScale));
    width = designSize.width * scale;
    height = designSize.height * scale;
  } else {
    width = Math.min(width, window.innerWidth);
    height = Math.min(height, window.innerHeight);
  }
  return {
    ...placement,
    width,
    height,
    x: clampPanelCoordinate(placement.x, width, window.innerWidth),
    y: clampPanelCoordinate(placement.y, height, window.innerHeight)
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
          x: clampPanelCoordinate(initial.x + dx, initial.width, window.innerWidth),
          y: clampPanelCoordinate(initial.y + dy, initial.height, window.innerHeight)
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
              (overlay === "damage" ? 90 : 120) / designSize.width,
              72 / designSize.height,
              0.1
            );
            const maximumScale = Math.max(minimumScale, Math.min(
              (window.innerWidth - initial.x) / designSize.width,
              (window.innerHeight - initial.y) / designSize.height
            ));
            const scale = Math.max(minimumScale, Math.min(requestedScale, maximumScale));
            return {
              width: designSize.width * scale,
              height: designSize.height * scale
            };
          })()
        };
    layout[overlay] = placement;
    applyPlacement(panel, placement);
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
  iframe.title = overlayTitles[overlay];
  iframe.allow = "none";
  iframe.tabIndex = -1;
  iframe.draggable = false;

  const chrome = document.createElement("div");
  chrome.className = "composite-panel-chrome";
  chrome.addEventListener("pointerdown", (event) => bindPointerMove(event, overlay, "move"));
  const label = document.createElement("span");
  label.className = "composite-panel-label";
  label.textContent = overlayTitles[overlay];
  const resize = document.createElement("button");
  resize.className = "composite-resize";
  resize.type = "button";
  resize.ariaLabel = `Redimensionar ${overlayTitles[overlay]}`;
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
  for (const overlay of overlayIds) {
    let placement = layout[overlay];
    if (placement.monitor === monitorIndex) {
      const fitted = fitPlacementToMonitor(placement);
      normalized ||= fitted.x !== placement.x || fitted.y !== placement.y
        || fitted.width !== placement.width || fitted.height !== placement.height;
      placement = fitted;
      layout[overlay] = fitted;
    }
    if (visible.has(overlay) && placement.monitor === monitorIndex) createPanel(overlay, placement);
    else removePanel(overlay);
  }
  if (normalized) localStorage.setItem(COMPOSITE_LAYOUT_KEY, JSON.stringify(layout));
};

const applyInteractionMode = (mode: InteractionMode): void => {
  clickThrough = mode.click_through;
  stage.classList.toggle("editing", !clickThrough);
  for (const overlay of frames.keys()) {
    postEvent(overlay, "overlay://interaction-mode", mode);
  }
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
  "overlay://background-transparency",
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

window.addEventListener("storage", (event) => {
  if (event.key === COMPOSITE_LAYOUT_KEY) void synchronizePanels();
});

window.addEventListener("message", (event: MessageEvent<RuntimeMessage>) => {
  if (event.origin !== window.location.origin || event.data?.source !== "lmu-overlay-composite") return;
  if (event.data.kind === "fit") {
    const overlay = Array.from(frames.entries())
      .find(([, frame]) => frame.contentWindow === event.source)?.[0];
    const size = event.data.payload as Partial<OverlayDesignSize> | undefined;
    if (!overlay || !size || !Number.isFinite(size.width) || !Number.isFinite(size.height)
      || Number(size.width) <= 0 || Number(size.height) <= 0) return;
    const nextSize = { width: Number(size.width), height: Number(size.height) };
    const previousSize = designSizes.get(overlay);
    designSizes.set(overlay, nextSize);
    const layout = readCompositeLayout();
    const panel = panels.get(overlay);
    if (!layout || !panel) return;
    const current = layout[overlay];
    const scale = previousSize
      ? Math.min(current.width / previousSize.width, current.height / previousSize.height)
      : Math.min(current.width / nextSize.width, current.height / nextSize.height);
    const fitted = fitPlacementToMonitor({
      ...current,
      width: nextSize.width * scale,
      height: nextSize.height * scale
    });
    const changed = Math.abs(fitted.width - current.width) > 0.5
      || Math.abs(fitted.height - current.height) > 0.5;
    if (changed) {
      layout[overlay] = fitted;
      applyPlacement(panel, fitted);
      void saveOverlayPlacement(fitted);
    }
    return;
  }
  if (event.data.kind !== "invoke" || !event.data.command || !event.data.requestId) return;
  const source = event.source as WindowProxy | null;
  void invoke(event.data.command, event.data.args)
    .then((payload) => source?.postMessage({
      source: "lmu-overlay-composite",
      kind: "event",
      event: `invoke:${event.data.requestId}:ok`,
      payload
    } satisfies RuntimeMessage, event.origin))
    .catch((error) => source?.postMessage({
      source: "lmu-overlay-composite",
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
