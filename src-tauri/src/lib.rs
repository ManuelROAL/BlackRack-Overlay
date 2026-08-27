mod app_paths;
mod browser_source;
mod lmu_install;
mod startup_log;
mod telemetry;

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
#[cfg(windows)]
use std::sync::OnceLock;
#[cfg(windows)]
use std::time::Duration;
use tauri::{
    window::Color, AppHandle, Emitter, EventTarget, Manager, PhysicalPosition, PhysicalSize,
    Position, Size, State, WebviewUrl, WebviewWindow, WebviewWindowBuilder,
};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

const OVERLAY_LABELS: [&str; 14] = [
    "delta",
    "timing",
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
    "forecast",
    "conditions",
];

const TRANSPARENT_BACKGROUND: Color = Color(0, 0, 0, 0);
const OVERLAY_HOST_PREFIX: &str = "overlay-monitor-";
const DEFAULT_CLICK_THROUGH: bool = true;
const KOFI_SUPPORT_URL: &str = "https://ko-fi.com/blackrack";

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

#[derive(Clone, Deserialize, Serialize)]
struct ControlWindowPosition {
    x: i32,
    y: i32,
}

fn control_window_position_path() -> PathBuf {
    app_paths::data_directory().join("control-window.json")
}

fn load_control_window_position() -> Option<ControlWindowPosition> {
    fs::read_to_string(control_window_position_path())
        .ok()
        .and_then(|contents| serde_json::from_str(&contents).ok())
        .filter(|position: &ControlWindowPosition| !is_windows_minimized_position(position))
}

fn is_windows_minimized_position(position: &ControlWindowPosition) -> bool {
    // Windows reports an iconic/minimized window around (-32000, -32000). It is
    // not a real desktop position and restoring it makes the panel unreachable.
    position.x <= -30_000 || position.y <= -30_000
}

#[cfg(test)]
mod control_window_position_tests {
    use super::{is_windows_minimized_position, ControlWindowPosition};

    #[test]
    fn rejects_windows_minimized_sentinel() {
        assert!(is_windows_minimized_position(&ControlWindowPosition {
            x: -32_000,
            y: -32_000,
        }));
    }

    #[test]
    fn keeps_valid_negative_monitor_coordinates() {
        assert!(!is_windows_minimized_position(&ControlWindowPosition {
            x: -2_560,
            y: 120,
        }));
    }
}

fn save_control_window_position(window: &tauri::Window) {
    if window.is_minimized().unwrap_or(false) {
        return;
    }
    let Ok(position) = window.outer_position() else {
        return;
    };
    let state = ControlWindowPosition {
        x: position.x,
        y: position.y,
    };
    if is_windows_minimized_position(&state) {
        return;
    }
    let path = control_window_position_path();
    let Some(directory) = path.parent() else {
        return;
    };
    if fs::create_dir_all(directory).is_err() {
        return;
    }
    if let Ok(contents) = serde_json::to_vec_pretty(&state) {
        let _ = fs::write(path, contents);
    }
}

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

#[derive(Clone, Deserialize)]
struct OverlayInteractionRegion {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

#[cfg(windows)]
#[derive(Default)]
struct NativeOverlayInputState {
    click_through: bool,
    regions_by_window: HashMap<isize, Vec<(i32, i32, i32, i32)>>,
}

#[cfg(windows)]
static NATIVE_OVERLAY_INPUT: OnceLock<Mutex<NativeOverlayInputState>> = OnceLock::new();
#[cfg(windows)]
static NATIVE_OVERLAY_INPUT_THREAD: OnceLock<std::thread::Thread> = OnceLock::new();

#[cfg(windows)]
fn native_overlay_input() -> &'static Mutex<NativeOverlayInputState> {
    NATIVE_OVERLAY_INPUT.get_or_init(|| {
        Mutex::new(NativeOverlayInputState {
            click_through: DEFAULT_CLICK_THROUGH,
            regions_by_window: HashMap::new(),
        })
    })
}

#[cfg(windows)]
fn refresh_native_overlay_input() {
    use windows_sys::Win32::Foundation::{HWND, POINT};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetCursorPos, GetWindowLongPtrW, SetWindowLongPtrW, SetWindowPos, GWL_EXSTYLE,
        SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, WS_EX_LAYERED,
        WS_EX_TRANSPARENT,
    };

    let mut cursor = POINT { x: 0, y: 0 };
    if unsafe { GetCursorPos(&mut cursor) } == 0 {
        return;
    }
    // SetWindowPos can synchronously wait for the window's UI thread. Copy the
    // small hit-test snapshot first so that thread never waits while this mutex
    // is held; the UI thread also updates the regions through a Tauri command.
    let (click_through, regions_by_window) = {
        let input = native_overlay_input()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        (input.click_through, input.regions_by_window.clone())
    };
    for (raw_hwnd, regions) in regions_by_window {
        let hwnd = raw_hwnd as HWND;
        let should_ignore = click_through
            || !regions.iter().any(|&(left, top, right, bottom)| {
                cursor.x >= left && cursor.x < right && cursor.y >= top && cursor.y < bottom
            });
        let current_style = unsafe { GetWindowLongPtrW(hwnd, GWL_EXSTYLE) };
        let transparent_style = WS_EX_TRANSPARENT as isize;
        let layered_style = WS_EX_LAYERED as isize;
        let next_style = if should_ignore {
            current_style | transparent_style | layered_style
        } else {
            (current_style & !transparent_style) | layered_style
        };
        if next_style != current_style {
            unsafe {
                SetWindowLongPtrW(hwnd, GWL_EXSTYLE, next_style);
                let _ = SetWindowPos(
                    hwnd,
                    std::ptr::null_mut(),
                    0,
                    0,
                    0,
                    0,
                    SWP_NOZORDER | SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_FRAMECHANGED,
                );
            }
        }
    }
}

#[cfg(windows)]
fn register_native_overlay_host(window: &WebviewWindow) -> Result<(), String> {
    let hwnd = window.hwnd().map_err(|error| error.to_string())?;
    native_overlay_input()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .regions_by_window
        .entry(hwnd.0 as isize)
        .or_default();
    Ok(())
}

#[cfg(windows)]
fn start_native_overlay_input_tracker() -> Result<(), String> {
    let tracker = std::thread::Builder::new()
        .name("overlay-input-hit-test".into())
        .spawn(|| loop {
            refresh_native_overlay_input();
            let click_through = native_overlay_input()
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .click_through;
            if click_through {
                std::thread::park();
            } else {
                std::thread::park_timeout(Duration::from_millis(4));
            }
        })
        .map_err(|error| format!("No se pudo iniciar el hit-test de overlays: {error}"))?;
    NATIVE_OVERLAY_INPUT_THREAD
        .set(tracker.thread().clone())
        .map_err(|_| "El hit-test nativo ya estaba iniciado".to_string())?;
    Ok(())
}

#[cfg(windows)]
fn set_native_overlay_click_through(click_through: bool) {
    native_overlay_input()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .click_through = click_through;
    refresh_native_overlay_input();
    NATIVE_OVERLAY_INPUT_THREAD
        .get()
        .map(std::thread::Thread::unpark);
}

fn physical_interaction_region(
    region: &OverlayInteractionRegion,
    window_width: i32,
    window_height: i32,
) -> Option<(i32, i32, i32, i32)> {
    if !region.x.is_finite()
        || !region.y.is_finite()
        || !region.width.is_finite()
        || !region.height.is_finite()
        || region.width <= 0.0
        || region.height <= 0.0
    {
        return None;
    }

    // The frontend already converts CSS pixels to physical pixels with the
    // WebView's own `devicePixelRatio`, so no extra scaling is applied here.
    // Rounding outward keeps the region covering fractional panel edges.
    let left = region.x.floor() as i32;
    let top = region.y.floor() as i32;
    let right = (region.x + region.width).ceil() as i32;
    let bottom = (region.y + region.height).ceil() as i32;
    let left = left.clamp(0, window_width);
    let top = top.clamp(0, window_height);
    let right = right.clamp(0, window_width);
    let bottom = bottom.clamp(0, window_height);
    (right > left && bottom > top).then_some((left, top, right, bottom))
}

#[cfg(test)]
mod interaction_region_tests {
    use super::{physical_interaction_region, OverlayInteractionRegion};

    #[test]
    fn rounds_outward_to_cover_fractional_panel_edges() {
        // Regions are already in physical pixels: CSS x 10.25 * 1.5 = 15.375.
        let region = OverlayInteractionRegion {
            x: 15.375,
            y: 30.75,
            width: 150.375,
            height: 75.375,
        };

        assert_eq!(
            physical_interaction_region(&region, 1920, 1080),
            Some((15, 30, 166, 107))
        );
    }

    #[test]
    fn clips_partially_offscreen_panels_to_the_host() {
        let region = OverlayInteractionRegion {
            x: -80.0,
            y: 40.0,
            width: 120.0,
            height: 90.0,
        };

        assert_eq!(
            physical_interaction_region(&region, 800, 600),
            Some((0, 40, 40, 130))
        );
    }

    #[test]
    fn rejects_empty_invalid_and_fully_offscreen_regions() {
        let empty = OverlayInteractionRegion {
            x: 10.0,
            y: 10.0,
            width: 0.0,
            height: 50.0,
        };
        let invalid = OverlayInteractionRegion {
            x: f64::NAN,
            y: 10.0,
            width: 50.0,
            height: 50.0,
        };
        let outside = OverlayInteractionRegion {
            x: 900.0,
            y: 10.0,
            width: 50.0,
            height: 50.0,
        };

        assert_eq!(physical_interaction_region(&empty, 800, 600), None);
        assert_eq!(physical_interaction_region(&invalid, 800, 600), None);
        assert_eq!(physical_interaction_region(&outside, 800, 600), None);
    }
}

#[tauri::command]
fn set_overlay_interaction_regions(
    window: WebviewWindow,
    regions: Vec<OverlayInteractionRegion>,
) -> Result<(), String> {
    if !window.label().starts_with(OVERLAY_HOST_PREFIX) {
        return Err("La región de interacción solo puede aplicarse a un host de overlays".into());
    }

    #[cfg(windows)]
    {
        let size = window.inner_size().map_err(|error| error.to_string())?;
        let hwnd = window.hwnd().map_err(|error| error.to_string())?;
        let client_origin = window.inner_position().map_err(|error| error.to_string())?;
        let physical_regions = regions
            .iter()
            .filter_map(|region| {
                physical_interaction_region(
                    region,
                    size.width.min(i32::MAX as u32) as i32,
                    size.height.min(i32::MAX as u32) as i32,
                )
            })
            .map(|(left, top, right, bottom)| {
                (
                    client_origin.x.saturating_add(left),
                    client_origin.y.saturating_add(top),
                    client_origin.x.saturating_add(right),
                    client_origin.y.saturating_add(bottom),
                )
            })
            .collect();
        native_overlay_input()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .regions_by_window
            .insert(hwnd.0 as isize, physical_regions);
        refresh_native_overlay_input();
        NATIVE_OVERLAY_INPUT_THREAD
            .get()
            .map(std::thread::Thread::unpark);
    }

    #[cfg(not(windows))]
    let _ = regions;

    Ok(())
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

#[cfg(windows)]
fn utf16_display_name(value: &[u16]) -> String {
    let length = value
        .iter()
        .position(|character| *character == 0)
        .unwrap_or(value.len());
    String::from_utf16_lossy(&value[..length])
}

#[cfg(windows)]
fn windows_monitor_friendly_names() -> HashMap<String, String> {
    use std::mem::size_of;
    use windows_sys::Win32::Devices::Display::{
        DisplayConfigGetDeviceInfo, GetDisplayConfigBufferSizes, QueryDisplayConfig,
        DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME, DISPLAYCONFIG_DEVICE_INFO_GET_TARGET_NAME,
        DISPLAYCONFIG_DEVICE_INFO_HEADER, DISPLAYCONFIG_MODE_INFO, DISPLAYCONFIG_PATH_INFO,
        DISPLAYCONFIG_SOURCE_DEVICE_NAME, DISPLAYCONFIG_TARGET_DEVICE_NAME, QDC_ONLY_ACTIVE_PATHS,
    };
    use windows_sys::Win32::Foundation::{ERROR_INSUFFICIENT_BUFFER, ERROR_SUCCESS};

    for _ in 0..3 {
        let mut path_count = 0;
        let mut mode_count = 0;
        if unsafe {
            GetDisplayConfigBufferSizes(QDC_ONLY_ACTIVE_PATHS, &mut path_count, &mut mode_count)
        } != ERROR_SUCCESS
        {
            return HashMap::new();
        }

        let mut paths = vec![DISPLAYCONFIG_PATH_INFO::default(); path_count as usize];
        let mut modes = vec![DISPLAYCONFIG_MODE_INFO::default(); mode_count as usize];
        let result = unsafe {
            QueryDisplayConfig(
                QDC_ONLY_ACTIVE_PATHS,
                &mut path_count,
                paths.as_mut_ptr(),
                &mut mode_count,
                modes.as_mut_ptr(),
                std::ptr::null_mut(),
            )
        };
        if result == ERROR_INSUFFICIENT_BUFFER {
            continue;
        }
        if result != ERROR_SUCCESS {
            return HashMap::new();
        }

        paths.truncate(path_count as usize);
        let mut names = HashMap::new();
        for path in paths {
            let mut source = DISPLAYCONFIG_SOURCE_DEVICE_NAME {
                header: DISPLAYCONFIG_DEVICE_INFO_HEADER {
                    r#type: DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME,
                    size: size_of::<DISPLAYCONFIG_SOURCE_DEVICE_NAME>() as u32,
                    adapterId: path.sourceInfo.adapterId,
                    id: path.sourceInfo.id,
                },
                ..Default::default()
            };
            let mut target = DISPLAYCONFIG_TARGET_DEVICE_NAME {
                header: DISPLAYCONFIG_DEVICE_INFO_HEADER {
                    r#type: DISPLAYCONFIG_DEVICE_INFO_GET_TARGET_NAME,
                    size: size_of::<DISPLAYCONFIG_TARGET_DEVICE_NAME>() as u32,
                    adapterId: path.targetInfo.adapterId,
                    id: path.targetInfo.id,
                },
                ..Default::default()
            };
            if unsafe { DisplayConfigGetDeviceInfo(&mut source.header) } != 0
                || unsafe { DisplayConfigGetDeviceInfo(&mut target.header) } != 0
            {
                continue;
            }

            let gdi_name = utf16_display_name(&source.viewGdiDeviceName);
            let friendly_name = utf16_display_name(&target.monitorFriendlyDeviceName);
            if !gdi_name.is_empty() && !friendly_name.is_empty() {
                names.insert(gdi_name, friendly_name);
            }
        }
        return names;
    }

    HashMap::new()
}

#[cfg(not(windows))]
fn windows_monitor_friendly_names() -> HashMap<String, String> {
    HashMap::new()
}

fn overlay_displays(app: &AppHandle) -> Result<Vec<OverlayDisplay>, String> {
    sorted_monitors(app).map(|monitors| {
        let friendly_names = windows_monitor_friendly_names();
        monitors
            .into_iter()
            .enumerate()
            .map(|(index, monitor)| {
                let native_name = monitor
                    .name()
                    .cloned()
                    .unwrap_or_else(|| format!("Monitor {}", index + 1));
                OverlayDisplay {
                    index,
                    label: format!("{OVERLAY_HOST_PREFIX}{index}"),
                    name: friendly_names
                        .get(&native_name)
                        .cloned()
                        .unwrap_or(native_name),
                    x: monitor.position().x,
                    y: monitor.position().y,
                    width: monitor.size().width,
                    height: monitor.size().height,
                    scale_factor: monitor.scale_factor(),
                }
            })
            .collect()
    })
}

#[cfg(all(test, windows))]
mod display_name_tests {
    use super::utf16_display_name;

    #[test]
    fn reads_a_null_terminated_display_name() {
        assert_eq!(utf16_display_name(&[65, 79, 67, 0, 88]), "AOC");
    }
}

#[derive(Clone, Deserialize, Serialize)]
struct OverlayMonitorSettings {
    index: usize,
}

fn overlay_monitor_settings_path(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    let _ = app;
    Ok(app_paths::data_directory().join("overlay-monitor.json"))
}

fn load_overlay_monitor_index(app: &AppHandle) -> usize {
    let Ok(path) = overlay_monitor_settings_path(app) else {
        return 0;
    };
    let Ok(contents) = fs::read_to_string(&path) else {
        return 0;
    };
    serde_json::from_str::<OverlayMonitorSettings>(&contents)
        .map(|settings| settings.index)
        .unwrap_or(0)
}

fn save_overlay_monitor_index(app: &AppHandle, index: usize) -> Result<(), String> {
    let path = overlay_monitor_settings_path(app)?;
    if let Some(directory) = path.parent() {
        fs::create_dir_all(directory)
            .map_err(|error| format!("No se pudo crear la carpeta de configuracion: {error}"))?;
    }
    let contents = serde_json::to_string_pretty(&OverlayMonitorSettings { index })
        .map_err(|error| error.to_string())?;
    fs::write(&path, contents)
        .map_err(|error| format!("No se pudo guardar la configuracion: {error}"))
}

fn create_control_window(app: &AppHandle) -> Result<(), String> {
    WebviewWindowBuilder::new(app, "control", WebviewUrl::App("index.html".into()))
        .data_directory(app_paths::webview_data_directory())
        .title("BlackRack Overlay · Panel de control")
        .inner_size(590.0, 910.0)
        .min_inner_size(560.0, 880.0)
        .transparent(false)
        .decorations(true)
        .shadow(true)
        .always_on_top(false)
        .skip_taskbar(false)
        .resizable(false)
        .devtools(false)
        .center()
        .build()
        .map_err(|error| format!("No se pudo crear el panel de control: {error}"))?;
    Ok(())
}

fn create_overlay_host(app: &AppHandle) -> Result<(), String> {
    let monitors = sorted_monitors(app)?;
    if monitors.is_empty() {
        return Err("No se detectaron monitores para alojar los overlays".into());
    }
    let saved_index = load_overlay_monitor_index(app);
    let (host_index, monitor) = match monitors.get(saved_index) {
        Some(monitor) => (saved_index, monitor),
        None => (0, &monitors[0]),
    };
    let label = format!("{OVERLAY_HOST_PREFIX}0");
    let logical_size = monitor.size().to_logical::<f64>(monitor.scale_factor());
    let window = WebviewWindowBuilder::new(app, &label, WebviewUrl::App("composite.html".into()))
        .data_directory(app_paths::webview_data_directory())
        .title(format!("BlackRack Overlay · Monitor {}", host_index + 1))
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
        .map_err(|error| format!("No se pudo crear el host del monitor {saved_index}: {error}"))?;

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
    #[cfg(windows)]
    register_native_overlay_host(&window)?;
    #[cfg(windows)]
    start_native_overlay_input_tracker()?;
    Ok(())
}

#[tauri::command]
fn get_overlay_monitor(app: AppHandle) -> Result<usize, String> {
    let monitors = sorted_monitors(&app)?;
    if monitors.is_empty() {
        return Err("No se detectaron monitores para alojar los overlays".into());
    }
    let saved = load_overlay_monitor_index(&app);
    Ok(if saved < monitors.len() { saved } else { 0 })
}

#[tauri::command]
fn set_overlay_monitor(app: AppHandle, index: usize) -> Result<usize, String> {
    let monitors = sorted_monitors(&app)?;
    let Some(monitor) = monitors.get(index) else {
        return Err("El monitor seleccionado no está disponible".into());
    };
    save_overlay_monitor_index(&app, index)?;
    if let Some(window) = app.get_webview_window(&format!("{OVERLAY_HOST_PREFIX}0")) {
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
    }
    startup_log::record(format!("overlay monitor changed to {index}"));
    Ok(index)
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
    let mut app_has_focus = app
        .get_webview_window("control")
        .and_then(|window| window.is_focused().ok())
        .unwrap_or(false);
    for_each_overlay_host(app, |window| {
        app_has_focus |= window.is_focused().unwrap_or(false);
    });
    let should_hide = frame.should_hide_overlays(app_has_focus);
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
        "delta" => (610.0, 20.0, 420.0, 72.0),
        "timing" => (610.0, 110.0, 250.0, 292.0),
        "driving" => (20.0, 380.0, 540.0, 120.0),
        "tires" => (588.0, 380.0, 174.0, 130.0),
        "damage" => (798.0, 380.0, 94.0, 94.0),
        "standings" => (20.0, 70.0, 980.0, 500.0),
        "relative" => (20.0, 590.0, 980.0, 300.0),
        "fuel" => (1020.0, 70.0, 292.0, 198.0),
        "pitstop" => (1020.0, 320.0, 210.0, 150.0),
        "flags" => (1240.0, 320.0, 360.0, 120.0),
        "rejoin" => (1240.0, 460.0, 360.0, 140.0),
        "trackmap" => (1460.0, 70.0, 436.0, 436.0),
        "forecast" => (1020.0, 530.0, 352.0, 118.0),
        "conditions" => (1020.0, 410.0, 480.0, 96.0),
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
        x,
        y,
        width,
        height,
    })
}

#[tauri::command]
fn get_composite_layout_seed(
    app: AppHandle,
    _monitor: usize,
) -> Result<Vec<OverlayPlacementSeed>, String> {
    let displays = overlay_displays(&app)?;
    if displays.is_empty() {
        return Err("No se detectaron monitores para alojar los overlays".into());
    }

    Ok(OVERLAY_LABELS
        .iter()
        .map(|label| {
            let (x, y, width, height) = default_overlay_geometry(label);
            OverlayPlacementSeed {
                overlay: label,
                x,
                y,
                width,
                height,
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
fn set_performance_profile(profile: String) -> Result<(), String> {
    telemetry::set_performance_profile(&profile)
}

#[tauri::command]
fn get_driver_rank_estimate_logging() -> telemetry::DriverRankEstimateLoggingStatus {
    telemetry::driver_rank_estimate_logging_status()
}

#[tauri::command]
fn set_driver_rank_estimate_logging(
    enabled: bool,
) -> Result<telemetry::DriverRankEstimateLoggingStatus, String> {
    telemetry::set_driver_rank_estimate_logging(enabled)
}

#[tauri::command]
fn get_strategy_logging() -> telemetry::StrategyLoggingStatus {
    telemetry::strategy_logging_status()
}

#[tauri::command]
fn set_strategy_logging(enabled: bool) -> Result<telemetry::StrategyLoggingStatus, String> {
    telemetry::set_strategy_logging(enabled)
}

#[tauri::command]
fn record_frontend_error(source: String, kind: String, message: String, stack: Option<String>) {
    startup_log::record_frontend_error(&source, &kind, &message, stack.as_deref());
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
fn set_delta_settings(settings: telemetry::DeltaSettings) {
    telemetry::set_delta_settings(settings);
}

#[tauri::command]
fn set_timing_settings(settings: telemetry::TimingSettings) {
    telemetry::set_timing_settings(settings);
}

#[tauri::command]
fn get_lmu_dependency_status() -> LmuDependencyStatus {
    let plugin = lmu_install::telemetry_plugin();
    LmuDependencyStatus {
        telemetry_plugin_available: plugin.is_some(),
        telemetry_plugin_path: plugin.map(|path| path.display().to_string()),
    }
}

#[tauri::command]
fn open_support_page() -> Result<(), String> {
    #[cfg(windows)]
    {
        use std::iter::once;
        use windows_sys::Win32::UI::Shell::ShellExecuteW;
        use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

        let url = KOFI_SUPPORT_URL
            .encode_utf16()
            .chain(once(0))
            .collect::<Vec<_>>();
        let result = unsafe {
            ShellExecuteW(
                std::ptr::null_mut(),
                std::ptr::null(),
                url.as_ptr(),
                std::ptr::null(),
                std::ptr::null(),
                SW_SHOWNORMAL,
            )
        };
        if result as isize <= 32 {
            return Err("No se pudo abrir Ko-fi en el navegador predeterminado".into());
        }
        return Ok(());
    }

    #[cfg(not(windows))]
    {
        let launcher = if cfg!(target_os = "macos") {
            "open"
        } else {
            "xdg-open"
        };
        std::process::Command::new(launcher)
            .arg(KOFI_SUPPORT_URL)
            .spawn()
            .map(|_| ())
            .map_err(|error| format!("No se pudo abrir Ko-fi: {error}"))
    }
}

fn shortcut_settings_path(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    let _ = app;
    Ok(app_paths::data_directory().join("shortcuts.json"))
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
    let schema_version = parsed
        .get("schemaVersion")
        .and_then(serde_json::Value::as_u64);
    let format = parsed.get("format").and_then(serde_json::Value::as_str);
    if !matches!(
        format,
        Some("blackrack-overlay-configuration" | "lmu-overlay-configuration")
    ) || !matches!(schema_version, Some(1..))
    {
        return Err("Formato de configuración no reconocido".into());
    }
    Ok(())
}

#[cfg(test)]
mod configuration_tests {
    use super::validate_overlay_configuration;

    #[test]
    fn accepts_current_and_future_schema_versions_without_backend_lockstep() {
        for version in [1, 9, 10] {
            let contents = format!(
                r#"{{"format":"blackrack-overlay-configuration","schemaVersion":{version}}}"#
            );
            assert!(validate_overlay_configuration(&contents).is_ok());
        }
    }

    #[test]
    fn accepts_legacy_product_format() {
        let contents = r#"{"format":"lmu-overlay-configuration","schemaVersion":7}"#;
        assert!(validate_overlay_configuration(contents).is_ok());
    }

    #[test]
    fn rejects_unknown_or_unversioned_documents() {
        for contents in [
            r#"{"format":"other","schemaVersion":9}"#,
            r#"{"format":"blackrack-overlay-configuration","schemaVersion":0}"#,
            r#"{"format":"blackrack-overlay-configuration"}"#,
        ] {
            assert!(validate_overlay_configuration(contents).is_err());
        }
    }
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

    #[cfg(windows)]
    set_native_overlay_click_through(next);

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
        return Err("invalid".into());
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
            _ => return Err("unknown_action".into()),
        }
    };

    if shortcut.eq_ignore_ascii_case(&other_shortcut) {
        return Err("duplicate".into());
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
        if let Err(error) = app.global_shortcut().unregister(old.as_str()) {
            startup_log::record(format!("could not unregister shortcut {old}: {error}"));
            return Err("unregister_failed".into());
        }
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
        startup_log::record(format!("shortcut {shortcut} is unavailable: {error}"));
        return Err("unavailable".into());
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
        startup_log::record(format!("could not save shortcut settings: {error}"));
        return Err("persistence_failed".into());
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
            get_overlay_monitor,
            set_overlay_monitor,
            get_composite_layout_seed,
            get_default_overlay_placement,
            get_interaction_mode,
            set_overlay_interaction_regions,
            toggle_interaction_mode_command,
            get_telemetry_logging,
            set_telemetry_logging,
            set_performance_profile,
            get_driver_rank_estimate_logging,
            set_driver_rank_estimate_logging,
            get_strategy_logging,
            set_strategy_logging,
            record_frontend_error,
            record_frontend_performance,
            get_track_map_geometry,
            migrate_legacy_track_map_learning,
            get_browser_source_status,
            set_browser_source_enabled,
            set_browser_source_preferences,
            set_overlay_view_settings,
            set_delta_settings,
            set_timing_settings,
            get_lmu_dependency_status,
            open_support_page,
            get_shortcut_settings,
            set_shortcut,
            export_overlay_configuration,
            import_overlay_configuration
        ])
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            startup_log::record("Tauri setup started");
            create_control_window(app.handle())?;
            if let Some(panel) = app.get_webview_window("control") {
                startup_log::record("control window available");
                if let Some(position) = load_control_window_position() {
                    let _ = panel.set_position(PhysicalPosition::new(position.x, position.y));
                }
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

            create_overlay_host(app.handle())?;
            startup_log::record("overlay host created");
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
                save_control_window_position(window);
                startup_log::record("control window close requested; exiting normally");
                window.app_handle().exit(0);
                return;
            }
        })
        .run(tauri::generate_context!())
        .unwrap_or_else(|error| {
            startup_log::record(format!("fatal Tauri error: {error}"));
            panic!("error al ejecutar BlackRack Overlay: {error}");
        });

    startup_log::record("Tauri event loop finished");
}
