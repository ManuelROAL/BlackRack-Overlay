mod app_paths;
mod browser_source;
mod startup_log;
mod telemetry;
mod updater;
#[cfg(windows)]
mod wheel_input;

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(windows)]
use std::sync::OnceLock;
use std::sync::{mpsc, Mutex};
#[cfg(windows)]
use std::time::Duration;
use tauri::{
    window::Color, AppHandle, Emitter, EventTarget, Manager, PhysicalPosition, PhysicalSize,
    Position, Size, State, WebviewUrl, WebviewWindow, WebviewWindowBuilder, WindowEvent,
};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

const OVERLAY_LABELS: [&str; 18] = [
    "delta",
    "timing",
    "stinthistory",
    "driving",
    "liftcoast",
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
    "dashboard",
    "sessioninfo",
];

const TRANSPARENT_BACKGROUND: Color = Color(0, 0, 0, 0);
const OVERLAY_HOST_PREFIX: &str = "overlay-monitor-";
const CONTROL_WINDOW_LABEL: &str = "control";
const DEFAULT_CLICK_THROUGH: bool = true;
const KOFI_SUPPORT_URL: &str = "https://ko-fi.com/blackrack";
const PAYPAL_SUPPORT_URL: &str = "https://www.paypal.com/paypalme/BlackRack";

/// Chromium arguments for every webview of the application. WebView2 keeps one
/// browser process per user data directory, so the environment created first
/// decides the arguments for the whole application; both windows share
/// `app_paths::webview_data_directory` and therefore must pass this same string.
///
/// The leading `--disable-features` entries restore wry's own defaults, which
/// this method replaces: without them the mini menu and SmartScreen come back.
/// The rest removes browser subsystems the overlay never uses and that only
/// cost resident memory.
///
/// `--js-flags=--max-old-space-size` is deliberately absent. Bounding the V8
/// heap to 192 MB did hold private memory to about 605 MB instead of growing
/// past 1.2 GB, but it left the host renderer living against the limit: a 300 s
/// capture measured bursts of 42-48% CPU across every logical processor, about
/// 30 s long and roughly every 80 s, which the uncapped build never showed.
/// Collection storms on the surface drawn over the game cost far more than the
/// memory they save.
///
/// `--renderer-process-limit=1` is deliberately absent. It would save one
/// renderer process, but it also puts the control panel on the same renderer
/// main thread as the overlay host, so opening the panel mid-session could
/// stall the overlays. Latency on the surface drawn over the game outweighs
/// those megabytes.
///
/// Native window occlusion stays enabled on purpose. The control panel then
/// stops rendering while the game covers it, and the overlay host is always on
/// top, so it is never reported as occluded.
const WEBVIEW_BROWSER_ARGUMENTS: &str = concat!(
    "--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection,",
    "Translate,MediaRouter,OptimizationHints,AutofillServerCommunication,",
    "BackForwardCache,InterestFeedContentSuggestions",
    " --disable-background-networking",
    " --disable-component-update",
    " --disable-sync",
);

/// Guard for privileged commands that must only ever run on behalf of the
/// control panel. Overlay webviews render live session data (driver names,
/// race control) and are also served over the local network by the browser
/// source, so they must not be able to reach filesystem, shell or configuration
/// commands even if their content were ever compromised.
pub(crate) fn require_control_window(window: &WebviewWindow) -> Result<(), String> {
    if window.label() == CONTROL_WINDOW_LABEL {
        return Ok(());
    }
    startup_log::record(format!(
        "denied privileged command from window '{}'",
        window.label()
    ));
    Err("control_window_required".into())
}

struct OverlayControl {
    click_through: AtomicBool,
    auto_hidden: AtomicBool,
    shortcut_hidden: AtomicBool,
    shutdown: AtomicBool,
    focused_windows: Mutex<HashSet<String>>,
    desired_visible: Mutex<HashSet<&'static str>>,
    #[cfg(windows)]
    edit_previous_foreground_window: Mutex<Option<isize>>,
}

#[derive(Clone, Deserialize, Serialize)]
struct ShortcutSettings {
    interaction_mode: String,
    show_panel: String,
    #[serde(default = "default_toggle_overlays_shortcut")]
    toggle_overlays: String,
    #[serde(default = "default_overlay_shortcuts")]
    hide_overlays: HashMap<String, String>,
}

fn default_toggle_overlays_shortcut() -> String {
    "Ctrl+Shift+H".into()
}

fn default_overlay_shortcuts() -> HashMap<String, String> {
    OVERLAY_LABELS
        .iter()
        .map(|label| ((*label).into(), String::new()))
        .collect()
}

fn overlay_label_for_shortcut_action(action: &str) -> Option<&'static str> {
    action
        .strip_prefix("hide_")
        .and_then(|label| OVERLAY_LABELS.iter().copied().find(|known| *known == label))
}

impl Default for ShortcutSettings {
    fn default() -> Self {
        Self {
            interaction_mode: "Ctrl+Shift+O".into(),
            show_panel: "Ctrl+Shift+M".into(),
            toggle_overlays: default_toggle_overlays_shortcut(),
            hide_overlays: default_overlay_shortcuts(),
        }
    }
}

#[derive(Default)]
struct ShortcutRuntime {
    settings: ShortcutSettings,
    active_interaction_mode: Option<String>,
    active_show_panel: Option<String>,
    active_toggle_overlays: Option<String>,
    interaction_mode_error: Option<String>,
    show_panel_error: Option<String>,
    toggle_overlays_error: Option<String>,
    active_hide_overlays: HashMap<String, String>,
    hide_overlays_error: HashMap<String, String>,
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
    toggle_overlays: ShortcutBindingStatus,
    hide_overlays: HashMap<String, ShortcutBindingStatus>,
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

/// Rectangle the overlay host must cover, in physical pixels relative to the
/// monitor's top-left corner.
#[derive(Clone, Deserialize)]
struct OverlayHostBounds {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

/// The host monitor measured in CSS pixels. The composite needs it because once
/// the host stops covering the whole monitor its own viewport no longer
/// describes the surface the panels are laid out on.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct OverlayHostViewport {
    width: f64,
    height: f64,
    scale_factor: f64,
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

    // The tracker repeats this hit test every 4 ms while the user interacts, so
    // the regions are evaluated in place and only one decision per window is
    // copied into a buffer that this thread reuses.
    thread_local! {
        static DECISIONS: std::cell::RefCell<Vec<(isize, bool)>> =
            const { std::cell::RefCell::new(Vec::new()) };
    }

    let mut cursor = POINT { x: 0, y: 0 };
    if unsafe { GetCursorPos(&mut cursor) } == 0 {
        return;
    }
    DECISIONS.with_borrow_mut(|decisions| {
        // SetWindowPos can synchronously wait for the window's UI thread. Resolve
        // the hit test first so that thread never waits while this mutex is held;
        // the UI thread also updates the regions through a Tauri command.
        decisions.clear();
        {
            let input = native_overlay_input()
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            decisions.extend(input.regions_by_window.iter().map(|(&raw_hwnd, regions)| {
                let should_ignore = input.click_through
                    || !regions.iter().any(|&(left, top, right, bottom)| {
                        cursor.x >= left && cursor.x < right && cursor.y >= top && cursor.y < bottom
                    });
                (raw_hwnd, should_ignore)
            }));
        }

        for &(raw_hwnd, should_ignore) in decisions.iter() {
            let hwnd = raw_hwnd as HWND;
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
    });
}

#[cfg(windows)]
fn register_native_overlay_host(window: &WebviewWindow) -> Result<(), String> {
    let hwnd = window
        .hwnd()
        .map_err(|error| startup_log::command_error("overlay_host_handle_failed", error))?;
    native_overlay_input()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .regions_by_window
        .entry(hwnd.0 as isize)
        .or_default();
    Ok(())
}

#[cfg(windows)]
fn unregister_native_overlay_host(window: &WebviewWindow) {
    let Ok(hwnd) = window.hwnd() else {
        return;
    };
    native_overlay_input()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .regions_by_window
        .remove(&(hwnd.0 as isize));
}

#[cfg(windows)]
fn start_native_overlay_input_tracker() -> Result<(), String> {
    if NATIVE_OVERLAY_INPUT_THREAD.get().is_some() {
        return Ok(());
    }
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
        .map_err(|error| startup_log::command_error("overlay_input_tracker_failed", error))?;
    NATIVE_OVERLAY_INPUT_THREAD
        .set(tracker.thread().clone())
        .map_err(|_| "overlay_input_tracker_already_started".to_string())?;
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

#[cfg(windows)]
fn current_foreground_window() -> Option<isize> {
    use windows_sys::Win32::UI::WindowsAndMessaging::GetForegroundWindow;

    let window = unsafe { GetForegroundWindow() };
    (!window.is_null()).then_some(window as isize)
}

#[cfg(windows)]
fn restore_foreground_window(window: Option<isize>) {
    use windows_sys::Win32::Foundation::HWND;
    use windows_sys::Win32::UI::WindowsAndMessaging::{IsWindow, SetForegroundWindow};

    let Some(window) = window else {
        return;
    };
    let window = window as HWND;
    unsafe {
        if IsWindow(window) != 0 {
            let _ = SetForegroundWindow(window);
        }
    }
}

/// Clamp a requested host rectangle to the monitor. The result is always at
/// least one pixel wide and tall and never reaches outside the monitor, so a
/// stale or malformed layout can never move the host off the display or
/// collapse it to nothing.
fn clamped_host_bounds(
    bounds: &OverlayHostBounds,
    monitor_width: u32,
    monitor_height: u32,
) -> Option<(i32, i32, u32, u32)> {
    if !bounds.x.is_finite()
        || !bounds.y.is_finite()
        || !bounds.width.is_finite()
        || !bounds.height.is_finite()
        || bounds.width <= 0.0
        || bounds.height <= 0.0
        || monitor_width == 0
        || monitor_height == 0
    {
        return None;
    }

    // Round outward: a host one pixel short of a panel edge would clip it.
    let monitor_width = monitor_width.min(i32::MAX as u32) as i32;
    let monitor_height = monitor_height.min(i32::MAX as u32) as i32;
    let left = (bounds.x.floor() as i64).clamp(0, monitor_width as i64 - 1) as i32;
    let top = (bounds.y.floor() as i64).clamp(0, monitor_height as i64 - 1) as i32;
    let right = ((bounds.x + bounds.width).ceil() as i64)
        .clamp(left as i64 + 1, monitor_width as i64) as i32;
    let bottom = ((bounds.y + bounds.height).ceil() as i64)
        .clamp(top as i64 + 1, monitor_height as i64) as i32;
    Some((left, top, (right - left) as u32, (bottom - top) as u32))
}

#[cfg(test)]
mod host_bounds_tests {
    use super::{clamped_host_bounds, OverlayHostBounds};

    #[test]
    fn rounds_outward_and_keeps_the_panel_edges_inside() {
        let bounds = OverlayHostBounds {
            x: 10.4,
            y: 20.6,
            width: 100.3,
            height: 50.2,
        };
        assert_eq!(
            clamped_host_bounds(&bounds, 1920, 1080),
            Some((10, 20, 101, 51))
        );
    }

    #[test]
    fn clamps_a_rectangle_that_overflows_the_monitor() {
        let bounds = OverlayHostBounds {
            x: -50.0,
            y: -50.0,
            width: 4000.0,
            height: 4000.0,
        };
        assert_eq!(
            clamped_host_bounds(&bounds, 1920, 1080),
            Some((0, 0, 1920, 1080))
        );
    }

    #[test]
    fn keeps_a_rectangle_starting_at_the_last_pixel_visible() {
        let bounds = OverlayHostBounds {
            x: 1919.0,
            y: 1079.0,
            width: 10.0,
            height: 10.0,
        };
        assert_eq!(
            clamped_host_bounds(&bounds, 1920, 1080),
            Some((1919, 1079, 1, 1))
        );
    }

    #[test]
    fn rejects_empty_and_invalid_rectangles() {
        let empty = OverlayHostBounds {
            x: 0.0,
            y: 0.0,
            width: 0.0,
            height: 100.0,
        };
        let invalid = OverlayHostBounds {
            x: f64::NAN,
            y: 0.0,
            width: 100.0,
            height: 100.0,
        };
        assert_eq!(clamped_host_bounds(&empty, 1920, 1080), None);
        assert_eq!(clamped_host_bounds(&invalid, 1920, 1080), None);
    }
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
        return Err("overlay_host_required".into());
    }

    #[cfg(windows)]
    {
        let size = window
            .inner_size()
            .map_err(|error| startup_log::command_error("overlay_host_geometry_failed", error))?;
        let hwnd = window
            .hwnd()
            .map_err(|error| startup_log::command_error("overlay_host_geometry_failed", error))?;
        let client_origin = window
            .inner_position()
            .map_err(|error| startup_log::command_error("overlay_host_geometry_failed", error))?;
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

fn overlay_host_label(monitor: usize) -> String {
    format!("{OVERLAY_HOST_PREFIX}{monitor}")
}

fn monitor_index_for_host(app: &AppHandle, window: &WebviewWindow) -> usize {
    window
        .label()
        .strip_prefix(OVERLAY_HOST_PREFIX)
        .and_then(|value| value.parse().ok())
        .unwrap_or_else(|| load_overlay_monitor_index(app))
}

/// Shrink the transparent host to the rectangle the visible panels occupy, or
/// restore the whole monitor with `None`.
///
/// A full-screen transparent always-on-top window costs the game a composited
/// surface the size of the display on every present. Bounding the host to the
/// panels bounds that cost, and the host keeps covering the monitor while the
/// user edits the layout, where panels must be draggable anywhere.
///
/// Returns the monitor in CSS pixels: once the host is smaller than the display
/// its own viewport no longer describes the surface the layout is placed on.
#[tauri::command]
fn set_overlay_host_bounds(
    app: AppHandle,
    window: WebviewWindow,
    bounds: Option<OverlayHostBounds>,
) -> Result<OverlayHostViewport, String> {
    if !window.label().starts_with(OVERLAY_HOST_PREFIX) {
        return Err("overlay_host_required".into());
    }

    let monitors = sorted_monitors(&app)?;
    let monitor_index = monitor_index_for_host(&app, &window);
    let monitor = monitors
        .get(monitor_index)
        .or_else(|| monitors.first())
        .cloned()
        .ok_or_else(|| "monitors_unavailable".to_string())?;
    let monitor_position = *monitor.position();
    let monitor_size = *monitor.size();
    let scale_factor = if monitor.scale_factor() > 0.0 {
        monitor.scale_factor()
    } else {
        1.0
    };
    let (left, top, width, height) = bounds
        .as_ref()
        .and_then(|bounds| clamped_host_bounds(bounds, monitor_size.width, monitor_size.height))
        .unwrap_or((0, 0, monitor_size.width, monitor_size.height));

    window
        .set_size(Size::Physical(PhysicalSize::new(width, height)))
        .map_err(|error| startup_log::command_error("overlay_host_geometry_failed", error))?;
    window
        .set_position(Position::Physical(PhysicalPosition::new(
            monitor_position.x.saturating_add(left),
            monitor_position.y.saturating_add(top),
        )))
        .map_err(|error| startup_log::command_error("overlay_host_geometry_failed", error))?;

    Ok(OverlayHostViewport {
        width: f64::from(monitor_size.width) / scale_factor,
        height: f64::from(monitor_size.height) / scale_factor,
        scale_factor,
    })
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

fn sorted_monitors(app: &AppHandle) -> Result<Vec<tauri::Monitor>, String> {
    let mut monitors = app
        .available_monitors()
        .map_err(|error| startup_log::command_error("monitors_unavailable", error))?;
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
            .map_err(|error| startup_log::command_error("settings_directory_failed", error))?;
    }
    let contents = serde_json::to_string_pretty(&OverlayMonitorSettings { index })
        .map_err(|error| startup_log::command_error("settings_encode_failed", error))?;
    fs::write(&path, contents)
        .map_err(|error| startup_log::command_error("settings_write_failed", error))
}

fn create_control_window(app: &AppHandle) -> Result<(), String> {
    WebviewWindowBuilder::new(app, "control", WebviewUrl::App("index.html".into()))
        .data_directory(app_paths::webview_data_directory())
        .additional_browser_args(WEBVIEW_BROWSER_ARGUMENTS)
        .title("BlackRack Overlay · Panel de control")
        .inner_size(700.0, 950.0)
        .min_inner_size(670.0, 920.0)
        .transparent(false)
        .decorations(true)
        .shadow(true)
        .always_on_top(false)
        .skip_taskbar(false)
        .resizable(false)
        .devtools(cfg!(debug_assertions))
        .center()
        .build()
        .map_err(|error| startup_log::command_error("control_window_failed", error))?;
    startup_log::record("window created label=control");
    Ok(())
}

fn create_overlay_host(app: &AppHandle, monitor_index: usize) -> Result<WebviewWindow, String> {
    let monitors = sorted_monitors(app)?;
    if monitors.is_empty() {
        return Err("monitors_unavailable".into());
    }
    let Some(monitor) = monitors.get(monitor_index) else {
        return Err("monitor_unavailable".into());
    };
    let host_index = monitor_index;
    let label = overlay_host_label(host_index);
    if let Some(window) = app.get_webview_window(&label) {
        return Ok(window);
    }
    let logical_size = monitor.size().to_logical::<f64>(monitor.scale_factor());
    let window = WebviewWindowBuilder::new(
        app,
        &label,
        WebviewUrl::App(format!("composite.html?monitor={host_index}").into()),
    )
    .data_directory(app_paths::webview_data_directory())
    .additional_browser_args(WEBVIEW_BROWSER_ARGUMENTS)
    .title(format!("BlackRack Overlay · Monitor {}", host_index + 1))
    .inner_size(logical_size.width, logical_size.height)
    .transparent(true)
    .background_color(TRANSPARENT_BACKGROUND)
    .decorations(false)
    .shadow(false)
    .always_on_top(true)
    .skip_taskbar(true)
    .resizable(false)
    .devtools(cfg!(debug_assertions))
    .visible(false)
    .build()
    .map_err(|error| {
        startup_log::command_error(
            "overlay_host_failed",
            format!("monitor {monitor_index}: {error}"),
        )
    })?;

    window
        .set_position(Position::Physical(PhysicalPosition::new(
            monitor.position().x,
            monitor.position().y,
        )))
        .map_err(|error| startup_log::command_error("overlay_host_geometry_failed", error))?;
    window
        .set_size(Size::Physical(PhysicalSize::new(
            monitor.size().width,
            monitor.size().height,
        )))
        .map_err(|error| startup_log::command_error("overlay_host_geometry_failed", error))?;
    window
        .set_ignore_cursor_events(DEFAULT_CLICK_THROUGH)
        .map_err(|error| startup_log::command_error("overlay_host_geometry_failed", error))?;
    #[cfg(windows)]
    register_native_overlay_host(&window)?;
    #[cfg(windows)]
    start_native_overlay_input_tracker()?;
    startup_log::record(format!(
        "overlay host created label={label} monitor={monitor_index}"
    ));
    Ok(window)
}

fn close_overlay_host(host: WebviewWindow) -> Option<mpsc::Receiver<()>> {
    let label = host.label().to_string();
    let (sender, receiver) = mpsc::channel();
    host.on_window_event(move |event| {
        if matches!(event, WindowEvent::Destroyed) {
            let _ = sender.send(());
        }
    });
    match host.close() {
        Ok(()) => Some(receiver),
        Err(error) => {
            startup_log::record(format!(
                "overlay host close failed label={label} error={}",
                startup_log::sanitize(&error.to_string(), 500)
            ));
            None
        }
    }
}

async fn wait_for_overlay_hosts(hosts: Vec<WebviewWindow>) {
    let waits = hosts
        .into_iter()
        .filter_map(|host| close_overlay_host(host))
        .map(|receiver| tauri::async_runtime::spawn_blocking(move || receiver.recv()))
        .collect::<Vec<_>>();
    for wait in waits {
        let _ = wait.await;
    }
}

#[tauri::command]
async fn sync_overlay_hosts(
    app: AppHandle,
    window: WebviewWindow,
    monitors: Vec<usize>,
) -> Result<Vec<usize>, String> {
    require_control_window(&window)?;

    let available = sorted_monitors(&app)?;
    let mut requested = monitors;
    requested.sort_unstable();
    requested.dedup();
    if let Some(index) = requested.iter().find(|&&index| index >= available.len()) {
        return Err(startup_log::command_error(
            "monitor_unavailable",
            format!("monitor {index}"),
        ));
    }

    for &index in &requested {
        create_overlay_host(&app, index)?;
    }

    refresh_overlay_host_visibility(&app);

    let requested_set = requested.iter().copied().collect();
    remove_overlay_hosts_except(&app, &requested_set).await;

    Ok(requested)
}

async fn remove_overlay_hosts_except(app: &AppHandle, keep: &HashSet<usize>) {
    let windows: Vec<_> = app
        .webview_windows()
        .into_iter()
        .filter_map(|(label, host)| {
            let index = label
                .strip_prefix(OVERLAY_HOST_PREFIX)
                .and_then(|value| value.parse::<usize>().ok())?;
            (!keep.contains(&index)).then_some(host)
        })
        .collect();
    for host in &windows {
        #[cfg(windows)]
        unregister_native_overlay_host(host);
    }
    wait_for_overlay_hosts(windows).await;
}

#[tauri::command]
fn get_overlay_monitor(app: AppHandle) -> Result<usize, String> {
    let monitors = sorted_monitors(&app)?;
    if monitors.is_empty() {
        return Err("monitors_unavailable".into());
    }
    let saved = load_overlay_monitor_index(&app);
    Ok(if saved < monitors.len() { saved } else { 0 })
}

#[tauri::command]
fn set_overlay_monitor(app: AppHandle, index: usize) -> Result<usize, String> {
    let monitors = sorted_monitors(&app)?;
    if monitors.get(index).is_none() {
        return Err("monitor_unavailable".into());
    }
    save_overlay_monitor_index(&app, index)?;
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
        && !control.shortcut_hidden.load(Ordering::Relaxed)
        && overlay_is_enabled(app, label)
}

fn overlays_are_temporarily_hidden(control: &OverlayControl) -> bool {
    control.auto_hidden.load(Ordering::Relaxed) || control.shortcut_hidden.load(Ordering::Relaxed)
}

fn refresh_overlay_host_visibility(app: &AppHandle) {
    let visibility_app = app.clone();
    if let Err(error) = app.run_on_main_thread(move || {
        let control = visibility_app.state::<OverlayControl>();
        apply_overlay_host_visibility(&visibility_app, &control);
    }) {
        startup_log::record(format!(
            "could not schedule overlay host visibility update: {error}"
        ));
    }
}

fn apply_overlay_host_visibility(app: &AppHandle, control: &OverlayControl) {
    let should_show = !overlays_are_temporarily_hidden(control)
        && !control
            .desired_visible
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .is_empty();
    for_each_overlay_host(app, |window| {
        if should_show {
            let _ = window.show();
        } else {
            let _ = window.hide();
        }
    });
}

pub(crate) fn overlay_is_enabled(app: &AppHandle, label: &str) -> bool {
    app.state::<OverlayControl>()
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
    let control = app.state::<OverlayControl>();
    let app_has_focus = !control
        .focused_windows
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .is_empty();
    let should_hide = frame.should_hide_overlays(app_has_focus);
    if control.auto_hidden.swap(should_hide, Ordering::Relaxed) == should_hide {
        return;
    }

    refresh_overlay_host_visibility(app);
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
        .ok_or_else(|| startup_log::command_error("unknown_overlay", &label))?;

    set_overlay_visible_state(&app, &control, label, visible)
}

fn set_overlay_visible_state(
    app: &AppHandle,
    control: &OverlayControl,
    label: &'static str,
    requested_visible: bool,
) -> Result<bool, String> {
    let visible = requested_visible && telemetry::overlay_allowed_in_current_mode(label);
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

    emit_overlay_visibility_state(app, label, visible)
}

fn toggle_overlay_visible_state(
    app: &AppHandle,
    control: &OverlayControl,
    label: &'static str,
) -> Result<bool, String> {
    let visible = {
        let mut desired = control
            .desired_visible
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if desired.remove(label) {
            false
        } else if telemetry::overlay_allowed_in_current_mode(label) {
            desired.insert(label);
            true
        } else {
            false
        }
    };

    emit_overlay_visibility_state(app, label, visible)
}

fn emit_overlay_visibility_state(
    app: &AppHandle,
    label: &'static str,
    visible: bool,
) -> Result<bool, String> {
    app.emit(
        "overlay://visibility",
        &OverlayWindowState { label, visible },
    )
    .map_err(|error| startup_log::command_error("overlay_visibility_failed", error))?;

    refresh_overlay_host_visibility(app);

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
        "stinthistory" => (1020.0, 650.0, 520.0, 134.0),
        "driving" => (20.0, 380.0, 468.0, 120.0),
        "liftcoast" => (20.0, 520.0, 190.0, 64.0),
        "tires" => (588.0, 380.0, 194.0, 130.0),
        "damage" => (798.0, 380.0, 94.0, 94.0),
        "standings" => (20.0, 70.0, 970.0, 500.0),
        "relative" => (20.0, 590.0, 344.0, 255.0),
        "fuel" => (1020.0, 70.0, 292.0, 198.0),
        "pitstop" => (1020.0, 320.0, 210.0, 150.0),
        "flags" => (1240.0, 320.0, 360.0, 120.0),
        "rejoin" => (1240.0, 460.0, 360.0, 140.0),
        "trackmap" => (1460.0, 70.0, 436.0, 436.0),
        "forecast" => (1020.0, 530.0, 366.0, 112.0),
        "conditions" => (1020.0, 410.0, 390.0, 90.0),
        "dashboard" => (620.0, 640.0, 362.0, 54.0),
        "sessioninfo" => (620.0, 700.0, 520.0, 54.0),
        _ => (20.0, 20.0, 360.0, 180.0),
    }
}

// Default geometry is authored for a 1920x1080 logical desktop. Panels anchored
// near the right or bottom edge would seed off-screen on a smaller or scaled
// monitor, so every placement is fitted to the host monitor's logical size.
const SEED_MARGIN: f64 = 20.0;

fn fit_seed_axis(position: f64, size: f64, available: f64) -> (f64, f64) {
    let size = size.min((available - SEED_MARGIN * 2.0).max(SEED_MARGIN));
    let max_position = (available - SEED_MARGIN - size).max(0.0);
    (position.clamp(0.0, max_position), size)
}

fn fit_seed_geometry(
    (x, y, width, height): (f64, f64, f64, f64),
    (available_width, available_height): (f64, f64),
) -> (f64, f64, f64, f64) {
    let (x, width) = fit_seed_axis(x, width, available_width);
    let (y, height) = fit_seed_axis(y, height, available_height);
    (x, y, width, height)
}

fn overlay_monitor_logical_size(app: &AppHandle, index: usize) -> Result<(f64, f64), String> {
    let monitors = sorted_monitors(app)?;
    let monitor = monitors
        .get(index)
        .or_else(|| monitors.first())
        .ok_or_else(|| "monitors_unavailable".to_string())?;
    let logical = monitor.size().to_logical::<f64>(monitor.scale_factor());
    Ok((logical.width, logical.height))
}

#[tauri::command]
fn get_default_overlay_placement(
    app: AppHandle,
    label: String,
    monitor: Option<usize>,
) -> Result<OverlayPlacementSeed, String> {
    let overlay = OVERLAY_LABELS
        .iter()
        .copied()
        .find(|candidate| *candidate == label)
        .ok_or_else(|| startup_log::command_error("unknown_overlay", &label))?;
    let available = overlay_monitor_logical_size(
        &app,
        monitor.unwrap_or_else(|| load_overlay_monitor_index(&app)),
    )?;
    let (x, y, width, height) = fit_seed_geometry(default_overlay_geometry(overlay), available);
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
    monitor: usize,
) -> Result<Vec<OverlayPlacementSeed>, String> {
    let available = overlay_monitor_logical_size(&app, monitor)?;

    Ok(OVERLAY_LABELS
        .iter()
        .map(|label| {
            let (x, y, width, height) =
                fit_seed_geometry(default_overlay_geometry(label), available);
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

#[cfg(test)]
mod layout_seed_tests {
    use super::{default_overlay_geometry, fit_seed_geometry, OVERLAY_LABELS, SEED_MARGIN};

    const REFERENCE: (f64, f64) = (1920.0, 1080.0);

    #[test]
    fn authored_defaults_are_kept_on_the_reference_desktop() {
        for label in OVERLAY_LABELS {
            let geometry = default_overlay_geometry(label);
            assert_eq!(
                fit_seed_geometry(geometry, REFERENCE),
                geometry,
                "{label} should keep its authored placement"
            );
        }
    }

    #[test]
    fn every_panel_stays_visible_on_a_smaller_or_scaled_monitor() {
        for available in [(1280.0, 720.0), (1024.0, 768.0), (800.0, 600.0)] {
            for label in OVERLAY_LABELS {
                let (x, y, width, height) =
                    fit_seed_geometry(default_overlay_geometry(label), available);
                assert!(x >= 0.0 && y >= 0.0, "{label} seeded before the origin");
                assert!(
                    x + width <= available.0 - SEED_MARGIN
                        && y + height <= available.1 - SEED_MARGIN,
                    "{label} seeded outside {available:?}"
                );
            }
        }
    }

    #[test]
    fn a_panel_wider_than_the_monitor_is_reduced_instead_of_pushed_out() {
        let (x, y, width, height) = fit_seed_geometry((20.0, 70.0, 970.0, 500.0), (640.0, 480.0));
        assert_eq!((x, y), (20.0, 20.0));
        assert_eq!((width, height), (600.0, 440.0));
    }
}

#[tauri::command]
fn get_telemetry_logging() -> telemetry::TelemetryLoggingStatus {
    telemetry::telemetry_logging_status()
}

#[tauri::command]
fn set_telemetry_logging(
    window: WebviewWindow,
    enabled: bool,
) -> Result<telemetry::TelemetryLoggingStatus, String> {
    require_control_window(&window)?;
    telemetry::set_telemetry_logging(enabled)
}

#[tauri::command]
fn set_performance_profile(window: WebviewWindow, profile: String) -> Result<(), String> {
    require_control_window(&window)?;
    telemetry::set_performance_profile(&profile)
}

#[tauri::command]
fn set_spectator_mode(window: WebviewWindow, enabled: bool) {
    if require_control_window(&window).is_err() {
        return;
    }
    telemetry::set_spectator_mode(enabled);
}

#[tauri::command]
fn set_team_mode(window: WebviewWindow, enabled: bool) {
    if require_control_window(&window).is_err() {
        return;
    }
    telemetry::set_team_mode(enabled);
}

#[tauri::command]
fn get_driver_rank_estimate_logging() -> telemetry::DriverRankEstimateLoggingStatus {
    telemetry::driver_rank_estimate_logging_status()
}

#[tauri::command]
fn set_driver_rank_estimate_logging(
    window: WebviewWindow,
    enabled: bool,
) -> Result<telemetry::DriverRankEstimateLoggingStatus, String> {
    require_control_window(&window)?;
    telemetry::set_driver_rank_estimate_logging(enabled)
}

#[tauri::command]
fn get_strategy_logging() -> telemetry::StrategyLoggingStatus {
    telemetry::strategy_logging_status()
}

#[tauri::command]
fn set_strategy_logging(
    window: WebviewWindow,
    enabled: bool,
) -> Result<telemetry::StrategyLoggingStatus, String> {
    require_control_window(&window)?;
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
) -> Result<bool, String> {
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
fn set_browser_source_enabled(
    window: WebviewWindow,
    enabled: bool,
) -> browser_source::BrowserSourceStatus {
    if require_control_window(&window).is_err() {
        return browser_source::status();
    }
    browser_source::set_enabled(enabled)
}

#[tauri::command]
fn set_browser_source_preferences(window: WebviewWindow, preferences: serde_json::Value) {
    if require_control_window(&window).is_err() {
        return;
    }
    browser_source::set_preferences(preferences);
}

#[tauri::command]
fn set_overlay_view_settings(window: WebviewWindow, settings: telemetry::OverlayViewSettings) {
    if require_control_window(&window).is_err() {
        return;
    }
    telemetry::set_overlay_view_settings(settings);
}

#[tauri::command]
fn set_delta_settings(window: WebviewWindow, settings: telemetry::DeltaSettings) {
    if require_control_window(&window).is_err() {
        return;
    }
    telemetry::set_delta_settings(settings);
}

#[tauri::command]
fn set_timing_settings(window: WebviewWindow, settings: telemetry::TimingSettings) {
    if require_control_window(&window).is_err() {
        return;
    }
    telemetry::set_timing_settings(settings);
}

#[tauri::command]
fn set_fuel_refuel_margin(window: WebviewWindow, liters: f64) {
    if require_control_window(&window).is_ok() {
        telemetry::set_fuel_refuel_margin(liters);
    }
}

#[tauri::command]
fn set_energy_refill_margin(window: WebviewWindow, percent: f64) {
    if require_control_window(&window).is_ok() {
        telemetry::set_energy_refill_margin(percent);
    }
}

#[tauri::command]
fn get_simulator_status(window: WebviewWindow) -> Result<telemetry::SimulatorStatus, String> {
    require_control_window(&window)?;
    Ok(telemetry::simulator_status())
}

#[tauri::command]
fn set_simulator_preference(window: WebviewWindow, simulator: String) -> Result<(), String> {
    require_control_window(&window)?;
    telemetry::set_simulator_preference(&simulator)
}

#[tauri::command]
fn open_support_url(url: &str) -> Result<(), String> {
    #[cfg(windows)]
    {
        use std::iter::once;
        use windows_sys::Win32::UI::Shell::ShellExecuteW;
        use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

        let url = url.encode_utf16().chain(once(0)).collect::<Vec<_>>();
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
            return Err("support_page_failed".into());
        }
        Ok(())
    }

    #[cfg(not(windows))]
    {
        let launcher = if cfg!(target_os = "macos") {
            "open"
        } else {
            "xdg-open"
        };
        std::process::Command::new(launcher)
            .arg(url)
            .spawn()
            .map(|_| ())
            .map_err(|error| startup_log::command_error("support_page_failed", error))
    }
}

#[tauri::command]
fn open_support_page(window: WebviewWindow) -> Result<(), String> {
    require_control_window(&window)?;
    open_support_url(KOFI_SUPPORT_URL)
}

#[tauri::command]
fn open_paypal_page(window: WebviewWindow) -> Result<(), String> {
    require_control_window(&window)?;
    open_support_url(PAYPAL_SUPPORT_URL)
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
        // A hand-edited settings file goes through the same validation as the
        // panel, so nothing unexpected reaches the accelerator parser.
        Ok(mut settings)
            if is_valid_shortcut(&settings.interaction_mode)
                && is_valid_shortcut(&settings.show_panel)
                && is_valid_shortcut(&settings.toggle_overlays)
                && settings.hide_overlays.len() == OVERLAY_LABELS.len()
                && OVERLAY_LABELS.iter().all(|label| {
                    settings
                        .hide_overlays
                        .get(*label)
                        .is_some_and(|shortcut| is_valid_overlay_shortcut(shortcut))
                }) =>
        {
            for label in OVERLAY_LABELS {
                settings
                    .hide_overlays
                    .entry(label.into())
                    .or_insert_with(|| {
                        default_overlay_shortcuts()
                            .remove(label)
                            .expect("default overlay shortcut")
                    });
            }
            settings
        }
        Ok(_) => {
            startup_log::record("warning: invalid shortcut configuration; using defaults");
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
            .map_err(|error| startup_log::command_error("settings_directory_failed", error))?;
    }
    let contents = serde_json::to_string_pretty(settings)
        .map_err(|error| startup_log::command_error("settings_encode_failed", error))?;
    fs::write(&path, contents).map_err(|error| {
        startup_log::command_error(
            "settings_write_failed",
            format!("{}: {error}", path.display()),
        )
    })
}

fn validate_overlay_configuration(contents: &str) -> Result<(), String> {
    let parsed: serde_json::Value = serde_json::from_str(contents)
        .map_err(|error| startup_log::command_error("configuration_invalid_json", error))?;
    let schema_version = parsed
        .get("schemaVersion")
        .and_then(serde_json::Value::as_u64);
    let format = parsed.get("format").and_then(serde_json::Value::as_str);
    if !matches!(
        format,
        Some("blackrack-overlay-configuration" | "lmu-overlay-configuration")
    ) || !matches!(schema_version, Some(1..))
    {
        return Err("configuration_unrecognized".into());
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
fn export_overlay_configuration(
    window: WebviewWindow,
    path: PathBuf,
    contents: String,
) -> Result<String, String> {
    require_control_window(&window)?;
    validate_overlay_configuration(&contents)?;
    if !path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("json"))
    {
        return Err("configuration_extension_required".into());
    }
    if let Some(directory) = path.parent() {
        fs::create_dir_all(directory)
            .map_err(|error| startup_log::command_error("configuration_directory_failed", error))?;
    }
    fs::write(&path, contents).map_err(|error| {
        startup_log::command_error(
            "configuration_write_failed",
            format!("{}: {error}", path.display()),
        )
    })?;
    Ok(path.display().to_string())
}

#[tauri::command]
fn import_overlay_configuration(window: WebviewWindow, path: PathBuf) -> Result<String, String> {
    require_control_window(&window)?;
    if !path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("json"))
    {
        return Err("configuration_extension_required".into());
    }
    let contents = fs::read_to_string(&path).map_err(|error| {
        startup_log::command_error(
            "configuration_read_failed",
            format!("{}: {error}", path.display()),
        )
    })?;
    validate_overlay_configuration(&contents)?;
    Ok(contents)
}

fn toggle_interaction_mode(app: &AppHandle) {
    let control = app.state::<OverlayControl>();
    let next = !control.click_through.load(Ordering::Relaxed);

    #[cfg(windows)]
    if !next {
        let previous = current_foreground_window();
        *control
            .edit_previous_foreground_window
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = previous;
    }
    #[cfg(windows)]
    let previous_foreground_window = if next {
        control
            .edit_previous_foreground_window
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .take()
    } else {
        None
    };

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

    #[cfg(windows)]
    if next {
        // Entering edit mode focuses the control panel so placement works. Put
        // keyboard input back in the window that was active before editing;
        // otherwise Escape and the other simulator controls stay with the
        // panel even though the overlays are locked again.
        restore_foreground_window(previous_foreground_window);
    }
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

fn toggle_all_overlays_shortcut(app: &AppHandle) {
    let control = app.state::<OverlayControl>();
    control.shortcut_hidden.fetch_xor(true, Ordering::Relaxed);
    refresh_overlay_host_visibility(app);
}

fn register_toggle_overlays_shortcut(app: &AppHandle, shortcut: &str) -> Result<(), String> {
    app.global_shortcut()
        .on_shortcut(shortcut, |app, _, event| {
            if event.state() == ShortcutState::Pressed {
                toggle_all_overlays_shortcut(app);
            }
        })
        .map_err(|error| error.to_string())
}

fn register_overlay_shortcut(
    app: &AppHandle,
    label: &'static str,
    shortcut: &str,
) -> Result<(), String> {
    if shortcut.is_empty() {
        return Ok(());
    }
    app.global_shortcut()
        .on_shortcut(shortcut, move |app, _, event| {
            if event.state() != ShortcutState::Pressed {
                return;
            }
            let control = app.state::<OverlayControl>();
            if let Err(error) = toggle_overlay_visible_state(app, &control, label) {
                startup_log::record(format!(
                    "could not emit visibility for overlay {label}: {error}"
                ));
            }
        })
        .map_err(|error| error.to_string())
}

/// Opens the inspector on every window of the application in debug builds.
///
/// The overlay host suppresses its own context menu and is click-through in
/// game mode, so right-clicking can never reach the inspector; this shortcut is
/// the only way in. It covers the control panel too, which otherwise would have
/// no way to reach it either once the release profile stops enabling devtools. A failed registration is recorded and ignored, because a diagnostic
/// aid must never keep the application from starting the way a missing
/// interaction shortcut does.
#[cfg(debug_assertions)]
fn register_devtools_shortcut(app: &AppHandle) {
    const DEVTOOLS_SHORTCUT: &str = "Ctrl+Shift+D";
    match app
        .global_shortcut()
        .on_shortcut(DEVTOOLS_SHORTCUT, |app, _, event| {
            if event.state() == ShortcutState::Pressed {
                for_each_overlay_host(app, |window| window.open_devtools());
                if let Some(panel) = app.get_webview_window(CONTROL_WINDOW_LABEL) {
                    panel.open_devtools();
                }
            }
        }) {
        Ok(()) => startup_log::record(format!(
            "debug devtools shortcut {DEVTOOLS_SHORTCUT} registered"
        )),
        Err(error) => startup_log::record(format!(
            "warning: debug devtools shortcut {DEVTOOLS_SHORTCUT} unavailable: {error}"
        )),
    }
}

fn shortcut_status(runtime: &ShortcutRuntime) -> ShortcutSettingsStatus {
    let hide_overlays = OVERLAY_LABELS
        .iter()
        .map(|label| {
            (
                (*label).into(),
                ShortcutBindingStatus {
                    shortcut: runtime.settings.hide_overlays[*label].clone(),
                    active: runtime.active_hide_overlays.contains_key(*label),
                    error: runtime.hide_overlays_error.get(*label).cloned(),
                },
            )
        })
        .collect();
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
        toggle_overlays: ShortcutBindingStatus {
            shortcut: runtime.settings.toggle_overlays.clone(),
            active: runtime.active_toggle_overlays.is_some(),
            error: runtime.toggle_overlays_error.clone(),
        },
        hide_overlays,
    }
}

/// Accelerators end up registered process-wide, so only the shape the plugin
/// actually understands is accepted: `Modifier+Modifier+Key` written with ASCII
/// letters and digits. Anything else is rejected before it reaches the parser
/// instead of relying on the length cap alone.
fn is_valid_shortcut(shortcut: &str) -> bool {
    const MAX_SEGMENTS: usize = 5;
    if shortcut.is_empty() || shortcut.len() > 64 {
        return false;
    }
    let mut segments = 0;
    for segment in shortcut.split('+') {
        segments += 1;
        if segments > MAX_SEGMENTS
            || segment.is_empty()
            || !segment.chars().all(|value| value.is_ascii_alphanumeric())
        {
            return false;
        }
    }
    segments > 1
}

fn is_valid_overlay_shortcut(shortcut: &str) -> bool {
    shortcut.is_empty() || is_valid_shortcut(shortcut)
}

#[cfg(test)]
mod shortcut_validation_tests {
    use super::{
        default_overlay_shortcuts, is_valid_shortcut, overlay_label_for_shortcut_action,
        OVERLAY_LABELS,
    };

    #[test]
    fn overlay_defaults_are_deterministic_and_cover_every_label() {
        let defaults = default_overlay_shortcuts();
        assert_eq!(defaults.len(), OVERLAY_LABELS.len());
        assert!(defaults.values().all(String::is_empty));
    }

    #[test]
    fn overlay_shortcut_actions_accept_known_labels_only() {
        assert_eq!(
            overlay_label_for_shortcut_action("hide_delta"),
            Some("delta")
        );
        assert_eq!(
            overlay_label_for_shortcut_action("hide_dashboard"),
            Some("dashboard")
        );
        assert_eq!(overlay_label_for_shortcut_action("hide_unknown"), None);
        assert_eq!(overlay_label_for_shortcut_action("show_panel"), None);
    }

    #[test]
    fn accepts_the_accelerators_the_panel_can_produce() {
        for shortcut in [
            "Ctrl+Shift+O",
            "Ctrl+Alt+M",
            "Ctrl+Shift+F12",
            "CommandOrControl+Shift+KeyK",
            "Super+Numpad0",
        ] {
            assert!(is_valid_shortcut(shortcut), "{shortcut} should be valid");
        }
    }

    #[test]
    fn rejects_malformed_or_oversized_accelerators() {
        for shortcut in [
            "",
            "O",
            "Ctrl+",
            "+O",
            "Ctrl++O",
            "Ctrl+Shift+O;rm",
            "Ctrl+Shift+\u{202e}O",
            "Ctrl+Alt+Shift+Super+Meta+O",
            "Ctrl+Shift+ThisAcceleratorIsFarTooLongToEverBeRegisteredByTheUser",
        ] {
            assert!(
                !is_valid_shortcut(shortcut),
                "{shortcut} should be rejected"
            );
        }
    }

    #[test]
    fn overlay_shortcuts_may_be_empty_but_global_shortcuts_may_not() {
        assert!(super::is_valid_overlay_shortcut(""));
        assert!(!is_valid_shortcut(""));
    }

    #[test]
    fn legacy_shortcut_settings_get_global_overlay_toggle_default() {
        let settings: super::ShortcutSettings = serde_json::from_str(
            r#"{"interaction_mode":"Ctrl+Shift+O","show_panel":"Ctrl+Shift+M","hide_overlays":{}}"#,
        )
        .unwrap();
        assert_eq!(settings.toggle_overlays, "Ctrl+Shift+H");
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
    window: WebviewWindow,
    control: State<'_, ShortcutControl>,
    action: String,
    shortcut: String,
) -> Result<ShortcutSettingsStatus, String> {
    require_control_window(&window)?;
    let shortcut = shortcut.trim().replace(' ', "");
    let is_overlay_action = overlay_label_for_shortcut_action(&action).is_some();
    if !(if is_overlay_action {
        is_valid_overlay_shortcut(&shortcut)
    } else {
        is_valid_shortcut(&shortcut)
    }) {
        return Err("invalid".into());
    }

    let (old_active, old_settings) = {
        let runtime = control
            .0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let old_active = match action.as_str() {
            "interaction_mode" => runtime.active_interaction_mode.clone(),
            "show_panel" => runtime.active_show_panel.clone(),
            "toggle_overlays" => runtime.active_toggle_overlays.clone(),
            value if overlay_label_for_shortcut_action(value).is_some() => runtime
                .active_hide_overlays
                .get(overlay_label_for_shortcut_action(value).unwrap())
                .cloned(),
            _ => return Err("unknown_action".into()),
        };
        (old_active, runtime.settings.clone())
    };

    let mut configured_shortcuts = vec![
        (
            "interaction_mode".to_string(),
            old_settings.interaction_mode.as_str(),
        ),
        ("show_panel".to_string(), old_settings.show_panel.as_str()),
        (
            "toggle_overlays".to_string(),
            old_settings.toggle_overlays.as_str(),
        ),
    ];
    configured_shortcuts.extend(OVERLAY_LABELS.iter().map(|label| {
        (
            format!("hide_{label}"),
            old_settings.hide_overlays[*label].as_str(),
        )
    }));
    let duplicate = configured_shortcuts
        .into_iter()
        .any(|(other_action, other)| {
            !shortcut.is_empty() && other_action != action && shortcut.eq_ignore_ascii_case(other)
        });
    if duplicate {
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

    let register = register_shortcut_action(&app, &action, &shortcut);

    if let Err(error) = register {
        if let Some(old) = &old_active {
            let restored = register_shortcut_action(&app, &action, old);
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
    } else if action == "show_panel" {
        new_settings.show_panel = shortcut.clone();
    } else if action == "toggle_overlays" {
        new_settings.toggle_overlays = shortcut.clone();
    } else {
        new_settings.hide_overlays.insert(
            overlay_label_for_shortcut_action(&action).unwrap().into(),
            shortcut.clone(),
        );
    }
    if let Err(error) = save_shortcut_settings(&app, &new_settings) {
        if !shortcut.is_empty() {
            let _ = app.global_shortcut().unregister(shortcut.as_str());
        }
        if let Some(old) = &old_active {
            let _ = register_shortcut_action(&app, &action, old);
        }
        startup_log::record(format!("could not save shortcut settings: {error}"));
        return Err("persistence_failed".into());
    }

    let mut runtime = control
        .0
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    runtime.settings = new_settings;
    match action.as_str() {
        "interaction_mode" => {
            runtime.active_interaction_mode = Some(shortcut.clone());
            runtime.interaction_mode_error = None;
        }
        "show_panel" => {
            runtime.active_show_panel = Some(shortcut.clone());
            runtime.show_panel_error = None;
        }
        "toggle_overlays" => {
            runtime.active_toggle_overlays = Some(shortcut.clone());
            runtime.toggle_overlays_error = None;
        }
        _ => {
            let label = overlay_label_for_shortcut_action(&action).unwrap();
            if shortcut.is_empty() {
                runtime.active_hide_overlays.remove(label);
            } else {
                runtime
                    .active_hide_overlays
                    .insert(label.into(), shortcut.clone());
            }
            runtime.hide_overlays_error.remove(label);
        }
    }
    startup_log::record(format!("shortcut {action} changed to {shortcut}"));
    Ok(shortcut_status(&runtime))
}

fn register_shortcut_action(app: &AppHandle, action: &str, shortcut: &str) -> Result<(), String> {
    match action {
        "interaction_mode" => register_interaction_shortcut(app, shortcut),
        "show_panel" => register_panel_shortcut(app, shortcut),
        "toggle_overlays" => register_toggle_overlays_shortcut(app, shortcut),
        value => {
            let label = overlay_label_for_shortcut_action(value)
                .ok_or_else(|| "unknown_action".to_string())?;
            register_overlay_shortcut(app, label, shortcut)
        }
    }
}

pub fn run() {
    if updater::run_helper_if_requested() {
        return;
    }

    let log_path = startup_log::initialize();
    startup_log::record(format!("startup log path={}", log_path.display()));
    startup_log::record("building Tauri application");

    let builder = tauri::Builder::default()
        .manage(OverlayControl {
            click_through: AtomicBool::new(DEFAULT_CLICK_THROUGH),
            auto_hidden: AtomicBool::new(true),
            shortcut_hidden: AtomicBool::new(false),
            shutdown: AtomicBool::new(false),
            focused_windows: Mutex::new(HashSet::new()),
            desired_visible: Mutex::new(HashSet::new()),
            #[cfg(windows)]
            edit_previous_foreground_window: Mutex::new(None),
        })
        .manage(ShortcutControl(Mutex::new(ShortcutRuntime::default())));
    #[cfg(windows)]
    let builder = builder.manage(wheel_input::WheelInputControl::default());

    builder
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
            set_overlay_host_bounds,
            sync_overlay_hosts,
            toggle_interaction_mode_command,
            get_telemetry_logging,
            set_telemetry_logging,
            set_performance_profile,
            set_spectator_mode,
            set_team_mode,
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
            set_fuel_refuel_margin,
            set_energy_refill_margin,
            get_simulator_status,
            set_simulator_preference,
            open_support_page,
            open_paypal_page,
            updater::check_for_update,
            updater::download_and_install_update,
            updater::get_update_status,
            get_shortcut_settings,
            set_shortcut,
            export_overlay_configuration,
            import_overlay_configuration,
            #[cfg(windows)]
            wheel_input::get_wheel_input_status,
            #[cfg(windows)]
            wheel_input::capture_delta_wheel_button,
            #[cfg(windows)]
            wheel_input::cancel_delta_wheel_button_capture,
            #[cfg(windows)]
            wheel_input::clear_delta_wheel_button
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
                #[cfg(windows)]
                if let Ok(hwnd) = panel.hwnd() {
                    wheel_input::spawn(app.handle().clone(), hwnd.0 as isize);
                }
                if panel
                    .inner_size()
                    .map(|size| size.height < 920)
                    .unwrap_or(false)
                {
                    let _ = panel.set_size(tauri::LogicalSize::new(700.0, 950.0));
                    startup_log::record("control window size restored to 700x950");
                }
            } else {
                startup_log::record("warning: control window not found during setup");
            }

            startup_log::record("overlay hosts will be created on demand");
            #[cfg(debug_assertions)]
            register_devtools_shortcut(app.handle());
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
            let overlay_results: Vec<(&'static str, Result<(), String>)> = OVERLAY_LABELS
                .iter()
                .map(|label| {
                    (
                        *label,
                        register_overlay_shortcut(
                            app.handle(),
                            label,
                            settings.hide_overlays[*label].as_str(),
                        ),
                    )
                })
                .collect();
            // Preserve bindings saved before the global toggle was introduced.
            // If one already uses the new default, let that binding win and
            // report the global shortcut as unavailable until the user changes it.
            let toggle_overlays_result =
                register_toggle_overlays_shortcut(app.handle(), &settings.toggle_overlays);
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
            match toggle_overlays_result {
                Ok(()) => {
                    runtime.active_toggle_overlays = Some(settings.toggle_overlays.clone());
                    startup_log::record(format!(
                        "shortcut {} registered for toggling overlays",
                        settings.toggle_overlays
                    ));
                }
                Err(error) => {
                    runtime.toggle_overlays_error = Some(error.clone());
                    startup_log::record(format!(
                        "warning: {} unavailable; continuing without it: {error}",
                        settings.toggle_overlays
                    ));
                }
            }
            for (label, result) in overlay_results {
                if settings.hide_overlays[label].is_empty() {
                    continue;
                }
                match result {
                    Ok(()) => {
                        runtime
                            .active_hide_overlays
                            .insert(label.into(), settings.hide_overlays[label].clone());
                        startup_log::record(format!(
                            "shortcut {} registered for hiding overlay {label}",
                            settings.hide_overlays[label]
                        ));
                    }
                    Err(error) => {
                        runtime
                            .hide_overlays_error
                            .insert(label.into(), error.clone());
                        startup_log::record(format!(
                            "warning: {} unavailable; continuing without it: {error}",
                            settings.hide_overlays[label]
                        ));
                    }
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
            let control = window.app_handle().state::<OverlayControl>();
            match event {
                tauri::WindowEvent::Focused(focused) => {
                    let mut focused_windows = control
                        .focused_windows
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner());
                    if *focused {
                        focused_windows.insert(window.label().to_string());
                    } else {
                        focused_windows.remove(window.label());
                    }
                }
                tauri::WindowEvent::Destroyed => {
                    startup_log::record(format!("window destroyed label={}", window.label()));
                    let mut focused_windows = control
                        .focused_windows
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner());
                    focused_windows.remove(window.label());
                }
                tauri::WindowEvent::CloseRequested { .. }
                    if window.label() == CONTROL_WINDOW_LABEL =>
                {
                    control.shutdown.store(true, Ordering::Relaxed);
                    save_control_window_position(window);
                    startup_log::record("shutdown requested reason=control_window_close_requested");
                    window.app_handle().exit(0);
                }
                tauri::WindowEvent::CloseRequested { .. }
                    if window.label().starts_with(OVERLAY_HOST_PREFIX) =>
                {
                    startup_log::record(format!(
                        "overlay host close requested label={}",
                        window.label()
                    ));
                }
                _ => {}
            }
        })
        .build(tauri::generate_context!())
        .unwrap_or_else(|error| {
            startup_log::record(format!("fatal Tauri error cause={error} details={error:?}"));
            panic!("error al ejecutar BlackRack Overlay: {error}");
        })
        .run(|_, event| {
            if matches!(event, tauri::RunEvent::Exit) {
                startup_log::record("session end status=normal reason=event_loop_exit");
            }
        });
}
