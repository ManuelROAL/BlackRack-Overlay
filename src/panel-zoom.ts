import { getCurrentWebview } from "@tauri-apps/api/webview";
import { getLocale } from "./i18n";

const PANEL_ZOOM_STORAGE_KEY = "blackrack-overlay.panel-zoom.v1";
const PANEL_ZOOM_STEPS = [0.75, 0.8, 0.9, 1, 1.1, 1.25, 1.5] as const;
const DEFAULT_PANEL_ZOOM = 1;

const readPanelZoom = (): number => {
  try {
    const stored = Number(localStorage.getItem(PANEL_ZOOM_STORAGE_KEY));
    return PANEL_ZOOM_STEPS.some((step) => step === stored) ? stored : DEFAULT_PANEL_ZOOM;
  } catch {
    return DEFAULT_PANEL_ZOOM;
  }
};

/**
 * Zooms the control panel's own webview, like browser zoom: CSS pixels grow,
 * so the panel's narrow-width layouts take over when zoomed in. The choice
 * stays in this webview's storage; overlays are sized separately.
 */
export const installPanelZoom = (): void => {
  const zoomOut = document.getElementById("panel-zoom-out") as HTMLButtonElement | null;
  const zoomReset = document.getElementById("panel-zoom-reset") as HTMLButtonElement | null;
  const zoomIn = document.getElementById("panel-zoom-in") as HTMLButtonElement | null;
  const percent = new Intl.NumberFormat(getLocale(), { style: "percent", maximumFractionDigits: 0 });
  let zoom = readPanelZoom();

  const apply = (next: number): void => {
    zoom = next;
    if (zoomReset) zoomReset.textContent = percent.format(zoom);
    if (zoomOut) zoomOut.disabled = zoom <= PANEL_ZOOM_STEPS[0];
    if (zoomIn) zoomIn.disabled = zoom >= PANEL_ZOOM_STEPS[PANEL_ZOOM_STEPS.length - 1];
    try {
      void getCurrentWebview().setZoom(zoom).catch(() => undefined);
    } catch {
      // Outside Tauri (a plain browser preview) there is no webview to zoom.
    }
  };

  const update = (next: number): void => {
    if (next === zoom) return;
    try {
      localStorage.setItem(PANEL_ZOOM_STORAGE_KEY, String(next));
    } catch {
      // The zoom still applies for this session.
    }
    apply(next);
  };

  const step = (direction: 1 | -1): void => {
    const index = PANEL_ZOOM_STEPS.findIndex((value) => value === zoom);
    const next = PANEL_ZOOM_STEPS[Math.min(PANEL_ZOOM_STEPS.length - 1, Math.max(0, index + direction))];
    update(next);
  };

  zoomOut?.addEventListener("click", () => step(-1));
  zoomIn?.addEventListener("click", () => step(1));
  zoomReset?.addEventListener("click", () => update(DEFAULT_PANEL_ZOOM));

  document.addEventListener("keydown", (event) => {
    if (!event.ctrlKey || event.altKey || event.metaKey) return;
    if (event.key === "+" || event.key === "=" || event.code === "NumpadAdd") step(1);
    else if (event.key === "-" || event.code === "NumpadSubtract") step(-1);
    else if (event.key === "0" || event.code === "Numpad0") update(DEFAULT_PANEL_ZOOM);
    else return;
    event.preventDefault();
  });

  // Ctrl + wheel, which also covers a touchpad pinch. A pinch fires a burst
  // of events, so one step per short window keeps it from racing to the end.
  let lastWheelStep = 0;
  document.addEventListener("wheel", (event) => {
    if (!event.ctrlKey || event.deltaY === 0) return;
    event.preventDefault();
    if (event.timeStamp - lastWheelStep < 120) return;
    lastWheelStep = event.timeStamp;
    step(event.deltaY < 0 ? 1 : -1);
  }, { passive: false });

  apply(zoom);
};
