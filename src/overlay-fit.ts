import { reportCompositeOverlaySize } from "./runtime-events";

interface OverlaySize {
  width: number;
  height: number;
}

/**
 * Mantiene el overlay completo y escala su superficie de diseño como una sola
 * unidad. Evita que un tamaño restaurado antiguo recorte filas o widgets.
 */
export const fitOverlay = (initialSize: OverlaySize): ((size: OverlaySize) => void) => {
  const root = document.documentElement;
  const body = document.body;
  let currentSize = initialSize;
  const initialFontScale = Number.parseFloat(getComputedStyle(root).getPropertyValue("--overlay-font-scale"));
  let fontExpansion = Number.isFinite(initialFontScale) && initialFontScale > 1
    ? 1 + (initialFontScale - 1) * 1.3
    : 1;

  body.dataset.overlayFit = "true";

  const updateScale = (): void => {
    const effectiveWidth = currentSize.width * fontExpansion;
    const effectiveHeight = currentSize.height * fontExpansion;
    const scale = Math.min(
      window.innerWidth / effectiveWidth,
      window.innerHeight / effectiveHeight
    );
    root.style.setProperty("--overlay-scale", Math.max(scale, 0.1).toString());
  };

  const setSize = (size: OverlaySize): void => {
    currentSize = size;
    const effectiveSize = {
      width: Math.round(size.width * fontExpansion),
      height: Math.round(size.height * fontExpansion)
    };
    root.style.setProperty("--overlay-base-width", `${effectiveSize.width}px`);
    root.style.setProperty("--overlay-base-height", `${effectiveSize.height}px`);
    updateScale();
    reportCompositeOverlaySize(effectiveSize);
  };

  setSize(initialSize);
  window.addEventListener("resize", updateScale, { passive: true });
  window.addEventListener("overlay-font-size-change", (event) => {
    const scale = Number((event as CustomEvent<{ scale?: number }>).detail?.scale);
    fontExpansion = Number.isFinite(scale) && scale > 1 ? 1 + (scale - 1) * 1.3 : 1;
    setSize(currentSize);
  });
  return setSize;
};
