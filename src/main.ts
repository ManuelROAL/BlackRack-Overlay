import { invoke } from "@tauri-apps/api/core";
import { getVersion } from "@tauri-apps/api/app";
import { emit, listen } from "@tauri-apps/api/event";
import { open, save } from "@tauri-apps/plugin-dialog";
import "./control-panel.css";
import { installFrontendDiagnostics } from "./frontend-diagnostics";
import {
  applyTranslations,
  formatNumber,
  getLocale,
  isLocale,
  LOCALE_OPTIONS,
  LOCALE_STORAGE_KEY,
  SUPPORTED_LOCALES,
  setLocale,
  t,
  type Locale,
  type TranslationKey
} from "./i18n";
import type { InteractionMode, TelemetryFrame } from "./telemetry-types";
import {
  DRIVER_NAME_FORMATS,
  isDriverNameFormat,
  type DriverNameFormat
} from "./driver-name-format";
import {
  defaultStandingsSettings,
  readStandingsSettings,
  STANDINGS_COLUMNS,
  STANDINGS_HEADER_OPTIONS,
  STANDINGS_SETTINGS_KEY,
  type StandingsSettings
} from "./standings-settings";
import {
  defaultRelativeSettings,
  readRelativeSettings,
  RELATIVE_COLUMNS,
  RELATIVE_COLUMN_OPTIONS,
  RELATIVE_HEADER_OPTIONS,
  RELATIVE_SETTINGS_KEY,
  type RelativeSettings
} from "./relative-settings";
import {
  defaultDrivingSettings,
  DRIVING_PEDALS,
  DRIVING_SETTINGS_KEY,
  readDrivingSettings,
  type DrivingPedalId,
  type DrivingSettings
} from "./driving-settings";
import {
  defaultDeltaSettings,
  DELTA_MODES,
  DELTA_SETTINGS_KEY,
  isDeltaMode,
  readDeltaSettings,
  type DeltaSettings
} from "./delta-settings";
import {
  defaultTimingSettings,
  isTimingSectorReference,
  readTimingSettings,
  TIMING_SECTOR_REFERENCES,
  TIMING_SETTINGS_KEY,
  TIMING_TIMES,
  type TimingSettings
} from "./timing-settings";
import {
  defaultTrackMapSettings,
  readTrackMapSettings,
  TRACK_MAP_SETTINGS_KEY,
  type TrackMapSettings
} from "./trackmap-settings";
import {
  DEFAULT_OVERLAY_FONT_SIZE,
  DEFAULT_OVERLAY_TRANSPARENCY,
  effectiveOverlayFontSize,
  effectiveOverlayTransparency,
  OVERLAY_FONT_SIZE_KEY,
  OVERLAY_FONT_SIZE_SCOPE_KEY,
  OVERLAY_TRANSPARENCY_KEY,
  OVERLAY_TRANSPARENCY_SCOPE_KEY,
  readOverlayFontSize,
  readOverlayFontSizeScope,
  readOverlayTransparency,
  readOverlayTransparencyScope,
  type OverlayFontSizeChange,
  type OverlayFontSizeScope,
  type OverlayId,
  type OverlayTransparencyChange,
  type OverlayTransparencyScope
} from "./overlay-appearance";
import {
  ensureCompositeLayout,
  COMPOSITE_LAYOUT_KEY,
  getOverlayDisplays,
  readCompositeLayout,
  resetOverlayPlacement,
  resolveOverlayMonitor,
  setOverlayMonitor
} from "./composite-layout";

installFrontendDiagnostics("control", (diagnostic) =>
  invoke("record_frontend_error", { ...diagnostic })
);

interface OverlayState {
  label: OverlayId;
  visible: boolean;
}

interface TelemetryLoggingStatus {
  enabled: boolean;
  directory: string;
  active_file: string | null;
}

type StrategyLoggingStatus = TelemetryLoggingStatus;
type DriverRankEstimateLoggingStatus = TelemetryLoggingStatus;

interface LmuDependencyStatus {
  telemetry_plugin_available: boolean;
  telemetry_plugin_path: string | null;
}

interface ShortcutBindingStatus {
  shortcut: string;
  active: boolean;
  error: string | null;
}

interface ShortcutSettingsStatus {
  interaction_mode: ShortcutBindingStatus;
  show_panel: ShortcutBindingStatus;
}

interface BrowserSourceStatus {
  enabled: boolean;
  running: boolean;
  url: string;
  clients: number;
  error_kind: "not_initialized" | "missing_assets" | "address_unavailable" | "server_configuration" | "server_startup" | "settings_persistence" | null;
  error_detail: string | null;
}

interface OverlayConfigurationExport {
  format: "blackrack-overlay-configuration";
  schemaVersion: 8;
  exportedAt: string;
  ui: { locale: Locale };
  overlays: {
    visibility: Record<OverlayId, boolean>;
    transparency: {
      scope: OverlayTransparencyScope;
      values: Record<OverlayId, number>;
    };
    fontSize: {
      scope: OverlayFontSizeScope;
      values: Record<OverlayId, number>;
    };
    monitor: number;
    layout: Awaited<ReturnType<typeof ensureCompositeLayout>>;
    standings: StandingsSettings;
    relative: RelativeSettings;
    driving: DrivingSettings;
    delta: DeltaSettings;
    timing: TimingSettings;
    trackMap: TrackMapSettings;
  };
}

type ShortcutAction = "interaction_mode" | "show_panel";

let lmuDependencyStatus: LmuDependencyStatus | null = null;

applyTranslations();

const localeSelect = document.getElementById("interface-locale") as HTMLSelectElement | null;
if (localeSelect) {
  for (const option of LOCALE_OPTIONS) localeSelect.add(new Option(option.label, option.code));
  localeSelect.value = getLocale();
  localeSelect.addEventListener("change", () => {
    if (!isLocale(localeSelect.value) || localeSelect.value === getLocale()) return;
    setLocale(localeSelect.value);
    void emit("locale://change", { locale: localeSelect.value }).finally(() => window.location.reload());
  });
}

const overlayIds: OverlayId[] = ["delta", "timing", "driving", "tires", "damage", "standings", "relative", "fuel", "pitstop", "flags", "rejoin", "trackmap", "forecast", "conditions"];
const storageKey = "blackrack-overlay.visible-windows.v1";

const readPreferences = (): Record<OverlayId, boolean> => {
  const defaults: Record<OverlayId, boolean> = {
    delta: false,
    timing: false,
    driving: false,
    tires: false,
    damage: false,
    standings: false,
    relative: false,
    fuel: false,
    pitstop: false,
    flags: false,
    rejoin: false,
    trackmap: false,
    forecast: false,
    conditions: false
  };

  try {
    const saved = JSON.parse(localStorage.getItem(storageKey) ?? "{}") as Partial<
      Record<OverlayId, boolean>
    >;
    for (const id of overlayIds) {
      if (typeof saved[id] === "boolean") defaults[id] = saved[id];
    }
  } catch {
    localStorage.removeItem(storageKey);
  }

  return defaults;
};

const preferences = readPreferences();

const viewButtons = [...document.querySelectorAll<HTMLButtonElement>("[data-control-view]")];
const viewSections = [...document.querySelectorAll<HTMLElement>("[data-view-section]")];

const selectControlView = (view: string): void => {
  for (const button of viewButtons) {
    const selected = button.dataset.controlView === view;
    button.classList.toggle("active", selected);
    button.setAttribute("aria-current", selected ? "page" : "false");
  }
  for (const section of viewSections) section.hidden = section.dataset.viewSection !== view;
  document.querySelector(".control-panel")?.setAttribute("data-active-view", view);
  window.scrollTo({ top: 0, behavior: "smooth" });
};

for (const button of viewButtons) {
  button.addEventListener("click", () => selectControlView(button.dataset.controlView ?? "overlays"));
}
selectControlView("overlays");

const appVersion = document.getElementById("app-version");
getVersion()
  .then((version) => {
    if (appVersion) appVersion.textContent = `v${version}`;
  })
  .catch((error) => {
    console.error("No se pudo obtener la versión de la aplicación", error);
  });

const supportButton = document.getElementById("open-kofi") as HTMLButtonElement | null;
const supportStatus = document.getElementById("support-status");
supportButton?.addEventListener("click", () => {
  supportButton.disabled = true;
  if (supportStatus) supportStatus.textContent = t("support.opening");
  invoke("open_support_page")
    .then(() => {
      if (supportStatus) supportStatus.textContent = t("support.opened");
    })
    .catch((error) => {
      console.error("No se pudo abrir Ko-fi", error);
      if (supportStatus) supportStatus.textContent = t("support.error");
    })
    .finally(() => {
      supportButton.disabled = false;
    });
});

let activeOverlayFilter = "all";
const overlaySearch = document.getElementById("overlay-search") as HTMLInputElement | null;
const filterButtons = [...document.querySelectorAll<HTMLButtonElement>("[data-overlay-filter]")];

const filterOverlays = (): void => {
  const query = overlaySearch?.value.trim().toLocaleLowerCase(getLocale()) ?? "";
  let matches = 0;
  for (const card of document.querySelectorAll<HTMLElement>("[data-overlay-card]")) {
    const categoryMatches = activeOverlayFilter === "all" || card.dataset.overlayCategory === activeOverlayFilter;
    const queryMatches = !query || (card.textContent ?? "").toLocaleLowerCase(getLocale()).includes(query);
    const visible = categoryMatches && queryMatches;
    card.hidden = !visible;
    const settings = card.nextElementSibling as HTMLElement | null;
    if (settings?.matches("[data-settings-for]")) settings.hidden = !visible || !card.classList.contains("expanded");
    if (visible) matches += 1;
  }
  const empty = document.getElementById("overlay-filter-empty");
  if (empty) empty.hidden = matches > 0;
};

overlaySearch?.addEventListener("input", filterOverlays);
for (const button of filterButtons) {
  button.addEventListener("click", () => {
    activeOverlayFilter = button.dataset.overlayFilter ?? "all";
    for (const candidate of filterButtons) {
      const selected = candidate === button;
      candidate.classList.toggle("active", selected);
      candidate.setAttribute("aria-pressed", String(selected));
    }
    filterOverlays();
  });
}
filterOverlays();

let standingsSettings: StandingsSettings = readStandingsSettings();
let relativeSettings: RelativeSettings = readRelativeSettings();
let drivingSettings: DrivingSettings = readDrivingSettings();
let deltaSettings: DeltaSettings = readDeltaSettings();
let timingSettings: TimingSettings = readTimingSettings();
let trackMapSettings: TrackMapSettings = readTrackMapSettings();
const overlayTransparency = readOverlayTransparency();
let overlayTransparencyScope: OverlayTransparencyScope = readOverlayTransparencyScope();
const overlayFontSize = readOverlayFontSize();
let overlayFontSizeScope: OverlayFontSizeScope = readOverlayFontSizeScope();

const syncBrowserSourcePreferences = (): void => {
  void invoke("set_overlay_view_settings", {
    settings: {
      standings: {
        ownClassRows: standingsSettings.ownClassRows,
        otherClassRows: standingsSettings.otherClassRows,
        showOtherClasses: standingsSettings.showOtherClasses
      },
      relative: {
        aheadRows: relativeSettings.aheadRows,
        behindRows: relativeSettings.behindRows
      }
    }
  }).catch(() => undefined);
  void invoke("set_browser_source_preferences", {
    preferences: {
      standings: standingsSettings,
      relative: relativeSettings,
      driving: drivingSettings,
      delta: deltaSettings,
      timing: timingSettings,
      trackMap: trackMapSettings,
      transparency: effectiveOverlayTransparency(overlayTransparency, overlayTransparencyScope),
      fontSize: effectiveOverlayFontSize(overlayFontSize, overlayFontSizeScope),
      locale: getLocale(),
      supportedLocales: SUPPORTED_LOCALES
    }
  }).catch(() => undefined);
  void invoke("set_delta_settings", { settings: deltaSettings }).catch(() => undefined);
  void invoke("set_timing_settings", { settings: timingSettings }).catch(() => undefined);
};

const shortcutInputs: Record<ShortcutAction, HTMLInputElement | null> = {
  interaction_mode: document.getElementById("shortcut-interaction-mode") as HTMLInputElement | null,
  show_panel: document.getElementById("shortcut-show-panel") as HTMLInputElement | null
};

const setShortcutMessage = (message: string, state: "normal" | "error" | "success" = "normal"): void => {
  const element = document.getElementById("shortcut-message");
  if (!element) return;
  element.textContent = message;
  element.classList.toggle("error", state === "error");
  element.classList.toggle("success", state === "success");
};

const renderShortcutSettings = (status: ShortcutSettingsStatus): void => {
  for (const action of ["interaction_mode", "show_panel"] as const) {
    const input = shortcutInputs[action];
    const binding = status[action];
    if (input) {
      input.value = binding.shortcut;
      input.dataset.state = binding.active ? "active" : "error";
      input.title = t(binding.active ? "shortcuts.active" : "shortcuts.unavailable");
    }
  }

  const interactionFooter = document.getElementById("footer-interaction-shortcut");
  const panelFooter = document.getElementById("footer-panel-shortcut");
  if (interactionFooter) interactionFooter.textContent = status.interaction_mode.shortcut;
  if (panelFooter) panelFooter.textContent = status.show_panel.shortcut;

  const unavailable = [status.interaction_mode, status.show_panel].find((binding) => !binding.active);
  if (unavailable) {
    setShortcutMessage(
      t("shortcuts.occupied", { shortcut: unavailable.shortcut }),
      "error"
    );
  }
};

const shortcutFromKeyboardEvent = (event: KeyboardEvent): string | null => {
  if (["Control", "Shift", "Alt", "Meta"].includes(event.key)) return null;
  if (!(event.ctrlKey || event.altKey || event.metaKey)) return null;

  let key: string | null = null;
  if (/^Key[A-Z]$/.test(event.code)) key = event.code.slice(3);
  else if (/^Digit[0-9]$/.test(event.code)) key = event.code.slice(5);
  else if (/^F(?:[1-9]|1[0-2])$/.test(event.key)) key = event.key;
  if (!key) return null;

  const parts: string[] = [];
  if (event.ctrlKey) parts.push("Ctrl");
  if (event.altKey) parts.push("Alt");
  if (event.shiftKey) parts.push("Shift");
  if (event.metaKey) parts.push("Super");
  parts.push(key);
  return parts.join("+");
};

const saveShortcut = async (action: ShortcutAction, shortcut: string): Promise<void> => {
  const input = shortcutInputs[action];
  if (input) input.disabled = true;
  setShortcutMessage(t("shortcuts.checking", { shortcut }));
  try {
    const status = await invoke<ShortcutSettingsStatus>("set_shortcut", { action, shortcut });
    renderShortcutSettings(status);
    setShortcutMessage(t("shortcuts.saved", { shortcut }), "success");
  } catch (error) {
    const kind = String(error);
    const key = kind === "invalid"
      ? "shortcuts.invalid"
      : kind === "duplicate"
        ? "shortcuts.duplicate"
        : kind === "persistence_failed"
          ? "shortcuts.persistenceError"
          : "shortcuts.unavailableFor";
    setShortcutMessage(t(key, { shortcut }), "error");
    console.error("Could not update global shortcut:", error);
    renderShortcutSettings(await invoke<ShortcutSettingsStatus>("get_shortcut_settings"));
  } finally {
    if (input) input.disabled = false;
  }
};

for (const action of ["interaction_mode", "show_panel"] as const) {
  const input = shortcutInputs[action];
  input?.addEventListener("focus", () => {
    setShortcutMessage(t("shortcuts.capture"));
    input.select();
  });
  input?.addEventListener("keydown", (event) => {
    if (event.key === "Tab") return;
    event.preventDefault();
    if (event.key === "Escape") {
      input.blur();
      setShortcutMessage(t("shortcuts.cancelled"));
      return;
    }
    const shortcut = shortcutFromKeyboardEvent(event);
    if (shortcut) {
      input.value = shortcut;
      input.blur();
      void saveShortcut(action, shortcut);
    }
  });
}

const persistStandingsSettings = (): void => {
  localStorage.setItem(STANDINGS_SETTINGS_KEY, JSON.stringify(standingsSettings));
  void emit("standings://settings", standingsSettings);
  syncBrowserSourcePreferences();
};

const persistRelativeSettings = (): void => {
  localStorage.setItem(RELATIVE_SETTINGS_KEY, JSON.stringify(relativeSettings));
  void emit("relative://settings", relativeSettings);
  syncBrowserSourcePreferences();
};

const persistDrivingSettings = (): void => {
  localStorage.setItem(DRIVING_SETTINGS_KEY, JSON.stringify(drivingSettings));
  void emit("driving://settings", drivingSettings);
  syncBrowserSourcePreferences();
};

const persistDeltaSettings = (): void => {
  localStorage.setItem(DELTA_SETTINGS_KEY, JSON.stringify(deltaSettings));
  void emit("delta://settings", deltaSettings);
  syncBrowserSourcePreferences();
};

const persistTimingSettings = (): void => {
  localStorage.setItem(TIMING_SETTINGS_KEY, JSON.stringify(timingSettings));
  void emit("timing://settings", timingSettings);
  syncBrowserSourcePreferences();
};
const persistTrackMapSettings = (): void => {
  localStorage.setItem(TRACK_MAP_SETTINGS_KEY, JSON.stringify(trackMapSettings));
  void emit("trackmap://settings", trackMapSettings);
  syncBrowserSourcePreferences();
};


const deltaModeSelect = document.getElementById("delta-mode") as HTMLSelectElement | null;
const deltaRangeSelect = document.getElementById("delta-display-range") as HTMLSelectElement | null;
if (deltaModeSelect) {
  deltaModeSelect.replaceChildren(...DELTA_MODES.map(({ value, labelKey }) => {
    const option = document.createElement("option");
    option.value = value;
    option.textContent = t(labelKey);
    return option;
  }));
  deltaModeSelect.value = deltaSettings.mode;
  deltaModeSelect.addEventListener("change", () => {
    if (!isDeltaMode(deltaModeSelect.value)) return;
    deltaSettings = { ...deltaSettings, mode: deltaModeSelect.value };
    persistDeltaSettings();
  });
}
if (deltaRangeSelect) {
  for (const option of deltaRangeSelect.options) option.textContent = `±${formatNumber(Number(option.value))} s`;
  deltaRangeSelect.value = String(deltaSettings.displayRange);
  deltaRangeSelect.addEventListener("change", () => {
    const displayRange = Number(deltaRangeSelect.value);
    if (![0.5, 1, 2, 5].includes(displayRange)) return;
    deltaSettings = { ...deltaSettings, displayRange };
    persistDeltaSettings();
  });
}
const timingHistorySelect = document.getElementById("timing-history-laps") as HTMLSelectElement | null;
if (timingHistorySelect) {
  for (const option of timingHistorySelect.options) {
    const count = Number(option.value);
    option.textContent = count === 0 ? t("settings.hidden") : t("settings.laps", { count });
  }
  timingHistorySelect.value = String(timingSettings.historyLaps);
  timingHistorySelect.addEventListener("change", () => {
    const historyLaps = Number(timingHistorySelect.value);
    if (historyLaps !== 0 && historyLaps !== 3 && historyLaps !== 5) return;
    timingSettings = { ...timingSettings, historyLaps };
    persistTimingSettings();
  });
}

const timingSectorReferenceSelect = document.getElementById("timing-sector-reference") as HTMLSelectElement | null;
if (timingSectorReferenceSelect) {
  timingSectorReferenceSelect.replaceChildren(...TIMING_SECTOR_REFERENCES.map(({ value, labelKey }) => {
    const option = document.createElement("option");
    option.value = value;
    option.textContent = t(labelKey);
    return option;
  }));
  timingSectorReferenceSelect.value = timingSettings.sectorReference;
  timingSectorReferenceSelect.addEventListener("change", () => {
    if (!isTimingSectorReference(timingSectorReferenceSelect.value)) return;
    timingSettings = { ...timingSettings, sectorReference: timingSectorReferenceSelect.value };
    persistTimingSettings();
  });
}

const trackMapPitPrediction = document.getElementById("trackmap-pit-prediction") as HTMLInputElement | null;
if (trackMapPitPrediction) {
  trackMapPitPrediction.checked = trackMapSettings.showPitPrediction;
  trackMapPitPrediction.addEventListener("change", () => {
    trackMapSettings = { showPitPrediction: trackMapPitPrediction.checked };
    persistTrackMapSettings();
  });
}

const inputFor = (id: OverlayId): HTMLInputElement | null =>
  document.querySelector<HTMLInputElement>(`input[data-overlay="${id}"]`);

const setCardState = (id: OverlayId, visible: boolean): void => {
  const input = inputFor(id);
  if (input) input.checked = visible;
  document
    .querySelector<HTMLElement>(`[data-overlay-card="${id}"]`)
    ?.classList.toggle("active", visible);
};

const persist = (): void => {
  localStorage.setItem(storageKey, JSON.stringify(preferences));
};

const setOverlay = async (id: OverlayId, visible: boolean): Promise<void> => {
  const input = inputFor(id);
  if (input) input.disabled = true;

  try {
    const actual = await invoke<boolean>("set_overlay_visible", { label: id, visible });
    preferences[id] = actual;
    setCardState(id, actual);
    persist();
  } catch (error) {
    console.error(`No se pudo cambiar la ventana ${id}:`, error);
    setCardState(id, preferences[id]);
  } finally {
    if (input) input.disabled = false;
  }
};

const setAll = async (visible: boolean): Promise<void> => {
  for (const id of overlayIds) {
    await setOverlay(id, visible);
  }
};

const transparencyMode = document.getElementById("transparency-mode") as HTMLSelectElement | null;
const globalTransparencyControl = document.getElementById("global-transparency-control");
const globalTransparency = document.getElementById("global-transparency") as HTMLInputElement | null;
const globalTransparencyOutput = document.getElementById("global-transparency-output");
const transparencyRanges = new Map<OverlayId, HTMLInputElement>();
const fontSizeMode = document.getElementById("font-size-mode") as HTMLSelectElement | null;
const globalFontSizeControl = document.getElementById("global-font-size-control");
const globalFontSize = document.getElementById("global-font-size") as HTMLInputElement | null;
const globalFontSizeOutput = document.getElementById("global-font-size-output");
const fontSizeRanges = new Map<OverlayId, HTMLInputElement>();

const persistTransparencyScope = (): void => {
  localStorage.setItem(OVERLAY_TRANSPARENCY_SCOPE_KEY, JSON.stringify(overlayTransparencyScope));
};

const emitEffectiveTransparency = (): void => {
  const effective = effectiveOverlayTransparency(overlayTransparency, overlayTransparencyScope);
  for (const overlay of overlayIds) {
    const change: OverlayTransparencyChange = { overlay, transparency: effective[overlay] };
    void emit("overlay://background-transparency", change);
  }
  syncBrowserSourcePreferences();
};

const renderTransparencyMode = (): void => {
  if (transparencyMode) transparencyMode.value = overlayTransparencyScope.mode;
  if (globalTransparency) globalTransparency.value = String(overlayTransparencyScope.globalTransparency);
  if (globalTransparencyOutput) {
    globalTransparencyOutput.textContent = `${overlayTransparencyScope.globalTransparency}%`;
  }
  if (globalTransparencyControl) globalTransparencyControl.hidden = overlayTransparencyScope.mode !== "global";
  for (const range of transparencyRanges.values()) {
    range.disabled = overlayTransparencyScope.mode === "global";
  }
};

const persistFontSizeScope = (): void => {
  localStorage.setItem(OVERLAY_FONT_SIZE_SCOPE_KEY, JSON.stringify(overlayFontSizeScope));
};

const emitEffectiveFontSize = (): void => {
  const effective = effectiveOverlayFontSize(overlayFontSize, overlayFontSizeScope);
  for (const overlay of overlayIds) {
    const change: OverlayFontSizeChange = { overlay, fontSize: effective[overlay] };
    void emit("overlay://font-size", change);
  }
  syncBrowserSourcePreferences();
};

const renderFontSizeMode = (): void => {
  if (fontSizeMode) fontSizeMode.value = overlayFontSizeScope.mode;
  if (globalFontSize) globalFontSize.value = String(overlayFontSizeScope.globalFontSize);
  if (globalFontSizeOutput) globalFontSizeOutput.textContent = `${overlayFontSizeScope.globalFontSize}%`;
  if (globalFontSizeControl) globalFontSizeControl.hidden = overlayFontSizeScope.mode !== "global";
  for (const range of fontSizeRanges.values()) range.disabled = overlayFontSizeScope.mode === "global";
};

const confirmReset = (message: string): Promise<boolean> => {
  const dialog = document.getElementById("reset-confirmation") as HTMLDialogElement | null;
  const messageElement = document.getElementById("reset-confirmation-message");
  if (!dialog || !messageElement) return Promise.resolve(false);
  messageElement.textContent = message;
  dialog.returnValue = "cancel";
  dialog.showModal();
  dialog.querySelector<HTMLButtonElement>('button[value="confirm"]')?.focus();
  return new Promise((resolve) => {
    dialog.addEventListener("close", () => resolve(dialog.returnValue === "confirm"), { once: true });
  });
};

const overlayDisplayName = (id: OverlayId): string =>
  document.querySelector<HTMLElement>(`[data-overlay-card="${id}"] .overlay-copy strong`)
    ?.textContent?.trim() || id;

const resetOverlayConfiguration = async (id: OverlayId): Promise<void> => {
  if (!await confirmReset(
    t("overlay.configConfirm", { overlay: overlayDisplayName(id) })
  )) return;
  overlayTransparency[id] = DEFAULT_OVERLAY_TRANSPARENCY[id];
  localStorage.setItem(OVERLAY_TRANSPARENCY_KEY, JSON.stringify(overlayTransparency));
  overlayFontSize[id] = DEFAULT_OVERLAY_FONT_SIZE[id];
  localStorage.setItem(OVERLAY_FONT_SIZE_KEY, JSON.stringify(overlayFontSize));

  const events: Promise<unknown>[] = [];
  if (id === "standings") {
    standingsSettings = defaultStandingsSettings();
    localStorage.setItem(STANDINGS_SETTINGS_KEY, JSON.stringify(standingsSettings));
    events.push(emit("standings://settings", standingsSettings));
  } else if (id === "relative") {
    relativeSettings = defaultRelativeSettings();
    localStorage.setItem(RELATIVE_SETTINGS_KEY, JSON.stringify(relativeSettings));
    events.push(emit("relative://settings", relativeSettings));
  } else if (id === "driving") {
    drivingSettings = defaultDrivingSettings();
    localStorage.setItem(DRIVING_SETTINGS_KEY, JSON.stringify(drivingSettings));
    events.push(emit("driving://settings", drivingSettings));
  } else if (id === "delta") {
    deltaSettings = defaultDeltaSettings();
    localStorage.setItem(DELTA_SETTINGS_KEY, JSON.stringify(deltaSettings));
    events.push(emit("delta://settings", deltaSettings));
  } else if (id === "timing") {
    timingSettings = defaultTimingSettings();
    localStorage.setItem(TIMING_SETTINGS_KEY, JSON.stringify(timingSettings));
    events.push(emit("timing://settings", timingSettings));
  } else if (id === "trackmap") {
    trackMapSettings = defaultTrackMapSettings();
    localStorage.setItem(TRACK_MAP_SETTINGS_KEY, JSON.stringify(trackMapSettings));
    events.push(emit("trackmap://settings", trackMapSettings));
  }

  const effective = effectiveOverlayTransparency(overlayTransparency, overlayTransparencyScope);
  events.push(emit("overlay://background-transparency", {
    overlay: id,
    transparency: effective[id]
  } satisfies OverlayTransparencyChange));
  const effectiveFontSize = effectiveOverlayFontSize(overlayFontSize, overlayFontSizeScope);
  events.push(emit("overlay://font-size", {
    overlay: id,
    fontSize: effectiveFontSize[id]
  } satisfies OverlayFontSizeChange));
  syncBrowserSourcePreferences();
  await Promise.all(events);
  window.location.reload();
};

const resetOverlayPosition = async (id: OverlayId, button: HTMLButtonElement): Promise<void> => {
  if (!await confirmReset(
    t("overlay.positionConfirm", { overlay: overlayDisplayName(id) })
  )) return;
  button.disabled = true;
  const previous = button.textContent;
  try {
    await resetOverlayPlacement(id);
    button.textContent = t("overlay.done");
    window.setTimeout(() => { button.textContent = previous; }, 900);
  } catch (error) {
    console.error(`No se pudo restaurar la posición de ${id}:`, error);
    button.textContent = t("overlay.error");
    window.setTimeout(() => { button.textContent = previous; }, 1200);
  } finally {
    button.disabled = false;
  }
};

for (const id of overlayIds) {
  const input = inputFor(id);
  setCardState(id, preferences[id]);
  input?.setAttribute("aria-label", t("overlay.show", { overlay: overlayDisplayName(id) }));
  input?.addEventListener("change", () => void setOverlay(id, input.checked));

  const card = document.querySelector<HTMLElement>(`[data-overlay-card="${id}"]`);
  const switchElement = card?.querySelector(".switch");
  if (card && switchElement) {
    const detailsToggle = document.createElement("button");
    detailsToggle.type = "button";
    detailsToggle.className = "overlay-details-toggle";
    detailsToggle.textContent = t("overlay.settings");
    detailsToggle.setAttribute("aria-expanded", "false");
    detailsToggle.addEventListener("click", () => {
      const expanded = card.classList.toggle("expanded");
      detailsToggle.textContent = t(expanded ? "overlay.close" : "overlay.settings");
      detailsToggle.setAttribute("aria-expanded", String(expanded));
      const settings = card.nextElementSibling as HTMLElement | null;
      if (settings?.matches("[data-settings-for]")) settings.hidden = !expanded;
    });
    card.insertBefore(detailsToggle, switchElement);

    const control = document.createElement("label");
    control.className = "overlay-transparency";
    const caption = document.createElement("span");
    caption.textContent = t("overlay.transparency");
    const range = document.createElement("input");
    range.type = "range";
    range.min = "0";
    range.max = "100";
    range.step = "5";
    range.value = String(overlayTransparency[id]);
    range.setAttribute("aria-label", t("overlay.transparencyAria", { overlay: overlayDisplayName(id) }));
    const output = document.createElement("output");
    output.textContent = `${overlayTransparency[id]}%`;
    control.append(caption, range, output);
    card.insertBefore(control, switchElement);
    transparencyRanges.set(id, range);

    const fontSizeControl = document.createElement("label");
    fontSizeControl.className = "overlay-font-size";
    const fontSizeCaption = document.createElement("span");
    fontSizeCaption.textContent = t("overlay.fontSize");
    const fontSizeRange = document.createElement("input");
    fontSizeRange.type = "range";
    fontSizeRange.min = "75";
    fontSizeRange.max = "150";
    fontSizeRange.step = "5";
    fontSizeRange.value = String(overlayFontSize[id]);
    fontSizeRange.setAttribute("aria-label", t("overlay.fontSizeAria", { overlay: overlayDisplayName(id) }));
    const fontSizeOutput = document.createElement("output");
    fontSizeOutput.textContent = `${overlayFontSize[id]}%`;
    fontSizeControl.append(fontSizeCaption, fontSizeRange, fontSizeOutput);
    card.insertBefore(fontSizeControl, switchElement);
    fontSizeRanges.set(id, fontSizeRange);

    const resetActions = document.createElement("div");
    resetActions.className = "overlay-reset-actions";
    const resetConfiguration = document.createElement("button");
    resetConfiguration.type = "button";
    resetConfiguration.textContent = t("overlay.configShort");
    resetConfiguration.title = t("overlay.configTitle");
    resetConfiguration.setAttribute("aria-label", t("overlay.configAria", { overlay: overlayDisplayName(id) }));
    resetConfiguration.addEventListener("click", () => void resetOverlayConfiguration(id));
    const resetPosition = document.createElement("button");
    resetPosition.type = "button";
    resetPosition.textContent = t("overlay.positionShort");
    resetPosition.title = t("overlay.positionTitle");
    resetPosition.setAttribute("aria-label", t("overlay.positionAria", { overlay: overlayDisplayName(id) }));
    resetPosition.addEventListener("click", () => void resetOverlayPosition(id, resetPosition));
    resetActions.append(resetConfiguration, resetPosition);
    card.insertBefore(resetActions, switchElement);

    range.addEventListener("input", () => {
      const transparency = Math.max(0, Math.min(100, Number(range.value)));
      overlayTransparency[id] = transparency;
      output.textContent = `${transparency}%`;
      localStorage.setItem(OVERLAY_TRANSPARENCY_KEY, JSON.stringify(overlayTransparency));
      const change: OverlayTransparencyChange = { overlay: id, transparency };
      void emit("overlay://background-transparency", change);
      syncBrowserSourcePreferences();
    });
    fontSizeRange.addEventListener("input", () => {
      const fontSize = Math.max(75, Math.min(150, Number(fontSizeRange.value)));
      overlayFontSize[id] = fontSize;
      fontSizeOutput.textContent = `${fontSize}%`;
      localStorage.setItem(OVERLAY_FONT_SIZE_KEY, JSON.stringify(overlayFontSize));
      const change: OverlayFontSizeChange = { overlay: id, fontSize };
      void emit("overlay://font-size", change);
      syncBrowserSourcePreferences();
    });
  }
}

transparencyMode?.addEventListener("change", () => {
  overlayTransparencyScope = {
    ...overlayTransparencyScope,
    mode: transparencyMode.value === "global" ? "global" : "individual"
  };
  persistTransparencyScope();
  renderTransparencyMode();
  emitEffectiveTransparency();
});

globalTransparency?.addEventListener("input", () => {
  overlayTransparencyScope = {
    ...overlayTransparencyScope,
    globalTransparency: Math.max(0, Math.min(100, Number(globalTransparency.value)))
  };
  persistTransparencyScope();
  renderTransparencyMode();
  emitEffectiveTransparency();
});

renderTransparencyMode();

fontSizeMode?.addEventListener("change", () => {
  overlayFontSizeScope = {
    ...overlayFontSizeScope,
    mode: fontSizeMode.value === "global" ? "global" : "individual"
  };
  persistFontSizeScope();
  renderFontSizeMode();
  emitEffectiveFontSize();
});

globalFontSize?.addEventListener("input", () => {
  overlayFontSizeScope = {
    ...overlayFontSizeScope,
    globalFontSize: Math.max(75, Math.min(150, Number(globalFontSize.value)))
  };
  persistFontSizeScope();
  renderFontSizeMode();
  emitEffectiveFontSize();
});

renderFontSizeMode();

const bindMonitorSelector = async (): Promise<void> => {
  const [displays, monitor] = await Promise.all([getOverlayDisplays(), resolveOverlayMonitor()]);
  const select = document.getElementById("overlay-monitor") as HTMLSelectElement | null;
  if (!select) return;
  for (const display of displays) {
    const option = document.createElement("option");
    option.value = String(display.index);
    option.textContent = `${display.name} · ${display.width}×${display.height}`;
    select.append(option);
  }
  select.value = String(monitor);
  select.addEventListener("change", () => {
    select.disabled = true;
    void setOverlayMonitor(Number(select.value)).catch(() => {
      select.value = String(monitor);
    }).finally(() => {
      select.disabled = false;
    });
  });
};

document.getElementById("show-all")?.addEventListener("click", () => void setAll(true));
document.getElementById("hide-all")?.addEventListener("click", () => void setAll(false));

const exportConfigurationButton = document.getElementById("export-overlay-configuration") as HTMLButtonElement | null;
const importConfigurationButton = document.getElementById("import-overlay-configuration") as HTMLButtonElement | null;
const exportConfigurationStatus = document.getElementById("export-overlay-configuration-status");
const configurationFileFilters = [{ name: t("config.filter"), extensions: ["json"] }];

const configurationObject = (value: unknown): Record<string, unknown> | null =>
  value !== null && typeof value === "object" && !Array.isArray(value)
    ? value as Record<string, unknown>
    : null;

const parseOverlayConfiguration = (contents: string): OverlayConfigurationExport => {
  let parsed: unknown;
  try {
    parsed = JSON.parse(contents);
  } catch {
    throw new Error(t("config.invalidJson"));
  }
  const root = configurationObject(parsed);
  const ui = configurationObject(root?.ui);
  const overlays = configurationObject(root?.overlays);
  const visibility = configurationObject(overlays?.visibility);
  const transparency = configurationObject(overlays?.transparency);
  const transparencyScope = configurationObject(transparency?.scope);
  const transparencyValues = configurationObject(transparency?.values);
  const fontSize = configurationObject(overlays?.fontSize);
  const fontSizeScope = configurationObject(fontSize?.scope);
  const fontSizeValues = configurationObject(fontSize?.values);
  const monitorSelection = configurationObject(overlays?.monitorSelection);
  const layout = configurationObject(overlays?.layout);
  const standings = configurationObject(overlays?.standings);
  const relative = configurationObject(overlays?.relative);
  const driving = configurationObject(overlays?.driving);
  const delta = configurationObject(overlays?.delta);
  const timing = configurationObject(overlays?.timing);
  const trackMap = configurationObject(overlays?.trackMap);
  const schemaVersion = root?.schemaVersion;
  const schemaMonitor = typeof overlays?.monitor === "number"
    && Number.isInteger(overlays.monitor) && Number(overlays.monitor) >= 0
    ? Number(overlays.monitor)
    : null;
  const legacyMonitor = Number.isInteger(monitorSelection?.globalMonitor)
    && Number(monitorSelection?.globalMonitor) >= 0
    ? Number(monitorSelection?.globalMonitor)
    : null;
  const monitor = schemaMonitor ?? legacyMonitor ?? 0;
  const percentageIsValid = (value: unknown): value is number =>
    typeof value === "number" && Number.isFinite(value) && value >= 0 && value <= 100;
  if (root?.format !== "blackrack-overlay-configuration"
    || ![1, 2, 3, 4, 5, 6, 7, 8].includes(Number(schemaVersion))
    || !overlays || !visibility || !transparency || !transparencyScope || !transparencyValues
    || (Number(schemaVersion) >= 8 && (!fontSize || !fontSizeScope || !fontSizeValues))
    || !layout || !standings || !relative
    || (Number(schemaVersion) >= 2 && !driving)
    || (Number(schemaVersion) >= 3 && !delta) || (Number(schemaVersion) >= 4 && !timing)
    || (Number(schemaVersion) >= 6 && !trackMap)) {
    throw new Error(t("config.incompatible"));
  }
  if (transparencyScope.mode !== "global" && transparencyScope.mode !== "individual") {
    throw new Error(t("config.invalidTransparency"));
  }
  if (Number(schemaVersion) >= 8
    && fontSizeScope?.mode !== "global" && fontSizeScope?.mode !== "individual") {
    throw new Error(t("config.invalidFontSize"));
  }
  if (Number(schemaVersion) >= 5) {
    if (schemaMonitor === null) {
      throw new Error(t("config.invalidMonitor"));
    }
  } else if (!monitorSelection
    || (monitorSelection.mode !== "global" && monitorSelection.mode !== "individual")
    || legacyMonitor === null) {
    throw new Error(t("config.invalidMonitorMode"));
  }
  if (!percentageIsValid(transparencyScope.globalTransparency)) {
    throw new Error(t("config.invalidGeneral"));
  }
  const fontSizeIsValid = (value: unknown): value is number =>
    typeof value === "number" && Number.isFinite(value) && value >= 75 && value <= 150;
  if (Number(schemaVersion) >= 8 && !fontSizeIsValid(fontSizeScope?.globalFontSize)) {
    throw new Error(t("config.invalidGeneral"));
  }
  if (Number(schemaVersion) < 3) {
    visibility.delta = false;
    transparencyValues.delta = 5;
    layout.delta = { overlay: "delta", x: 610, y: 20, width: 420, height: 72 };
  }
  if (Number(schemaVersion) < 4) {
    visibility.timing = false;
    transparencyValues.timing = 5;
    layout.timing = { overlay: "timing", x: 610, y: 110, width: 250, height: 263 };
  }
  const completeBooleanRecord = (value: unknown, keys: string[]): boolean => {
    const record = configurationObject(value);
    return record !== null && keys.every((key) => typeof record[key] === "boolean");
  };
  const completeOrder = (value: unknown, keys: string[]): boolean =>
    Array.isArray(value) && value.length === keys.length
      && new Set(value).size === keys.length
      && value.every((key) => typeof key === "string" && keys.includes(key));
  const defaultStandings = defaultStandingsSettings();
  const standingsColumnIds = Object.keys(defaultStandings.columns);
  const standingsHeaderIds = Object.keys(defaultStandings.header);
  if (!completeBooleanRecord(standings.columns, standingsColumnIds)
    || !completeOrder(standings.columnOrder, standingsColumnIds)
    || typeof standings.showHeader !== "boolean"
    || !completeBooleanRecord(standings.header, standingsHeaderIds)
    || !Number.isInteger(standings.ownClassRows) || Number(standings.ownClassRows) < 3
    || Number(standings.ownClassRows) > 30
    || !Number.isInteger(standings.otherClassRows) || Number(standings.otherClassRows) < 1
    || Number(standings.otherClassRows) > 15
    || typeof standings.showOtherClasses !== "boolean"
    || (standings.driverNameFormat !== undefined && !isDriverNameFormat(standings.driverNameFormat))) {
    throw new Error(t("config.invalidStandings"));
  }
  const defaultRelative = defaultRelativeSettings();
  const relativeOptionIds = Object.keys(defaultRelative.options);
  const relativeColumnIds = defaultRelative.columnOrder;
  if (!completeBooleanRecord(relative.options, relativeOptionIds)
    || !completeOrder(relative.columnOrder, relativeColumnIds)
    || !Number.isInteger(relative.aheadRows) || Number(relative.aheadRows) < 1
    || Number(relative.aheadRows) > 10
    || !Number.isInteger(relative.behindRows) || Number(relative.behindRows) < 1
    || Number(relative.behindRows) > 10
    || (relative.driverNameFormat !== undefined && !isDriverNameFormat(relative.driverNameFormat))) {
    throw new Error(t("config.invalidRelative"));
  }
  const defaultDriving = defaultDrivingSettings();
  const drivingPedalIds = Object.keys(defaultDriving.graphPedals);
  if (driving && (!completeBooleanRecord(driving.graphPedals, drivingPedalIds)
    || !completeBooleanRecord(driving.inputPedals, drivingPedalIds)
    || typeof driving.showSteering !== "boolean"
    || typeof driving.showForceFeedback !== "boolean"
    || typeof driving.showSpeed !== "boolean"
    || typeof driving.showGear !== "boolean")) {
    throw new Error(t("config.invalidDriving"));
  }
  const normalizedDelta = delta ?? defaultDeltaSettings();
  if (!isDeltaMode(normalizedDelta.mode)
    || typeof normalizedDelta.displayRange !== "number"
    || ![0.5, 1, 2, 5].includes(normalizedDelta.displayRange)) {
    throw new Error(t("config.invalidDelta"));
  }
  const normalizedTiming = timing ?? defaultTimingSettings();
  if (normalizedTiming.historyLaps !== 0
    && normalizedTiming.historyLaps !== 3
    && normalizedTiming.historyLaps !== 5) {
    throw new Error(t("config.invalidTiming"));
  }
  if (!isTimingSectorReference(normalizedTiming.sectorReference)) {
    throw new Error(t("config.invalidTiming"));
  }
  const timingTimeIds = TIMING_TIMES.map(({ id }) => id);
  if (normalizedTiming.times !== undefined
    && !completeBooleanRecord(normalizedTiming.times, timingTimeIds)) {
    throw new Error(t("config.invalidTiming"));
  }
  const normalizedTimingWithTimes = {
    ...normalizedTiming,
    times: normalizedTiming.times ?? defaultTimingSettings().times
  };
  const normalizedTrackMap = trackMap ?? defaultTrackMapSettings();
  if (typeof normalizedTrackMap.showPitPrediction !== "boolean") {
    throw new Error(t("config.invalidMap"));
  }

  for (const id of overlayIds) {
    const placement = configurationObject(layout[id]);
    if (typeof visibility[id] !== "boolean" || !percentageIsValid(transparencyValues[id])
      || (Number(schemaVersion) >= 8 && !fontSizeIsValid(fontSizeValues?.[id]))
      || !placement || placement.overlay !== id
      || ![placement.x, placement.y, placement.width, placement.height]
        .every((value) => typeof value === "number" && Number.isFinite(value))
      || Number(placement.width) <= 0 || Number(placement.height) <= 0) {
      throw new Error(t("config.invalidOverlay", { overlay: id }));
    }
  }
  const cleanedLayout = Object.fromEntries(overlayIds.map((id) => {
    const placement: Record<string, unknown> = { ...(layout[id] as Record<string, unknown>) };
    delete placement.monitor;
    return [id, placement];
  })) as unknown as OverlayConfigurationExport["overlays"]["layout"];
  const normalized = parsed as OverlayConfigurationExport;
  const normalizedFontSize = Number(schemaVersion) >= 8
    ? {
        scope: fontSizeScope as unknown as OverlayFontSizeScope,
        values: fontSizeValues as unknown as Record<OverlayId, number>
      }
    : {
        scope: { mode: "individual", globalFontSize: 100 } as OverlayFontSizeScope,
        values: { ...DEFAULT_OVERLAY_FONT_SIZE }
      };
  return {
    ...normalized,
    schemaVersion: 8,
    ui: { locale: isLocale(ui?.locale) ? ui.locale : getLocale() },
    overlays: {
      ...normalized.overlays,
      monitor,
      layout: cleanedLayout,
      fontSize: normalizedFontSize,
      driving: driving ? driving as unknown as DrivingSettings : defaultDriving,
      delta: normalizedDelta as unknown as DeltaSettings,
      timing: normalizedTimingWithTimes as unknown as TimingSettings,
      trackMap: normalizedTrackMap as unknown as TrackMapSettings
    }
  };
};

const normalizeImportedMonitor = async (
  configuration: OverlayConfigurationExport
): Promise<OverlayConfigurationExport> => {
  const displays = await getOverlayDisplays();
  const availableMonitors = new Set(displays.map(({ index }) => index));
  const primaryMonitor = displays[0]?.index ?? 0;
  const monitor = availableMonitors.has(configuration.overlays.monitor)
    ? configuration.overlays.monitor
    : primaryMonitor;
  return { ...configuration, overlays: { ...configuration.overlays, monitor } };
};

const applyImportedConfiguration = (configuration: OverlayConfigurationExport): void => {
  const entries: Array<[string, unknown]> = [
    [storageKey, configuration.overlays.visibility],
    [OVERLAY_TRANSPARENCY_KEY, configuration.overlays.transparency.values],
    [OVERLAY_TRANSPARENCY_SCOPE_KEY, configuration.overlays.transparency.scope],
    [OVERLAY_FONT_SIZE_KEY, configuration.overlays.fontSize.values],
    [OVERLAY_FONT_SIZE_SCOPE_KEY, configuration.overlays.fontSize.scope],
    [COMPOSITE_LAYOUT_KEY, configuration.overlays.layout],
    [STANDINGS_SETTINGS_KEY, configuration.overlays.standings],
    [RELATIVE_SETTINGS_KEY, configuration.overlays.relative],
    [DRIVING_SETTINGS_KEY, configuration.overlays.driving],
    [DELTA_SETTINGS_KEY, configuration.overlays.delta],
    [TIMING_SETTINGS_KEY, configuration.overlays.timing],
    [TRACK_MAP_SETTINGS_KEY, configuration.overlays.trackMap]
  ];
  const previous = entries.map(([key]) => [key, localStorage.getItem(key)] as const);
  const previousLocale = localStorage.getItem(LOCALE_STORAGE_KEY);
  try {
    for (const [key, value] of entries) localStorage.setItem(key, JSON.stringify(value));
    localStorage.setItem(LOCALE_STORAGE_KEY, configuration.ui.locale);
  } catch (error) {
    for (const [key, value] of previous) {
      if (value === null) localStorage.removeItem(key);
      else localStorage.setItem(key, value);
    }
    if (previousLocale === null) localStorage.removeItem(LOCALE_STORAGE_KEY);
    else localStorage.setItem(LOCALE_STORAGE_KEY, previousLocale);
    throw error;
  }
};

const setConfigurationTransferBusy = (busy: boolean): void => {
  if (exportConfigurationButton) exportConfigurationButton.disabled = busy;
  if (importConfigurationButton) importConfigurationButton.disabled = busy;
};

exportConfigurationButton?.addEventListener("click", () => {
  setConfigurationTransferBusy(true);
  if (exportConfigurationStatus) exportConfigurationStatus.textContent = t("config.preparing");
  void (async () => {
    const now = new Date();
    const layout = readCompositeLayout() ?? await ensureCompositeLayout();
    const monitor = await resolveOverlayMonitor();
    const configuration: OverlayConfigurationExport = {
      format: "blackrack-overlay-configuration",
      schemaVersion: 8,
      exportedAt: now.toISOString(),
      ui: { locale: getLocale() },
      overlays: {
        visibility: { ...preferences },
        transparency: {
          scope: { ...overlayTransparencyScope },
          values: { ...overlayTransparency }
        },
        fontSize: {
          scope: { ...overlayFontSizeScope },
          values: { ...overlayFontSize }
        },
        monitor,
        layout,
        standings: standingsSettings,
        relative: relativeSettings,
        driving: drivingSettings,
        delta: deltaSettings,
        timing: timingSettings,
        trackMap: trackMapSettings
      }
    };
    const timestamp = now.toISOString().replace(/[-:]/g, "").replace(/\.\d{3}Z$/, "Z");
    const selectedPath = await save({
      title: t("config.exportDialog"),
      defaultPath: `BlackRackOverlay-config-${timestamp}.json`,
      filters: configurationFileFilters
    });
    if (!selectedPath) {
      if (exportConfigurationStatus) exportConfigurationStatus.textContent = t("config.exportCancelled");
      return;
    }
    const pathWithExtension = selectedPath.toLocaleLowerCase().endsWith(".json")
      ? selectedPath
      : `${selectedPath}.json`;
    const path = await invoke<string>("export_overlay_configuration", {
      path: pathWithExtension,
      contents: JSON.stringify(configuration, null, 2)
    });
    if (exportConfigurationStatus) {
      exportConfigurationStatus.textContent = t("config.exported");
      exportConfigurationStatus.title = path;
    }
  })().catch((error) => {
    if (exportConfigurationStatus) {
      exportConfigurationStatus.textContent = t("config.error", { detail: String(error) });
      exportConfigurationStatus.title = String(error);
    }
  }).finally(() => {
    setConfigurationTransferBusy(false);
  });
});

importConfigurationButton?.addEventListener("click", () => {
  setConfigurationTransferBusy(true);
  if (exportConfigurationStatus) exportConfigurationStatus.textContent = t("config.chooseImport");
  void (async () => {
    const selectedPath = await open({
      title: t("config.importDialog"),
      multiple: false,
      directory: false,
      filters: configurationFileFilters
    });
    if (!selectedPath) {
      if (exportConfigurationStatus) exportConfigurationStatus.textContent = t("config.importCancelled");
      return;
    }
    const contents = await invoke<string>("import_overlay_configuration", { path: selectedPath });
    const configuration = await normalizeImportedMonitor(parseOverlayConfiguration(contents));
    if (!await confirmReset(
      t("config.confirmImport")
    )) {
      if (exportConfigurationStatus) exportConfigurationStatus.textContent = t("config.importCancelled");
      return;
    }
    applyImportedConfiguration(configuration);
    await setOverlayMonitor(configuration.overlays.monitor).catch(() => undefined);
    if (exportConfigurationStatus) {
      exportConfigurationStatus.textContent = t("config.imported");
      exportConfigurationStatus.title = selectedPath;
    }
    window.setTimeout(() => window.location.reload(), 250);
  })().catch((error) => {
    if (exportConfigurationStatus) {
      exportConfigurationStatus.textContent = t("config.error", { detail: String(error) });
      exportConfigurationStatus.title = String(error);
    }
  }).finally(() => {
    setConfigurationTransferBusy(false);
  });
});

const standingsColumns = document.getElementById("standings-columns");
for (const column of STANDINGS_COLUMNS.filter(({ configurable }) => configurable)) {
  const label = document.createElement("label");
  label.className = "column-toggle";
  const input = document.createElement("input");
  input.type = "checkbox";
  input.checked = standingsSettings.columns[column.id];
  input.dataset.standingsColumn = column.id;
  const mark = document.createElement("span");
  const text = document.createElement("b");
  text.textContent = t(column.labelKey);
  label.append(input, mark, text);
  input.addEventListener("change", () => {
    standingsSettings = {
      ...standingsSettings,
      columns: { ...standingsSettings.columns, [column.id]: input.checked }
    };
    persistStandingsSettings();
  });
  standingsColumns?.append(label);
}

const bindColumnOrder = <Id extends string>(
  container: HTMLElement | null,
  definitions: ReadonlyArray<{ id: Id; labelKey: TranslationKey }>,
  getOrder: () => Id[],
  setOrder: (order: Id[]) => void,
  lockedIds: ReadonlySet<Id> = new Set()
): void => {
  if (!container) return;
  const definitionsById = new Map(definitions.map((definition) => [definition.id, definition]));
  let draggedId: Id | null = null;

  const move = (sourceId: Id, targetIndex: number): void => {
    if (lockedIds.has(sourceId)) return;
    const order = [...getOrder()];
    const sourceIndex = order.indexOf(sourceId);
    if (sourceIndex < 0) return;
    order.splice(sourceIndex, 1);
    const insertionIndex = Math.max(0, Math.min(targetIndex, order.length));
    order.splice(insertionIndex, 0, sourceId);
    setOrder(order);
    render();
  };

  const render = (): void => {
    container.replaceChildren();
    const order = getOrder();
    order.forEach((id, index) => {
      const definition = definitionsById.get(id);
      if (!definition) return;
      const item = document.createElement("div");
      item.className = "column-order-item";
      const locked = lockedIds.has(id);
      item.classList.toggle("locked", locked);
      item.draggable = !locked;
      item.dataset.columnId = id;

      const grip = document.createElement("span");
      grip.className = "column-order-grip";
      grip.textContent = locked ? "•" : "⠿";
      grip.setAttribute("aria-hidden", "true");
      const label = document.createElement("span");
      label.className = "column-order-label";
      const translatedLabel = t(definition.labelKey);
      label.textContent = translatedLabel;
      label.title = translatedLabel;

      const previous = document.createElement("button");
      previous.type = "button";
      previous.className = "column-order-button";
      previous.textContent = "←";
      previous.disabled = locked || index === 0 || lockedIds.has(order[index - 1]);
      previous.title = t("settings.left", { label: translatedLabel });
      previous.setAttribute("aria-label", previous.title);
      previous.addEventListener("click", () => move(id, index - 1));

      const next = document.createElement("button");
      next.type = "button";
      next.className = "column-order-button";
      next.textContent = "→";
      next.disabled = locked || index === order.length - 1 || lockedIds.has(order[index + 1]);
      next.title = t("settings.right", { label: translatedLabel });
      next.setAttribute("aria-label", next.title);
      next.addEventListener("click", () => move(id, index + 1));

      if (!locked) {
        item.addEventListener("dragstart", (event) => {
          draggedId = id;
          item.classList.add("dragging");
          event.dataTransfer?.setData("text/plain", id);
          if (event.dataTransfer) event.dataTransfer.effectAllowed = "move";
        });
        item.addEventListener("dragend", () => {
          draggedId = null;
          item.classList.remove("dragging");
        });
        item.addEventListener("dragover", (event) => {
          event.preventDefault();
          if (event.dataTransfer) event.dataTransfer.dropEffect = "move";
        });
        item.addEventListener("drop", (event) => {
          event.preventDefault();
          const sourceId = draggedId ?? event.dataTransfer?.getData("text/plain") as Id | undefined;
          if (!sourceId || sourceId === id || lockedIds.has(sourceId)) return;
          const currentOrder = getOrder();
          const targetIndex = currentOrder.indexOf(id);
          move(sourceId, targetIndex);
        });
      }

      if (locked) {
        const lockedMark = document.createElement("span");
        lockedMark.className = "column-order-lock";
        lockedMark.textContent = t("settings.fixed");
        item.append(grip, label, lockedMark);
      } else {
        item.append(grip, label, previous, next);
      }
      container.append(item);
    });
  };

  render();
};

bindColumnOrder(
  document.getElementById("standings-column-order"),
  STANDINGS_COLUMNS,
  () => standingsSettings.columnOrder,
  (columnOrder) => {
    standingsSettings = { ...standingsSettings, columnOrder };
    persistStandingsSettings();
  },
  new Set(["signals"])
);

const appendToggle = (
  container: HTMLElement | null,
  labelText: string,
  checked: boolean,
  onChange: (checked: boolean) => void
): void => {
  const label = document.createElement("label");
  label.className = "column-toggle";
  const input = document.createElement("input");
  input.type = "checkbox";
  input.checked = checked;
  const mark = document.createElement("span");
  const text = document.createElement("b");
  text.textContent = labelText;
  label.append(input, mark, text);
  input.addEventListener("change", () => onChange(input.checked));
  container?.append(label);
};

const standingsHeaderOptions = document.getElementById("standings-header-options");
appendToggle(standingsHeaderOptions, t("settings.showHeader"), standingsSettings.showHeader, (checked) => {
  standingsSettings = { ...standingsSettings, showHeader: checked };
  persistStandingsSettings();
});
for (const option of STANDINGS_HEADER_OPTIONS) {
  appendToggle(standingsHeaderOptions, t(option.labelKey), standingsSettings.header[option.id], (checked) => {
    standingsSettings = {
      ...standingsSettings,
      header: { ...standingsSettings.header, [option.id]: checked }
    };
    persistStandingsSettings();
  });
}

const timingTimeOptions = document.getElementById("timing-time-options");
for (const option of TIMING_TIMES) {
  appendToggle(timingTimeOptions, t(option.labelKey), timingSettings.times[option.id], (checked) => {
    timingSettings = {
      ...timingSettings,
      times: { ...timingSettings.times, [option.id]: checked }
    };
    persistTimingSettings();
  });
}

const showOtherClasses = document.getElementById("standings-show-other-classes") as HTMLInputElement | null;
const ownClassRows = document.getElementById("standings-own-class-rows") as HTMLInputElement | null;
const otherClassRows = document.getElementById("standings-other-class-rows") as HTMLInputElement | null;

if (showOtherClasses) {
  showOtherClasses.checked = standingsSettings.showOtherClasses;
  showOtherClasses.addEventListener("change", () => {
    standingsSettings = { ...standingsSettings, showOtherClasses: showOtherClasses.checked };
    persistStandingsSettings();
  });
}

const bindRowCount = (
  input: HTMLInputElement | null,
  key: "ownClassRows" | "otherClassRows",
  minimum: number,
  maximum: number
): void => {
  if (!input) return;
  input.value = String(standingsSettings[key]);
  input.addEventListener("change", () => {
    const value = Math.max(minimum, Math.min(Math.round(Number(input.value)), maximum));
    input.value = String(value);
    standingsSettings = { ...standingsSettings, [key]: value };
    persistStandingsSettings();
  });
};

bindRowCount(ownClassRows, "ownClassRows", 3, 30);
bindRowCount(otherClassRows, "otherClassRows", 1, 15);

const bindDriverNameFormat = (
  id: string,
  current: () => DriverNameFormat,
  update: (format: DriverNameFormat) => void
): void => {
  const select = document.getElementById(id) as HTMLSelectElement | null;
  if (!select) return;
  for (const format of DRIVER_NAME_FORMATS) {
    select.add(new Option(t(format.labelKey), format.id));
  }
  select.value = current();
  select.addEventListener("change", () => {
    if (isDriverNameFormat(select.value)) update(select.value);
  });
};

bindDriverNameFormat("standings-name-format", () => standingsSettings.driverNameFormat, (driverNameFormat) => {
  standingsSettings = { ...standingsSettings, driverNameFormat };
  persistStandingsSettings();
});

const appendRelativeOption = (
  container: HTMLElement | null,
  id: keyof RelativeSettings["options"],
  label: string
): void => {
  appendToggle(container, label, relativeSettings.options[id], (checked) => {
    relativeSettings = {
      ...relativeSettings,
      options: { ...relativeSettings.options, [id]: checked }
    };
    persistRelativeSettings();
  });
};

const relativeHeaderOptions = document.getElementById("relative-header-options");
appendRelativeOption(relativeHeaderOptions, "tableHeader", t("settings.showHeader"));
for (const option of RELATIVE_HEADER_OPTIONS) {
  appendRelativeOption(relativeHeaderOptions, option.id, t(option.labelKey));
}

const relativeColumns = document.getElementById("relative-columns");
for (const option of RELATIVE_COLUMN_OPTIONS) {
  appendRelativeOption(relativeColumns, option.id, t(option.labelKey));
}

bindColumnOrder(
  document.getElementById("relative-column-order"),
  RELATIVE_COLUMNS,
  () => relativeSettings.columnOrder,
  (columnOrder) => {
    relativeSettings = { ...relativeSettings, columnOrder };
    persistRelativeSettings();
  },
  new Set(["signals"])
);

const bindRelativeRowCount = (
  id: string,
  key: "aheadRows" | "behindRows"
): void => {
  const input = document.getElementById(id) as HTMLInputElement | null;
  if (!input) return;
  input.value = String(relativeSettings[key]);
  input.addEventListener("change", () => {
    const value = Math.max(1, Math.min(Math.round(Number(input.value)), 10));
    input.value = String(value);
    relativeSettings = { ...relativeSettings, [key]: value };
    persistRelativeSettings();
  });
};

bindRelativeRowCount("relative-ahead-rows", "aheadRows");
bindRelativeRowCount("relative-behind-rows", "behindRows");
bindDriverNameFormat("relative-name-format", () => relativeSettings.driverNameFormat, (driverNameFormat) => {
  relativeSettings = { ...relativeSettings, driverNameFormat };
  persistRelativeSettings();
});

const appendDrivingPedalToggle = (
  container: HTMLElement | null,
  group: "graphPedals" | "inputPedals",
  id: DrivingPedalId,
  label: string
): void => {
  appendToggle(container, label, drivingSettings[group][id], (checked) => {
    drivingSettings = {
      ...drivingSettings,
      [group]: { ...drivingSettings[group], [id]: checked }
    };
    persistDrivingSettings();
  });
};

const drivingGraphPedals = document.getElementById("driving-graph-pedals");
const drivingInputPedals = document.getElementById("driving-input-pedals");
for (const pedal of DRIVING_PEDALS) {
  appendDrivingPedalToggle(drivingGraphPedals, "graphPedals", pedal.id, t(pedal.labelKey));
  appendDrivingPedalToggle(drivingInputPedals, "inputPedals", pedal.id, t(pedal.labelKey));
}

const drivingReadoutOptions = document.getElementById("driving-readout-options");
for (const [key, label] of [
  ["showSteering", t("readout.steering")],
  ["showForceFeedback", t("readout.ffb")],
  ["showSpeed", t("readout.speed")],
  ["showGear", t("readout.gear")]
] as const) {
  appendToggle(drivingReadoutOptions, label, drivingSettings[key], (checked) => {
    drivingSettings = { ...drivingSettings, [key]: checked };
    persistDrivingSettings();
  });
}

const loggingInput = document.getElementById("telemetry-logging") as HTMLInputElement | null;

const driverRankEstimateLoggingInput = document.getElementById("dr-estimate-logging") as HTMLInputElement | null;

const strategyLoggingInput = document.getElementById("strategy-logging") as HTMLInputElement | null;

const renderStrategyLoggingStatus = (status: StrategyLoggingStatus): void => {
  if (strategyLoggingInput) strategyLoggingInput.checked = status.enabled;
  document.getElementById("strategy-logging-card")?.classList.toggle("active", status.enabled);
  const description = document.getElementById("strategy-logging-description");
  if (description) description.textContent = t(status.enabled ? "strategyLog.onSub" : "strategyLog.offSub");
  const path = document.getElementById("strategy-logging-path");
  if (path) {
    const location = status.active_file ?? status.directory;
    const name = location.split(/[\\/]/).filter(Boolean).at(-1) ?? location;
    path.textContent = t(status.enabled ? "strategyLog.active" : "strategyLog.disabled", { name });
    path.title = location;
  }
};

const refreshStrategyLoggingStatus = async (): Promise<void> => {
  renderStrategyLoggingStatus(await invoke<StrategyLoggingStatus>("get_strategy_logging"));
};

strategyLoggingInput?.addEventListener("change", () => {
  const enabled = strategyLoggingInput.checked;
  strategyLoggingInput.disabled = true;
  void invoke<StrategyLoggingStatus>("set_strategy_logging", { enabled })
    .then(renderStrategyLoggingStatus)
    .then(() => new Promise((resolve) => window.setTimeout(resolve, 200)))
    .then(refreshStrategyLoggingStatus)
    .catch((error) => {
      console.error("No se pudo cambiar el registro estratégico:", error);
      strategyLoggingInput.checked = !enabled;
    })
    .finally(() => {
      strategyLoggingInput.disabled = false;
    });
});

void refreshStrategyLoggingStatus().catch((error) =>
  console.error("No se pudo leer el estado del registro estratégico:", error)
);

window.setInterval(() => {
  void refreshStrategyLoggingStatus().catch(() => undefined);
}, 5_000);

const renderDriverRankEstimateLoggingStatus = (status: DriverRankEstimateLoggingStatus): void => {
  if (driverRankEstimateLoggingInput) driverRankEstimateLoggingInput.checked = status.enabled;
  document.getElementById("dr-estimate-logging-card")?.classList.toggle("active", status.enabled);
  const description = document.getElementById("dr-estimate-logging-description");
  if (description) description.textContent = t(status.enabled ? "drEstimateLog.onSub" : "drEstimateLog.offSub");
  const path = document.getElementById("dr-estimate-logging-path");
  if (path) {
    const location = status.active_file ?? status.directory;
    const name = location.split(/[\\/]/).filter(Boolean).at(-1) ?? location;
    path.textContent = t(status.enabled ? "drEstimateLog.active" : "drEstimateLog.disabled", { name });
    path.title = location;
  }
};

const refreshDriverRankEstimateLoggingStatus = async (): Promise<void> => {
  renderDriverRankEstimateLoggingStatus(
    await invoke<DriverRankEstimateLoggingStatus>("get_driver_rank_estimate_logging")
  );
};

driverRankEstimateLoggingInput?.addEventListener("change", () => {
  const enabled = driverRankEstimateLoggingInput.checked;
  driverRankEstimateLoggingInput.disabled = true;
  void invoke<DriverRankEstimateLoggingStatus>("set_driver_rank_estimate_logging", { enabled })
    .then(renderDriverRankEstimateLoggingStatus)
    .then(() => new Promise((resolve) => window.setTimeout(resolve, 200)))
    .then(refreshDriverRankEstimateLoggingStatus)
    .catch((error) => {
      console.error("No se pudo cambiar el registro de estimación de DR:", error);
      driverRankEstimateLoggingInput.checked = !enabled;
    })
    .finally(() => {
      driverRankEstimateLoggingInput.disabled = false;
    });
});

void refreshDriverRankEstimateLoggingStatus().catch((error) =>
  console.error("No se pudo leer el estado del registro de estimación de DR:", error)
);

window.setInterval(() => {
  void refreshDriverRankEstimateLoggingStatus().catch(() => undefined);
}, 5_000);

const renderLoggingStatus = (status: TelemetryLoggingStatus): void => {
  void emit("performance://logging", { enabled: status.enabled });
  if (loggingInput) loggingInput.checked = status.enabled;
  document.getElementById("logging-card")?.classList.toggle("active", status.enabled);

  const description = document.getElementById("logging-description");
  if (description) {
    description.textContent = status.enabled
      ? t("logging.onSub")
      : t("logging.offSub");
  }

  const path = document.getElementById("logging-path");
  if (path) {
    const location = status.active_file ?? status.directory;
    const name = location.split(/[\\/]/).filter(Boolean).at(-1) ?? location;
    path.textContent = t(status.enabled ? "logging.active" : "logging.disabled", { name });
    path.title = location;
  }
};

const refreshLoggingStatus = async (): Promise<void> => {
  renderLoggingStatus(await invoke<TelemetryLoggingStatus>("get_telemetry_logging"));
};

loggingInput?.addEventListener("change", () => {
  const enabled = loggingInput.checked;
  loggingInput.disabled = true;
  void invoke<TelemetryLoggingStatus>("set_telemetry_logging", { enabled })
    .then(renderLoggingStatus)
    .then(() => new Promise((resolve) => window.setTimeout(resolve, 200)))
    .then(refreshLoggingStatus)
    .catch((error) => {
      console.error("No se pudo cambiar el registro de telemetría:", error);
      loggingInput.checked = !enabled;
    })
    .finally(() => {
      loggingInput.disabled = false;
    });
});

const browserSourceInput = document.getElementById("browser-source-enabled") as HTMLInputElement | null;

const browserSourceError = (kind: BrowserSourceStatus["error_kind"]): string => {
  switch (kind) {
    case "not_initialized": return t("browser.errorNotInitialized");
    case "missing_assets": return t("browser.errorMissingAssets");
    case "address_unavailable": return t("browser.errorAddress");
    case "server_configuration": return t("browser.errorConfiguration");
    case "server_startup": return t("browser.errorStartup");
    case "settings_persistence": return t("browser.errorPersistence");
    default: return t("browser.offSub");
  }
};

const renderBrowserSourceStatus = (status: BrowserSourceStatus): void => {
  if (browserSourceInput) browserSourceInput.checked = status.enabled && status.running;
  document.getElementById("browser-source-card")?.classList.toggle("active", status.running);
  const urls = document.getElementById("browser-source-urls");
  if (urls) urls.hidden = !status.running;

  const description = document.getElementById("browser-source-description");
  if (description) {
    description.textContent = status.running
      ? t("browser.onSub")
      : browserSourceError(status.error_kind);
  }
  const label = document.getElementById("browser-source-status");
  if (label) {
    label.textContent = status.running
      ? t("browser.clients", { count: status.clients })
      : status.error_kind ? t("browser.error") : t("browser.disabled");
    label.title = status.error_detail ?? status.url;
  }

  document.querySelectorAll<HTMLElement>("[data-browser-overlay]").forEach((row) => {
    const overlay = row.dataset.browserOverlay;
    const url = `${status.url}/${overlay}`;
    const code = row.querySelector("code");
    if (code) code.textContent = url;
    const button = row.querySelector<HTMLButtonElement>("button");
    if (button) button.dataset.url = url;
  });
};

browserSourceInput?.addEventListener("change", () => {
  const enabled = browserSourceInput.checked;
  browserSourceInput.disabled = true;
  syncBrowserSourcePreferences();
  void invoke<BrowserSourceStatus>("set_browser_source_enabled", { enabled })
    .then(renderBrowserSourceStatus)
    .catch((error) => {
      console.error("No se pudo cambiar la fuente de navegador:", error);
      browserSourceInput.checked = !enabled;
    })
    .finally(() => {
      browserSourceInput.disabled = false;
    });
});

document.querySelectorAll<HTMLButtonElement>("[data-browser-overlay] button").forEach((button) => {
  button.addEventListener("click", () => {
    const url = button.dataset.url;
    if (!url) return;
    void navigator.clipboard.writeText(url).then(() => {
      const previous = button.textContent;
      button.textContent = t("browser.copied");
      window.setTimeout(() => { button.textContent = previous; }, 900);
    });
  });
});

const restoreWindows = async (): Promise<void> => {
  for (const id of overlayIds) {
    await setOverlay(id, preferences[id]);
  }
  const states = await invoke<OverlayState[]>("get_overlay_states");
  for (const state of states) setCardState(state.label, state.visible);
};

const renderConnection = (frame: TelemetryFrame): void => {
  const status = document.getElementById("game-status");
  const label = status?.querySelector("span");
  if (!status || !label) return;
  status.title = "";

  if (!frame.connected) {
    const pluginMissing = lmuDependencyStatus?.telemetry_plugin_available === false;
    status.dataset.state = pluginMissing ? "error" : "offline";
    label.textContent = t(pluginMissing ? "status.plugin" : "status.waiting");
    if (pluginMissing) {
      status.title = t("status.pluginTitle");
    }
  } else if (!frame.player_active) {
    status.dataset.state = "standby";
    label.textContent = t("status.noCar");
  } else {
    status.dataset.state = "live";
    label.textContent = t("status.active");
  }
};

void listen<TelemetryFrame>("telemetry://frame", ({ payload }) => renderConnection(payload));
const renderInteractionMode = (mode: InteractionMode): void => {
  const status = document.getElementById("interaction-status");
  if (status) status.textContent = t(mode.click_through ? "mode.game" : "mode.edit");
  const button = document.getElementById("toggle-interaction-mode");
  if (button) button.textContent = t(mode.click_through ? "mode.toEdit" : "mode.toGame");
};

void listen<InteractionMode>("overlay://interaction-mode", ({ payload }) => renderInteractionMode(payload));

document.getElementById("toggle-interaction-mode")?.addEventListener("click", () => {
  void invoke<InteractionMode>("toggle_interaction_mode_command").catch((error) => {
    console.error("Could not toggle interaction mode:", error);
    setShortcutMessage(t("mode.error"), "error");
  });
});

void restoreWindows();
void bindMonitorSelector();
void refreshLoggingStatus();
syncBrowserSourcePreferences();
void invoke<BrowserSourceStatus>("get_browser_source_status").then(renderBrowserSourceStatus);
void invoke<ShortcutSettingsStatus>("get_shortcut_settings")
  .then(renderShortcutSettings)
  .catch((error) => setShortcutMessage(t("shortcuts.loadError", { detail: String(error) }), "error"));
void invoke<InteractionMode>("get_interaction_mode").then(renderInteractionMode);
void invoke<LmuDependencyStatus>("get_lmu_dependency_status").then((status) => {
  lmuDependencyStatus = status;
});
