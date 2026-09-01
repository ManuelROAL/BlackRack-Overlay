import { t } from "./i18n";
import type { TranslationKey } from "./i18n/catalogs";

// Tauri commands reject with a stable snake_case code instead of prose: the
// backend writes the English detail to the diagnostics log and the wording the
// user reads lives here, next to the rest of the interface copy. Keep this table
// in step with the `Err` payloads in `src-tauri/src`.
const BACKEND_ERROR_KEYS: Readonly<Record<string, TranslationKey>> = {
  control_window_required: "error.controlWindowRequired",
  control_window_failed: "error.controlWindowFailed",
  overlay_host_required: "error.overlayHostRequired",
  overlay_host_failed: "error.overlayHostFailed",
  overlay_host_handle_failed: "error.overlayHostGeometry",
  overlay_host_geometry_failed: "error.overlayHostGeometry",
  overlay_visibility_failed: "error.overlayVisibility",
  overlay_input_tracker_failed: "error.overlayInputTracker",
  overlay_input_tracker_already_started: "error.overlayInputTracker",
  unknown_overlay: "error.unknownOverlay",
  monitors_unavailable: "error.monitorsUnavailable",
  monitor_unavailable: "error.monitorUnavailable",
  settings_directory_failed: "error.settingsDirectory",
  settings_encode_failed: "error.settingsEncode",
  settings_write_failed: "error.settingsWrite",
  support_page_failed: "error.supportPage",
  // The importer already validates the file itself, so a rejected configuration
  // reuses the wording the frontend shows for the same problem.
  configuration_invalid_json: "config.invalidJson",
  configuration_unrecognized: "config.incompatible",
  configuration_extension_required: "error.configurationExtension",
  configuration_directory_failed: "error.configurationDirectory",
  configuration_write_failed: "error.configurationWrite",
  configuration_read_failed: "error.configurationRead",
  // Every way the official layout can fail reads the same to the user; the code
  // only has to tell the diagnostics log which step gave up.
  track_geometry_key_missing: "error.trackGeometry",
  track_geometry_client_failed: "error.trackGeometry",
  track_geometry_request_failed: "error.trackGeometry",
  track_geometry_malformed: "error.trackGeometry",
  track_geometry_main_path_invalid: "error.trackGeometry",
  track_geometry_pit_path_invalid: "error.trackGeometry",
  track_geometry_unsupported: "error.trackGeometry"
};

export const isBackendErrorCode = (value: unknown): value is string =>
  typeof value === "string" && Object.hasOwn(BACKEND_ERROR_KEYS, value);

/**
 * Turns a rejected command into text the user can read. Known backend codes are
 * translated; an `Error` thrown by the frontend already carries a localized
 * message, and anything else is an IPC failure with no wording of its own.
 */
export const backendErrorMessage = (error: unknown): string => {
  if (isBackendErrorCode(error)) return t(BACKEND_ERROR_KEYS[error]);
  if (error instanceof Error && error.message) return error.message;
  if (typeof error === "string" && error.trim() !== "") return error;
  return t("error.unexpected");
};
