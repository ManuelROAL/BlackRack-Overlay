import { invoke } from "@tauri-apps/api/core";
import { getVersion } from "@tauri-apps/api/app";
import { emit, listen } from "@tauri-apps/api/event";
import { open, save } from "@tauri-apps/plugin-dialog";
import "./control-panel.css";
import "./dashboard-settings.css";
import { backendErrorMessage } from "./backend-errors";
import {
  CHAT_SETTINGS_EVENT,
  CHAT_SETTINGS_KEY,
  defaultChatSettings,
  normalizeChatSettings,
  readChatSettings,
  type ChatSettings
} from "./chat-settings";
import { installFrontendDiagnostics } from "./frontend-diagnostics";
import {
  applyDisplayUnits,
  normalizeDisplayUnits,
  persistDisplayUnits,
  readDisplayUnits,
  DISPLAY_UNITS_STORAGE_KEY,
  type DisplayUnits
} from "./display-units";
import {
  applyTranslations,
  formatNumber,
  getLocale,
  isLocale,
  LOCALE_OPTIONS,
  LOCALE_STORAGE_KEY,
  SUPPORTED_LOCALES,
  sentenceCase,
  setLocale,
  t,
  type Locale,
  type TranslationKey
} from "./i18n";
import type { InteractionMode, SourceCapabilities, TelemetryFrame } from "./telemetry-types";
import {
  DRIVER_NAME_FORMATS,
  isDriverNameFormat,
  type DriverNameFormat
} from "./driver-name-format";
import {
  defaultStandingsSettings,
  normalizeStandingsColumnOrder,
  readStandingsSettings,
  STANDINGS_COLUMNS,
  STANDINGS_HEADER_OPTIONS,
  STANDINGS_SETTINGS_KEY,
  type StandingsColumnId,
  type StandingsSettings
} from "./standings-settings";
import {
  defaultRelativeSettings,
  readRelativeSettings,
  RELATIVE_COLUMNS,
  RELATIVE_COLUMN_OPTIONS,
  RELATIVE_HEADER_OPTIONS,
  RELATIVE_SETTINGS_KEY,
  type RelativeColumnId,
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
  normalizeTimingOrder,
  normalizeTimingSettings,
  readTimingSettings,
  TIMING_SECTOR_REFERENCES,
  TIMING_SETTINGS_KEY,
  TIMING_TIMES,
  type TimingSettings,
  type TimingTimeId
} from "./timing-settings";
import {
  defaultTrackMapSettings,
  readTrackMapSettings,
  normalizeTrackMapSettings,
  TRACK_MAP_SETTINGS_KEY,
  type TrackMapSettings
} from "./trackmap-settings";
import {
  defaultFuelSettings,
  FUEL_FIELDS,
  normalizeFuelSettings,
  FUEL_SETTINGS_KEY,
  isFuelScenarioMode,
  readFuelSettings,
  type FuelSettings
} from "./fuel-settings";
import {
  defaultTiresSettings,
  normalizeTiresSettings,
  readTiresSettings,
  TIRES_SETTINGS_KEY,
  type TiresSettings
} from "./tires-settings";
import {
  CONDITIONS_OPTIONS,
  CONDITIONS_SETTINGS_KEY,
  defaultConditionsSettings,
  normalizeConditionsSettings,
  readConditionsSettings,
  type ConditionsSettings
} from "./conditions-settings";
import {
  DASHBOARD_FIELDS,
  DASHBOARD_SETTINGS_KEY,
  defaultDashboardSettings,
  normalizeDashboardSettings,
  readDashboardSettings,
  type DashboardSettings
} from "./dashboard-settings";
import {
  SESSIONINFO_FIELDS,
  SESSIONINFO_SETTINGS_KEY,
  defaultSessionInfoSettings,
  normalizeSessionInfoSettings,
  readSessionInfoSettings,
  type SessionInfoSettings
} from "./sessioninfo-settings";
import {
  defaultLiftCoastSettings,
  isLiftCoastDisplayMode,
  LIFTCOAST_SETTINGS_KEY,
  normalizeLiftCoastSettings,
  readLiftCoastSettings,
  type LiftCoastSettings
} from "./liftcoast-settings";
import {
  defaultPitStopSettings,
  normalizePitStopSettings,
  PITSTOP_SETTINGS_KEY,
  readPitStopSettings,
  type PitStopSettings
} from "./pitstop-settings";
import {
  DEFAULT_OVERLAY_FONT_SIZE,
  DEFAULT_OVERLAY_TRANSPARENCY,
  effectiveOverlayFontSize,
  effectiveOverlayTransparency,
  OVERLAY_FONT_SIZE_KEY,
  OVERLAY_FONT_SIZE_MAX,
  OVERLAY_FONT_SIZE_MIN,
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
  type CompositeLayout,
  getDefaultCompositeLayout,
  getOverlayDisplays,
  readCompositeLayout,
  refreshOverlayDisplays,
  resetOverlayPlacement,
  resolveOverlayMonitor,
  setOverlayMonitor,
  setOverlayMonitorScope,
  setOverlayMonitorPreference,
  setOverlayPlacementMonitor,
  synchronizeOverlayHosts,
  type OverlayDisplay
} from "./composite-layout";
import {
  DEFAULT_PERFORMANCE_PROFILE,
  isPerformanceProfile,
  PERFORMANCE_PROFILE_KEY,
  readPerformanceProfile,
  savePerformanceProfile,
  type PerformanceProfile
} from "./performance-settings";
import {
  readSimulatorPreference,
  saveSimulatorPreference
} from "./simulator-settings";
import {
  readSpectatorMode,
  readTeamMode,
  saveSpectatorMode,
  saveTeamMode,
  SPECTATOR_MODE_KEY,
  TEAM_MODE_KEY
} from "./spectator-settings";
import {
  createProfileId,
  isOverlayMode,
  isSessionKind,
  MAX_OVERLAY_PROFILES,
  MAX_PROFILE_NAME_LENGTH,
  modeFromFlags,
  normalizeBindings,
  normalizeProfiles,
  normalizeSessionBindings,
  OVERLAY_MODES,
  OVERLAY_PROFILES_KEY,
  PROFILE_BINDINGS_KEY,
  readProfileState,
  resolveProfileId,
  sanitizeProfileName,
  saveProfileState,
  SESSION_BINDINGS_KEY,
  SESSION_KINDS,
  sessionKindFromType,
  type OverlayMode,
  type OverlayProfile,
  type OverlayProfileData,
  type ProfileBindings,
  type ProfileState,
  type SessionBindings,
  type SessionKind
} from "./overlay-profiles";
import {
  defaultOverlayMonitorScope,
  effectiveOverlayMonitor,
  normalizeOverlayMonitorScope,
  OVERLAY_MONITOR_SCOPE_KEY,
  readOverlayMonitorScope,
  saveOverlayMonitorScope,
  type OverlayMonitorScope
} from "./overlay-monitor";
import { OVERLAY_GUIDE, OVERLAY_GUIDE_ORDER } from "./overlay-guide";
import { RELEASE_NOTES, selectReleaseNotes, type ReleaseNotesManifest } from "./release-notes";

installFrontendDiagnostics("control", (diagnostic) =>
  invoke("record_frontend_error", { ...diagnostic })
);

const reportInitializationError = (context: string) => (error: unknown): void => {
  console.error(`Could not initialize ${context}:`, error);
};

interface OverlayState {
  label: OverlayId;
  visible: boolean;
}

interface TelemetryLoggingStatus {
  enabled: boolean;
  directory: string;
  active_file: string | null;
}

interface SimulatorOption {
  id: string;
  display_name: string;
}

interface SimulatorStatus {
  id: string;
  display_name: string;
  /** Absent when the simulator has nothing installable to check. */
  dependency: { available: boolean; detail: string | null } | null;
  /** "auto" or the id of a pinned simulator. */
  preference: string;
  options: SimulatorOption[];
}

interface ShortcutBindingStatus {
  shortcut: string;
  active: boolean;
  error: string | null;
}

type HideOverlayShortcutAction = `hide_${OverlayId}`;
type ShortcutAction = "interaction_mode" | "show_panel" | "toggle_overlays" | HideOverlayShortcutAction;

interface ShortcutSettingsStatus {
  interaction_mode: ShortcutBindingStatus;
  show_panel: ShortcutBindingStatus;
  toggle_overlays: ShortcutBindingStatus;
  /** Optional while older backend builds are still in use. */
  hide_overlays?: Partial<Record<OverlayId, ShortcutBindingStatus>>;
}

interface WheelInputStatus {
  available: boolean;
  capturing: boolean;
  binding: { deviceId: string; deviceName: string; button: number } | null;
  error: string | null;
}

interface BrowserSourceStatus {
  enabled: boolean;
  running: boolean;
  url: string;
  clients: number;
  dropped_frames: number;
  error_kind: "not_initialized" | "missing_assets" | "address_unavailable" | "server_configuration" | "server_startup" | "settings_persistence" | null;
  error_detail: string | null;
}

type UpdateInfo = ReleaseNotesManifest;

interface UpdateCheckResponse {
  currentVersion: string;
  available: UpdateInfo | null;
}

interface UpdateProgress {
  stage: "downloading" | "verifying" | "installing";
  downloadedBytes: number;
  totalBytes: number | null;
  percent: number | null;
}

interface OverlayConfigurationExport {
  format: "blackrack-overlay-configuration";
  schemaVersion: 27;
  exportedAt: string;
  ui: { locale: Locale; displayUnits: DisplayUnits };
  profiles: OverlayProfile[];
  modeBindings: ProfileBindings;
  sessionBindings: SessionBindings;
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
    monitorScope?: OverlayMonitorScope;
    layout: Awaited<ReturnType<typeof ensureCompositeLayout>>;
    standings: StandingsSettings;
    relative: RelativeSettings;
    driving: DrivingSettings;
    delta: DeltaSettings;
    timing: TimingSettings;
    trackMap: TrackMapSettings;
    fuel: FuelSettings;
    tires: TiresSettings;
    conditions: ConditionsSettings;
    dashboard: DashboardSettings;
    sessionInfo: SessionInfoSettings;
    liftCoast: LiftCoastSettings;
    pitstop: PitStopSettings;
    chat: ChatSettings;
    performanceProfile: PerformanceProfile;
    spectatorMode: boolean;
    teamMode: boolean;
  };
}

let simulatorStatus: SimulatorStatus | null = null;

/** Names the simulator the overlays are reading, for copy that has to say it. */
const activeSimulatorName = (): string => simulatorStatus?.display_name || t("simulator.unknown");

applyTranslations();

let simulatorPreference = readSimulatorPreference();
const simulatorPickerTrigger = document.getElementById("simulator-picker-trigger") as HTMLButtonElement | null;
const simulatorPickerLabel = document.getElementById("simulator-picker-label");
const simulatorPickerMenu = document.getElementById("simulator-picker-menu");
if (simulatorPickerLabel) {
  simulatorPickerLabel.textContent = simulatorPreference === "auto" ? t("simulator.auto") : t("simulator.unknown");
}

const closeSimulatorPicker = (): void => {
  if (simulatorPickerMenu) simulatorPickerMenu.hidden = true;
  simulatorPickerTrigger?.setAttribute("aria-expanded", "false");
};

simulatorPickerTrigger?.addEventListener("click", () => {
  if (!simulatorPickerMenu) return;
  const opening = simulatorPickerMenu.hidden;
  simulatorPickerMenu.hidden = !opening;
  simulatorPickerTrigger.setAttribute("aria-expanded", String(opening));
});

document.addEventListener("click", (event) => {
  if (!simulatorPickerMenu || simulatorPickerMenu.hidden) return;
  const target = event.target as Node;
  if (simulatorPickerTrigger?.contains(target) || simulatorPickerMenu.contains(target)) return;
  closeSimulatorPicker();
});

document.addEventListener("keydown", (event) => {
  if (event.key === "Escape") closeSimulatorPicker();
});

/**
 * Fills the header picker once the simulator options are known. Their labels
 * come from the backend rather than the catalog, the same way
 * `activeSimulatorName` does, so no simulator name lives in visible copy.
 */
const applySimulatorPreferenceOptions = (options: SimulatorOption[]): void => {
  const label = simulatorPickerLabel;
  const menu = simulatorPickerMenu;
  if (!label || !menu) return;

  // A build that no longer offers the pinned simulator would keep rejecting
  // it, so fall back to following whichever one is running.
  if (simulatorPreference !== "auto" && !options.some((option) => option.id === simulatorPreference)) {
    simulatorPreference = "auto";
    saveSimulatorPreference("auto");
    void invoke("set_simulator_preference", { simulator: "auto" })
      .catch(reportInitializationError("simulator preference"));
  }

  // With a single simulator offered there is nothing to choose between, so
  // the header keeps the status alone.
  simulatorPickerTrigger?.closest<HTMLElement>(".simulator-picker")?.toggleAttribute("hidden", options.length < 2);
  if (options.length < 2 || menu.childElementCount > 0) return;

  const entries = [{ id: "auto", display_name: t("simulator.auto") }, ...options];
  const optionButtons: HTMLButtonElement[] = [];

  const highlightSimulatorPreference = (): void => {
    label.textContent =
      entries.find((entry) => entry.id === simulatorPreference)?.display_name ?? t("simulator.auto");
    for (const button of optionButtons) {
      const selected = button.dataset.simulatorPreference === simulatorPreference;
      button.classList.toggle("active", selected);
      button.setAttribute("aria-selected", String(selected));
    }
  };

  for (const entry of entries) {
    const option = document.createElement("button");
    option.type = "button";
    option.setAttribute("role", "option");
    option.className = "simulator-picker-option";
    option.dataset.simulatorPreference = entry.id;
    option.textContent = entry.display_name;
    option.addEventListener("click", () => {
      closeSimulatorPicker();
      if (entry.id === simulatorPreference) return;
      const previous = simulatorPreference;
      simulatorPreference = entry.id;
      highlightSimulatorPreference();
      void invoke("set_simulator_preference", { simulator: entry.id }).then(() => {
        saveSimulatorPreference(entry.id);
      }).catch(() => {
        simulatorPreference = previous;
        highlightSimulatorPreference();
      });
    });
    optionButtons.push(option);
    menu.append(option);
  }
  highlightSimulatorPreference();
};

void invoke("set_simulator_preference", { simulator: simulatorPreference })
  .catch(reportInitializationError("simulator preference"));

let performanceProfile = readPerformanceProfile();
const performanceProfileButtons = [
  ...document.querySelectorAll<HTMLButtonElement>("[data-performance-profile]")
];
const renderPerformanceProfile = (): void => {
  for (const button of performanceProfileButtons) {
    const selected = button.dataset.performanceProfile === performanceProfile;
    button.classList.toggle("active", selected);
    button.setAttribute("aria-pressed", String(selected));
  }
};

renderPerformanceProfile();
void invoke("set_performance_profile", { profile: performanceProfile })
  .catch(reportInitializationError("performance profile"));

let spectatorMode = readSpectatorMode();
let teamMode = readTeamMode();
if (spectatorMode && teamMode) {
  spectatorMode = false;
  saveSpectatorMode(false);
}
void invoke("set_spectator_mode", { enabled: spectatorMode })
  .then(() => invoke("set_team_mode", { enabled: teamMode }))
  .catch(reportInitializationError("spectator modes"));

for (const button of performanceProfileButtons) {
  button.addEventListener("click", () => {
    const next = button.dataset.performanceProfile;
    if (!isPerformanceProfile(next) || next === performanceProfile) return;
    const previous = performanceProfile;
    performanceProfile = next;
    renderPerformanceProfile();
    for (const profileButton of performanceProfileButtons) profileButton.disabled = true;
    void invoke("set_performance_profile", { profile: next }).then(() => {
      savePerformanceProfile(next);
    }).catch(() => {
      performanceProfile = previous;
      renderPerformanceProfile();
    }).finally(() => {
      for (const profileButton of performanceProfileButtons) profileButton.disabled = false;
    });
  });
}

const localeSelect = document.getElementById("interface-locale") as HTMLSelectElement | null;
if (localeSelect) {
  for (const option of LOCALE_OPTIONS) localeSelect.add(new Option(option.label, option.code));
  localeSelect.value = getLocale();
  localeSelect.addEventListener("change", () => {
    if (!isLocale(localeSelect.value) || localeSelect.value === getLocale()) return;
    setLocale(localeSelect.value);
    void emit("locale://change", { locale: localeSelect.value })
      .catch(reportInitializationError("locale propagation"))
      .finally(() => window.location.reload());
  });
}

const CURRENT_CONFIGURATION_SCHEMA = 27;
const CURRENT_CONFIGURATION_FORMAT = "blackrack-overlay-configuration";
const LEGACY_CONFIGURATION_FORMAT = "lmu-overlay-configuration";
const overlayIds: OverlayId[] = ["delta", "timing", "stinthistory", "driving", "liftcoast", "tires", "damage", "standings", "relative", "fuel", "pitstop", "flags", "rejoin", "trackmap", "forecast", "conditions", "dashboard", "sessioninfo", "chat"];
const storageKey = "blackrack-overlay.visible-windows.v1";

const defaultVisibility = (): Record<OverlayId, boolean> => Object.fromEntries(
  overlayIds.map((id) => [id, false])
) as Record<OverlayId, boolean>;

const readPreferences = (): Record<OverlayId, boolean> => {
  const defaults = defaultVisibility();

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
const PENDING_CONTROL_VIEW_KEY = "blackrack-overlay.pending-control-view.v1";
const controlViews = new Set(viewButtons.map((button) => button.dataset.controlView).filter(Boolean));

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
const pendingControlView = sessionStorage.getItem(PENDING_CONTROL_VIEW_KEY);
sessionStorage.removeItem(PENDING_CONTROL_VIEW_KEY);
selectControlView(pendingControlView && controlViews.has(pendingControlView) ? pendingControlView : "general");

const appVersion = document.getElementById("app-version");

const UPDATE_CHECK_INTERVAL_MS = 6 * 60 * 60 * 1000;
const UPDATE_CHECK_STORAGE_KEY = "blackrack-overlay.last-update-check.v1";
const UPDATE_AVAILABLE_STORAGE_KEY = "blackrack-overlay.available-update.v1";
const updateCheckButton = document.getElementById("check-for-updates") as HTMLButtonElement | null;
const installUpdateButton = document.getElementById("install-update") as HTMLButtonElement | null;
const updateStatus = document.getElementById("update-status");
const updateDetails = document.getElementById("update-details");
const updateVersion = document.getElementById("update-version");
const updateChangelog = document.getElementById("update-changelog");
const releaseNotesList = document.getElementById("release-notes-list");
const rollbackStatus = document.getElementById("rollback-status");
let updateRequestInFlight = false;
let installedAppVersion: string | null = null;
let rollbackInProgress = false;

const setUpdateControlsDisabled = (disabled: boolean): void => {
  if (updateCheckButton) updateCheckButton.disabled = disabled;
  if (installUpdateButton && !installUpdateButton.hidden) installUpdateButton.disabled = disabled;
  for (const button of document.querySelectorAll<HTMLButtonElement>("[data-release-version]")) {
    button.disabled = disabled;
  }
};

const renderUpdateInfo = (info: UpdateInfo | null): void => {
  if (!updateDetails || !updateVersion || !updateChangelog || !installUpdateButton) return;
  updateChangelog.replaceChildren();
  if (!info) {
    updateDetails.hidden = true;
    installUpdateButton.hidden = true;
    return;
  }
  const selected = selectReleaseNotes(info, getLocale());
  updateVersion.textContent = selected.fullTitle || selected.updateTitle || t("update.available", { version: info.version });
  for (const entry of selected.changelog) {
    const item = document.createElement("li");
    item.textContent = entry;
    updateChangelog.append(item);
  }
  updateDetails.hidden = false;
  installUpdateButton.hidden = false;
}

const setUpdateStatus = (message: string): void => {
  if (updateStatus) updateStatus.textContent = message;
  if (rollbackInProgress && rollbackStatus) rollbackStatus.textContent = message;
};

const renderReleaseNotes = (currentVersion: string): void => {
  if (!releaseNotesList) return;
  releaseNotesList.replaceChildren();
  for (const release of RELEASE_NOTES) {
    const selected = selectReleaseNotes(release, getLocale());
    const article = document.createElement("article");
    article.className = "release-note";

    const header = document.createElement("header");
    const version = document.createElement("span");
    version.className = "release-note-version";
    version.textContent = `v${release.version}`;
    const title = document.createElement("strong");
    title.textContent = selected.fullTitle || selected.updateTitle || `v${release.version}`;
    header.append(version, title);

    const changelog = document.createElement("ul");
    for (const entry of selected.changelog) {
      const item = document.createElement("li");
      item.textContent = entry;
      changelog.append(item);
    }

    article.append(header, changelog);
    if (isNewerUpdateVersion(currentVersion, release.version)) {
      const rollback = document.createElement("button");
      rollback.type = "button";
      rollback.className = "release-note-install";
      rollback.dataset.releaseVersion = release.version;
      rollback.textContent = t("rollback.button", { version: release.version });
      rollback.addEventListener("click", () => void installPreviousVersion(release.version));
      article.append(rollback);
    }
    releaseNotesList.append(article);
  }
};

const readStoredUpdate = (): UpdateInfo | null => {
  const raw = localStorage.getItem(UPDATE_AVAILABLE_STORAGE_KEY);
  if (!raw) return null;
  try {
    const value: unknown = JSON.parse(raw);
    if (value === null || typeof value !== "object") return null;
    const record = value as Record<string, unknown>;
    if (typeof record.version !== "string" || !Array.isArray(record.changelog)
      || !record.changelog.every((entry) => typeof entry === "string")) return null;
    const localized: UpdateInfo["localized"] = {};
    if (record.localized !== undefined) {
      if (typeof record.localized !== "object" || record.localized === null) return null;
      for (const [locale, value] of Object.entries(record.localized)) {
        if (typeof value !== "object" || value === null) return null;
        const localizedRecord = value as Record<string, unknown>;
        if (!Array.isArray(localizedRecord.changelog)
          || !localizedRecord.changelog.every((entry) => typeof entry === "string")) return null;
        localized[locale] = {
          updateTitle: typeof localizedRecord.updateTitle === "string" ? localizedRecord.updateTitle : null,
          fullTitle: typeof localizedRecord.fullTitle === "string" ? localizedRecord.fullTitle : null,
          changelog: localizedRecord.changelog as string[]
        };
      }
    }
    return {
      version: record.version,
      releasePageUrl: typeof record.releasePageUrl === "string" ? record.releasePageUrl : null,
      updateTitle: typeof record.updateTitle === "string" ? record.updateTitle : null,
      fullTitle: typeof record.fullTitle === "string" ? record.fullTitle : null,
      changelog: record.changelog as string[],
      localized
    };
  } catch {
    return null;
  }
};

const parseUpdateVersion = (value: string): number[] | null => {
  const parts = value.split(".");
  if (parts.length === 0 || parts.length > 4 || parts.some((part) => !/^\d+$/.test(part))) return null;
  return parts.map((part) => Number(part));
};

const isNewerUpdateVersion = (candidate: string, current: string): boolean => {
  const candidateParts = parseUpdateVersion(candidate);
  const currentParts = parseUpdateVersion(current);
  if (!candidateParts || !currentParts) return false;
  for (let index = 0; index < 4; index += 1) {
    const candidatePart = candidateParts[index] ?? 0;
    const currentPart = currentParts[index] ?? 0;
    if (candidatePart !== currentPart) return candidatePart > currentPart;
  }
  return false;
};

const restoreStoredUpdate = (currentVersion: string): void => {
  const storedUpdate = readStoredUpdate();
  if (storedUpdate && isNewerUpdateVersion(storedUpdate.version, currentVersion)) {
    renderUpdateInfo(storedUpdate);
    setUpdateStatus(t("update.available", { version: storedUpdate.version }));
    return;
  }
  if (storedUpdate) localStorage.removeItem(UPDATE_AVAILABLE_STORAGE_KEY);
  renderUpdateInfo(null);
};

const checkForUpdates = async (manual: boolean): Promise<void> => {
  if (updateRequestInFlight) return;
  updateRequestInFlight = true;
  setUpdateControlsDisabled(true);
  if (manual && updateCheckButton) updateCheckButton.textContent = t("update.checking");
  try {
    const result = await invoke<UpdateCheckResponse>("check_for_update");
    localStorage.setItem(UPDATE_CHECK_STORAGE_KEY, String(Date.now()));
    if (result.available) {
      localStorage.setItem(UPDATE_AVAILABLE_STORAGE_KEY, JSON.stringify(result.available));
    } else {
      localStorage.removeItem(UPDATE_AVAILABLE_STORAGE_KEY);
    }
    renderUpdateInfo(result.available);
    setUpdateStatus(result.available
      ? t("update.available", { version: result.available.version })
      : t("update.current", { version: result.currentVersion }));
  } catch (error) {
    console.error("No se pudo comprobar si hay actualizaciones", error);
    setUpdateStatus(t("update.unavailable"));
  } finally {
    updateRequestInFlight = false;
    setUpdateControlsDisabled(false);
    if (updateCheckButton) {
      if (manual) updateCheckButton.textContent = t("update.check");
    }
  }
};

const installUpdate = async (): Promise<void> => {
  if (updateRequestInFlight || !installUpdateButton) return;
  updateRequestInFlight = true;
  setUpdateControlsDisabled(true);
  setUpdateStatus(t("update.downloadingUnknown"));
  try {
    await invoke("download_and_install_update");
  } catch (error) {
    console.error("No se pudo instalar la actualización", error);
    setUpdateStatus(t("update.failed"));
    setUpdateControlsDisabled(false);
  } finally {
    updateRequestInFlight = false;
  }
};

const installPreviousVersion = async (version: string): Promise<void> => {
  if (updateRequestInFlight || !installedAppVersion
    || !isNewerUpdateVersion(installedAppVersion, version)) return;
  updateRequestInFlight = true;
  setUpdateControlsDisabled(true);
  const confirmed = await confirmReset(t("rollback.confirmMessage", { version }), {
    heading: t("rollback.confirmHeading"),
    title: t("rollback.confirmTitle"),
    confirmLabel: t("rollback.confirmAction")
  });
  if (!confirmed) {
    updateRequestInFlight = false;
    setUpdateControlsDisabled(false);
    return;
  }

  rollbackInProgress = true;
  setUpdateStatus(t("rollback.installing", { version }));
  try {
    await invoke("rollback_to_version", { version });
  } catch (error) {
    console.error(`No se pudo instalar BlackRack Overlay v${version}`, error);
    setUpdateStatus(t("rollback.failed", { version }));
  } finally {
    rollbackInProgress = false;
    updateRequestInFlight = false;
    setUpdateControlsDisabled(false);
  }
};

updateCheckButton?.addEventListener("click", () => void checkForUpdates(true));
installUpdateButton?.addEventListener("click", () => void installUpdate());
void listen<UpdateProgress>("update://progress", ({ payload }) => {
  if (payload.stage === "downloading") {
    setUpdateStatus(payload.percent === null
      ? t("update.downloadingUnknown")
      : t("update.downloading", { percent: payload.percent }));
  } else if (payload.stage === "verifying") {
    setUpdateStatus(t("update.verifying"));
  } else {
    setUpdateStatus(t("update.installing"));
  }
}).catch((error) => console.error("No se pudo escuchar el progreso de actualización", error));

void getVersion()
  .then((version) => {
    installedAppVersion = version;
    if (appVersion) appVersion.textContent = `v${version}`;
    renderReleaseNotes(version);
    restoreStoredUpdate(version);
  })
  .catch((error) => {
    console.error("No se pudo obtener la versión de la aplicación", error);
  });

void invoke<string | null>("get_update_status")
  .then((status) => {
    if (status) setUpdateStatus(t("update.failed"));
  })
  .catch((error) => console.error("No se pudo leer el estado de actualización", error));

const lastUpdateCheck = Number(localStorage.getItem(UPDATE_CHECK_STORAGE_KEY));
if (!Number.isFinite(lastUpdateCheck) || Date.now() - lastUpdateCheck >= UPDATE_CHECK_INTERVAL_MS) {
  window.setTimeout(() => void checkForUpdates(false), 2000);
}
window.setInterval(() => void checkForUpdates(false), UPDATE_CHECK_INTERVAL_MS);

const supportStatus = document.getElementById("support-status");
const bindSupportButton = (
  buttonId: string,
  command: string,
  openingKey: TranslationKey,
  openedKey: TranslationKey,
  errorKey: TranslationKey,
  errorMessage: string
): void => {
  const button = document.getElementById(buttonId) as HTMLButtonElement | null;
  button?.addEventListener("click", () => {
    button.disabled = true;
    if (supportStatus) supportStatus.textContent = t(openingKey);
    invoke(command)
      .then(() => {
        if (supportStatus) supportStatus.textContent = t(openedKey);
      })
      .catch((error) => {
        console.error(errorMessage, error);
        if (supportStatus) supportStatus.textContent = t(errorKey);
      })
      .finally(() => {
        button.disabled = false;
      });
  });
};

bindSupportButton("open-kofi", "open_support_page", "support.opening", "support.opened", "support.error", "No se pudo abrir Ko-fi");
bindSupportButton("open-paypal", "open_paypal_page", "support.paypalOpening", "support.paypalOpened", "support.paypalError", "No se pudo abrir PayPal");

let activeOverlayFilter = "all";
const overlaySearch = document.getElementById("overlay-search") as HTMLInputElement | null;
const filterButtons = [...document.querySelectorAll<HTMLButtonElement>("[data-overlay-filter]")];

const filterOverlays = (): void => {
  const query = overlaySearch?.value.trim().toLocaleLowerCase(getLocale()) ?? "";
  let matches = 0;
  for (const card of document.querySelectorAll<HTMLElement>("[data-overlay-card]")) {
    const categoryMatches = activeOverlayFilter === "all"
      || (activeOverlayFilter === "active"
        ? card.classList.contains("active")
        : card.dataset.overlayCategory === activeOverlayFilter);
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
let fuelSettings: FuelSettings = readFuelSettings();
let tiresSettings: TiresSettings = readTiresSettings();
let conditionsSettings: ConditionsSettings = readConditionsSettings();
let dashboardSettings: DashboardSettings = readDashboardSettings();
let sessionInfoSettings: SessionInfoSettings = readSessionInfoSettings();
let chatSettings: ChatSettings = readChatSettings();
let liftCoastSettings: LiftCoastSettings = readLiftCoastSettings();
let pitStopSettings: PitStopSettings = readPitStopSettings();
let displayUnits: DisplayUnits = readDisplayUnits();
const temperatureUnitSelect = document.getElementById("temperature-unit") as HTMLSelectElement | null;
const speedUnitSelect = document.getElementById("speed-unit") as HTMLSelectElement | null;
const updateDisplayUnits = (value: unknown): void => {
  displayUnits = persistDisplayUnits(value);
  displayUnits = applyDisplayUnits(displayUnits);
  if (temperatureUnitSelect) temperatureUnitSelect.value = displayUnits.temperature;
  if (speedUnitSelect) speedUnitSelect.value = displayUnits.speed;
  void emit("display-units://change", displayUnits);
  syncBrowserSourcePreferences();
};
if (temperatureUnitSelect) {
  temperatureUnitSelect.value = displayUnits.temperature;
  temperatureUnitSelect.addEventListener("change", () => updateDisplayUnits({
    ...displayUnits, temperature: temperatureUnitSelect.value
  }));
}
if (speedUnitSelect) {
  speedUnitSelect.value = displayUnits.speed;
  speedUnitSelect.addEventListener("change", () => updateDisplayUnits({
    ...displayUnits, speed: speedUnitSelect.value
  }));
}
const overlayTransparency = readOverlayTransparency();
let overlayTransparencyScope: OverlayTransparencyScope = readOverlayTransparencyScope();
const overlayFontSize = readOverlayFontSize();
let overlayFontSizeScope: OverlayFontSizeScope = readOverlayFontSizeScope();

/**
 * Assigned once the profile store exists. Every live overlay setting funnels
 * through syncBrowserSourcePreferences or persist, so the active profile can be
 * snapshotted from a single place without touching each control handler.
 */
let onLiveSettingsChanged: () => void = () => {};

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
      fuel: fuelSettings,
      tires: tiresSettings,
      conditions: conditionsSettings,
      dashboard: dashboardSettings,
      sessionInfo: sessionInfoSettings,
      liftCoast: liftCoastSettings,
      pitstop: pitStopSettings,
      chat: chatSettings,
      displayUnits,
      transparency: effectiveOverlayTransparency(overlayTransparency, overlayTransparencyScope),
      fontSize: effectiveOverlayFontSize(overlayFontSize, overlayFontSizeScope),
      locale: getLocale(),
      supportedLocales: SUPPORTED_LOCALES
    }
  }).catch(() => undefined);
  void invoke("set_delta_settings", { settings: deltaSettings }).catch(() => undefined);
  void invoke("set_timing_settings", { settings: timingSettings }).catch(() => undefined);
  void invoke("set_fuel_refuel_margin", { liters: fuelSettings.refuelMarginLiters }).catch(() => undefined);
  void invoke("set_energy_refill_margin", { percent: fuelSettings.energyMarginPercent }).catch(() => undefined);
  onLiveSettingsChanged();
};

const shortcutInputs: Partial<Record<ShortcutAction, HTMLInputElement | null>> = {
  interaction_mode: document.getElementById("shortcut-interaction-mode") as HTMLInputElement | null,
  show_panel: document.getElementById("shortcut-show-panel") as HTMLInputElement | null,
  toggle_overlays: document.getElementById("shortcut-toggle-overlays") as HTMLInputElement | null
};

const setShortcutMessage = (message: string, state: "normal" | "error" | "success" = "normal"): void => {
  const element = document.getElementById("shortcut-message");
  if (!element) return;
  element.textContent = message;
  element.classList.toggle("error", state === "error");
  element.classList.toggle("success", state === "success");
};

const renderShortcutSettings = (status: ShortcutSettingsStatus): void => {
  for (const action of ["interaction_mode", "show_panel", "toggle_overlays"] as const) {
    const input = shortcutInputs[action];
    const binding = status[action];
    if (input) {
      input.value = binding.shortcut;
      input.dataset.state = binding.active ? "active" : "error";
      input.title = t(binding.active ? "shortcuts.active" : "shortcuts.unavailable");
    }
  }

  for (const id of overlayIds) {
    const input = overlayShortcutInputs.get(id);
    const binding = status.hide_overlays?.[id];
    if (!input || !binding) continue;
    const disabled = !binding.shortcut;
    input.value = binding.shortcut;
    input.dataset.state = binding.active ? "active" : disabled ? "disabled" : "error";
    input.title = binding.error
      ? `${t("shortcuts.unavailable")}: ${binding.error}`
      : t(disabled ? "shortcuts.disabled" : binding.active ? "shortcuts.active" : "shortcuts.unavailable");
  }

  const interactionFooter = document.getElementById("footer-interaction-shortcut");
  const panelFooter = document.getElementById("footer-panel-shortcut");
  if (interactionFooter) interactionFooter.textContent = status.interaction_mode.shortcut;
  if (panelFooter) panelFooter.textContent = status.show_panel.shortcut;

  const unavailable = [
    status.interaction_mode,
    status.show_panel,
    status.toggle_overlays,
    ...overlayIds.map((id) => status.hide_overlays?.[id]).filter(
      (binding): binding is ShortcutBindingStatus => Boolean(binding)
    )
  ].find((binding) => Boolean(binding.shortcut) && !binding.active);
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
    setShortcutMessage(
      shortcut ? t("shortcuts.saved", { shortcut }) : t("shortcuts.cleared"),
      "success"
    );
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
    console.error("Could not update shortcut:", error);
    try {
      renderShortcutSettings(await invoke<ShortcutSettingsStatus>("get_shortcut_settings"));
    } catch (reloadError) {
      console.error("Could not reload shortcut settings:", reloadError);
    }
  } finally {
    if (input) input.disabled = false;
  }
};

const bindShortcutCapture = (action: ShortcutAction, input: HTMLInputElement): void => {
  input.addEventListener("focus", () => {
    setShortcutMessage(t("shortcuts.capture"));
    input.select();
  });
  input.addEventListener("keydown", (event) => {
    if (event.key === "Tab") return;
    event.preventDefault();
    if (event.key === "Escape") {
      input.blur();
      setShortcutMessage(t("shortcuts.cancelled"));
      return;
    }
    if (
      (event.key === "Delete" || event.key === "Backspace")
      && action.startsWith("hide_")
    ) {
      input.value = "";
      input.blur();
      void saveShortcut(action, "");
      return;
    }
    const shortcut = shortcutFromKeyboardEvent(event);
    if (shortcut) {
      input.value = shortcut;
      input.blur();
      void saveShortcut(action, shortcut);
    }
  });
};

for (const action of ["interaction_mode", "show_panel", "toggle_overlays"] as const) {
  const input = shortcutInputs[action];
  if (input) bindShortcutCapture(action, input);
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
    option.textContent = sentenceCase(t(labelKey));
    return option;
  }));
  deltaModeSelect.value = deltaSettings.mode;
  deltaModeSelect.addEventListener("change", () => {
    if (!isDeltaMode(deltaModeSelect.value)) return;
    deltaSettings = { ...deltaSettings, mode: deltaModeSelect.value };
    persistDeltaSettings();
  });
}

const wheelBindingOutput = document.getElementById("delta-wheel-binding") as HTMLOutputElement | null;
const wheelCaptureButton = document.getElementById("capture-delta-wheel-button") as HTMLButtonElement | null;
const wheelClearButton = document.getElementById("clear-delta-wheel-button") as HTMLButtonElement | null;
const wheelMessage = document.getElementById("delta-wheel-message");
let wheelInputStatus: WheelInputStatus | null = null;

const renderWheelInputStatus = (status: WheelInputStatus): void => {
  wheelInputStatus = status;
  if (wheelBindingOutput) {
    wheelBindingOutput.textContent = status.binding
      ? t("wheel.saved", { device: status.binding.deviceName, button: status.binding.button + 1 })
      : t("wheel.unassigned");
    wheelBindingOutput.title = wheelBindingOutput.textContent;
  }
  if (wheelCaptureButton) {
    wheelCaptureButton.disabled = !status.available || status.capturing;
    wheelCaptureButton.textContent = t(status.capturing ? "wheel.capturing" : "wheel.assign");
  }
  if (wheelClearButton) wheelClearButton.disabled = !status.binding || status.capturing;
  if (wheelMessage) {
    wheelMessage.textContent = t(
      status.error === "persistence_failed"
        ? "wheel.error"
        : !status.available
          ? "wheel.unavailable"
          : status.capturing
            ? "wheel.capturing"
            : "wheel.instruction"
    );
  }
};
const persistFuelSettings = (): void => {
  localStorage.setItem(FUEL_SETTINGS_KEY, JSON.stringify(fuelSettings));
  void emit("fuel://settings", fuelSettings);
  syncBrowserSourcePreferences();
};

const persistTiresSettings = (): void => {
  localStorage.setItem(TIRES_SETTINGS_KEY, JSON.stringify(tiresSettings));
  void emit("tires://settings", tiresSettings);
  syncBrowserSourcePreferences();
};

const persistConditionsSettings = (): void => {
  localStorage.setItem(CONDITIONS_SETTINGS_KEY, JSON.stringify(conditionsSettings));
  void emit("conditions://settings", conditionsSettings);
  syncBrowserSourcePreferences();
};

const persistDashboardSettings = (): void => {
  localStorage.setItem(DASHBOARD_SETTINGS_KEY, JSON.stringify(dashboardSettings));
  void emit("dashboard://settings", dashboardSettings);
  syncBrowserSourcePreferences();
};

const persistSessionInfoSettings = (): void => {
  localStorage.setItem(SESSIONINFO_SETTINGS_KEY, JSON.stringify(sessionInfoSettings));
  void emit("sessioninfo://settings", sessionInfoSettings);
  syncBrowserSourcePreferences();
};
const overlayShortcutInputs = new Map<OverlayId, HTMLInputElement>();

const persistLiftCoastSettings = (): void => {
  localStorage.setItem(LIFTCOAST_SETTINGS_KEY, JSON.stringify(liftCoastSettings));
  void emit("liftcoast://settings", liftCoastSettings);
  syncBrowserSourcePreferences();
};

const persistPitStopSettings = (): void => {
  localStorage.setItem(PITSTOP_SETTINGS_KEY, JSON.stringify(pitStopSettings));
  void emit("pitstop://settings", pitStopSettings);
  syncBrowserSourcePreferences();
};

wheelCaptureButton?.addEventListener("click", () => {
  void invoke<WheelInputStatus>("capture_delta_wheel_button")
    .then(renderWheelInputStatus)
    .catch(() => { if (wheelMessage) wheelMessage.textContent = t("wheel.unavailable"); });
});
wheelClearButton?.addEventListener("click", () => {
  void invoke<WheelInputStatus>("clear_delta_wheel_button")
    .then(renderWheelInputStatus)
    .catch(() => { if (wheelMessage) wheelMessage.textContent = t("wheel.error"); });
});
window.addEventListener("keydown", (event) => {
  if (event.key !== "Escape" || !wheelInputStatus?.capturing) return;
  event.preventDefault();
  event.stopPropagation();
  void invoke<WheelInputStatus>("cancel_delta_wheel_button_capture")
    .then(renderWheelInputStatus)
    .catch(() => { if (wheelMessage) wheelMessage.textContent = t("wheel.error"); });
});
void listen<WheelInputStatus>("wheel-input://status", ({ payload }) => renderWheelInputStatus(payload))
  .catch(reportInitializationError("wheel input listener"));
void invoke<WheelInputStatus>("get_wheel_input_status").then(renderWheelInputStatus).catch(() => {
  renderWheelInputStatus({ available: false, capturing: false, binding: null, error: "unavailable" });
});
void listen<import("./delta-settings").DeltaMode>("delta://mode-changed", ({ payload: mode }) => {
  if (!isDeltaMode(mode)) return;
  deltaSettings = { ...deltaSettings, mode };
  if (deltaModeSelect) deltaModeSelect.value = mode;
  persistDeltaSettings();
}).catch(reportInitializationError("delta mode listener"));
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
    option.textContent = sentenceCase(t(labelKey));
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
    trackMapSettings = { ...trackMapSettings, showPitPrediction: trackMapPitPrediction.checked };
    persistTrackMapSettings();
  });
}

const mapColor = document.getElementById("trackmap-player-color") as HTMLInputElement | null;
const mapColorMode = document.getElementById("trackmap-player-color-mode");
const mapImage = document.getElementById("trackmap-player-image") as HTMLInputElement | null;
const mapChooseImage = document.getElementById("trackmap-player-image-choose") as HTMLButtonElement | null;
const mapRemoveImage = document.getElementById("trackmap-player-image-remove") as HTMLButtonElement | null;
const mapResetColor = document.getElementById("trackmap-player-color-reset") as HTMLButtonElement | null;
const mapImageStatus = document.getElementById("trackmap-player-image-status");
let mapIconUploadGeneration = 0;
const syncMapStyleControls = (): void => {
  if (mapColor) mapColor.value = trackMapSettings.playerColor ?? "#e33b3b";
  if (mapColorMode) mapColorMode.textContent = t(trackMapSettings.playerColor
    ? "settings.mapPlayerColorCustom" : "settings.mapPlayerColorClass");
  if (mapResetColor) mapResetColor.disabled = trackMapSettings.playerColor === null;
  if (mapRemoveImage) mapRemoveImage.disabled = !trackMapSettings.playerIconDataUrl;
};
syncMapStyleControls();
mapColor?.addEventListener("input", () => {
  trackMapSettings = { ...trackMapSettings, playerColor: mapColor.value };
  syncMapStyleControls();
  persistTrackMapSettings();
});
mapResetColor?.addEventListener("click", () => {
  trackMapSettings = { ...trackMapSettings, playerColor: null };
  syncMapStyleControls();
  persistTrackMapSettings();
});
mapRemoveImage?.addEventListener("click", () => {
  mapIconUploadGeneration += 1;
  trackMapSettings = { ...trackMapSettings, playerIconDataUrl: null };
  if (mapImage) mapImage.value = "";
  if (mapImageStatus) mapImageStatus.textContent = "";
  syncMapStyleControls();
  persistTrackMapSettings();
});
mapChooseImage?.addEventListener("click", () => mapImage?.click());
const imageDimensions = (bytes: Uint8Array): [number, number] | null => {
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  if (bytes.length >= 33 && bytes[0] === 137 && bytes[1] === 80 && bytes[2] === 78 && bytes[3] === 71
    && bytes[4] === 13 && bytes[5] === 10 && bytes[6] === 26 && bytes[7] === 10) {
    if (view.getUint32(8) !== 13 || String.fromCharCode(...bytes.slice(12, 16)) !== "IHDR") return null;
    const dimensions: [number, number] = [view.getUint32(16), view.getUint32(20)];
    let offset = 8;
    let chunkCount = 0;
    let hasImageData = false;
    let hasEnd = false;
    while (offset + 12 <= bytes.length) {
      if (++chunkCount > 1024) return null;
      const length = view.getUint32(offset);
      if (length > bytes.length - offset - 12) return null;
      const type = String.fromCharCode(...bytes.slice(offset + 4, offset + 8));
      if (type === "acTL" || type === "fcTL" || type === "fdAT") return null;
      if (type === "IDAT") hasImageData = true;
      offset += length + 12;
      if (type === "IEND") {
        hasEnd = length === 0 && offset === bytes.length;
        break;
      }
    }
    return hasImageData && hasEnd ? dimensions : null;
  }
  if (bytes[0] === 0xff && bytes[1] === 0xd8) {
    let i = 2;
    while (i + 9 < bytes.length) {
      if (bytes[i++] !== 0xff) continue;
      let marker = bytes[i++]; while (marker === 0xff) marker = bytes[i++];
      if (marker === 0xd8 || marker === 0xd9 || (marker >= 0xd0 && marker <= 0xd7)) continue;
      const length = view.getUint16(i); if (length < 2 || i + length > bytes.length) return null;
      if ([0xc0,0xc1,0xc2,0xc3,0xc5,0xc6,0xc7,0xc9,0xca,0xcb,0xcd,0xce,0xcf].includes(marker)) return [view.getUint16(i + 5), view.getUint16(i + 3)];
      i += length;
    }
  }
  if (bytes.length >= 30 && String.fromCharCode(...bytes.slice(0,4)) === "RIFF" && String.fromCharCode(...bytes.slice(8,12)) === "WEBP") {
    const riffEnd = view.getUint32(4, true) + 8;
    if (riffEnd !== bytes.length) return null;
    let offset = 12;
    let chunks = 0;
    while (offset + 8 <= riffEnd) {
      if (++chunks > 1024) return null;
      const type = String.fromCharCode(...bytes.slice(offset, offset + 4));
      const length = view.getUint32(offset + 4, true);
      if (type === "ANIM" || type === "ANMF" || length > riffEnd - offset - 8) return null;
      offset += 8 + length + (length & 1);
    }
    if (offset !== riffEnd) return null;
    const kind = String.fromCharCode(...bytes.slice(12,16));
    if (kind === "VP8X") {
      if (bytes[20] & 0x02) return null;
      return [1 + bytes[24] + (bytes[25]<<8) + (bytes[26]<<16), 1 + bytes[27] + (bytes[28]<<8) + (bytes[29]<<16)];
    }
    if (kind === "VP8L" && bytes[20] === 0x2f) return [1 + bytes[21] + ((bytes[22]&0x3f)<<8), 1 + (bytes[22]>>6) + (bytes[23]<<2) + ((bytes[24]&0x0f)<<10)];
    if (kind === "VP8 " && bytes[23] === 0x9d && bytes[24] === 0x01 && bytes[25] === 0x2a) return [view.getUint16(26,true)&0x3fff, view.getUint16(28,true)&0x3fff];
  }
  return null;
};
mapImage?.addEventListener("change", async () => {
  const generation = ++mapIconUploadGeneration;
  const file = mapImage.files?.[0]; if (!file) return;
  const fail = (key: "settings.mapImageSizeError" | "settings.mapImageFormatError" | "settings.mapImageDimensionError" | "settings.mapImageOutputError"): void => {
    if (generation !== mapIconUploadGeneration) return;
    if (mapImageStatus) mapImageStatus.textContent = t(key);
    mapImage.value = "";
  };
  if (file.size > 1024 * 1024) { fail("settings.mapImageSizeError"); return; }
  try {
    const bytes = new Uint8Array(await file.arrayBuffer());
    if (generation !== mapIconUploadGeneration) return;
    const dims = imageDimensions(bytes);
    const isPng = bytes[0] === 137 && bytes[1] === 80 && bytes[2] === 78 && bytes[3] === 71;
    const isJpeg = bytes[0] === 0xff && bytes[1] === 0xd8;
    const isWebp = String.fromCharCode(...bytes.slice(0, 4)) === "RIFF" && String.fromCharCode(...bytes.slice(8, 12)) === "WEBP";
    if (!dims || !((file.type === "image/png" && isPng) || (file.type === "image/jpeg" && isJpeg)
      || (file.type === "image/webp" && isWebp))) { fail("settings.mapImageFormatError"); return; }
    if (dims[0] < 1 || dims[1] < 1 || dims[0] > 256 || dims[1] > 256) { fail("settings.mapImageDimensionError"); return; }
    const bitmap = await createImageBitmap(new Blob([bytes], { type: file.type }));
    if (generation !== mapIconUploadGeneration) { bitmap.close(); return; }
    const scale = Math.min(128 / bitmap.width, 128 / bitmap.height);
    const width = Math.max(1, Math.round(bitmap.width * scale));
    const height = Math.max(1, Math.round(bitmap.height * scale));
    const canvas = document.createElement("canvas"); canvas.width = 128; canvas.height = 128;
    try {
      canvas.getContext("2d")!.drawImage(bitmap, (128 - width) / 2, (128 - height) / 2, width, height);
    } finally {
      bitmap.close();
    }
    const dataUrl = canvas.toDataURL("image/png");
    const normalized = normalizeTrackMapSettings({ ...trackMapSettings, playerIconDataUrl: dataUrl });
    if (!normalized.playerIconDataUrl || dataUrl.length > 128 * 1024) { fail("settings.mapImageOutputError"); return; }
    trackMapSettings = normalized;
    if (mapImageStatus) mapImageStatus.textContent = t("settings.mapImageReady");
    syncMapStyleControls(); persistTrackMapSettings();
  } catch { fail("settings.mapImageFormatError"); }
});

const fuelScenarioMode = document.getElementById("fuel-scenario-mode") as HTMLSelectElement | null;
const fuelRefuelMargin = document.getElementById("fuel-refuel-margin") as HTMLInputElement | null;
const fuelEnergyMargin = document.getElementById("fuel-energy-margin") as HTMLInputElement | null;
if (fuelEnergyMargin) {
  fuelEnergyMargin.value = String(fuelSettings.energyMarginPercent);
  fuelEnergyMargin.addEventListener("change", () => {
    const value = fuelEnergyMargin.valueAsNumber;
    if (!Number.isFinite(value) || value < 0 || value > 20) {
      fuelEnergyMargin.value = String(fuelSettings.energyMarginPercent);
      return;
    }
    fuelSettings = { ...fuelSettings, energyMarginPercent: value };
    persistFuelSettings();
  });
}
if (fuelRefuelMargin) {
  fuelRefuelMargin.value = String(fuelSettings.refuelMarginLiters);
  fuelRefuelMargin.addEventListener("change", () => {
    const value = fuelRefuelMargin.valueAsNumber;
    if (!Number.isFinite(value) || value < 0 || value > 20) {
      fuelRefuelMargin.value = String(fuelSettings.refuelMarginLiters);
      return;
    }
    fuelSettings = { ...fuelSettings, refuelMarginLiters: value };
    persistFuelSettings();
  });
}
if (fuelScenarioMode) {
  fuelScenarioMode.value = fuelSettings.scenarioMode;
  fuelScenarioMode.addEventListener("change", () => {
    if (!isFuelScenarioMode(fuelScenarioMode.value)) return;
    fuelSettings = { ...fuelSettings, scenarioMode: fuelScenarioMode.value };
    persistFuelSettings();
  });
}

const tiresToggles: Array<[string, keyof TiresSettings]> = [
  ["tires-tire-temperature", "showTireTemperature"],
  ["tires-brake-temperature", "showBrakeTemperature"],
  ["tires-flat-spot", "showFlatSpot"],
  ["tires-tire-wear", "showTireWear"],
  ["tires-oil-temperature", "showOilTemperature"],
  ["tires-water-temperature", "showWaterTemperature"]
];
for (const [id, setting] of tiresToggles) {
  const toggle = document.getElementById(id) as HTMLInputElement | null;
  if (toggle) {
    toggle.checked = tiresSettings[setting];
    toggle.addEventListener("change", () => {
      tiresSettings = { ...tiresSettings, [setting]: toggle.checked };
      persistTiresSettings();
    });
  }
}

const inputFor = (id: OverlayId): HTMLInputElement | null =>
  document.querySelector<HTMLInputElement>(`input[data-overlay="${id}"]`);

const spectatorDisabledOverlays: readonly OverlayId[] = ["liftcoast", "stinthistory", "fuel"];
const disabledInSpectator = (id: OverlayId): boolean =>
  spectatorMode && spectatorDisabledOverlays.includes(id);

const renderOverlayAvailability = (id: OverlayId): void => {
  const card = document.querySelector<HTMLElement>(`[data-overlay-card="${id}"]`);
  const blocked = disabledInSpectator(id);
  const unsupported = card?.hasAttribute("data-unsupported") ?? false;
  card?.toggleAttribute("data-spectator-disabled", blocked);
  if (card) card.title = blocked ? t("card.spectatorDisabled")
    : unsupported ? t("card.unsupported", { simulator: activeSimulatorName() }) : "";
  const input = inputFor(id);
  if (input) input.disabled = blocked || unsupported;
};

const activeOverlaySummary = document.getElementById("overlay-active-summary");

const renderActiveOverlaySummary = (): void => {
  if (!activeOverlaySummary) return;
  const count = document.querySelectorAll("[data-overlay-card].active").length;
  activeOverlaySummary.textContent = t("control.activeCount", { count, total: overlayIds.length });
};

const setCardState = (id: OverlayId, visible: boolean): void => {
  visible = visible && !disabledInSpectator(id);
  const input = inputFor(id);
  if (input) input.checked = visible;
  document
    .querySelector<HTMLElement>(`[data-overlay-card="${id}"]`)
    ?.classList.toggle("active", visible);
  renderActiveOverlaySummary();
  if (activeOverlayFilter === "active") filterOverlays();
};

const persist = (): void => {
  localStorage.setItem(storageKey, JSON.stringify(preferences));
  onLiveSettingsChanged();
};

const setOverlay = async (id: OverlayId, visible: boolean): Promise<void> => {
  const input = inputFor(id);
  if (input) input.disabled = true;

  try {
    const blocked = disabledInSpectator(id);
    const actual = await invoke<boolean>("set_overlay_visible", { label: id, visible: visible && !blocked });
    // Mode restrictions affect the mounted panels, not the saved profile.
    preferences[id] = blocked ? visible : actual;
    setCardState(id, actual);
    persist();
    await synchronizeOverlayHosts();
  } catch (error) {
    console.error(`No se pudo cambiar la ventana ${id}:`, error);
    setCardState(id, preferences[id]);
  } finally {
    renderOverlayAvailability(id);
  }
};

const setAll = async (visible: boolean): Promise<void> => {
  for (const id of overlayIds) {
    if (disabledInSpectator(id)) continue;
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

const confirmReset = (
  message: string,
  labels: { heading?: string; title?: string; confirmLabel?: string } = {}
): Promise<boolean> => {
  const dialog = document.getElementById("reset-confirmation") as HTMLDialogElement | null;
  const headingElement = document.getElementById("reset-confirmation-heading");
  const titleElement = document.getElementById("reset-confirmation-title");
  const messageElement = document.getElementById("reset-confirmation-message");
  const confirmButton = dialog?.querySelector<HTMLButtonElement>('button[value="confirm"]');
  if (!dialog || !headingElement || !titleElement || !messageElement || !confirmButton) return Promise.resolve(false);
  headingElement.textContent = labels.heading ?? t("reset.heading");
  titleElement.textContent = labels.title ?? t("reset.title");
  messageElement.textContent = message;
  confirmButton.textContent = labels.confirmLabel ?? t("reset.confirm");
  dialog.returnValue = "cancel";
  dialog.showModal();
  confirmButton.focus();
  return new Promise((resolve) => {
    dialog.addEventListener("close", () => resolve(dialog.returnValue === "confirm"), { once: true });
  });
};

const overlayDisplayName = (id: OverlayId): string =>
  document.querySelector<HTMLElement>(`[data-overlay-card="${id}"] .overlay-copy strong`)
    ?.textContent?.trim() || id;

const overlayGuideDialog = document.getElementById("overlay-guide") as HTMLDialogElement | null;
const overlayGuideNavigation = document.getElementById("overlay-guide-navigation");
let selectedGuideOverlay: OverlayId = "standings";

const renderOverlayGuide = (id: OverlayId): void => {
  selectedGuideOverlay = id;
  const entry = OVERLAY_GUIDE[id];
  const icon = document.getElementById("overlay-guide-icon");
  const title = document.getElementById("overlay-guide-overlay-title");
  const purpose = document.getElementById("overlay-guide-purpose");
  const reading = document.getElementById("overlay-guide-reading");
  const warnings = document.getElementById("overlay-guide-warnings");
  const warningsSection = document.getElementById("overlay-guide-warnings-section");
  const tip = document.getElementById("overlay-guide-tip");
  if (icon) icon.textContent = entry.icon;
  if (title) title.textContent = t(entry.title);
  const simulator = activeSimulatorName();
  if (purpose) purpose.textContent = t(entry.purpose, { simulator });
  if (reading) reading.textContent = t(entry.reading, { simulator });
  // Only some overlays have cues that appear on their own; the rest never show
  // this section rather than showing an empty one.
  warningsSection?.toggleAttribute("hidden", !entry.warnings);
  if (warnings && entry.warnings) warnings.textContent = t(entry.warnings, { simulator });
  if (tip) tip.textContent = t(entry.tip, { simulator });
  overlayGuideNavigation?.querySelectorAll<HTMLButtonElement>("button[data-guide-overlay]").forEach((button) => {
    const selected = button.dataset.guideOverlay === id;
    button.classList.toggle("active", selected);
    button.setAttribute("aria-current", selected ? "true" : "false");
  });
};

if (overlayGuideNavigation) {
  for (const id of OVERLAY_GUIDE_ORDER) {
    const entry = OVERLAY_GUIDE[id];
    const button = document.createElement("button");
    const icon = document.createElement("span");
    const label = document.createElement("b");
    button.type = "button";
    button.dataset.guideOverlay = id;
    icon.textContent = entry.icon;
    icon.setAttribute("aria-hidden", "true");
    label.textContent = t(entry.title);
    button.append(icon, label);
    button.addEventListener("click", () => renderOverlayGuide(id));
    overlayGuideNavigation.append(button);
  }
}

const openOverlayGuide = (id: OverlayId): void => {
  if (!overlayGuideDialog) return;
  renderOverlayGuide(id);
  if (!overlayGuideDialog.open) overlayGuideDialog.showModal();
  window.setTimeout(() => {
    overlayGuideNavigation
      ?.querySelector<HTMLButtonElement>(`button[data-guide-overlay="${id}"]`)
      ?.scrollIntoView({ block: "nearest" });
  });
};

document.getElementById("open-overlay-guide")?.addEventListener("click", () => {
  openOverlayGuide(selectedGuideOverlay);
});
overlayGuideDialog?.addEventListener("click", (event) => {
  if (event.target === overlayGuideDialog) overlayGuideDialog.close();
});
renderOverlayGuide(selectedGuideOverlay);

const applyOverlayConfigurationDefaults = (id: OverlayId, events: Promise<unknown>[]): void => {
  overlayTransparency[id] = DEFAULT_OVERLAY_TRANSPARENCY[id];
  overlayFontSize[id] = DEFAULT_OVERLAY_FONT_SIZE[id];

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
  } else if (id === "fuel") {
    fuelSettings = defaultFuelSettings();
    localStorage.setItem(FUEL_SETTINGS_KEY, JSON.stringify(fuelSettings));
    events.push(emit("fuel://settings", fuelSettings));
  } else if (id === "trackmap") {
    trackMapSettings = defaultTrackMapSettings();
    localStorage.setItem(TRACK_MAP_SETTINGS_KEY, JSON.stringify(trackMapSettings));
    events.push(emit("trackmap://settings", trackMapSettings));
  } else if (id === "tires") {
    tiresSettings = defaultTiresSettings();
    localStorage.setItem(TIRES_SETTINGS_KEY, JSON.stringify(tiresSettings));
    events.push(emit("tires://settings", tiresSettings));
  } else if (id === "conditions") {
    conditionsSettings = defaultConditionsSettings();
    localStorage.setItem(CONDITIONS_SETTINGS_KEY, JSON.stringify(conditionsSettings));
    events.push(emit("conditions://settings", conditionsSettings));
  } else if (id === "dashboard") {
    dashboardSettings = defaultDashboardSettings();
    localStorage.setItem(DASHBOARD_SETTINGS_KEY, JSON.stringify(dashboardSettings));
    events.push(emit("dashboard://settings", dashboardSettings));
  } else if (id === "sessioninfo") {
    sessionInfoSettings = defaultSessionInfoSettings();
    localStorage.setItem(SESSIONINFO_SETTINGS_KEY, JSON.stringify(sessionInfoSettings));
    events.push(emit("sessioninfo://settings", sessionInfoSettings));
  } else if (id === "chat") {
    chatSettings = defaultChatSettings();
    localStorage.setItem(CHAT_SETTINGS_KEY, JSON.stringify(chatSettings));
    events.push(emit(CHAT_SETTINGS_EVENT, chatSettings));
    const input = document.getElementById("chat-max-messages") as HTMLInputElement | null;
    const output = document.getElementById("chat-max-messages-value");
    const heightInput = document.getElementById("chat-max-height") as HTMLInputElement | null;
    const heightOutput = document.getElementById("chat-max-height-value");
    if (input) input.value = String(chatSettings.maxMessages);
    if (output) output.textContent = String(chatSettings.maxMessages);
    if (heightInput) heightInput.value = String(chatSettings.maxHeight);
    if (heightOutput) heightOutput.textContent = `${chatSettings.maxHeight} px`;
  } else if (id === "liftcoast") {
    liftCoastSettings = defaultLiftCoastSettings();
    localStorage.setItem(LIFTCOAST_SETTINGS_KEY, JSON.stringify(liftCoastSettings));
    events.push(emit("liftcoast://settings", liftCoastSettings));
  } else if (id === "pitstop") {
    pitStopSettings = defaultPitStopSettings();
    localStorage.setItem(PITSTOP_SETTINGS_KEY, JSON.stringify(pitStopSettings));
    events.push(emit("pitstop://settings", pitStopSettings));
  }
};

const reloadKeepingActiveView = (): void => {
  const activeView = document.querySelector<HTMLElement>(".control-panel")?.dataset.activeView;
  if (activeView && controlViews.has(activeView)) {
    sessionStorage.setItem(PENDING_CONTROL_VIEW_KEY, activeView);
  }
  window.location.reload();
};

const publishOverlayConfigurationReset = async (
  ids: readonly OverlayId[],
  events: Promise<unknown>[]
): Promise<void> => {
  localStorage.setItem(OVERLAY_TRANSPARENCY_KEY, JSON.stringify(overlayTransparency));
  localStorage.setItem(OVERLAY_FONT_SIZE_KEY, JSON.stringify(overlayFontSize));
  const effective = effectiveOverlayTransparency(overlayTransparency, overlayTransparencyScope);
  const effectiveFontSize = effectiveOverlayFontSize(overlayFontSize, overlayFontSizeScope);
  for (const id of ids) {
    events.push(emit("overlay://background-transparency", {
      overlay: id,
      transparency: effective[id]
    } satisfies OverlayTransparencyChange));
    events.push(emit("overlay://font-size", {
      overlay: id,
      fontSize: effectiveFontSize[id]
    } satisfies OverlayFontSizeChange));
  }
  syncBrowserSourcePreferences();
  await Promise.all(events);
  reloadKeepingActiveView();
};

/**
 * Overlay profiles. A profile owns the overlay-facing configuration only;
 * performance profile, locale, shortcuts and the browser source stay global.
 * The general monitor control moves every placement together; per-overlay
 * monitor assignments remain part of the profile layout.
 */
const layoutIsComplete = (layout: unknown): layout is CompositeLayout =>
  layout !== null && typeof layout === "object"
    && overlayIds.every((id) => (layout as Record<string, unknown>)[id] !== undefined);

const completeProfileLayout = async (value: unknown): Promise<CompositeLayout> => {
  const fallback = await ensureCompositeLayout();
  const source = value !== null && typeof value === "object" && !Array.isArray(value)
    ? value as Record<string, unknown>
    : {};
  return Object.fromEntries(overlayIds.map((id) => {
    const placement = source[id];
    if (!placement || typeof placement !== "object" || Array.isArray(placement)) {
      return [id, fallback[id]];
    }
    const candidate = placement as Record<string, unknown>;
    const valid = candidate.overlay === id
      && [candidate.x, candidate.y, candidate.width, candidate.height]
        .every((number) => typeof number === "number" && Number.isFinite(number))
      && Number(candidate.width) > 0 && Number(candidate.height) > 0
      && (candidate.monitor === undefined
        || (typeof candidate.monitor === "number"
          && Number.isInteger(candidate.monitor) && candidate.monitor >= 0))
      && (candidate.scale === undefined
        || (typeof candidate.scale === "number"
          && Number.isFinite(candidate.scale) && candidate.scale > 0));
    return [id, valid ? candidate : fallback[id]];
  })) as unknown as CompositeLayout;
};

const captureProfileData = (previous?: OverlayProfileData): OverlayProfileData => {
  const layout = readCompositeLayout();
  return {
    visibility: { ...preferences },
    transparency: {
      scope: { ...overlayTransparencyScope },
      values: { ...overlayTransparency }
    },
    fontSize: {
      scope: { ...overlayFontSizeScope },
      values: { ...overlayFontSize }
    },
    monitorScope: { ...readOverlayMonitorScope() },
    layout: layoutIsComplete(layout) ? layout : (previous?.layout ?? {} as CompositeLayout),
    standings: standingsSettings,
    relative: relativeSettings,
    driving: drivingSettings,
    delta: deltaSettings,
    timing: timingSettings,
    trackMap: trackMapSettings,
    fuel: fuelSettings,
    tires: tiresSettings,
    conditions: conditionsSettings,
    dashboard: dashboardSettings,
    sessionInfo: sessionInfoSettings,
    liftCoast: liftCoastSettings,
    pitstop: pitStopSettings,
    chat: chatSettings
  };
};

const defaultProfileData = (layout: CompositeLayout): OverlayProfileData => ({
  visibility: defaultVisibility(),
  transparency: {
    scope: { mode: "individual", globalTransparency: 100 },
    values: { ...DEFAULT_OVERLAY_TRANSPARENCY }
  },
  fontSize: {
    scope: { mode: "individual", globalFontSize: 100 },
    values: { ...DEFAULT_OVERLAY_FONT_SIZE }
  },
  monitorScope: defaultOverlayMonitorScope(),
  layout,
  standings: defaultStandingsSettings(),
  relative: defaultRelativeSettings(),
  driving: defaultDrivingSettings(),
  delta: defaultDeltaSettings(),
  timing: defaultTimingSettings(),
  trackMap: defaultTrackMapSettings(),
  fuel: defaultFuelSettings(),
  tires: defaultTiresSettings(),
  conditions: defaultConditionsSettings(),
  dashboard: defaultDashboardSettings(),
  sessionInfo: defaultSessionInfoSettings(),
  liftCoast: defaultLiftCoastSettings(),
  pitstop: defaultPitStopSettings(),
  chat: defaultChatSettings()
});

let activeMode: OverlayMode = modeFromFlags(spectatorMode, teamMode);
let profileState: ProfileState = readProfileState(t("profiles.defaultName"))
  ?? (() => {
    const profile: OverlayProfile = {
      id: createProfileId(),
      name: t("profiles.defaultName"),
      data: captureProfileData()
    };
    const state: ProfileState = {
      profiles: [profile],
      bindings: { game: profile.id, spectator: profile.id, team: profile.id },
      sessionBindings: { practice: null, qualifying: null, race: null }
    };
    saveProfileState(state);
    return state;
  })();

/**
 * The live session kind, kept in `sessionStorage` because applying a profile
 * reloads the panel: without it the reloaded panel would resolve back to the
 * game binding, the next frame would switch again and the two would loop. It is
 * deliberately not persisted beyond the window, so a fresh start waits for
 * telemetry instead of trusting a session that ended.
 */
const ACTIVE_SESSION_KIND_KEY = "blackrack-overlay.active-session-kind";
const readActiveSessionKind = (): SessionKind | null => {
  const stored = sessionStorage.getItem(ACTIVE_SESSION_KIND_KEY);
  return isSessionKind(stored) ? stored : null;
};
let activeSessionKind: SessionKind | null = readActiveSessionKind();
let activeProfileId = resolveProfileId(profileState, activeMode, activeSessionKind);
let applyingProfile = false;
let profileSnapshotTimer = 0;

const activeProfile = (): OverlayProfile | undefined =>
  profileState.profiles.find(({ id }) => id === activeProfileId);

const writeProfileSnapshot = (): void => {
  const profile = activeProfile();
  if (!profile) return;
  profile.data = captureProfileData(profile.data);
  saveProfileState(profileState);
};

const scheduleProfileSnapshot = (): void => {
  if (applyingProfile) return;
  if (profileSnapshotTimer) window.clearTimeout(profileSnapshotTimer);
  profileSnapshotTimer = window.setTimeout(() => {
    profileSnapshotTimer = 0;
    writeProfileSnapshot();
  }, 1000);
};

const flushProfileSnapshot = (): void => {
  if (profileSnapshotTimer) {
    window.clearTimeout(profileSnapshotTimer);
    profileSnapshotTimer = 0;
  }
  if (applyingProfile) return;
  writeProfileSnapshot();
};

onLiveSettingsChanged = scheduleProfileSnapshot;

// Panel geometry is written by the composite host, so its storage event is the
// only signal that the active profile layout changed.
window.addEventListener("storage", (event) => {
  if (event.key === COMPOSITE_LAYOUT_KEY) scheduleProfileSnapshot();
});
window.addEventListener("beforeunload", flushProfileSnapshot);

// The layout may not exist yet on a first run; record it once it does.
void ensureCompositeLayout()
  .then(() => scheduleProfileSnapshot())
  .catch(() => undefined);

const applyProfileData = async (data: OverlayProfileData): Promise<void> => {
  applyingProfile = true;
  try {
    const profileLayout = await completeProfileLayout(data.layout);
    const defaultStandings = defaultStandingsSettings();
    const defaultRelative = defaultRelativeSettings();
    standingsSettings = {
      ...defaultStandings,
      ...data.standings,
      columns: { ...defaultStandings.columns, ...data.standings?.columns },
      header: { ...defaultStandings.header, ...data.standings?.header },
      combineLapTimes: data.standings?.combineLapTimes ?? defaultStandings.combineLapTimes,
      deltaLapCount: Number.isInteger(data.standings?.deltaLapCount)
        ? Math.max(1, Math.min(Number(data.standings?.deltaLapCount), 5))
        : defaultStandings.deltaLapCount,
      deltaReference: data.standings?.deltaReference === "best_lap"
        ? "best_lap"
        : defaultStandings.deltaReference,
      invertDeltaLayout: data.standings?.invertDeltaLayout ?? defaultStandings.invertDeltaLayout
    };
    relativeSettings = {
      ...defaultRelative,
      ...data.relative,
      options: { ...defaultRelative.options, ...data.relative?.options },
      combineLapTimes: data.relative?.combineLapTimes ?? defaultRelative.combineLapTimes,
      deltaLapCount: data.relative?.deltaLapCount ?? defaultRelative.deltaLapCount,
      invertDeltaLayout: data.relative?.invertDeltaLayout ?? defaultRelative.invertDeltaLayout
    };
    const defaultDriving = defaultDrivingSettings();
    drivingSettings = {
      ...defaultDriving,
      ...data.driving,
      graphPosition: data.driving?.graphPosition === "left" || data.driving?.graphPosition === "right"
        ? data.driving.graphPosition
        : defaultDriving.graphPosition
    };
    deltaSettings = data.delta;
    timingSettings = normalizeTimingSettings(data.timing);
    trackMapSettings = normalizeTrackMapSettings(data.trackMap);
    fuelSettings = normalizeFuelSettings(data.fuel);
    tiresSettings = data.tires;
    conditionsSettings = data.conditions;
    dashboardSettings = normalizeDashboardSettings(data.dashboard) ?? defaultDashboardSettings();
    sessionInfoSettings = normalizeSessionInfoSettings(data.sessionInfo) ?? defaultSessionInfoSettings();
    liftCoastSettings = normalizeLiftCoastSettings(data.liftCoast) ?? defaultLiftCoastSettings();
    pitStopSettings = normalizePitStopSettings(data.pitstop) ?? defaultPitStopSettings();
    chatSettings = normalizeChatSettings(data.chat);
    overlayTransparencyScope = data.transparency.scope;
    overlayFontSizeScope = data.fontSize.scope;
    const monitorFallback = await resolveOverlayMonitor();
    const monitorScope = normalizeOverlayMonitorScope(data.monitorScope, {
      mode: "individual",
      globalMonitor: monitorFallback
    });
    overlayMonitorScope = monitorScope;
    saveOverlayMonitorScope(monitorScope);
    await setOverlayMonitorPreference(monitorScope.globalMonitor).catch(() => undefined);
    for (const id of overlayIds) {
      overlayTransparency[id] = data.transparency.values[id] ?? DEFAULT_OVERLAY_TRANSPARENCY[id];
      overlayFontSize[id] = data.fontSize.values[id] ?? DEFAULT_OVERLAY_FONT_SIZE[id];
    }

    localStorage.setItem(STANDINGS_SETTINGS_KEY, JSON.stringify(standingsSettings));
    localStorage.setItem(RELATIVE_SETTINGS_KEY, JSON.stringify(relativeSettings));
    localStorage.setItem(DRIVING_SETTINGS_KEY, JSON.stringify(drivingSettings));
    localStorage.setItem(DELTA_SETTINGS_KEY, JSON.stringify(deltaSettings));
    localStorage.setItem(TIMING_SETTINGS_KEY, JSON.stringify(timingSettings));
    localStorage.setItem(TRACK_MAP_SETTINGS_KEY, JSON.stringify(trackMapSettings));
    localStorage.setItem(FUEL_SETTINGS_KEY, JSON.stringify(fuelSettings));
    localStorage.setItem(TIRES_SETTINGS_KEY, JSON.stringify(tiresSettings));
    localStorage.setItem(CONDITIONS_SETTINGS_KEY, JSON.stringify(conditionsSettings));
    localStorage.setItem(DASHBOARD_SETTINGS_KEY, JSON.stringify(dashboardSettings));
    localStorage.setItem(SESSIONINFO_SETTINGS_KEY, JSON.stringify(sessionInfoSettings));
    localStorage.setItem(LIFTCOAST_SETTINGS_KEY, JSON.stringify(liftCoastSettings));
    localStorage.setItem(PITSTOP_SETTINGS_KEY, JSON.stringify(pitStopSettings));
    localStorage.setItem(CHAT_SETTINGS_KEY, JSON.stringify(chatSettings));
    localStorage.setItem(OVERLAY_TRANSPARENCY_KEY, JSON.stringify(overlayTransparency));
    localStorage.setItem(OVERLAY_TRANSPARENCY_SCOPE_KEY, JSON.stringify(overlayTransparencyScope));
    localStorage.setItem(OVERLAY_FONT_SIZE_KEY, JSON.stringify(overlayFontSize));
    localStorage.setItem(OVERLAY_FONT_SIZE_SCOPE_KEY, JSON.stringify(overlayFontSizeScope));
    localStorage.setItem(COMPOSITE_LAYOUT_KEY, JSON.stringify(profileLayout));

    const effectiveTransparency = effectiveOverlayTransparency(overlayTransparency, overlayTransparencyScope);
    const effectiveFontSize = effectiveOverlayFontSize(overlayFontSize, overlayFontSizeScope);
    const events: Promise<unknown>[] = [
      emit("standings://settings", standingsSettings),
      emit("relative://settings", relativeSettings),
      emit("driving://settings", drivingSettings),
      emit("delta://settings", deltaSettings),
      emit("timing://settings", timingSettings),
      emit("trackmap://settings", trackMapSettings),
      emit("fuel://settings", fuelSettings),
      emit("tires://settings", tiresSettings),
      emit("conditions://settings", conditionsSettings),
      emit("dashboard://settings", dashboardSettings),
      emit("sessioninfo://settings", sessionInfoSettings),
      emit("liftcoast://settings", liftCoastSettings),
      emit("pitstop://settings", pitStopSettings),
      emit(CHAT_SETTINGS_EVENT, chatSettings)
    ];
    for (const id of overlayIds) {
      events.push(emit("overlay://background-transparency", {
        overlay: id,
        transparency: effectiveTransparency[id]
      } satisfies OverlayTransparencyChange));
      events.push(emit("overlay://font-size", {
        overlay: id,
        fontSize: effectiveFontSize[id]
      } satisfies OverlayFontSizeChange));
    }
    // Visibility travels through the backend so it also updates the desired set
    // that drives automatic hiding.
    for (const id of overlayIds) await setOverlay(id, data.visibility[id] === true);
    syncBrowserSourcePreferences();
    await Promise.all(events);
  } finally {
    applyingProfile = false;
  }
  reloadKeepingActiveView();
};

const profileListElement = document.getElementById("overlay-profile-list");
const createProfileButton = document.getElementById("create-overlay-profile") as HTMLButtonElement | null;
const profileStatus = document.getElementById("overlay-profile-status");
const profileBindingSelects = new Map<OverlayMode, HTMLSelectElement>(
  OVERLAY_MODES.flatMap((mode) => {
    const select = document.querySelector<HTMLSelectElement>(`select[data-profile-binding="${mode}"]`);
    return select ? [[mode, select] as [OverlayMode, HTMLSelectElement]] : [];
  })
);
const modeButtons = [...document.querySelectorAll<HTMLButtonElement>("[data-overlay-mode]")];
const sessionBindingSelects = new Map<SessionKind, HTMLSelectElement>(
  SESSION_KINDS.flatMap((kind) => {
    const select = document.querySelector<HTMLSelectElement>(`select[data-profile-session-binding="${kind}"]`);
    return select ? [[kind, select] as [SessionKind, HTMLSelectElement]] : [];
  })
);
const sessionBindingsSection = document.getElementById("overlay-profile-session-bindings");

const modeLabel = (mode: OverlayMode): string =>
  t(mode === "spectator" ? "follow.spectator" : mode === "team" ? "follow.team" : "follow.game");

const sessionKindLabel = (kind: SessionKind): string =>
  t(kind === "race" ? "session.race" : kind === "qualifying" ? "session.qualifying" : "session.practice");

const setProfileControlsBusy = (busy: boolean): void => {
  if (createProfileButton) {
    createProfileButton.disabled = busy || profileState.profiles.length >= MAX_OVERLAY_PROFILES;
  }
  for (const select of profileBindingSelects.values()) select.disabled = busy;
  for (const select of sessionBindingSelects.values()) select.disabled = busy;
  for (const button of modeButtons) button.disabled = busy;
  for (const button of profileListElement?.querySelectorAll("button") ?? []) button.disabled = busy;
};

const renderModeSelection = (): void => {
  for (const button of modeButtons) {
    const selected = button.dataset.overlayMode === activeMode;
    button.classList.toggle("active", selected);
    button.setAttribute("aria-pressed", String(selected));
  }
  // Only the driver's own weekend changes shape with the session, so the
  // per-session bindings exist while game mode is selected and nowhere else.
  sessionBindingsSection?.toggleAttribute("hidden", activeMode !== "game");
};

const addProfile = (name: string, data: OverlayProfileData): void => {
  if (profileState.profiles.length >= MAX_OVERLAY_PROFILES) {
    if (profileStatus) profileStatus.textContent = t("profiles.limit");
    return;
  }
  profileState.profiles.push({ id: createProfileId(), name, data });
  saveProfileState(profileState);
  if (profileStatus) profileStatus.textContent = "";
  renderProfiles();
};

const profileOptions = (): HTMLOptionElement[] =>
  profileState.profiles.map((profile) => {
    const option = document.createElement("option");
    option.value = profile.id;
    option.textContent = profile.name;
    return option;
  });

const renderProfiles = (): void => {
  for (const [mode, select] of profileBindingSelects) {
    select.replaceChildren(...profileOptions());
    select.value = profileState.bindings[mode];
  }
  for (const [kind, select] of sessionBindingSelects) {
    // The empty value is the default: the session keeps following game mode.
    const follow = document.createElement("option");
    follow.value = "";
    follow.textContent = t("profiles.followGame");
    select.replaceChildren(follow, ...profileOptions());
    select.value = profileState.sessionBindings[kind] ?? "";
  }
  if (profileListElement) {
    profileListElement.replaceChildren(...profileState.profiles.map((profile) => {
      const item = document.createElement("li");
      item.className = "overlay-profile";
      item.classList.toggle("active", profile.id === activeProfileId);

      const name = document.createElement("input");
      name.type = "text";
      name.className = "overlay-profile-name";
      name.value = profile.name;
      name.maxLength = MAX_PROFILE_NAME_LENGTH;
      name.setAttribute("aria-label", t("profiles.nameAria", { name: profile.name }));
      name.addEventListener("change", () => {
        profile.name = sanitizeProfileName(name.value, profile.name);
        name.value = profile.name;
        saveProfileState(profileState);
        renderProfiles();
      });

      const meta = document.createElement("span");
      meta.className = "overlay-profile-meta";
      const boundTo = [
        ...OVERLAY_MODES.filter((mode) => profileState.bindings[mode] === profile.id).map(modeLabel),
        ...SESSION_KINDS
          .filter((kind) => profileState.sessionBindings[kind] === profile.id)
          .map(sessionKindLabel)
      ];
      meta.textContent = profile.id === activeProfileId
        ? t("profiles.active")
        : boundTo.length > 0
          ? t("profiles.usedBy", { modes: boundTo.join(", ") })
          : "";

      const actions = document.createElement("div");
      actions.className = "overlay-profile-buttons";

      if (profile.id !== activeProfileId) {
        const use = document.createElement("button");
        use.type = "button";
        use.textContent = t("profiles.use");
        use.setAttribute("aria-label", t("profiles.useAria", { name: profile.name }));
        use.addEventListener("click", () => void useProfile(profile.id));
        actions.append(use);
      }

      const duplicate = document.createElement("button");
      duplicate.type = "button";
      duplicate.textContent = t("profiles.duplicate");
      duplicate.setAttribute("aria-label", t("profiles.duplicateAria", { name: profile.name }));
      duplicate.addEventListener("click", () => {
        flushProfileSnapshot();
        const source = profileState.profiles.find(({ id }) => id === profile.id);
        if (!source) return;
        addProfile(
          t("profiles.copyName", { name: source.name }),
          JSON.parse(JSON.stringify(source.data)) as OverlayProfileData
        );
      });
      actions.append(duplicate);

      if (profileState.profiles.length > 1) {
        const remove = document.createElement("button");
        remove.type = "button";
        remove.className = "overlay-profile-delete";
        remove.textContent = t("profiles.delete");
        remove.setAttribute("aria-label", t("profiles.deleteAria", { name: profile.name }));
        remove.addEventListener("click", () => void deleteProfile(profile.id));
        actions.append(remove);
      }

      item.append(name, meta, actions);
      return item;
    }));
  }
  setProfileControlsBusy(false);
};

/**
 * Brings the panel to whatever mode and session now resolve to. Every binding
 * edit, mode change and session change ends here, so the resolution rule lives
 * in one place. A switch already in flight ends in a reload, so a second one
 * is dropped rather than raced.
 */
let switchingProfile = false;

const applyResolvedProfile = async (): Promise<void> => {
  // Rendering during a switch would re-enable the controls it disabled.
  if (switchingProfile) return;
  const nextId = resolveProfileId(profileState, activeMode, activeSessionKind);
  if (nextId === activeProfileId) {
    renderProfiles();
    return;
  }
  // Pending edits belong to the profile being left, including when the session
  // itself made the switch and no button was pressed.
  flushProfileSnapshot();
  activeProfileId = nextId;
  const profile = activeProfile();
  renderProfiles();
  if (!profile) return;
  switchingProfile = true;
  setProfileControlsBusy(true);
  if (profileStatus) profileStatus.textContent = t("profiles.applying");
  try {
    await applyProfileData(profile.data);
  } finally {
    switchingProfile = false;
  }
};

const deleteProfile = async (id: string): Promise<void> => {
  const profile = profileState.profiles.find((candidate) => candidate.id === id);
  if (!profile || profileState.profiles.length <= 1) return;
  if (!await confirmReset(t("profiles.deleteConfirm", { name: profile.name }))) return;
  flushProfileSnapshot();
  const remaining = profileState.profiles.filter((candidate) => candidate.id !== id);
  const requested = Object.fromEntries(OVERLAY_MODES.map((mode) => [
    mode,
    profileState.bindings[mode] === id ? null : profileState.bindings[mode]
  ]));
  profileState = {
    profiles: remaining,
    bindings: normalizeBindings(requested, remaining),
    // A session bound to the deleted profile goes back to following game mode.
    sessionBindings: normalizeSessionBindings(profileState.sessionBindings, remaining)
  };
  saveProfileState(profileState);
  await applyResolvedProfile();
};

const bindMode = async (mode: OverlayMode, profileId: string): Promise<void> => {
  if (!profileState.profiles.some(({ id }) => id === profileId)) return;
  profileState.bindings = { ...profileState.bindings, [mode]: profileId };
  saveProfileState(profileState);
  await applyResolvedProfile();
};

const bindSession = async (kind: SessionKind, profileId: string | null): Promise<void> => {
  if (profileId !== null && !profileState.profiles.some(({ id }) => id === profileId)) return;
  profileState.sessionBindings = { ...profileState.sessionBindings, [kind]: profileId };
  saveProfileState(profileState);
  await applyResolvedProfile();
};

/**
 * "Use" rewrites whichever binding is deciding right now: the session one only
 * when it already overrides game mode, so a profile chosen during a session
 * that still follows the mode does not silently stop following it.
 */
const useProfile = (profileId: string): Promise<void> =>
  activeMode === "game" && activeSessionKind !== null
    && profileState.sessionBindings[activeSessionKind] !== null
    ? bindSession(activeSessionKind, profileId)
    : bindMode(activeMode, profileId);

const selectOverlayMode = async (mode: OverlayMode): Promise<void> => {
  if (mode === activeMode) return;
  flushProfileSnapshot();
  setProfileControlsBusy(true);
  try {
    await invoke("set_spectator_mode", { enabled: mode === "spectator" });
    await invoke("set_team_mode", { enabled: mode === "team" });
  } catch {
    // Restore the backend to the mode the panel still shows.
    await invoke("set_spectator_mode", { enabled: spectatorMode }).catch(() => undefined);
    await invoke("set_team_mode", { enabled: teamMode }).catch(() => undefined);
    renderModeSelection();
    setProfileControlsBusy(false);
    if (profileStatus) profileStatus.textContent = t("profiles.modeError");
    return;
  }
  spectatorMode = mode === "spectator";
  teamMode = mode === "team";
  saveSpectatorMode(spectatorMode);
  saveTeamMode(teamMode);
  activeMode = mode;
  renderModeSelection();
  if (resolveProfileId(profileState, activeMode, activeSessionKind) === activeProfileId) {
    for (const id of spectatorDisabledOverlays) await setOverlay(id, preferences[id]);
    renderProfiles();
    if (profileStatus) profileStatus.textContent = "";
    return;
  }
  await applyResolvedProfile();
};

/**
 * The panel already follows the frame for connection state, so the session kind
 * rides along. A frame from a disconnected source carries a stale session, so
 * the last known kind is kept instead of dragging the panel back to practice.
 */
const trackSessionKind = (frame: TelemetryFrame): void => {
  if (!frame.connected) return;
  const kind = sessionKindFromType(frame.session_type);
  if (kind === activeSessionKind) return;
  activeSessionKind = kind;
  sessionStorage.setItem(ACTIVE_SESSION_KIND_KEY, kind);
  if (activeMode !== "game") return;
  void applyResolvedProfile().catch(() => undefined);
};

for (const button of modeButtons) {
  button.addEventListener("click", () => {
    const mode = button.dataset.overlayMode;
    if (isOverlayMode(mode)) void selectOverlayMode(mode);
  });
}

for (const [mode, select] of profileBindingSelects) {
  select.addEventListener("change", () => {
    void bindMode(mode, select.value);
  });
}

for (const [kind, select] of sessionBindingSelects) {
  select.addEventListener("change", () => {
    void bindSession(kind, select.value === "" ? null : select.value);
  });
}

createProfileButton?.addEventListener("click", () => {
  void (async () => {
    // Without authored defaults the new profile keeps whatever layout is live,
    // because applying an incomplete layout would seed panels off-screen.
    const layout = await getDefaultCompositeLayout().catch(() => readCompositeLayout());
    addProfile(
      t("profiles.newName", { number: profileState.profiles.length + 1 }),
      defaultProfileData(layoutIsComplete(layout) ? layout : {} as CompositeLayout)
    );
  })().catch(() => undefined);
});

const gettingStarted = document.getElementById("getting-started") as HTMLDetailsElement;
document.getElementById("start-done")?.addEventListener("click", () => {
  gettingStarted.open = false;
  gettingStarted.querySelector("summary")?.focus();
});
document.getElementById("start-guide")?.addEventListener("click", () => {
  document.getElementById("open-overlay-guide")?.click();
});

renderModeSelection();
renderProfiles();

const resetOverlayConfiguration = async (id: OverlayId): Promise<void> => {
  if (!await confirmReset(
    t("overlay.configConfirm", { overlay: overlayDisplayName(id) })
  )) return;
  const events: Promise<unknown>[] = [];
  applyOverlayConfigurationDefaults(id, events);
  await publishOverlayConfigurationReset([id], events);
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

const resetAllStatus = document.getElementById("reset-all-status");
const resetAllConfigurationButton = document.getElementById("reset-all-configuration") as HTMLButtonElement | null;
const resetAllPositionButton = document.getElementById("reset-all-position") as HTMLButtonElement | null;

const setResetAllBusy = (busy: boolean): void => {
  if (resetAllConfigurationButton) resetAllConfigurationButton.disabled = busy;
  if (resetAllPositionButton) resetAllPositionButton.disabled = busy;
};

const resetAllOverlayConfigurations = async (): Promise<void> => {
  if (!await confirmReset(t("reset.allConfigConfirm"))) return;
  setResetAllBusy(true);
  if (resetAllStatus) resetAllStatus.textContent = t("reset.working");
  try {
    const events: Promise<unknown>[] = [];
    for (const id of overlayIds) applyOverlayConfigurationDefaults(id, events);
    await publishOverlayConfigurationReset(overlayIds, events);
  } catch (error) {
    console.error("No se pudo restaurar la configuración de los overlays:", error);
    if (resetAllStatus) resetAllStatus.textContent = t("reset.allError");
    setResetAllBusy(false);
  }
};

const resetAllOverlayPositions = async (): Promise<void> => {
  if (!await confirmReset(t("reset.allPositionConfirm"))) return;
  setResetAllBusy(true);
  if (resetAllStatus) resetAllStatus.textContent = t("reset.working");
  try {
    for (const id of overlayIds) await resetOverlayPlacement(id);
    if (resetAllStatus) resetAllStatus.textContent = t("reset.allPositionDone");
  } catch (error) {
    console.error("No se pudo restaurar la posición de los overlays:", error);
    if (resetAllStatus) resetAllStatus.textContent = t("reset.allError");
  } finally {
    setResetAllBusy(false);
  }
};

resetAllConfigurationButton?.addEventListener("click", () => void resetAllOverlayConfigurations());
resetAllPositionButton?.addEventListener("click", () => void resetAllOverlayPositions());

const overlayMonitorSelectors = new Map<OverlayId, HTMLSelectElement>();
const selectedOverlayMonitors = new Map<OverlayId, number>();
let selectedMonitor = 0;
let overlayMonitorScope: OverlayMonitorScope = readOverlayMonitorScope();
let monitorInventorySignature = "";
let monitorInventoryRefresh: Promise<void> | null = null;

const monitorDisplaySignature = (displays: readonly OverlayDisplay[]): string => displays
  .map((display) => [
    display.index,
    display.label,
    display.name,
    display.x,
    display.y,
    display.width,
    display.height,
    display.scaleFactor
  ].join(":"))
  .join("|");

const renderOverlayMonitorScope = (): void => {
  const modeSelect = document.getElementById("overlay-monitor-mode") as HTMLSelectElement | null;
  const globalControl = document.getElementById("global-monitor-control");
  const layout = readCompositeLayout();
  if (modeSelect) modeSelect.value = overlayMonitorScope.mode;
  if (globalControl) globalControl.hidden = overlayMonitorScope.mode !== "global";
  for (const [overlay, overlaySelect] of overlayMonitorSelectors) {
    overlaySelect.disabled = overlayMonitorScope.mode === "global";
    const monitor = overlayMonitorScope.mode === "global"
      ? overlayMonitorScope.globalMonitor
      : layout?.[overlay]?.monitor ?? selectedOverlayMonitors.get(overlay);
    if (monitor !== undefined) {
      overlaySelect.value = String(monitor);
      selectedOverlayMonitors.set(overlay, monitor);
    }
  }

};

const renderMonitorOptions = (
  select: HTMLSelectElement,
  displays: readonly OverlayDisplay[],
  selected: number,
  fallback: number
): number => {
  const available = new Set(displays.map(({ index }) => index));
  const resolved = available.has(selected) ? selected : fallback;
  const fragment = document.createDocumentFragment();
  for (const display of displays) {
    const option = document.createElement("option");
    option.value = String(display.index);
    option.textContent = `${display.name} · ${display.width}×${display.height}`;
    fragment.append(option);
  }
  select.replaceChildren(fragment);
  select.value = String(resolved);
  select.disabled = displays.length === 0;
  return resolved;
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

    const guideButton = document.createElement("button");
    guideButton.type = "button";
    guideButton.className = "overlay-guide-button";
    guideButton.textContent = "?";
    guideButton.title = t("guide.openFor", { overlay: overlayDisplayName(id) });
    guideButton.setAttribute("aria-label", t("guide.openFor", { overlay: overlayDisplayName(id) }));
    guideButton.setAttribute("aria-controls", "overlay-guide");
    guideButton.addEventListener("click", () => openOverlayGuide(id));
    card.insertBefore(guideButton, switchElement);

    const shortcutControl = document.createElement("label");
    shortcutControl.className = "overlay-shortcut-setting";
    const shortcutCopy = document.createElement("span");
    const shortcutCaption = document.createElement("b");
    shortcutCaption.textContent = t("overlay.shortcut");
    const shortcutSub = document.createElement("small");
    shortcutSub.textContent = t("overlay.shortcutSub");
    shortcutCopy.append(shortcutCaption, shortcutSub);
    const shortcutInput = document.createElement("input");
    shortcutInput.type = "text";
    shortcutInput.readOnly = true;
    shortcutInput.setAttribute("aria-label", t("overlay.shortcutAria", { overlay: overlayDisplayName(id) }));
    shortcutControl.append(shortcutCopy, shortcutInput);
    card.insertBefore(shortcutControl, switchElement);
    overlayShortcutInputs.set(id, shortcutInput);
    bindShortcutCapture(`hide_${id}`, shortcutInput);

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
    fontSizeRange.min = String(OVERLAY_FONT_SIZE_MIN);
    fontSizeRange.max = String(OVERLAY_FONT_SIZE_MAX);
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
    resetActions.setAttribute("aria-label", t("overlay.resetActions"));

    const monitorControl = document.createElement("label");
    monitorControl.className = "overlay-monitor-control";
    const monitorCaption = document.createElement("span");
    monitorCaption.textContent = t("overlay.monitor");
    const monitorSelect = document.createElement("select");
    monitorSelect.className = "overlay-monitor-select";
    monitorSelect.setAttribute("aria-label", t("overlay.monitorAria", { overlay: overlayDisplayName(id) }));
    monitorControl.append(monitorCaption, monitorSelect);
    resetActions.append(monitorControl);
    overlayMonitorSelectors.set(id, monitorSelect);

    const resetLabel = document.createElement("span");
    resetLabel.className = "overlay-reset-label";
    resetLabel.textContent = t("overlay.resetActions");
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
    resetActions.append(resetLabel, resetConfiguration, resetPosition);
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
      const fontSize = Math.max(
        OVERLAY_FONT_SIZE_MIN,
        Math.min(OVERLAY_FONT_SIZE_MAX, Number(fontSizeRange.value))
      );
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
    globalFontSize: Math.max(
      OVERLAY_FONT_SIZE_MIN,
      Math.min(OVERLAY_FONT_SIZE_MAX, Number(globalFontSize.value))
    )
  };
  persistFontSizeScope();
  renderFontSizeMode();
  emitEffectiveFontSize();
});

renderFontSizeMode();

const bindMonitorSelector = async (): Promise<void> => {
  const [displays, monitor] = await Promise.all([getOverlayDisplays(), resolveOverlayMonitor()]);
  const select = document.getElementById("overlay-monitor") as HTMLSelectElement | null;
  const modeSelect = document.getElementById("overlay-monitor-mode") as HTMLSelectElement | null;
  if (!select) return;
  selectedMonitor = renderMonitorOptions(select, displays, overlayMonitorScope.globalMonitor, monitor);
  overlayMonitorScope = { ...overlayMonitorScope, globalMonitor: selectedMonitor };
  saveOverlayMonitorScope(overlayMonitorScope);
  renderOverlayMonitorScope();
  modeSelect?.addEventListener("change", () => {
    if (modeSelect.value !== "global" && modeSelect.value !== "individual") return;
    void setOverlayMonitorScope({ ...overlayMonitorScope, mode: modeSelect.value }).then((scope) => {
      overlayMonitorScope = scope;
      renderOverlayMonitorScope();
    }).catch(() => { modeSelect.value = overlayMonitorScope.mode; });
  });
  monitorInventorySignature = monitorDisplaySignature(displays);
  select.addEventListener("change", () => {
    select.disabled = true;
    void setOverlayMonitor(Number(select.value)).then((result) => {
      selectedMonitor = result;
      overlayMonitorScope = { ...overlayMonitorScope, globalMonitor: result };
      renderOverlayMonitorScope();
    }).catch(() => {
      select.value = String(selectedMonitor);
    }).finally(() => {
      select.disabled = false;
    });
  });
};

const bindOverlayMonitorSelectors = async (): Promise<void> => {
  const [displays, layout, fallback] = await Promise.all([
    getOverlayDisplays(),
    ensureCompositeLayout(),
    resolveOverlayMonitor()
  ]);
  for (const [overlay, select] of overlayMonitorSelectors) {
    const selected = effectiveOverlayMonitor(layout[overlay]?.monitor, overlayMonitorScope);
    selectedOverlayMonitors.set(
      overlay,
      renderMonitorOptions(select, displays, selected, displays[0]?.index ?? 0)
    );
    select.disabled = overlayMonitorScope.mode === "global";
    select.addEventListener("change", () => {
      const next = Number(select.value);
      select.disabled = true;
      void setOverlayPlacementMonitor(overlay, next)
        .then(() => {
          selectedOverlayMonitors.set(overlay, next);
        })
        .catch(() => {
          select.value = String(selectedOverlayMonitors.get(overlay) ?? fallback);
        })
        .finally(() => {
          select.disabled = false;
        });
    });
  }
};

const refreshMonitorInventory = async (): Promise<void> => {
  if (monitorInventoryRefresh) return monitorInventoryRefresh;
  monitorInventoryRefresh = (async () => {
    const displays = await refreshOverlayDisplays();
    const signature = monitorDisplaySignature(displays);
    if (signature === monitorInventorySignature) return;
    monitorInventorySignature = signature;

    const fallback = displays[0]?.index ?? 0;
    const monitor = await resolveOverlayMonitor();
    overlayMonitorScope = normalizeOverlayMonitorScope(readOverlayMonitorScope(), {
      mode: overlayMonitorScope.mode,
      globalMonitor: monitor
    });
    overlayMonitorScope.globalMonitor = displays.some(({ index }) => index === overlayMonitorScope.globalMonitor)
      ? overlayMonitorScope.globalMonitor
      : monitor;
    saveOverlayMonitorScope(overlayMonitorScope);
    selectedMonitor = renderMonitorOptions(
      document.getElementById("overlay-monitor") as HTMLSelectElement,
      displays,
      overlayMonitorScope.globalMonitor,
      fallback
    );
    const layout = readCompositeLayout() ?? await ensureCompositeLayout();
    for (const [overlay, select] of overlayMonitorSelectors) {
      const selected = effectiveOverlayMonitor(layout[overlay]?.monitor, overlayMonitorScope)
        ?? selectedOverlayMonitors.get(overlay)
        ?? selectedMonitor;
      selectedOverlayMonitors.set(overlay, renderMonitorOptions(select, displays, selected, fallback));
    }

    await synchronizeOverlayHosts();

    // Synchronization may have reassigned panels from a disconnected monitor.
    const latestLayout = readCompositeLayout() ?? layout;
    overlayMonitorScope = readOverlayMonitorScope();
    const latestMonitor = overlayMonitorScope.globalMonitor;
    selectedMonitor = renderMonitorOptions(
      document.getElementById("overlay-monitor") as HTMLSelectElement,
      displays,
      latestMonitor,
      fallback
    );
    for (const [overlay, select] of overlayMonitorSelectors) {
      const selected = effectiveOverlayMonitor(latestLayout[overlay]?.monitor, overlayMonitorScope);
      selectedOverlayMonitors.set(overlay, renderMonitorOptions(select, displays, selected, fallback));
    }
    renderOverlayMonitorScope();
  })().finally(() => {
    monitorInventoryRefresh = null;
  });
  await monitorInventoryRefresh;
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

const parseOverlayConfiguration = (
  contents: string,
  defaultLayout: OverlayConfigurationExport["overlays"]["layout"]
): OverlayConfigurationExport => {
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
  const monitorScope = configurationObject(overlays?.monitorScope);
  const layout = configurationObject(overlays?.layout);
  const standings = configurationObject(overlays?.standings);
  const relative = configurationObject(overlays?.relative);
  const driving = configurationObject(overlays?.driving);
  const delta = configurationObject(overlays?.delta);
  const timing = configurationObject(overlays?.timing);
  const trackMap = configurationObject(overlays?.trackMap);
  const fuel = configurationObject(overlays?.fuel);
  const tires = configurationObject(overlays?.tires);
  const conditions = configurationObject(overlays?.conditions);
  const dashboard = configurationObject(overlays?.dashboard);
  const sessionInfo = configurationObject(overlays?.sessionInfo);
  const liftCoast = configurationObject(overlays?.liftCoast);
  const pitstop = configurationObject(overlays?.pitstop);
  const chat = configurationObject(overlays?.chat);
  const importedProfiles = root?.profiles;
  const importedBindings = root?.modeBindings;
  const importedSessionBindings = root?.sessionBindings;
  const importedPerformanceProfile = overlays?.performanceProfile;
  const importedSpectatorMode = overlays?.spectatorMode;
  const importedTeamMode = overlays?.teamMode;
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
  const importedMonitorScope = monitorScope
    ? normalizeOverlayMonitorScope(monitorScope)
    : { mode: "individual", globalMonitor: monitor } as OverlayMonitorScope;
  const percentageIsValid = (value: unknown): value is number =>
    typeof value === "number" && Number.isFinite(value) && value >= 0 && value <= 100;
  const numericSchemaVersion = Number(schemaVersion);
  const recognizedFormat = root?.format === CURRENT_CONFIGURATION_FORMAT
    || root?.format === LEGACY_CONFIGURATION_FORMAT;
  if (!recognizedFormat
    || !Number.isInteger(numericSchemaVersion) || numericSchemaVersion < 1
    || !overlays || !visibility || !transparency || !transparencyScope || !transparencyValues
    || (Number(schemaVersion) >= 8 && (!fontSize || !fontSizeScope || !fontSizeValues))
    || !layout || !standings || !relative
    || (numericSchemaVersion >= 9 && importedPerformanceProfile !== undefined
      && !isPerformanceProfile(importedPerformanceProfile))
    || (numericSchemaVersion >= 10 && typeof importedSpectatorMode !== "boolean")
    || (Number(schemaVersion) >= 2 && !driving)
    || (Number(schemaVersion) >= 3 && !delta) || (Number(schemaVersion) >= 4 && !timing)
    || (numericSchemaVersion >= 6 && !trackMap)
    || (numericSchemaVersion >= 12 && !fuel)
    || (numericSchemaVersion >= 15 && !tires)
    || (numericSchemaVersion >= 16 && !conditions)
    || (numericSchemaVersion >= 18 && !dashboard)
    || (numericSchemaVersion >= 25 && !sessionInfo)
    || (numericSchemaVersion >= 26 && !chat)
    || (numericSchemaVersion >= 20 && !liftCoast)
    || (numericSchemaVersion >= 23 && !pitstop)) {
    throw new Error(t("config.incompatible"));
  }
  if (transparencyScope.mode !== "global" && transparencyScope.mode !== "individual") {
    throw new Error(t("config.invalidTransparency"));
  }
  if (numericSchemaVersion >= 8
    && fontSizeScope?.mode !== "global" && fontSizeScope?.mode !== "individual") {
    throw new Error(t("config.invalidFontSize"));
  }
  if (numericSchemaVersion >= 5) {
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
    typeof value === "number" && Number.isFinite(value) &&
    value >= OVERLAY_FONT_SIZE_MIN && value <= OVERLAY_FONT_SIZE_MAX;
  if (numericSchemaVersion >= 8 && !fontSizeIsValid(fontSizeScope?.globalFontSize)) {
    throw new Error(t("config.invalidGeneral"));
  }
  if (numericSchemaVersion < 3) {
    visibility.delta = false;
    transparencyValues.delta = 5;
    layout.delta = { overlay: "delta", monitor, x: 610, y: 20, width: 420, height: 72 };
  }
  if (numericSchemaVersion < 4) {
    visibility.timing = false;
    transparencyValues.timing = 5;
    layout.timing = { overlay: "timing", monitor, x: 610, y: 110, width: 250, height: 292 };
  }
  const completeBooleanRecord = (value: unknown, keys: string[]): boolean => {
    const record = configurationObject(value);
    return record !== null && keys.every((key) => typeof record[key] === "boolean");
  };
  const mergeBooleanRecord = <Key extends string>(
    value: unknown,
    defaults: Record<Key, boolean>
  ): Record<Key, boolean> | null => {
    const record = configurationObject(value);
    if (!record) return null;
    const keys = Object.keys(defaults) as Key[];
    if (keys.some((key) => record[key] !== undefined && typeof record[key] !== "boolean")) {
      return null;
    }
    return Object.fromEntries(keys.map((key) => [key, record[key] ?? defaults[key]])) as Record<Key, boolean>;
  };
  const mergeOrder = <Key extends string>(value: unknown, defaults: Key[]): Key[] | null => {
    if (!Array.isArray(value) || value.some((key) => typeof key !== "string")) return null;
    const imported = value.filter((key): key is Key => defaults.includes(key as Key));
    if (new Set(imported).size !== imported.length) return null;
    return [...imported, ...defaults.filter((key) => !imported.includes(key))];
  };
  const defaultStandings = defaultStandingsSettings();
  const normalizedStandingsColumns = mergeBooleanRecord(standings.columns, defaultStandings.columns);
  for (const id of ["pitTime", "pitLap"] as const) {
    const imported = configurationObject(standings.columns);
    if (normalizedStandingsColumns && imported?.[id] === undefined && typeof imported?.pitStops === "boolean") {
      normalizedStandingsColumns[id] = imported.pitStops;
    }
  }
  const importedStandingsOrder = mergeOrder(standings.columnOrder, defaultStandings.columnOrder);
  const normalizedStandingsOrder = importedStandingsOrder
    ? normalizeStandingsColumnOrder(importedStandingsOrder)
    : null;
  const normalizedStandingsHeader = mergeBooleanRecord(standings.header, defaultStandings.header);
  if (!normalizedStandingsColumns
    || !normalizedStandingsOrder
    || typeof standings.showHeader !== "boolean"
    || !normalizedStandingsHeader
    || !Number.isInteger(standings.ownClassRows) || Number(standings.ownClassRows) < 3
    || Number(standings.ownClassRows) > 30
    || !Number.isInteger(standings.otherClassRows) || Number(standings.otherClassRows) < 1
    || Number(standings.otherClassRows) > 15
    || typeof standings.showOtherClasses !== "boolean"
    || (standings.deltaLapCount !== undefined && (!Number.isInteger(standings.deltaLapCount) || Number(standings.deltaLapCount) < 1 || Number(standings.deltaLapCount) > 5))
    || (standings.deltaReference !== undefined && !["last_lap", "best_lap"].includes(String(standings.deltaReference)))
    || (standings.pitInformationLayout !== undefined && !["inline", "above", "column"].includes(String(standings.pitInformationLayout)))
    || (standings.driverNameFormat !== undefined && !isDriverNameFormat(standings.driverNameFormat))) {
    throw new Error(t("config.invalidStandings"));
  }
  const defaultRelative = defaultRelativeSettings();
  const normalizedRelativeOptions = mergeBooleanRecord(relative.options, defaultRelative.options);
  for (const id of ["pitTime", "pitLap"] as const) {
    const imported = configurationObject(relative.options);
    if (normalizedRelativeOptions && imported?.[id] === undefined && typeof imported?.pitStops === "boolean") {
      normalizedRelativeOptions[id] = imported.pitStops;
    }
  }
  const normalizedRelativeOrder = mergeOrder(relative.columnOrder, defaultRelative.columnOrder);
  if (!normalizedRelativeOptions
    || !normalizedRelativeOrder
    || !Number.isInteger(relative.aheadRows) || Number(relative.aheadRows) < 1
    || Number(relative.aheadRows) > 10
    || !Number.isInteger(relative.behindRows) || Number(relative.behindRows) < 1
    || Number(relative.behindRows) > 10
    || (relative.pitInformationLayout !== undefined && !["inline", "above", "column"].includes(String(relative.pitInformationLayout)))
    || (relative.driverNameFormat !== undefined && !isDriverNameFormat(relative.driverNameFormat))) {
    throw new Error(t("config.invalidRelative"));
  }
  const defaultDriving = defaultDrivingSettings();
  const drivingPedalIds = Object.keys(defaultDriving.graphPedals);
  if (driving && (!completeBooleanRecord(driving.graphPedals, drivingPedalIds)
    || !completeBooleanRecord(driving.inputPedals, drivingPedalIds)
    || (driving.showGraph !== undefined && typeof driving.showGraph !== "boolean")
    || (driving.showPedalLabels !== undefined && typeof driving.showPedalLabels !== "boolean")
    || typeof driving.showSteering !== "boolean"
    || typeof driving.showForceFeedback !== "boolean"
    || typeof driving.showSpeed !== "boolean"
    || typeof driving.showGear !== "boolean"
    || (driving.showRpmLeds !== undefined && typeof driving.showRpmLeds !== "boolean")
    || (driving.graphPosition !== undefined && driving.graphPosition !== "left" && driving.graphPosition !== "right"))) {
    throw new Error(t("config.invalidDriving"));
  }
  const normalizedDelta = delta ?? defaultDeltaSettings();
  if (!isDeltaMode(normalizedDelta.mode)
    || typeof normalizedDelta.displayRange !== "number"
    || ![0.5, 1, 2, 5].includes(normalizedDelta.displayRange)) {
    throw new Error(t("config.invalidDelta"));
  }
  const defaultTiming = defaultTimingSettings();
  const normalizedTiming = timing ?? defaultTiming;
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
  if (numericSchemaVersion >= 22 && (!monitorScope
    || (monitorScope.mode !== "global" && monitorScope.mode !== "individual")
    || !Number.isInteger(monitorScope.globalMonitor) || Number(monitorScope.globalMonitor) < 0)) {
    throw new Error(t("config.invalidMonitorMode"));
  }
  const normalizedTimingOrder = normalizedTiming.timeOrder === undefined
    ? defaultTiming.timeOrder
    : normalizeTimingOrder(normalizedTiming.timeOrder);
  if (normalizedTiming.timeOrder !== undefined
    && (!Array.isArray(normalizedTiming.timeOrder)
      || normalizedTiming.timeOrder.some((id) => typeof id !== "string")
      || new Set(normalizedTiming.timeOrder).size !== normalizedTiming.timeOrder.length
      || normalizedTiming.timeOrder.some((id) => !timingTimeIds.includes(id as typeof timingTimeIds[number])))) {
    throw new Error(t("config.invalidTiming"));
  }
  const normalizedTimingWithTimes = {
    ...normalizedTiming,
    times: normalizedTiming.times ?? defaultTiming.times,
    timeOrder: normalizedTimingOrder
  };
  if (trackMap && typeof trackMap.showPitPrediction !== "boolean") {
    throw new Error(t("config.invalidMap"));
  }
  const normalizedTrackMap = normalizeTrackMapSettings(trackMap ?? defaultTrackMapSettings());
  const normalizedFuel = fuel ?? defaultFuelSettings();
  if (normalizedFuel.energyMarginPercent !== undefined
    && (typeof normalizedFuel.energyMarginPercent !== "number"
      || !Number.isFinite(normalizedFuel.energyMarginPercent)
      || normalizedFuel.energyMarginPercent < 0 || normalizedFuel.energyMarginPercent > 20)) {
    throw new Error(t("config.invalidFuel"));
  }
  if (normalizedFuel.refuelMarginLiters !== undefined
    && (typeof normalizedFuel.refuelMarginLiters !== "number"
      || !Number.isFinite(normalizedFuel.refuelMarginLiters)
      || normalizedFuel.refuelMarginLiters < 0 || normalizedFuel.refuelMarginLiters > 20)) {
    throw new Error(t("config.invalidFuel"));
  }
  if (normalizedFuel.visible !== undefined
    && !completeBooleanRecord(normalizedFuel.visible, FUEL_FIELDS.map(({ id }) => id))) {
    throw new Error(t("config.invalidFuel"));
  }
  if (!isFuelScenarioMode(normalizedFuel.scenarioMode)) {
    throw new Error(t("config.invalidFuel"));
  }
  const normalizedTires = normalizeTiresSettings(tires);
  if (!normalizedTires) {
    throw new Error(t("config.invalidTires"));
  }
  const normalizedConditions = conditions
    ? normalizeConditionsSettings(conditions)
    : defaultConditionsSettings();
  if (!normalizedConditions) {
    throw new Error(t("config.invalidConditions"));
  }
  const normalizedDashboard = dashboard
    ? normalizeDashboardSettings(dashboard)
    : defaultDashboardSettings();
  if (!normalizedDashboard) {
    throw new Error(t("config.invalidDashboard"));
  }
  const normalizedSessionInfo = sessionInfo
    ? normalizeSessionInfoSettings(sessionInfo)
    : defaultSessionInfoSettings();
  if (!normalizedSessionInfo) {
    throw new Error(t("config.invalidSessionInfo"));
  }
  const normalizedLiftCoast = liftCoast
    ? normalizeLiftCoastSettings(liftCoast)
    : defaultLiftCoastSettings();
  if (!normalizedLiftCoast) {
    throw new Error(t("config.invalidLiftCoast"));
  }
  const normalizedPitStop = pitstop
    ? normalizePitStopSettings(pitstop)
    : defaultPitStopSettings();
  if (!normalizedPitStop) {
    throw new Error(t("config.invalidPitStop"));
  }
  const normalizedChat = normalizeChatSettings(chat ?? defaultChatSettings());
  const invalidChatSettings = chat !== null && (
    typeof chat.maxMessages !== "number" || !Number.isInteger(chat.maxMessages)
    || chat.maxMessages < 1 || chat.maxMessages > 8
    || (chat.maxHeight !== undefined && (typeof chat.maxHeight !== "number"
      || !Number.isInteger(chat.maxHeight) || chat.maxHeight < 80 || chat.maxHeight > 400
      || (chat.maxHeight - 80) % 20 !== 0))
    || (numericSchemaVersion >= 27 && chat.maxHeight === undefined)
  );
  if ((overlays?.chat !== undefined && !chat) || invalidChatSettings) {
    throw new Error(t("config.invalidChat"));
  }
  const fallbackVisibility = defaultVisibility();
  for (const id of overlayIds) {
    if (visibility[id] === undefined) visibility[id] = fallbackVisibility[id];
    if (transparencyValues[id] === undefined) {
      transparencyValues[id] = DEFAULT_OVERLAY_TRANSPARENCY[id];
    }
    if (fontSizeValues && fontSizeValues[id] === undefined) {
      fontSizeValues[id] = DEFAULT_OVERLAY_FONT_SIZE[id];
    }
    if (layout[id] === undefined) layout[id] = defaultLayout[id];
  }

  for (const id of overlayIds) {
    const placement = configurationObject(layout[id]);
    if (typeof visibility[id] !== "boolean" || !percentageIsValid(transparencyValues[id])
      || (numericSchemaVersion >= 8 && !fontSizeIsValid(fontSizeValues?.[id]))
      || !placement || placement.overlay !== id
      || ![placement.x, placement.y, placement.width, placement.height]
        .every((value) => typeof value === "number" && Number.isFinite(value))
      || Number(placement.width) <= 0 || Number(placement.height) <= 0
      || (placement.monitor !== undefined
        && (typeof placement.monitor !== "number"
          || !Number.isInteger(placement.monitor) || Number(placement.monitor) < 0))
      || (placement.scale !== undefined
        && (typeof placement.scale !== "number"
          || !Number.isFinite(placement.scale) || placement.scale <= 0))) {
      throw new Error(t("config.invalidOverlay", { overlay: id }));
    }
  }
  const cleanedLayout = Object.fromEntries(overlayIds.map((id) => {
    const placement: Record<string, unknown> = { ...(layout[id] as Record<string, unknown>) };
    if (placement.monitor === undefined || numericSchemaVersion < 21) placement.monitor = monitor;
    return [id, placement];
  })) as unknown as OverlayConfigurationExport["overlays"]["layout"];
  const normalized = parsed as OverlayConfigurationExport;
  const normalizedFontSize = numericSchemaVersion >= 8
    ? {
        scope: fontSizeScope as unknown as OverlayFontSizeScope,
        values: fontSizeValues as unknown as Record<OverlayId, number>
      }
    : {
        scope: { mode: "individual", globalFontSize: 100 } as OverlayFontSizeScope,
        values: { ...DEFAULT_OVERLAY_FONT_SIZE }
      };
  const result: OverlayConfigurationExport = {
    ...normalized,
    profiles: [],
    modeBindings: { game: "", spectator: "", team: "" },
    sessionBindings: { practice: null, qualifying: null, race: null },
    format: CURRENT_CONFIGURATION_FORMAT,
    schemaVersion: CURRENT_CONFIGURATION_SCHEMA,
    ui: {
      locale: isLocale(ui?.locale) ? ui.locale : getLocale(),
      displayUnits: numericSchemaVersion >= 24 && ui?.displayUnits !== undefined
        ? normalizeDisplayUnits(ui.displayUnits)
        : readDisplayUnits()
    },
    overlays: {
      ...normalized.overlays,
      visibility: visibility as unknown as Record<OverlayId, boolean>,
      transparency: {
        scope: transparencyScope as unknown as OverlayTransparencyScope,
        values: transparencyValues as unknown as Record<OverlayId, number>
      },
      monitor,
      monitorScope: importedMonitorScope,
      layout: cleanedLayout,
      fontSize: normalizedFontSize,
      standings: {
        ...(standings as unknown as StandingsSettings),
        columns: normalizedStandingsColumns,
        columnOrder: normalizedStandingsOrder,
        header: normalizedStandingsHeader,
        pitInformationLayout: (standings.pitInformationLayout ?? "inline") as StandingsSettings["pitInformationLayout"],
        combineLapTimes: standings.combineLapTimes === true,
        deltaLapCount: Number.isInteger(standings.deltaLapCount)
          ? Math.max(1, Math.min(Number(standings.deltaLapCount), 5))
          : defaultStandings.deltaLapCount,
        deltaReference: standings.deltaReference === "best_lap" ? "best_lap" : "last_lap",
        driverNameFormat: isDriverNameFormat(standings.driverNameFormat)
          ? standings.driverNameFormat
          : defaultStandings.driverNameFormat
      },
      relative: {
        ...(relative as unknown as RelativeSettings),
        options: normalizedRelativeOptions,
        columnOrder: normalizedRelativeOrder,
        pitInformationLayout: (relative.pitInformationLayout ?? "inline") as RelativeSettings["pitInformationLayout"],
        combineLapTimes: relative.combineLapTimes === true,
        driverNameFormat: isDriverNameFormat(relative.driverNameFormat)
          ? relative.driverNameFormat
          : defaultRelative.driverNameFormat
      },
      driving: driving ? {
        ...(driving as unknown as DrivingSettings),
        graphPosition: driving.graphPosition === "left" || driving.graphPosition === "right"
          ? driving.graphPosition
          : defaultDriving.graphPosition,
        showGraph: typeof driving.showGraph === "boolean"
          ? driving.showGraph
          : defaultDriving.showGraph,
        showPedalLabels: typeof driving.showPedalLabels === "boolean"
          ? driving.showPedalLabels
          : defaultDriving.showPedalLabels,
        showRpmLeds: typeof driving.showRpmLeds === "boolean"
          ? driving.showRpmLeds
          : defaultDriving.showRpmLeds
      } : defaultDriving,
      delta: normalizedDelta as unknown as DeltaSettings,
      timing: normalizedTimingWithTimes as unknown as TimingSettings,
      trackMap: normalizedTrackMap as unknown as TrackMapSettings,
      fuel: normalizeFuelSettings(normalizedFuel),
      tires: normalizedTires as unknown as TiresSettings,
      conditions: normalizedConditions,
      dashboard: normalizedDashboard,
      sessionInfo: normalizedSessionInfo,
      liftCoast: normalizedLiftCoast,
      pitstop: normalizedPitStop,
      chat: normalizedChat,
      performanceProfile: isPerformanceProfile(importedPerformanceProfile)
        ? importedPerformanceProfile
        : DEFAULT_PERFORMANCE_PROFILE,
      spectatorMode: typeof importedSpectatorMode === "boolean" ? importedSpectatorMode : false,
      teamMode: typeof importedTeamMode === "boolean" ? importedTeamMode : false
    }
  };
  const activeData: OverlayProfileData = {
    visibility: result.overlays.visibility,
    transparency: result.overlays.transparency,
    fontSize: result.overlays.fontSize,
    monitorScope: result.overlays.monitorScope,
    layout: result.overlays.layout,
    standings: result.overlays.standings,
    relative: result.overlays.relative,
    driving: result.overlays.driving,
    delta: result.overlays.delta,
    timing: result.overlays.timing,
    trackMap: result.overlays.trackMap,
    fuel: result.overlays.fuel,
    tires: result.overlays.tires,
    conditions: result.overlays.conditions,
    dashboard: result.overlays.dashboard,
    sessionInfo: result.overlays.sessionInfo,
    liftCoast: result.overlays.liftCoast,
    pitstop: result.overlays.pitstop,
    chat: result.overlays.chat
  };
  // Documents written before schema 17 carry a single configuration; it becomes
  // the one profile every mode starts bound to.
  const profiles = normalizeProfiles(importedProfiles, t("profiles.defaultName"));
  if (profiles.length === 0) {
    profiles.push({ id: createProfileId(), name: t("profiles.defaultName"), data: activeData });
  }
  const modeBindings = normalizeBindings(importedBindings, profiles);
  // Documents written before schema 19 bind no session, so every kind follows
  // the imported game mode exactly as it did when the document was written.
  const sessionBindings = normalizeSessionBindings(importedSessionBindings, profiles);
  // The overlays block is the validated configuration the panel will run, so the
  // profile the imported bindings resolve to has to carry exactly that. The live
  // session takes part because the panel will resolve with it after the reload.
  const boundProfile = profiles.find(({ id }) => id === resolveProfileId(
    { profiles, bindings: modeBindings, sessionBindings },
    modeFromFlags(result.overlays.spectatorMode, result.overlays.teamMode),
    activeSessionKind
  ));
  if (boundProfile) boundProfile.data = activeData;
  return { ...result, profiles, modeBindings, sessionBindings };
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
  const monitorScope = normalizeOverlayMonitorScope(configuration.overlays.monitorScope, {
    mode: configuration.overlays.monitorScope?.mode === "global" ? "global" : "individual",
    globalMonitor: monitor
  });
  monitorScope.globalMonitor = availableMonitors.has(monitorScope.globalMonitor)
    ? monitorScope.globalMonitor
    : monitor;
  const normalizeLayout = (layout: CompositeLayout): CompositeLayout =>
    Object.fromEntries(overlayIds.map((id) => {
      const placement = layout[id];
      if (!placement) return [id, placement];
      const assigned = placement.monitor;
      const placementMonitor = Number.isInteger(assigned)
        && assigned !== undefined
        && availableMonitors.has(assigned)
        ? assigned
        : monitor;
      return [id, { ...placement, monitor: placementMonitor }];
    })) as CompositeLayout;
  const profiles = configuration.profiles.map((profile) => ({
    ...profile,
    data: {
      ...profile.data,
      monitorScope: normalizeOverlayMonitorScope(profile.data.monitorScope, {
        mode: profile.data.monitorScope?.mode === "global" ? "global" : "individual",
        globalMonitor: monitor
      }),
      layout: normalizeLayout({ ...configuration.overlays.layout, ...profile.data.layout })
    }
  }));
  return {
    ...configuration,
    profiles,
    overlays: {
      ...configuration.overlays,
      monitor,
      monitorScope,
      layout: normalizeLayout(configuration.overlays.layout)
    }
  };
};

const applyImportedConfiguration = (configuration: OverlayConfigurationExport): void => {
  const entries: Array<[string, unknown]> = [
    [DISPLAY_UNITS_STORAGE_KEY, configuration.ui.displayUnits],
    [storageKey, configuration.overlays.visibility],
    [OVERLAY_TRANSPARENCY_KEY, configuration.overlays.transparency.values],
    [OVERLAY_TRANSPARENCY_SCOPE_KEY, configuration.overlays.transparency.scope],
    [OVERLAY_FONT_SIZE_KEY, configuration.overlays.fontSize.values],
    [OVERLAY_FONT_SIZE_SCOPE_KEY, configuration.overlays.fontSize.scope],
    [COMPOSITE_LAYOUT_KEY, configuration.overlays.layout],
    [OVERLAY_MONITOR_SCOPE_KEY, configuration.overlays.monitorScope ?? {
      mode: "individual", globalMonitor: configuration.overlays.monitor
    }],
    [STANDINGS_SETTINGS_KEY, configuration.overlays.standings],
    [RELATIVE_SETTINGS_KEY, configuration.overlays.relative],
    [DRIVING_SETTINGS_KEY, configuration.overlays.driving],
    [DELTA_SETTINGS_KEY, configuration.overlays.delta],
    [TIMING_SETTINGS_KEY, configuration.overlays.timing],
    [TRACK_MAP_SETTINGS_KEY, configuration.overlays.trackMap],
    [FUEL_SETTINGS_KEY, configuration.overlays.fuel],
    [TIRES_SETTINGS_KEY, configuration.overlays.tires],
    [CONDITIONS_SETTINGS_KEY, configuration.overlays.conditions],
    [DASHBOARD_SETTINGS_KEY, configuration.overlays.dashboard],
    [SESSIONINFO_SETTINGS_KEY, configuration.overlays.sessionInfo],
    [LIFTCOAST_SETTINGS_KEY, configuration.overlays.liftCoast],
    [PITSTOP_SETTINGS_KEY, configuration.overlays.pitstop],
    [CHAT_SETTINGS_KEY, configuration.overlays.chat],
    [PERFORMANCE_PROFILE_KEY, configuration.overlays.performanceProfile],
    [SPECTATOR_MODE_KEY, configuration.overlays.spectatorMode && !configuration.overlays.teamMode],
    [TEAM_MODE_KEY, configuration.overlays.teamMode],
    [OVERLAY_PROFILES_KEY, configuration.profiles],
    [PROFILE_BINDINGS_KEY, configuration.modeBindings],
    [SESSION_BINDINGS_KEY, configuration.sessionBindings]
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
  updateDisplayUnits(configuration.ui.displayUnits);
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
    flushProfileSnapshot();
    const layout = readCompositeLayout() ?? await ensureCompositeLayout();
    const monitor = await resolveOverlayMonitor();
    const configuration: OverlayConfigurationExport = {
      format: CURRENT_CONFIGURATION_FORMAT,
      schemaVersion: CURRENT_CONFIGURATION_SCHEMA,
      exportedAt: now.toISOString(),
      ui: { locale: getLocale(), displayUnits: readDisplayUnits() },
      profiles: profileState.profiles,
      modeBindings: profileState.bindings,
      sessionBindings: profileState.sessionBindings,
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
        monitorScope: { ...overlayMonitorScope },
        layout,
        standings: standingsSettings,
        relative: relativeSettings,
        driving: drivingSettings,
        delta: deltaSettings,
        timing: timingSettings,
        trackMap: trackMapSettings,
        fuel: fuelSettings,
        tires: tiresSettings,
        conditions: conditionsSettings,
        dashboard: dashboardSettings,
        sessionInfo: sessionInfoSettings,
        liftCoast: liftCoastSettings,
        pitstop: pitStopSettings,
        chat: chatSettings,
        performanceProfile,
        spectatorMode,
        teamMode
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
      exportConfigurationStatus.textContent = t("config.error", { detail: backendErrorMessage(error) });
      exportConfigurationStatus.title = backendErrorMessage(error);
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
    const configuration = await normalizeImportedMonitor(parseOverlayConfiguration(
      contents,
      await getDefaultCompositeLayout()
    ));
    if (!await confirmReset(
      t("config.confirmImport")
    )) {
      if (exportConfigurationStatus) exportConfigurationStatus.textContent = t("config.importCancelled");
      return;
    }
    applyImportedConfiguration(configuration);
    await setOverlayMonitorPreference(configuration.overlays.monitor).catch(() => undefined);
    if (exportConfigurationStatus) {
      exportConfigurationStatus.textContent = t("config.imported");
      exportConfigurationStatus.title = selectedPath;
    }
    window.setTimeout(() => window.location.reload(), 250);
  })().catch((error) => {
    if (exportConfigurationStatus) {
      exportConfigurationStatus.textContent = t("config.error", { detail: backendErrorMessage(error) });
      exportConfigurationStatus.title = backendErrorMessage(error);
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
  lockedIds: ReadonlySet<Id> = new Set(),
  acceptsOrder: (order: ReadonlyArray<Id>) => boolean = () => true
): void => {
  if (!container) return;
  const definitionsById = new Map(definitions.map((definition) => [definition.id, definition]));
  let draggedId: Id | null = null;

  const movedOrder = (sourceId: Id, targetIndex: number): Id[] | null => {
    if (lockedIds.has(sourceId)) return null;
    const order = [...getOrder()];
    const sourceIndex = order.indexOf(sourceId);
    if (sourceIndex < 0) return null;
    order.splice(sourceIndex, 1);
    const insertionIndex = Math.max(0, Math.min(targetIndex, order.length));
    order.splice(insertionIndex, 0, sourceId);
    return acceptsOrder(order) ? order : null;
  };

  const move = (sourceId: Id, targetIndex: number): void => {
    const order = movedOrder(sourceId, targetIndex);
    if (!order) return;
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
      previous.disabled = locked || index === 0 || lockedIds.has(order[index - 1])
        || movedOrder(id, index - 1) === null;
      previous.title = t("settings.left", { label: translatedLabel });
      previous.setAttribute("aria-label", previous.title);
      previous.addEventListener("click", () => move(id, index - 1));

      const next = document.createElement("button");
      next.type = "button";
      next.className = "column-order-button";
      next.textContent = "→";
      next.disabled = locked || index === order.length - 1 || lockedIds.has(order[index + 1])
        || movedOrder(id, index + 1) === null;
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

bindColumnOrder<StandingsColumnId>(
  document.getElementById("standings-column-order"),
  STANDINGS_COLUMNS.filter(({ id }) => !["pitStops", "pitTime", "pitLap"].includes(id)),
  () => standingsSettings.columnOrder.filter((id) => !["pitStops", "pitTime", "pitLap"].includes(id)),
  (columnOrder) => {
    standingsSettings = {
      ...standingsSettings,
      columnOrder: [...columnOrder.filter((id) => id !== "signals"), "pitStops", "pitTime", "pitLap", "signals"]
    };
    persistStandingsSettings();
  },
  new Set<StandingsColumnId>(["signals"]),
  (order) => {
    const identityById = new Map(STANDINGS_COLUMNS.map(({ id, identity }) => [id, identity]));
    let reachedHeaderColumn = false;
    for (const id of order) {
      if (id === "signals") continue;
      if (!identityById.get(id)) reachedHeaderColumn = true;
      else if (reachedHeaderColumn) return false;
    }
    return true;
  }
);

const appendToggle = (
  container: HTMLElement | null,
  labelText: string,
  checked: boolean,
  onChange: (checked: boolean) => void
): HTMLInputElement => {
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
  return input;
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
const fuelFieldOptions = document.getElementById("fuel-field-options");
for (const option of FUEL_FIELDS) {
  appendToggle(fuelFieldOptions, t(option.labelKey), fuelSettings.visible[option.id], (checked) => {
    fuelSettings = { ...fuelSettings, visible: { ...fuelSettings.visible, [option.id]: checked } };
    persistFuelSettings();
  });
}

for (const option of TIMING_TIMES) {
  appendToggle(timingTimeOptions, t(option.labelKey), timingSettings.times[option.id], (checked) => {
    timingSettings = {
      ...timingSettings,
      times: { ...timingSettings.times, [option.id]: checked }
    };
    persistTimingSettings();
  });
}

bindColumnOrder<TimingTimeId>(
  document.getElementById("timing-time-order"),
  TIMING_TIMES,
  () => timingSettings.timeOrder,
  (timeOrder) => {
    timingSettings = { ...timingSettings, timeOrder: [...timeOrder] };
    persistTimingSettings();
  }
);

const conditionsOptions = document.getElementById("conditions-options");
for (const option of CONDITIONS_OPTIONS) {
  appendToggle(conditionsOptions, t(option.labelKey), conditionsSettings.visible[option.id], (checked) => {
    conditionsSettings = {
      ...conditionsSettings,
      visible: { ...conditionsSettings.visible, [option.id]: checked }
    };
    persistConditionsSettings();
  });
}

const dashboardOptions = document.getElementById("dashboard-options");
const dashboardPitTarget = document.getElementById("dashboard-pit-warning-target") as HTMLSelectElement | null;
if (dashboardPitTarget) {
  dashboardPitTarget.value = dashboardSettings.pitWarningTarget ?? "gear";
  dashboardPitTarget.addEventListener("change", () => {
    dashboardSettings = {
      ...dashboardSettings,
      pitWarningTarget: dashboardPitTarget.value === "overlay" ? "overlay" : "gear"
    };
    persistDashboardSettings();
  });
}
const sessionInfoOptions = document.getElementById("sessioninfo-options");
const chatMaxMessages = document.getElementById("chat-max-messages") as HTMLInputElement | null;
const chatMaxMessagesValue = document.getElementById("chat-max-messages-value");
if (chatMaxMessages) {
  chatMaxMessages.value = String(chatSettings.maxMessages);
  if (chatMaxMessagesValue) chatMaxMessagesValue.textContent = String(chatSettings.maxMessages);
  chatMaxMessages.addEventListener("input", () => {
    chatSettings = normalizeChatSettings({ ...chatSettings, maxMessages: Number(chatMaxMessages.value) });
    chatMaxMessages.value = String(chatSettings.maxMessages);
    if (chatMaxMessagesValue) chatMaxMessagesValue.textContent = String(chatSettings.maxMessages);
    localStorage.setItem(CHAT_SETTINGS_KEY, JSON.stringify(chatSettings));
    void emit(CHAT_SETTINGS_EVENT, chatSettings);
    syncBrowserSourcePreferences();
    onLiveSettingsChanged();
  });
}
const chatMaxHeight = document.getElementById("chat-max-height") as HTMLInputElement | null;
const chatMaxHeightValue = document.getElementById("chat-max-height-value");
if (chatMaxHeight) {
  chatMaxHeight.value = String(chatSettings.maxHeight);
  if (chatMaxHeightValue) chatMaxHeightValue.textContent = `${chatSettings.maxHeight} px`;
  chatMaxHeight.addEventListener("input", () => {
    chatSettings = normalizeChatSettings({ ...chatSettings, maxHeight: Number(chatMaxHeight.value) });
    chatMaxHeight.value = String(chatSettings.maxHeight);
    if (chatMaxHeightValue) chatMaxHeightValue.textContent = `${chatSettings.maxHeight} px`;
    localStorage.setItem(CHAT_SETTINGS_KEY, JSON.stringify(chatSettings));
    void emit(CHAT_SETTINGS_EVENT, chatSettings);
    syncBrowserSourcePreferences();
    onLiveSettingsChanged();
  });
}
const sessionInfoLayout = document.getElementById("sessioninfo-layout") as HTMLSelectElement | null;
if (sessionInfoLayout) {
  sessionInfoLayout.value = sessionInfoSettings.layout;
  sessionInfoLayout.addEventListener("change", () => {
    if (sessionInfoLayout.value !== "line" && sessionInfoLayout.value !== "column") return;
    sessionInfoSettings = { ...sessionInfoSettings, layout: sessionInfoLayout.value };
    persistSessionInfoSettings();
  });
}
appendToggle(
  sessionInfoOptions,
  t("sessioninfo.useSystemClock"),
  sessionInfoSettings.useSystemClock,
  (checked) => {
    sessionInfoSettings = { ...sessionInfoSettings, useSystemClock: checked };
    persistSessionInfoSettings();
  }
);
for (const field of SESSIONINFO_FIELDS) {
  appendToggle(
    sessionInfoOptions,
    t(field.labelKey),
    sessionInfoSettings.visible[field.id],
    (checked) => {
      sessionInfoSettings = {
        ...sessionInfoSettings,
        visible: { ...sessionInfoSettings.visible, [field.id]: checked }
      };
      persistSessionInfoSettings();
    }
  );
}
const liftCoastDisplayMode = document.getElementById("liftcoast-display-mode") as HTMLSelectElement | null;
if (liftCoastDisplayMode) {
  liftCoastDisplayMode.value = liftCoastSettings.displayMode;
  liftCoastDisplayMode.addEventListener("change", () => {
    if (!isLiftCoastDisplayMode(liftCoastDisplayMode.value)) return;
    liftCoastSettings = { displayMode: liftCoastDisplayMode.value };
    persistLiftCoastSettings();
  });
}
for (const field of DASHBOARD_FIELDS) {
  appendToggle(
    dashboardOptions,
    t(field.labelKey),
    dashboardSettings.visible[field.id],
    (checked) => {
      dashboardSettings = {
        ...dashboardSettings,
        visible: { ...dashboardSettings.visible, [field.id]: checked }
      };
      persistDashboardSettings();
    }
  );
}
appendToggle(
  document.getElementById("pitstop-settings-options"),
  t("settings.pitstopShowChanges"),
  pitStopSettings.showChanges,
  (checked) => {
    pitStopSettings = { ...pitStopSettings, showChanges: checked };
    persistPitStopSettings();
  }
);

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

const bindDeltaSettings = (
  inputId: string,
  invertId: string,
  settings: "standings" | "relative"
): void => {
  const target = settings === "standings" ? standingsSettings : relativeSettings;
  const count = document.getElementById(inputId) as HTMLInputElement | null;
  const invert = document.getElementById(invertId) as HTMLInputElement | null;
  if (count) {
    count.value = String(target.deltaLapCount);
    count.addEventListener("change", () => {
      const parsed = Number(count.value);
      const minimum = settings === "standings" ? 1 : 2;
      const value = Number.isFinite(parsed)
        ? Math.max(minimum, Math.min(Math.round(parsed), 5))
        : target.deltaLapCount;
      count.value = String(value);
      if (settings === "standings") {
        standingsSettings = { ...standingsSettings, deltaLapCount: value };
        persistStandingsSettings();
      } else {
        relativeSettings = { ...relativeSettings, deltaLapCount: value };
        persistRelativeSettings();
      }
    });
  }
  if (invert) {
    invert.checked = target.invertDeltaLayout;
    invert.addEventListener("change", () => {
      if (settings === "standings") {
        standingsSettings = { ...standingsSettings, invertDeltaLayout: invert.checked };
        persistStandingsSettings();
      } else {
        relativeSettings = { ...relativeSettings, invertDeltaLayout: invert.checked };
        persistRelativeSettings();
      }
    });
  }
};

bindDeltaSettings("standings-delta-lap-count", "standings-invert-delta-layout", "standings");
bindDeltaSettings("relative-delta-lap-count", "relative-invert-delta-layout", "relative");

const standingsDeltaReference = document.getElementById("standings-delta-reference") as HTMLSelectElement | null;
if (standingsDeltaReference) {
  standingsDeltaReference.value = standingsSettings.deltaReference;
  standingsDeltaReference.addEventListener("change", () => {
    const value = standingsDeltaReference.value === "best_lap" ? "best_lap" : "last_lap";
    standingsDeltaReference.value = value;
    standingsSettings = { ...standingsSettings, deltaReference: value };
    persistStandingsSettings();
  });
}

const bindLapTimeCombination = (id: string, overlay: "standings" | "relative"): void => {
  const input = document.getElementById(id) as HTMLInputElement | null;
  if (!input) return;
  input.checked = overlay === "standings"
    ? standingsSettings.combineLapTimes
    : relativeSettings.combineLapTimes;
  input.addEventListener("change", () => {
    if (overlay === "standings") {
      standingsSettings = { ...standingsSettings, combineLapTimes: input.checked };
      persistStandingsSettings();
    } else {
      relativeSettings = { ...relativeSettings, combineLapTimes: input.checked };
      persistRelativeSettings();
    }
  });
};

bindLapTimeCombination("standings-combine-lap-times", "standings");
bindLapTimeCombination("relative-combine-lap-times", "relative");

const bindDriverNameFormat = (
  id: string,
  current: () => DriverNameFormat,
  update: (format: DriverNameFormat) => void
): void => {
  const select = document.getElementById(id) as HTMLSelectElement | null;
  if (!select) return;
  for (const format of DRIVER_NAME_FORMATS) {
    select.add(new Option(sentenceCase(t(format.labelKey)), format.id));
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

bindColumnOrder<RelativeColumnId>(
  document.getElementById("relative-column-order"),
  RELATIVE_COLUMNS.filter(({ id }) => !["pitStops", "pitTime", "pitLap"].includes(id)),
  () => relativeSettings.columnOrder.filter((id) => !["pitStops", "pitTime", "pitLap"].includes(id)),
  (columnOrder) => {
    relativeSettings = {
      ...relativeSettings,
      columnOrder: [...columnOrder.filter((id) => id !== "signals"), "pitStops", "pitTime", "pitLap", "signals"]
    };
    persistRelativeSettings();
  },
  new Set<RelativeColumnId>(["signals"])
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
  ["showGraph", t("readout.graph")],
  ["showSteering", t("readout.steering")],
  ["showForceFeedback", t("readout.ffb")],
  ["showSpeed", t("readout.speed")],
  ["showGear", t("readout.gear")],
  ["showRpmLeds", t("readout.rpmLeds")]
] as const) {
  appendToggle(drivingReadoutOptions, label, drivingSettings[key], (checked) => {
    drivingSettings = { ...drivingSettings, [key]: checked };
    persistDrivingSettings();
  });
}

appendToggle(drivingInputPedals, t("settings.pedalLabels"), drivingSettings.showPedalLabels, (checked) => {
  drivingSettings = { ...drivingSettings, showPedalLabels: checked };
  persistDrivingSettings();
});

const graphPositionSelect = document.getElementById("driving-graph-position") as HTMLSelectElement | null;
if (graphPositionSelect) {
  graphPositionSelect.value = drivingSettings.graphPosition;
  graphPositionSelect.addEventListener("change", () => {
    const graphPosition = graphPositionSelect.value === "left" ? "left" : "right";
    drivingSettings = { ...drivingSettings, graphPosition };
    persistDrivingSettings();
  });
}

interface LoggingControlOptions {
  inputId: string;
  cardId: string;
  descriptionId: string;
  pathId: string;
  enabledDescription: TranslationKey;
  disabledDescription: TranslationKey;
  activePath: TranslationKey;
  disabledPath: TranslationKey;
  getCommand: string;
  setCommand: string;
  changeError: string;
  loadError: string;
  pollIntervalMs?: number;
  onRender?: (status: TelemetryLoggingStatus) => void;
}

const bindLoggingControl = (options: LoggingControlOptions): void => {
  const input = document.getElementById(options.inputId) as HTMLInputElement | null;
  const render = (status: TelemetryLoggingStatus): void => {
    options.onRender?.(status);
    if (input) input.checked = status.enabled;
    document.getElementById(options.cardId)?.classList.toggle("active", status.enabled);
    const description = document.getElementById(options.descriptionId);
    if (description) {
      description.textContent = t(status.enabled
        ? options.enabledDescription
        : options.disabledDescription);
    }
    const path = document.getElementById(options.pathId);
    if (path) {
      const location = status.active_file ?? status.directory;
      const name = location.split(/[\\/]/).filter(Boolean).at(-1) ?? location;
      path.textContent = t(status.enabled ? options.activePath : options.disabledPath, { name });
      path.title = location;
    }
  };
  const refresh = async (): Promise<void> => {
    render(await invoke<TelemetryLoggingStatus>(options.getCommand));
  };

  input?.addEventListener("change", () => {
    const enabled = input.checked;
    input.disabled = true;
    void invoke<TelemetryLoggingStatus>(options.setCommand, { enabled })
      .then(render)
      .then(() => new Promise((resolve) => window.setTimeout(resolve, 200)))
      .then(refresh)
      .catch((error) => {
        console.error(options.changeError, error);
        input.checked = !enabled;
      })
      .finally(() => {
        input.disabled = false;
      });
  });

  void refresh().catch((error) => console.error(options.loadError, error));
  if (options.pollIntervalMs) {
    window.setInterval(() => {
      void refresh().catch(() => undefined);
    }, options.pollIntervalMs);
  }
};

bindLoggingControl({
  inputId: "strategy-logging",
  cardId: "strategy-logging-card",
  descriptionId: "strategy-logging-description",
  pathId: "strategy-logging-path",
  enabledDescription: "strategyLog.onSub",
  disabledDescription: "strategyLog.offSub",
  activePath: "strategyLog.active",
  disabledPath: "strategyLog.disabled",
  getCommand: "get_strategy_logging",
  setCommand: "set_strategy_logging",
  changeError: "No se pudo cambiar el registro estratégico:",
  loadError: "No se pudo leer el estado del registro estratégico:",
  pollIntervalMs: 5_000
});

bindLoggingControl({
  inputId: "dr-estimate-logging",
  cardId: "dr-estimate-logging-card",
  descriptionId: "dr-estimate-logging-description",
  pathId: "dr-estimate-logging-path",
  enabledDescription: "drEstimateLog.onSub",
  disabledDescription: "drEstimateLog.offSub",
  activePath: "drEstimateLog.active",
  disabledPath: "drEstimateLog.disabled",
  getCommand: "get_driver_rank_estimate_logging",
  setCommand: "set_driver_rank_estimate_logging",
  changeError: "No se pudo cambiar el registro de estimación de DR:",
  loadError: "No se pudo leer el estado del registro de estimación de DR:",
  pollIntervalMs: 5_000
});

bindLoggingControl({
  inputId: "telemetry-logging",
  cardId: "logging-card",
  descriptionId: "logging-description",
  pathId: "logging-path",
  enabledDescription: "logging.onSub",
  disabledDescription: "logging.offSub",
  activePath: "logging.active",
  disabledPath: "logging.disabled",
  getCommand: "get_telemetry_logging",
  setCommand: "set_telemetry_logging",
  changeError: "No se pudo cambiar el registro de telemetría:",
  loadError: "No se pudo leer el estado del registro de telemetría:",
  onRender: (status) => {
    void emit("performance://logging", { enabled: status.enabled })
      .catch(reportInitializationError("performance logging state"));
  }
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
  await synchronizeOverlayHosts();
};

/**
 * Overlays whose whole reason to exist is one capability. Everything else
 * degrades inside the overlay: an empty column means this session has no such
 * data, while a missing capability means the simulator never will.
 */
const overlayCapability: Partial<Record<OverlayId, keyof SourceCapabilities>> = {
  damage: "damage_detail",
  forecast: "weather_forecast",
  liftcoast: "lift_and_coast",
  pitstop: "pit_service_estimate",
  dashboard: "car_electronics"
};

let unsupportedOverlays = "";

const renderSourceCapabilities = (frame: TelemetryFrame): void => {
  const capabilities = frame.capabilities;
  if (!capabilities) return;
  const entries = Object.entries(overlayCapability) as [OverlayId, keyof SourceCapabilities][];
  const unsupported = entries.filter(([, capability]) => !capabilities[capability]).map(([id]) => id);
  const signature = `${frame.source}:${unsupported.join(",")}`;
  if (signature === unsupportedOverlays) return;
  unsupportedOverlays = signature;
  for (const [id] of entries) {
    const card = document.querySelector<HTMLElement>(`[data-overlay-card="${id}"]`);
    const supported = !unsupported.includes(id);
    card?.toggleAttribute("data-unsupported", !supported);
    renderOverlayAvailability(id);
  }
};

const renderConnection = (frame: TelemetryFrame): void => {
  const status = document.getElementById("game-status");
  const label = status?.querySelector("span");
  if (!status || !label) return;
  status.title = "";

  const simulator = activeSimulatorName();
  if (!frame.connected) {
    const dependencyMissing = simulatorStatus?.dependency?.available === false;
    status.dataset.state = dependencyMissing ? "error" : "offline";
    label.textContent = t(dependencyMissing ? "status.plugin" : "status.waiting", { simulator });
    if (dependencyMissing) {
      status.title = t("status.pluginTitle", { dependency: simulatorStatus?.dependency?.detail ?? "" });
    }
  } else if (!frame.player_active) {
    status.dataset.state = "standby";
    label.textContent = t("status.noCar", { simulator });
  } else {
    status.dataset.state = "live";
    label.textContent = t("status.active");
  }
};

let supportConnected: boolean | null = null;
void listen<TelemetryFrame>("telemetry://frame", ({ payload }) => {
  supportConnected = payload.connected;
  // The active simulator is chosen at runtime, so the panel follows the frame
  // instead of the answer it got once at startup.
  if (simulatorStatus && payload.source !== simulatorStatus.id) refreshSimulatorStatus();
  renderConnection(payload);
  renderSourceCapabilities(payload);
  trackSessionKind(payload);
})
  .catch(reportInitializationError("telemetry listener"));
const diagnosticButton = document.getElementById("copy-support-diagnostic") as HTMLButtonElement;
diagnosticButton.addEventListener("click", () => {
  diagnosticButton.disabled = true;
  void (async () => {
    const output = document.getElementById("support-diagnostic-output") as HTMLTextAreaElement;
    const status = document.getElementById("support-diagnostic-status")!;
    // Explicit allowlist: never serialize source status, settings or logs wholesale.
    const report = JSON.stringify({
      application: "BlackRack Overlay",
      version: await getVersion().catch(() => null),
      simulator: simulatorStatus?.id ?? null,
      simulatorSelection: simulatorStatus?.preference ?? null,
      connected: supportConnected,
      dependencyAvailable: simulatorStatus?.dependency?.available ?? null,
      locale: getLocale(),
      performanceProfile,
      mode: activeMode,
      enabledOverlays: overlayIds.filter((id) => preferences[id]),
      displayScale: window.devicePixelRatio,
      screen: { width: window.screen.width, height: window.screen.height },
      overlayMonitor: (document.getElementById("overlay-monitor") as HTMLSelectElement | null)?.value || null
    }, null, 2);
    output.value = report;
    output.hidden = false;
    try {
      await navigator.clipboard.writeText(report);
      status.textContent = t("start.copied");
    } catch {
      status.textContent = t("start.copyFallback");
      output.focus();
      output.select();
    }
  })().finally(() => { diagnosticButton.disabled = false; });
});

const renderInteractionMode = (mode: InteractionMode): void => {
  const status = document.getElementById("interaction-status");
  if (status) status.textContent = t(mode.click_through ? "mode.game" : "mode.edit");
  const button = document.getElementById("toggle-interaction-mode");
  if (button) button.textContent = t(mode.click_through ? "mode.toEdit" : "mode.toGame");
};

void listen<InteractionMode>("overlay://interaction-mode", ({ payload }) => renderInteractionMode(payload))
  .catch(reportInitializationError("interaction mode listener"));

void listen<OverlayState>("overlay://visibility", ({ payload }) => {
  if (!overlayIds.includes(payload.label)) return;
  // Spectator restrictions only affect the mounted panel. Keep its saved
  // profile visibility while the panel is temporarily blocked.
  if (disabledInSpectator(payload.label)) {
    setCardState(payload.label, false);
    if (payload.visible) {
      void invoke("set_overlay_visible", { label: payload.label, visible: false })
        .catch((error) => console.error(`No se pudo mantener oculto ${payload.label}:`, error));
    }
    return;
  }
  preferences[payload.label] = payload.visible;
  persist();
  setCardState(payload.label, payload.visible);
  void synchronizeOverlayHosts().catch((error) => {
    console.error("No se pudieron sincronizar los hosts tras cambiar la visibilidad:", error);
  });
}).catch(reportInitializationError("overlay visibility listener"));

document.getElementById("toggle-interaction-mode")?.addEventListener("click", () => {
  void invoke<InteractionMode>("toggle_interaction_mode_command").catch((error) => {
    console.error("Could not toggle interaction mode:", error);
    setShortcutMessage(t("mode.error"), "error");
  });
});

void restoreWindows().catch(reportInitializationError("overlay visibility"));
void bindMonitorSelector().catch(reportInitializationError("monitor selector"));
void bindOverlayMonitorSelectors().catch(reportInitializationError("overlay monitor selectors"));
window.setInterval(() => {
  void refreshMonitorInventory().catch((error) => {
    console.warn("No se pudo actualizar la topología de monitores:", error);
  });
}, 2000);
syncBrowserSourcePreferences();
void invoke<BrowserSourceStatus>("get_browser_source_status")
  .then(renderBrowserSourceStatus)
  .catch(reportInitializationError("browser source status"));
void invoke<ShortcutSettingsStatus>("get_shortcut_settings")
  .then(renderShortcutSettings)
  .catch((error) => setShortcutMessage(t("shortcuts.loadError", { detail: backendErrorMessage(error) }), "error"));
void invoke<InteractionMode>("get_interaction_mode")
  .then(renderInteractionMode)
  .catch(reportInitializationError("interaction mode"));
const refreshSimulatorStatus = (): void => {
  void invoke<SimulatorStatus>("get_simulator_status").then((status) => {
    simulatorStatus = status;
    applySimulatorPreferenceOptions(status.options);
  }).catch(reportInitializationError("simulator status"));
};
refreshSimulatorStatus();

const standingsPitLayout = document.getElementById("standings-pit-layout") as HTMLSelectElement | null;
if (standingsPitLayout) {
  standingsPitLayout.value = standingsSettings.pitInformationLayout;
  standingsPitLayout.addEventListener("change", () => {
    const value = standingsPitLayout.value;
    if (value !== "inline" && value !== "above" && value !== "column") return;
    standingsSettings = { ...standingsSettings, pitInformationLayout: value };
    persistStandingsSettings();
  });
}

const relativePitLayout = document.getElementById("relative-pit-layout") as HTMLSelectElement | null;
if (relativePitLayout) {
  relativePitLayout.value = relativeSettings.pitInformationLayout;
  relativePitLayout.addEventListener("change", () => {
    const value = relativePitLayout.value;
    if (value !== "inline" && value !== "above" && value !== "column") return;
    relativeSettings = { ...relativeSettings, pitInformationLayout: value };
    persistRelativeSettings();
  });
}
