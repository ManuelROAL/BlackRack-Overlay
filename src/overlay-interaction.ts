import { getCurrentWindow } from "@tauri-apps/api/window";
import type { InteractionMode } from "./telemetry-types";
import { invokeRuntime, isCompositeOverlay, isTauriRuntime, listenRuntimeEvent } from "./runtime-events";

export const bindOverlayInteractionMode = (
  onChange?: (mode: InteractionMode) => void
): void => {
  let clickThrough = true;

  const apply = (mode: InteractionMode): void => {
    clickThrough = mode.click_through;
    document.body.classList.toggle("click-through", mode.click_through);
    onChange?.(mode);
  };

  if (!isTauriRuntime()) {
    apply({ click_through: true });
    return;
  }

  void listenRuntimeEvent<InteractionMode>("overlay://interaction-mode", apply);
  void invokeRuntime<InteractionMode>("get_interaction_mode").then(apply).catch((error) => {
    console.error("No se pudo consultar el modo de interacción:", error);
  });

  if (isCompositeOverlay()) return;

  document.addEventListener("mousedown", (event) => {
    if (clickThrough || event.button !== 0) return;
    const target = event.target;
    if (!(target instanceof Element) || !target.closest("[data-tauri-drag-region]")) return;

    event.preventDefault();
    void getCurrentWindow().startDragging().catch((error) => {
      console.error("No se pudo iniciar el arrastre del overlay:", error);
    });
  });
};
