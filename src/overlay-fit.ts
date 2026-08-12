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

  body.dataset.overlayFit = "true";

  const updateScale = (): void => {
    const scale = Math.min(
      window.innerWidth / currentSize.width,
      window.innerHeight / currentSize.height
    );
    root.style.setProperty("--overlay-scale", Math.max(scale, 0.1).toString());
  };

  const setSize = (size: OverlaySize): void => {
    currentSize = size;
    root.style.setProperty("--overlay-base-width", `${size.width}px`);
    root.style.setProperty("--overlay-base-height", `${size.height}px`);
    updateScale();
  };

  setSize(initialSize);
  window.addEventListener("resize", updateScale, { passive: true });
  return setSize;
};
