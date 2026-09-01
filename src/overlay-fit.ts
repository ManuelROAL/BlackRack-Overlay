import { reportCompositeOverlaySize } from "./runtime-events";

interface OverlaySize {
  width: number;
  height: number;
}

/**
 * Un factor de escala estable evita rerasterizar texto e iconos con cada ajuste
 * mínimo. 1/64 es lo bastante fino para que el recorte no se aprecie y lo
 * bastante grueso para reutilizar la misma escala de rasterizado.
 */
const SCALE_STEP = 1 / 64;

const quantizeScale = (scale: number): number =>
  Math.max(Math.floor(scale / SCALE_STEP) * SCALE_STEP, 0.1);

interface OverlayFitOptions {
  widthTextRatio?: number | (() => number);
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

  const widthExpansion = (): number => {
    const configuredRatio = typeof options.widthTextRatio === "function"
      ? options.widthTextRatio()
      : options.widthTextRatio;
    return fontScale > 1
      ? 1 + (fontScale - 1) * (configuredRatio ?? 0)
      : 1;
  };
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
    root.style.setProperty("--overlay-scale", quantizeScale(scale).toString());
  };

  const setSize = (size: OverlaySize): void => {
    currentSize = size;
    const effectiveSize = {
      width: Math.round(size.width * widthExpansion()),
      height: Math.round(size.height * heightExpansion())
    };
    root.style.setProperty("--overlay-font-track-expansion", Math.max(fontScale, 1).toString());
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

/** Ajusta el alto de diseño al contenido real sin alterar el ancho elegido. */
export const fitOverlayToContent = (
  width: number,
  content: HTMLElement
): (() => void) => {
  const body = document.body;
  const measureHeight = (): number => {
    const bodyStyle = getComputedStyle(body);
    const paddingBottom = Number.parseFloat(bodyStyle.paddingBottom) || 0;
    return Math.ceil(content.offsetTop + content.offsetHeight + paddingBottom);
  };
  const initialHeight = Math.max(1, measureHeight());
  const resizeOverlay = fitOverlay({ width, height: initialHeight });
  let measuredHeight = initialHeight;

  const synchronizeHeight = (): void => {
    const height = measureHeight();
    if (height === measuredHeight) return;
    measuredHeight = height;
    resizeOverlay({ width, height });
  };

  new ResizeObserver(synchronizeHeight).observe(content);
  synchronizeHeight();
  void document.fonts.ready.then(synchronizeHeight);
  return synchronizeHeight;
};
