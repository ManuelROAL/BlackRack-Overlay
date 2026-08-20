import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import "./i18n/overlay";
import { invoke } from "@tauri-apps/api/core";
import { installFrontendDiagnostics } from "./frontend-diagnostics";
import type { TelemetryFrame } from "./telemetry-types";

declare global {
  interface Window {
    __TAURI_INTERNALS__?: unknown;
  }
}

interface CompositeMessage {
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

export const isCompositeOverlay = (): boolean =>
  window.parent !== window && new URLSearchParams(window.location.search).get("composite") === "1";

export const isTauriRuntime = (): boolean => Boolean(window.__TAURI_INTERNALS__) || isCompositeOverlay();

if (isCompositeOverlay()) document.documentElement.classList.add("composite-embed");

export const reportCompositeOverlaySize = (size: OverlayDesignSize): void => {
  if (!isCompositeOverlay()) return;
  window.parent.postMessage({
    source: "blackrack-overlay-composite",
    kind: "fit",
    payload: size
  } satisfies CompositeMessage, window.location.origin);
};

export const listenRuntimeEvent = <T>(
  event: string,
  handler: (payload: T) => void
): Promise<UnlistenFn> => {
  if (isCompositeOverlay()) {
    const listener = (message: MessageEvent<CompositeMessage>): void => {
      if (message.origin !== window.location.origin) return;
      if (message.data?.source !== "blackrack-overlay-composite" || message.data.kind !== "event") return;
      if (message.data.event === event) handler(message.data.payload as T);
    };
    window.addEventListener("message", listener);
    return Promise.resolve(() => window.removeEventListener("message", listener));
  }
  if (!isTauriRuntime()) return Promise.resolve(() => undefined);
  return listen<T>(event, ({ payload }) => handler(payload));
};

export const invokeRuntime = <T>(
  command: string,
  args?: Record<string, unknown>
): Promise<T> => {
  if (!isCompositeOverlay()) return invoke<T>(command, args);
  const requestId = `${Date.now()}-${Math.random().toString(36).slice(2)}`;
  return new Promise<T>((resolve, reject) => {
    const timeout = window.setTimeout(() => {
      window.removeEventListener("message", listener);
      reject(new Error(`Tiempo agotado al ejecutar ${command}`));
    }, 5_000);
    const listener = (message: MessageEvent<CompositeMessage>): void => {
      if (message.origin !== window.location.origin) return;
      if (message.data?.source !== "blackrack-overlay-composite" || message.data.kind !== "event") return;
      if (message.data.event === `invoke:${requestId}:ok`) {
        window.clearTimeout(timeout);
        window.removeEventListener("message", listener);
        resolve(message.data.payload as T);
      } else if (message.data.event === `invoke:${requestId}:error`) {
        window.clearTimeout(timeout);
        window.removeEventListener("message", listener);
        reject(new Error(String(message.data.payload)));
      }
    };
    window.addEventListener("message", listener);
    window.parent.postMessage({
      source: "blackrack-overlay-composite",
      kind: "invoke",
      command,
      args,
      requestId
    } satisfies CompositeMessage, window.location.origin);
  });
};

export const listenTelemetry = (
  handler: (frame: TelemetryFrame) => void
): Promise<UnlistenFn> => {
  if (isTauriRuntime()) {
    return listenRuntimeEvent<TelemetryFrame>("telemetry://frame", handler);
  }

  const events = new EventSource("/api/events");
  events.onmessage = ({ data }) => {
    try {
      handler(JSON.parse(data) as TelemetryFrame);
    } catch (error) {
      console.error("Trama de telemetría local no válida:", error);
    }
  };
  return Promise.resolve(() => events.close());
};

if (isTauriRuntime()) {
  const overlay = window.location.pathname.split("/").pop()?.replace(/\.html$/, "") || "unknown";
  installFrontendDiagnostics(`overlay:${overlay}`, (diagnostic) =>
    invokeRuntime("record_frontend_error", { ...diagnostic })
  );
}
