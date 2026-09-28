import { invoke, transformCallback } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import "./composite.css";
import { applyTranslations, getLocale, t } from "./i18n";
import { installFrontendDiagnostics } from "./frontend-diagnostics";
import type { ChatUpdate } from "./runtime-events";
import {
  COMPOSITE_LAYOUT_KEY,
  clampPanelCoordinate,
  ensureCompositeLayout,
  getEffectiveCompositeLayout,
  readCompositeLayout,
  saveOverlayPlacement,
  type CompositeLayout,
  type OverlayPlacement
} from "./composite-layout";
import type { OverlayId } from "./overlay-appearance";
import type { InteractionMode, TelemetryFrame } from "./telemetry-types";
import overlayTelemetryFields from "./overlay-telemetry-fields.json";

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
  /** Only the union of the targets' projected fields is serialized. */
  frame: TelemetryFrame;
}

const overlayIds: OverlayId[] = [
  "delta", "timing", "stinthistory", "driving", "liftcoast", "tires", "damage", "standings",
  "relative", "fuel", "pitstop", "flags", "rejoin", "trackmap",
  "forecast", "conditions", "dashboard", "sessioninfo", "chat", "minimap"
];
// One table shared with the backend: Rust serializes only the union of the
// fields a batch's targets need, and the host projects each overlay onto its
// own list. `shared` reaches every overlay: an overlay has to know what the
// active simulator can report before it decides what to draw.
const sharedTelemetryFields = overlayTelemetryFields.shared as readonly (keyof TelemetryFrame)[];
const telemetryFields = overlayTelemetryFields.overlays as Record<OverlayId, readonly (keyof TelemetryFrame)[]>;
const overlayTitleKeys: Record<OverlayId, import("./i18n").TranslationKey> = {
  delta: "card.delta", timing: "card.timing", stinthistory: "card.stintHistory", driving: "card.driving", liftcoast: "card.liftcoast", tires: "card.tires",
  damage: "card.damage", standings: "card.standings", relative: "card.relative", fuel: "card.fuel",
  pitstop: "card.pitstop", flags: "card.flags", rejoin: "card.rejoin", trackmap: "card.trackmap",
  forecast: "card.forecast", conditions: "card.conditions", dashboard: "card.dashboard", sessioninfo: "card.sessioninfo",
  chat: "card.chat", minimap: "card.minimap"
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
const requestedHostMonitor = Number(new URLSearchParams(window.location.search).get("monitor"));
// Hosts are created by the control panel with an explicit monitor query. Keep
// the fallback for a manually opened/debug composite page and for old cached
// URLs; the backend still remains the authority for native geometry.
const hostMonitor = Number.isInteger(requestedHostMonitor) && requestedHostMonitor >= 0
  ? requestedHostMonitor
  : 0;
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
  chrome.addEventListener("pointerdown", (event) => {
    if (overlay === "chat" && event.target !== chrome) return;
    bindPointerMove(event, overlay, "move");
  });
  const label = document.createElement("span");
  label.className = "composite-panel-label";
  label.textContent = overlayTitle(overlay);
  label.addEventListener("pointerdown", (event) => {
    event.stopPropagation();
    bindPointerMove(event, overlay, "move");
  });
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
  const storedLayout = readCompositeLayout() ?? await ensureCompositeLayout();
  const layout = getEffectiveCompositeLayout(storedLayout);
  let normalized = false;
  // A stored layout written before an overlay existed has no placement for it.
  // Seeding the missing one here keeps a single new overlay from throwing out
  // of this loop and leaving the whole host without panels.
  let seeded: CompositeLayout | null = null;
  for (const overlay of overlayIds) {
    let placement = layout[overlay];
    if (!placement) {
      seeded ??= getEffectiveCompositeLayout(await ensureCompositeLayout());
      placement = seeded[overlay];
      if (!placement) continue;
      layout[overlay] = placement;
      normalized = true;
    }
    const assignedMonitor = placement.monitor ?? hostMonitor;
    if (assignedMonitor !== hostMonitor) {
      removePanel(overlay);
      continue;
    }
    const fitted = fitPlacementToMonitor(placement);
    normalized ||= fitted.x !== placement.x || fitted.y !== placement.y
      || fitted.width !== placement.width || fitted.height !== placement.height
      || fitted.scale !== placement.scale;
    layout[overlay] = fitted;
    if (visible.has(overlay)) createPanel(overlay, fitted);
    else removePanel(overlay);
  }
  if (normalized) {
    const stored = readCompositeLayout() ?? storedLayout;
    for (const overlay of overlayIds) {
      if (layout[overlay] && stored[overlay]) {
        stored[overlay] = { ...layout[overlay], monitor: stored[overlay].monitor };
      }
    }
    localStorage.setItem(COMPOSITE_LAYOUT_KEY, JSON.stringify(stored));
  }
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

// Telemetry arrives on an IPC channel as raw bytes rather than as an event.
// Tauri delivers an event by evaluating a fresh script with the payload
// embedded, and at ~50 batches a second V8 compiled megabytes of one-off code
// per second into the old generation: the host heap climbed to ~540 MB and its
// main-thread cost rose with it. Raw bytes are fetched and parsed instead.
interface TelemetryChannelMessage {
  message?: ArrayBuffer | number[];
  index: number;
}

const batchDecoder = new TextDecoder();
// Fetched payloads can resolve out of order. Telemetry only wants the newest
// frame, so a batch older than one already shown for an overlay is dropped for
// that overlay instead of stalling every later batch behind it.
const deliveredBatchIndex = new Map<OverlayId, number>();

const receiveTelemetryBatch = ({ message, index }: TelemetryChannelMessage): void => {
  if (!message) return;
  const bytes = message instanceof ArrayBuffer ? new Uint8Array(message) : Uint8Array.from(message);
  const batch = JSON.parse(batchDecoder.decode(bytes)) as TelemetryBatch;
  for (const overlay of batch.targets) {
    if (!frames.has(overlay) || (deliveredBatchIndex.get(overlay) ?? -1) > index) continue;
    deliveredBatchIndex.set(overlay, index);
    const frame = projectTelemetryFrame(overlay, batch.frame);
    latestFrames.set(overlay, frame);
    postEvent(overlay, "telemetry://frame", frame);
  }
};

const telemetryChannelId = transformCallback<TelemetryChannelMessage>(receiveTelemetryBatch);
void invoke("subscribe_overlay_telemetry", { channel: `__CHANNEL__:${telemetryChannelId}` })
  .catch((error) => console.error("No se pudo suscribir la telemetría del host:", error));

void listen<ChatUpdate>("chat://update", ({ payload }) => {
  if (frames.has("chat")) postEvent("chat", "chat://update", payload);
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
  "sessioninfo://settings",
  "liftcoast://settings",
  "pitstop://settings",
  "chat://settings",
  "minimap://settings",
  "overlay://background-transparency",
  "overlay://font-size",
  "display-units://change",
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
    const nextHeight = nextSize.height * scale;
    const fitted = fitPlacementToMonitor({
      ...current,
      width: nextSize.width * scale,
      height: nextHeight,
      // Chat stays anchored at the bottom of its saved box, even when the
      // first measurement after a host restart changes its intrinsic height.
      ...(overlay === "chat"
        ? { y: current.y + current.height - nextHeight }
        : {}),
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
