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
  kind: "event" | "invoke";
  event?: string;
  payload?: unknown;
  command?: string;
  args?: Record<string, unknown>;
  requestId?: string;
}

const overlayIds: OverlayId[] = [
  "dashboard", "driving", "tires", "damage", "standings",
  "relative", "fuel", "pitstop", "flags", "rejoin", "trackmap"
];
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

const applyPlacement = (panel: HTMLElement, placement: OverlayPlacement): void => {
  panel.style.left = `${placement.x}px`;
  panel.style.top = `${placement.y}px`;
  panel.style.width = `${placement.width}px`;
  panel.style.height = `${placement.height}px`;
};

const fitPlacementToMonitor = (placement: OverlayPlacement): OverlayPlacement => {
  const minimumWidth = placement.overlay === "damage" ? 90 : 120;
  const width = Math.max(minimumWidth, Math.min(placement.width, window.innerWidth));
  const height = Math.max(72, Math.min(placement.height, window.innerHeight));
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
          width: Math.max(120, Math.min(initial.width + dx, window.innerWidth - initial.x)),
          height: Math.max(72, Math.min(initial.height + dy, window.innerHeight - initial.y))
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

for (const overlay of overlayIds) {
  void listen<TelemetryFrame>(`telemetry://${overlay}`, ({ payload }) => {
    latestFrames.set(overlay, payload);
    postEvent(overlay, "telemetry://frame", payload);
  });
}

for (const event of [
  "standings://settings",
  "relative://settings",
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
