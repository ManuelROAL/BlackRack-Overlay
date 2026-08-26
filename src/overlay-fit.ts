import { reportCompositeOverlaySize } from "./runtime-events";

interface OverlaySize {
  width: number;
  height: number;
}

interface OverlayFitOptions {
  widthTextRatio?: number;
  heightTextRatio?: number;
}

/**
 * Mantiene el overlay completo y escala su superficie de diseño como una sola
 * unidad. Evita que un tamaño restaurado antiguo recorte filas o widgets.
 */
export const fitOverlay = (
  initialSize: OverlaySize,
  options: OverlayFitOptions = {}
): ((size: OverlaySize) => void) => {
  const root = document.documentElement;
  const body = document.body;
  let currentSize = initialSize;
  const initialFontScale = Number.parseFloat(getComputedStyle(root).getPropertyValue("--overlay-font-scale"));
  let fontScale = Number.isFinite(initialFontScale) ? initialFontScale : 1;

  const widthExpansion = (): number => fontScale > 1
    ? 1 + (fontScale - 1) * (options.widthTextRatio ?? 0)
    : 1;
  const heightExpansion = (): number => fontScale > 1
    ? 1 + (fontScale - 1) * (options.heightTextRatio ?? 0)
    : 1;

  body.dataset.overlayFit = "true";

  const updateScale = (): void => {
    const effectiveWidth = currentSize.width * widthExpansion();
    const effectiveHeight = currentSize.height * heightExpansion();
    const scale = Math.min(
      window.innerWidth / effectiveWidth,
      window.innerHeight / effectiveHeight
    );
    root.style.setProperty("--overlay-scale", Math.max(scale, 0.1).toString());
  };

  const setSize = (size: OverlaySize): void => {
    currentSize = size;
    const effectiveSize = {
      width: Math.round(size.width * widthExpansion()),
      height: Math.round(size.height * heightExpansion())
    };
    root.style.setProperty("--overlay-font-width-expansion", widthExpansion().toString());
    root.style.setProperty("--overlay-font-height-expansion", heightExpansion().toString());
    root.style.setProperty("--overlay-base-width", `${effectiveSize.width}px`);
    root.style.setProperty("--overlay-base-height", `${effectiveSize.height}px`);
    updateScale();
    reportCompositeOverlaySize(effectiveSize);
  };

  setSize(initialSize);
  window.addEventListener("resize", updateScale, { passive: true });
  window.addEventListener("overlay-font-size-change", (event) => {
    const scale = Number((event as CustomEvent<{ scale?: number }>).detail?.scale);
    fontScale = Number.isFinite(scale) ? scale : 1;
    setSize(currentSize);
  });
  return setSize;
};
