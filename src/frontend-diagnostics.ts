export interface FrontendDiagnostic {
  source: string;
  kind: "error" | "unhandledrejection" | "console.error";
  message: string;
  stack?: string;
}

type DiagnosticReporter = (diagnostic: FrontendDiagnostic) => Promise<unknown>;

const recentlyReported = new Map<string, number>();
const reportTimes: number[] = [];
const DUPLICATE_WINDOW_MS = 10_000;
const RATE_LIMIT_WINDOW_MS = 60_000;
const MAX_REPORTS_PER_WINDOW = 20;
let consoleErrorInstalled = false;

const errorDetails = (reason: unknown): { message: string; stack?: string } => {
  if (reason instanceof Error) return { message: reason.message, stack: reason.stack };
  if (typeof reason === "string") return { message: reason };
  try {
    return { message: JSON.stringify(reason) ?? String(reason) };
  } catch {
    return { message: String(reason) };
  }
};

const consoleErrorDetails = (values: unknown[]): { message: string; stack?: string } => {
  const details = values.map(errorDetails);
  const stacks = details.flatMap(({ stack }) => stack ? [stack] : []);
  return {
    message: details.map(({ message }) => message).join(" ") || "console.error()",
    stack: stacks.length > 0 ? stacks.join("\n") : undefined
  };
};

export const installFrontendDiagnostics = (
  source: string,
  report: DiagnosticReporter
): void => {
  const send = (diagnostic: FrontendDiagnostic): void => {
    const signature = `${diagnostic.kind}:${diagnostic.message}:${diagnostic.stack ?? ""}`;
    const now = Date.now();
    if (now - (recentlyReported.get(signature) ?? 0) < DUPLICATE_WINDOW_MS) return;
    while (reportTimes.length && now - reportTimes[0] >= RATE_LIMIT_WINDOW_MS) {
      reportTimes.shift();
    }
    if (reportTimes.length >= MAX_REPORTS_PER_WINDOW) return;
    reportTimes.push(now);
    recentlyReported.set(signature, now);
    if (recentlyReported.size > 100) {
      for (const [key, reportedAt] of recentlyReported) {
        if (now - reportedAt >= DUPLICATE_WINDOW_MS) recentlyReported.delete(key);
      }
    }
    void report(diagnostic).catch(() => undefined);
  };

  if (!consoleErrorInstalled) {
    const originalConsoleError = console.error.bind(console);
    console.error = (...values: unknown[]): void => {
      originalConsoleError(...values);
      send({ source, kind: "console.error", ...consoleErrorDetails(values) });
    };
    consoleErrorInstalled = true;
  }

  window.addEventListener("error", (event) => {
    const details = errorDetails(event.error ?? event.message);
    const location = event.filename
      ? ` (${event.filename}:${event.lineno}:${event.colno})`
      : "";
    send({
      source,
      kind: "error",
      message: `${details.message}${location}`,
      stack: details.stack
    });
  });

  window.addEventListener("unhandledrejection", (event) => {
    const details = errorDetails(event.reason);
    send({ source, kind: "unhandledrejection", ...details });
  });
};
