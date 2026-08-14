mod browser_source;
mod lmu_install;
mod startup_log;
mod telemetry;

use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use tauri::{
    window::Color, AppHandle, Emitter, EventTarget, Manager, PhysicalPosition, PhysicalSize,
    Position, Size, State, WebviewUrl, WebviewWindow, WebviewWindowBuilder,
};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};
use tauri_plugin_window_state::StateFlags;

const OVERLAY_LABELS: [&str; 11] = [
    "dashboard",
    "driving",
    "tires",
    "damage",
    "standings",
    "relative",
    "fuel",
    "pitstop",
    "flags",
    "rejoin",
    "trackmap",
];

const TRANSPARENT_BACKGROUND: Color = Color(0, 0, 0, 0);
const OVERLAY_HOST_PREFIX: &str = "overlay-monitor-";
const DEFAULT_CLICK_THROUGH: bool = true;

struct OverlayControl {
    click_through: AtomicBool,
    auto_hidden: AtomicBool,
    desired_visible: Mutex<HashSet<&'static str>>,
}

#[derive(Clone, Deserialize, Serialize)]
struct ShortcutSettings {
    interaction_mode: String,
    show_panel: String,
}

impl Default for ShortcutSettings {
    fn default() -> Self {
        Self {
            interaction_mode: "Ctrl+Shift+O".into(),
            show_panel: "Ctrl+Shift+M".into(),
        }
    }
}

#[derive(Default)]
struct ShortcutRuntime {
    settings: ShortcutSettings,
    active_interaction_mode: Option<String>,
    active_show_panel: Option<String>,
    interaction_mode_error: Option<String>,
    show_panel_error: Option<String>,
}

struct ShortcutControl(Mutex<ShortcutRuntime>);

#[derive(Clone, Serialize)]
struct ShortcutBindingStatus {
    shortcut: String,
    active: bool,
    error: Option<String>,
}

#[derive(Clone, Serialize)]
struct ShortcutSettingsStatus {
    interaction_mode: ShortcutBindingStatus,
    show_panel: ShortcutBindingStatus,
}

#[derive(Clone, Serialize)]
struct InteractionMode {
    click_through: bool,
}

#[tauri::command]
fn get_interaction_mode(control: State<'_, OverlayControl>) -> InteractionMode {
    InteractionMode {
        click_through: control.click_through.load(Ordering::Relaxed),
    }
}

#[derive(Serialize)]
struct OverlayWindowState {
    label: &'static str,
    visible: bool,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct OverlayDisplay {
    index: usize,
    label: String,
    name: String,
    x: i32,
    y: i32,
    width: u32,
    height: u32,
    scale_factor: f64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct OverlayPlacementSeed {
    overlay: &'static str,
    monitor: usize,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

#[derive(Serialize)]
struct LmuDependencyStatus {
    telemetry_plugin_available: bool,
    telemetry_plugin_path: Option<String>,
}

fn sorted_monitors(app: &AppHandle) -> Result<Vec<tauri::Monitor>, String> {
    let mut monitors = app
        .available_monitors()
        .map_err(|error| error.to_string())?;
    monitors.sort_by(|left, right| {
        left.position()
            .y
            .cmp(&right.position().y)
            .then(left.position().x.cmp(&right.position().x))
            .then(left.name().cmp(&right.name()))
    });
    Ok(monitors)
}

fn overlay_displays(app: &AppHandle) -> Result<Vec<OverlayDisplay>, String> {
    sorted_monitors(app).map(|monitors| {
        monitors
            .into_iter()
            .enumerate()
            .map(|(index, monitor)| OverlayDisplay {
                index,
                label: format!("{OVERLAY_HOST_PREFIX}{index}"),
                name: monitor
                    .name()
                    .cloned()
                    .unwrap_or_else(|| format!("Monitor {}", index + 1)),
                x: monitor.position().x,
                y: monitor.position().y,
                width: monitor.size().width,
                height: monitor.size().height,
                scale_factor: monitor.scale_factor(),
            })
            .collect()
    })
}

fn create_overlay_hosts(app: &AppHandle) -> Result<(), String> {
    for (index, monitor) in sorted_monitors(app)?.into_iter().enumerate() {
        let label = format!("{OVERLAY_HOST_PREFIX}{index}");
        let logical_size = monitor.size().to_logical::<f64>(monitor.scale_factor());
        let window =
            WebviewWindowBuilder::new(app, &label, WebviewUrl::App("composite.html".into()))
                .title(format!("LMU Overlay · Monitor {}", index + 1))
                .inner_size(logical_size.width, logical_size.height)
                .transparent(true)
                .background_color(TRANSPARENT_BACKGROUND)
                .decorations(false)
                .shadow(false)
                .always_on_top(true)
                .skip_taskbar(true)
                .resizable(false)
                .devtools(false)
                .visible(false)
                .build()
                .map_err(|error| {
                    format!("No se pudo crear el host del monitor {index}: {error}")
                })?;

        window
            .set_position(Position::Physical(PhysicalPosition::new(
                monitor.position().x,
                monitor.position().y,
            )))
            .map_err(|error| error.to_string())?;
        window
            .set_size(Size::Physical(PhysicalSize::new(
                monitor.size().width,
                monitor.size().height,
            )))
            .map_err(|error| error.to_string())?;
        window
            .set_ignore_cursor_events(DEFAULT_CLICK_THROUGH)
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn for_each_overlay_host(app: &AppHandle, mut action: impl FnMut(&WebviewWindow)) {
    for (label, window) in app.webview_windows() {
        if label.starts_with(OVERLAY_HOST_PREFIX) {
            action(&window);
        }
    }
}

pub(crate) fn overlay_is_active(app: &AppHandle, label: &str) -> bool {
    let control = app.state::<OverlayControl>();
    !control.auto_hidden.load(Ordering::Relaxed)
        && control
            .desired_visible
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .contains(label)
}

#[derive(Serialize)]
struct OverlayFrameBatch<'a, T> {
    targets: &'a [&'a str],
    frame: &'a T,
}

pub(crate) fn emit_overlay_frames<T: Serialize>(
    app: &AppHandle,
    targets: &[&str],
    frame: &T,
) -> bool {
    if targets.is_empty() {
        return false;
    }
    let batch = OverlayFrameBatch { targets, frame };
    app.emit_filter("telemetry://batch", &batch, is_overlay_host_target)
        .is_ok()
}

fn is_overlay_host_target(target: &EventTarget) -> bool {
    match target {
        EventTarget::Window { label }
        | EventTarget::Webview { label }
        | EventTarget::WebviewWindow { label }
        | EventTarget::AnyLabel { label } => label.starts_with(OVERLAY_HOST_PREFIX),
        _ => false,
    }
}

pub(crate) fn update_overlay_auto_visibility(app: &AppHandle, frame: &telemetry::TelemetryFrame) {
    let mut overlay_has_focus = false;
    for_each_overlay_host(app, |window| {
        overlay_has_focus |= window.is_focused().unwrap_or(false);
    });
    let should_hide = frame.should_hide_overlays(overlay_has_focus);
    let control = app.state::<OverlayControl>();
    if control.auto_hidden.swap(should_hide, Ordering::Relaxed) == should_hide {
        return;
    }

    if should_hide {
        for_each_overlay_host(app, |window| {
            let _ = window.hide();
        });
        return;
    }

    let has_desired_overlays = !control
        .desired_visible
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .is_empty();
    if has_desired_overlays {
        for_each_overlay_host(app, |window| {
            let _ = window.show();
        });
    }
}

#[tauri::command]
async fn set_overlay_visible(
    app: AppHandle,
    control: State<'_, OverlayControl>,
    label: String,
    visible: bool,
) -> Result<bool, String> {
    let label = OVERLAY_LABELS
        .iter()
        .copied()
        .find(|known| *known == label)
        .ok_or_else(|| format!("Overlay desconocido: {label}"))?;

    {
        let mut desired = control
            .desired_visible
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if visible {
            desired.insert(label);
        } else {
            desired.remove(label);
        }
    }

    let state = OverlayWindowState { label, visible };
    app.emit("overlay://visibility", &state)
        .map_err(|error| error.to_string())?;

    if !control.auto_hidden.load(Ordering::Relaxed) {
        let has_desired_overlays = !control
            .desired_visible
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .is_empty();
        for_each_overlay_host(&app, |window| {
            let _ = if has_desired_overlays {
                window.show()
            } else {
                window.hide()
            };
        });
    }

    Ok(visible)
}

#[tauri::command]
async fn get_overlay_states(app: AppHandle) -> Vec<OverlayWindowState> {
    let control = app.state::<OverlayControl>();
    let desired = control
        .desired_visible
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    OVERLAY_LABELS
        .iter()
        .map(|label| OverlayWindowState {
            label,
            visible: desired.contains(label),
        })
        .collect()
}

#[tauri::command]
fn get_overlay_displays(app: AppHandle) -> Result<Vec<OverlayDisplay>, String> {
    overlay_displays(&app)
}

fn default_overlay_geometry(label: &str) -> (f64, f64, f64, f64) {
    match label {
        "dashboard" => (20.0, 20.0, 780.0, 340.0),
        "driving" => (20.0, 380.0, 540.0, 120.0),
        "tires" => (588.0, 380.0, 174.0, 130.0),
        "damage" => (798.0, 380.0, 94.0, 94.0),
        "standings" => (20.0, 70.0, 980.0, 500.0),
        "relative" => (20.0, 590.0, 980.0, 300.0),
        "fuel" => (1020.0, 70.0, 356.0, 202.0),
        "pitstop" => (1020.0, 320.0, 210.0, 150.0),
        "flags" => (1240.0, 320.0, 360.0, 120.0),
        "rejoin" => (1240.0, 460.0, 360.0, 140.0),
        "trackmap" => (1460.0, 70.0, 436.0, 436.0),
        _ => (20.0, 20.0, 360.0, 180.0),
    }
}

#[tauri::command]
fn get_default_overlay_placement(label: String) -> Result<OverlayPlacementSeed, String> {
    let overlay = OVERLAY_LABELS
        .iter()
        .copied()
        .find(|candidate| *candidate == label)
        .ok_or_else(|| format!("Overlay desconocido: {label}"))?;
    let (x, y, width, height) = default_overlay_geometry(overlay);
    Ok(OverlayPlacementSeed {
        overlay,
        monitor: 0,
        x,
        y,
        width,
        height,
    })
}

#[tauri::command]
fn get_composite_layout_seed(app: AppHandle) -> Result<Vec<OverlayPlacementSeed>, String> {
    let displays = overlay_displays(&app)?;
    if displays.is_empty() {
        return Err("No se detectaron monitores para alojar los overlays".into());
    }
    let primary = displays
        .iter()
        .find(|display| display.x == 0 && display.y == 0)
        .unwrap_or(&displays[0]);
    let saved = app
        .path()
        .app_config_dir()
        .ok()
        .and_then(|directory| fs::read_to_string(directory.join(".window-state.json")).ok())
        .and_then(|contents| serde_json::from_str::<serde_json::Value>(&contents).ok());

    Ok(OVERLAY_LABELS
        .iter()
        .map(|label| {
            let (default_x, default_y, default_width, default_height) =
                default_overlay_geometry(label);
            let state = saved.as_ref().and_then(|value| value.get(*label));
            let saved_geometry = state.and_then(|value| {
                Some((
                    value.get("x")?.as_i64()? as i32,
                    value.get("y")?.as_i64()? as i32,
                    value.get("width")?.as_u64()? as u32,
                    value.get("height")?.as_u64()? as u32,
                ))
            });
            let Some((saved_x, saved_y, saved_width, saved_height)) = saved_geometry else {
                return OverlayPlacementSeed {
                    overlay: label,
                    monitor: primary.index,
                    x: default_x,
                    y: default_y,
                    width: default_width,
                    height: default_height,
                };
            };
            let center_x = saved_x + saved_width as i32 / 2;
            let center_y = saved_y + saved_height as i32 / 2;
            let display = displays
                .iter()
                .find(|display| {
                    center_x >= display.x
                        && center_x < display.x + display.width as i32
                        && center_y >= display.y
                        && center_y < display.y + display.height as i32
                })
                .unwrap_or(primary);
            let scale = display.scale_factor.max(0.1);
            OverlayPlacementSeed {
                overlay: label,
                monitor: display.index,
                x: (saved_x - display.x) as f64 / scale,
                y: (saved_y - display.y) as f64 / scale,
                width: saved_width as f64 / scale,
                height: saved_height as f64 / scale,
            }
        })
        .collect())
}

#[tauri::command]
fn get_telemetry_logging() -> telemetry::TelemetryLoggingStatus {
    telemetry::telemetry_logging_status()
}

#[tauri::command]
fn set_telemetry_logging(enabled: bool) -> Result<telemetry::TelemetryLoggingStatus, String> {
    telemetry::set_telemetry_logging(enabled)
}

#[tauri::command]
fn record_frontend_performance(sample: serde_json::Value) {
    telemetry::queue_frontend_performance(sample);
}

#[tauri::command]
fn get_track_map_geometry(cache_key: String) -> Result<telemetry::TrackMapGeometry, String> {
    telemetry::track_map_geometry(&cache_key)
}

#[tauri::command]
fn migrate_legacy_track_map_learning(
    cache_key: String,
    track_name: String,
    track_length: f64,
    points: Vec<telemetry::LearnedTrackPoint>,
    pit_traversal_samples: Vec<f64>,
) -> bool {
    telemetry::migrate_legacy_track_map_learning(
        &cache_key,
        track_name,
        track_length,
        points,
        pit_traversal_samples,
    )
}

#[tauri::command]
fn get_browser_source_status() -> browser_source::BrowserSourceStatus {
    browser_source::status()
}

#[tauri::command]
fn set_browser_source_enabled(enabled: bool) -> browser_source::BrowserSourceStatus {
    browser_source::set_enabled(enabled)
}

#[tauri::command]
fn set_browser_source_preferences(preferences: serde_json::Value) {
    browser_source::set_preferences(preferences);
}

#[tauri::command]
fn set_overlay_view_settings(settings: telemetry::OverlayViewSettings) {
    telemetry::set_overlay_view_settings(settings);
}

#[tauri::command]
fn get_lmu_dependency_status() -> LmuDependencyStatus {
    let plugin = lmu_install::telemetry_plugin();
    LmuDependencyStatus {
        telemetry_plugin_available: plugin.is_some(),
        telemetry_plugin_path: plugin.map(|path| path.display().to_string()),
    }
}

fn shortcut_settings_path(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    app.path()
        .app_config_dir()
        .map(|directory| directory.join("shortcuts.json"))
        .map_err(|error| format!("No se pudo localizar la configuracion: {error}"))
}

fn load_shortcut_settings(app: &AppHandle) -> ShortcutSettings {
    let Ok(path) = shortcut_settings_path(app) else {
        return ShortcutSettings::default();
    };
    let Ok(contents) = fs::read_to_string(&path) else {
        return ShortcutSettings::default();
    };
    match serde_json::from_str::<ShortcutSettings>(&contents) {
        Ok(settings)
            if !settings.interaction_mode.trim().is_empty()
                && !settings.show_panel.trim().is_empty() =>
        {
            settings
        }
        Ok(_) => {
            startup_log::record("warning: empty shortcut configuration; using defaults");
            ShortcutSettings::default()
        }
        Err(error) => {
            startup_log::record(format!(
                "warning: invalid shortcut configuration at {}: {error}",
                path.display()
            ));
            ShortcutSettings::default()
        }
    }
}

fn save_shortcut_settings(app: &AppHandle, settings: &ShortcutSettings) -> Result<(), String> {
    let path = shortcut_settings_path(app)?;
    if let Some(directory) = path.parent() {
        fs::create_dir_all(directory)
            .map_err(|error| format!("No se pudo crear la carpeta de configuracion: {error}"))?;
    }
    let contents = serde_json::to_string_pretty(settings)
        .map_err(|error| format!("No se pudo serializar la configuracion: {error}"))?;
    fs::write(&path, contents)
        .map_err(|error| format!("No se pudo guardar {}: {error}", path.display()))
}

fn validate_overlay_configuration(contents: &str) -> Result<(), String> {
    let parsed: serde_json::Value = serde_json::from_str(&contents)
        .map_err(|error| format!("La configuración no es JSON válido: {error}"))?;
    if parsed.get("format").and_then(serde_json::Value::as_str) != Some("lmu-overlay-configuration")
        || parsed
            .get("schemaVersion")
            .and_then(serde_json::Value::as_u64)
            != Some(1)
    {
        return Err("Formato de configuración no reconocido".into());
    }
    Ok(())
}

#[tauri::command]
fn export_overlay_configuration(path: PathBuf, contents: String) -> Result<String, String> {
    validate_overlay_configuration(&contents)?;
    if !path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("json"))
    {
        return Err("El archivo de configuración debe tener extensión .json".into());
    }
    if let Some(directory) = path.parent() {
        fs::create_dir_all(directory)
            .map_err(|error| format!("No se pudo preparar la carpeta elegida: {error}"))?;
    }
    fs::write(&path, contents)
        .map_err(|error| format!("No se pudo guardar {}: {error}", path.display()))?;
    Ok(path.display().to_string())
}

#[tauri::command]
fn import_overlay_configuration(path: PathBuf) -> Result<String, String> {
    if !path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("json"))
    {
        return Err("El archivo de configuración debe tener extensión .json".into());
    }
    let contents = fs::read_to_string(&path)
        .map_err(|error| format!("No se pudo leer {}: {error}", path.display()))?;
    validate_overlay_configuration(&contents)?;
    Ok(contents)
}

fn toggle_interaction_mode(app: &AppHandle) {
    let control = app.state::<OverlayControl>();
    let next = !control.click_through.load(Ordering::Relaxed);
    control.click_through.store(next, Ordering::Relaxed);

    for_each_overlay_host(app, |window| {
        let _ = window.set_ignore_cursor_events(next);
        let _ = window.set_always_on_top(true);
    });
    if let Some(panel) = app.get_webview_window("control") {
        let _ = panel.set_always_on_top(!next);
        if !next {
            let _ = panel.show();
            let _ = panel.set_focus();
        }
    }
    let _ = app.emit(
        "overlay://interaction-mode",
        InteractionMode {
            click_through: next,
        },
    );
}

#[tauri::command]
fn toggle_interaction_mode_command(app: AppHandle) -> InteractionMode {
    toggle_interaction_mode(&app);
    get_interaction_mode(app.state::<OverlayControl>())
}

fn register_interaction_shortcut(app: &AppHandle, shortcut: &str) -> Result<(), String> {
    app.global_shortcut()
        .on_shortcut(shortcut, |app, _, event| {
            if event.state() == ShortcutState::Pressed {
                toggle_interaction_mode(app);
            }
        })
        .map_err(|error| error.to_string())
}

fn register_panel_shortcut(app: &AppHandle, shortcut: &str) -> Result<(), String> {
    app.global_shortcut()
        .on_shortcut(shortcut, |app, _, event| {
            if event.state() != ShortcutState::Pressed {
                return;
            }
            if let Some(panel) = app.get_webview_window("control") {
                let _ = panel.show();
                let _ = panel.unminimize();
                let _ = panel.set_focus();
            }
        })
        .map_err(|error| error.to_string())
}

fn shortcut_status(runtime: &ShortcutRuntime) -> ShortcutSettingsStatus {
    ShortcutSettingsStatus {
        interaction_mode: ShortcutBindingStatus {
            shortcut: runtime.settings.interaction_mode.clone(),
            active: runtime.active_interaction_mode.is_some(),
            error: runtime.interaction_mode_error.clone(),
        },
        show_panel: ShortcutBindingStatus {
            shortcut: runtime.settings.show_panel.clone(),
            active: runtime.active_show_panel.is_some(),
            error: runtime.show_panel_error.clone(),
        },
    }
}

#[tauri::command]
fn get_shortcut_settings(control: State<'_, ShortcutControl>) -> ShortcutSettingsStatus {
    let runtime = control
        .0
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    shortcut_status(&runtime)
}

#[tauri::command]
fn set_shortcut(
    app: AppHandle,
    control: State<'_, ShortcutControl>,
    action: String,
    shortcut: String,
) -> Result<ShortcutSettingsStatus, String> {
    let shortcut = shortcut.trim().replace(' ', "");
    if shortcut.is_empty() || shortcut.len() > 64 {
        return Err("La combinacion no es valida".into());
    }

    let (old_active, other_shortcut, old_settings) = {
        let runtime = control
            .0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        match action.as_str() {
            "interaction_mode" => (
                runtime.active_interaction_mode.clone(),
                runtime.settings.show_panel.clone(),
                runtime.settings.clone(),
            ),
            "show_panel" => (
                runtime.active_show_panel.clone(),
                runtime.settings.interaction_mode.clone(),
                runtime.settings.clone(),
            ),
            _ => return Err(format!("Accion de atajo desconocida: {action}")),
        }
    };

    if shortcut.eq_ignore_ascii_case(&other_shortcut) {
        return Err("Cada funcion necesita una combinacion distinta".into());
    }
    if old_active
        .as_ref()
        .is_some_and(|old| old.eq_ignore_ascii_case(&shortcut))
    {
        let runtime = control
            .0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        return Ok(shortcut_status(&runtime));
    }

    if let Some(old) = &old_active {
        app.global_shortcut()
            .unregister(old.as_str())
            .map_err(|error| format!("No se pudo liberar el atajo anterior: {error}"))?;
    }

    let register = if action == "interaction_mode" {
        register_interaction_shortcut(&app, &shortcut)
    } else {
        register_panel_shortcut(&app, &shortcut)
    };

    if let Err(error) = register {
        if let Some(old) = &old_active {
            let restored = if action == "interaction_mode" {
                register_interaction_shortcut(&app, old)
            } else {
                register_panel_shortcut(&app, old)
            };
            if let Err(restore_error) = restored {
                startup_log::record(format!(
                    "error: could not restore shortcut {old}: {restore_error}"
                ));
            }
        }
        return Err(format!(
            "La combinacion {shortcut} no esta disponible: {error}"
        ));
    }

    let mut new_settings = old_settings;
    if action == "interaction_mode" {
        new_settings.interaction_mode = shortcut.clone();
    } else {
        new_settings.show_panel = shortcut.clone();
    }
    if let Err(error) = save_shortcut_settings(&app, &new_settings) {
        let _ = app.global_shortcut().unregister(shortcut.as_str());
        if let Some(old) = &old_active {
            let _ = if action == "interaction_mode" {
                register_interaction_shortcut(&app, old)
            } else {
                register_panel_shortcut(&app, old)
            };
        }
        return Err(error);
    }

    let mut runtime = control
        .0
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    runtime.settings = new_settings;
    if action == "interaction_mode" {
        runtime.active_interaction_mode = Some(shortcut.clone());
        runtime.interaction_mode_error = None;
    } else {
        runtime.active_show_panel = Some(shortcut.clone());
        runtime.show_panel_error = None;
    }
    startup_log::record(format!("shortcut {action} changed to {shortcut}"));
    Ok(shortcut_status(&runtime))
}

pub fn run() {
    let log_path = startup_log::initialize();
    startup_log::record(format!("startup log path={}", log_path.display()));
    startup_log::record("building Tauri application");

    tauri::Builder::default()
        .manage(OverlayControl {
            click_through: AtomicBool::new(DEFAULT_CLICK_THROUGH),
            auto_hidden: AtomicBool::new(true),
            desired_visible: Mutex::new(HashSet::new()),
        })
        .manage(ShortcutControl(Mutex::new(ShortcutRuntime::default())))
        .invoke_handler(tauri::generate_handler![
            set_overlay_visible,
            get_overlay_states,
            get_overlay_displays,
            get_composite_layout_seed,
            get_default_overlay_placement,
            get_interaction_mode,
            toggle_interaction_mode_command,
            get_telemetry_logging,
            set_telemetry_logging,
            record_frontend_performance,
            get_track_map_geometry,
            migrate_legacy_track_map_learning,
            get_browser_source_status,
            set_browser_source_enabled,
            set_browser_source_preferences,
            set_overlay_view_settings,
            get_lmu_dependency_status,
            get_shortcut_settings,
            set_shortcut,
            export_overlay_configuration,
            import_overlay_configuration
        ])
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_dialog::init())
        .plugin(
            tauri_plugin_window_state::Builder::default()
                .with_state_flags(StateFlags::POSITION | StateFlags::SIZE)
                .with_filter(|label| !label.starts_with(OVERLAY_HOST_PREFIX))
                .build(),
        )
        .setup(|app| {
            startup_log::record("Tauri setup started");
            if let Some(panel) = app.get_webview_window("control") {
                startup_log::record("control window available");
                if panel
                    .inner_size()
                    .map(|size| size.height < 880)
                    .unwrap_or(false)
                {
                    let _ = panel.set_size(tauri::LogicalSize::new(590.0, 910.0));
                    startup_log::record("control window size restored to 590x910");
                }
            } else {
                startup_log::record("warning: control window not found during setup");
            }

            create_overlay_hosts(app.handle())?;
            startup_log::record("per-monitor overlay hosts created");
            if let Some(panel) = app.get_webview_window("control") {
                let _ = panel.set_always_on_top(!DEFAULT_CLICK_THROUGH);
            }

            let mut settings = load_shortcut_settings(app.handle());
            let interaction_result =
                match register_interaction_shortcut(app.handle(), &settings.interaction_mode) {
                    Ok(()) => Ok(()),
                    Err(original_error) => ["Ctrl+Alt+O", "Ctrl+Shift+F12"]
                        .iter()
                        .find_map(|fallback| {
                            register_interaction_shortcut(app.handle(), fallback)
                                .ok()
                                .map(|_| {
                                    settings.interaction_mode = (*fallback).into();
                                })
                        })
                        .map_or(Err(original_error), |_| Ok(())),
                };
            let panel_result = match register_panel_shortcut(app.handle(), &settings.show_panel) {
                Ok(()) => Ok(()),
                Err(original_error) => ["Ctrl+Alt+M", "Ctrl+Shift+F11"]
                    .iter()
                    .find_map(|fallback| {
                        register_panel_shortcut(app.handle(), fallback)
                            .ok()
                            .map(|_| {
                                settings.show_panel = (*fallback).into();
                            })
                    })
                    .map_or(Err(original_error), |_| Ok(())),
            };
            let shortcut_control = app.state::<ShortcutControl>();
            let mut runtime = shortcut_control
                .0
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            runtime.settings = settings.clone();
            match interaction_result {
                Ok(()) => {
                    runtime.active_interaction_mode = Some(settings.interaction_mode.clone());
                    startup_log::record(format!(
                        "shortcut {} registered for interaction mode",
                        settings.interaction_mode
                    ));
                }
                Err(error) => {
                    runtime.interaction_mode_error = Some(error.clone());
                    startup_log::record(format!(
                        "warning: {} unavailable; continuing without it: {error}",
                        settings.interaction_mode
                    ));
                }
            }
            match panel_result {
                Ok(()) => {
                    runtime.active_show_panel = Some(settings.show_panel.clone());
                    startup_log::record(format!(
                        "shortcut {} registered for showing panel",
                        settings.show_panel
                    ));
                }
                Err(error) => {
                    runtime.show_panel_error = Some(error.clone());
                    startup_log::record(format!(
                        "warning: {} unavailable; continuing without it: {error}",
                        settings.show_panel
                    ));
                }
            }
            drop(runtime);

            browser_source::configure(app.handle());

            startup_log::record("starting telemetry thread");
            telemetry::spawn_source(app.handle().clone());
            startup_log::record("Tauri setup completed");
            Ok(())
        })
        .on_window_event(|window, event| {
            if window.label() == "control"
                && matches!(event, tauri::WindowEvent::CloseRequested { .. })
            {
                startup_log::record("control window close requested; exiting normally");
                window.app_handle().exit(0);
                return;
            }
        })
        .run(tauri::generate_context!())
        .unwrap_or_else(|error| {
            startup_log::record(format!("fatal Tauri error: {error}"));
            panic!("error al ejecutar LMU Overlay: {error}");
        });

    startup_log::record("Tauri event loop finished");
}
