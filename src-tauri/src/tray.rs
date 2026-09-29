//! System tray icon and the control panel's close-to-tray behavior.
//!
//! Closing the panel hides it instead of exiting while the tray icon is
//! available, so telemetry and the overlays keep running in the background.
//! Every real exit goes through `crate::request_exit` (or the updater's own
//! `app.exit`), which marks the application as quitting so the close handler
//! never keeps a process alive that was asked to end.

use crate::{app_paths, require_control_window, startup_log, CONTROL_WINDOW_LABEL};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, State, WebviewWindow, Wry};

const TRAY_ID: &str = "main";
const TRAY_TOOLTIP: &str = "BlackRack Overlay";
const MENU_SHOW: &str = "show";
const MENU_TOGGLE_OVERLAYS: &str = "toggle-overlays";
const MENU_QUIT: &str = "quit";

/// Emitted to the control panel whenever it is shown again, so it can refresh
/// what its timers could not keep current while it was hidden.
pub(crate) const CONTROL_SHOWN_EVENT: &str = "control://shown";

pub(crate) struct TrayState {
    quitting: AtomicBool,
    close_to_tray: AtomicBool,
    tray_available: AtomicBool,
    items: Mutex<Option<TrayMenuItems>>,
}

struct TrayMenuItems {
    show: MenuItem<Wry>,
    toggle_overlays: MenuItem<Wry>,
    quit: MenuItem<Wry>,
}

impl TrayState {
    pub(crate) fn load() -> Self {
        Self {
            quitting: AtomicBool::new(false),
            close_to_tray: AtomicBool::new(load_window_behavior().close_to_tray),
            tray_available: AtomicBool::new(false),
            items: Mutex::new(None),
        }
    }
}

#[derive(Clone, Deserialize, Serialize)]
struct WindowBehaviorSettings {
    close_to_tray: bool,
}

impl Default for WindowBehaviorSettings {
    fn default() -> Self {
        Self {
            close_to_tray: true,
        }
    }
}

fn window_behavior_path() -> PathBuf {
    app_paths::data_directory().join("window-behavior.json")
}

fn load_window_behavior() -> WindowBehaviorSettings {
    fs::read_to_string(window_behavior_path())
        .ok()
        .and_then(|contents| serde_json::from_str(&contents).ok())
        .unwrap_or_default()
}

fn save_window_behavior(settings: &WindowBehaviorSettings) -> Result<(), String> {
    let path = window_behavior_path();
    if let Some(directory) = path.parent() {
        fs::create_dir_all(directory)
            .map_err(|error| startup_log::command_error("settings_directory_failed", error))?;
    }
    let contents = serde_json::to_string_pretty(settings)
        .map_err(|error| startup_log::command_error("settings_encode_failed", error))?;
    fs::write(&path, contents)
        .map_err(|error| startup_log::command_error("settings_write_failed", error))
}

/// Creates the tray icon. The labels start in English, the frontend's default
/// locale, and the control panel replaces them with `set_tray_labels` as soon
/// as it loads.
pub(crate) fn install(app: &AppHandle) -> tauri::Result<()> {
    let show = MenuItem::with_id(app, MENU_SHOW, "Show panel", true, None::<&str>)?;
    let toggle_overlays = MenuItem::with_id(
        app,
        MENU_TOGGLE_OVERLAYS,
        "Hide / show overlays",
        true,
        None::<&str>,
    )?;
    let quit = MenuItem::with_id(app, MENU_QUIT, "Quit", true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;
    let menu = Menu::with_items(app, &[&show, &toggle_overlays, &separator, &quit])?;

    let mut builder = TrayIconBuilder::with_id(TRAY_ID)
        .tooltip(TRAY_TOOLTIP)
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            MENU_SHOW => show_control_panel(app),
            MENU_TOGGLE_OVERLAYS => crate::toggle_all_overlays_shortcut(app),
            MENU_QUIT => crate::request_exit(app, "tray_quit"),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_control_panel(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder.build(app)?;

    let state = app.state::<TrayState>();
    *state
        .items
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(TrayMenuItems {
        show,
        toggle_overlays,
        quit,
    });
    state.tray_available.store(true, Ordering::Relaxed);
    Ok(())
}

/// Brings the control panel back from the tray, the taskbar or behind other
/// windows, and tells it to refresh state its throttled timers may have missed.
pub(crate) fn show_control_panel(app: &AppHandle) {
    let Some(panel) = app.get_webview_window(CONTROL_WINDOW_LABEL) else {
        return;
    };
    let _ = panel.show();
    let _ = panel.unminimize();
    let _ = panel.set_focus();
    let _ = app.emit_to(CONTROL_WINDOW_LABEL, CONTROL_SHOWN_EVENT, ());
}

/// Whether closing the control panel should hide it. Without a tray icon the
/// panel could only come back through its shortcut, so closing exits instead.
pub(crate) fn should_hide_on_close(app: &AppHandle) -> bool {
    let state = app.state::<TrayState>();
    !state.quitting.load(Ordering::Relaxed)
        && state.close_to_tray.load(Ordering::Relaxed)
        && state.tray_available.load(Ordering::Relaxed)
}

pub(crate) fn mark_quitting(app: &AppHandle) {
    app.state::<TrayState>()
        .quitting
        .store(true, Ordering::Relaxed);
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TrayLabels {
    show: String,
    toggle_overlays: String,
    quit: String,
}

#[tauri::command]
pub(crate) fn set_tray_labels(
    window: WebviewWindow,
    state: State<'_, TrayState>,
    labels: TrayLabels,
) -> Result<(), String> {
    require_control_window(&window)?;
    let items = state
        .items
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let Some(items) = items.as_ref() else {
        return Ok(());
    };
    for (item, label) in [
        (&items.show, &labels.show),
        (&items.toggle_overlays, &labels.toggle_overlays),
        (&items.quit, &labels.quit),
    ] {
        item.set_text(label.trim())
            .map_err(|error| startup_log::command_error("tray_label_failed", error))?;
    }
    Ok(())
}

#[tauri::command]
pub(crate) fn get_close_to_tray(
    window: WebviewWindow,
    state: State<'_, TrayState>,
) -> Result<bool, String> {
    require_control_window(&window)?;
    Ok(state.close_to_tray.load(Ordering::Relaxed))
}

#[tauri::command]
pub(crate) fn set_close_to_tray(
    window: WebviewWindow,
    state: State<'_, TrayState>,
    enabled: bool,
) -> Result<bool, String> {
    require_control_window(&window)?;
    save_window_behavior(&WindowBehaviorSettings {
        close_to_tray: enabled,
    })?;
    state.close_to_tray.store(enabled, Ordering::Relaxed);
    startup_log::record(format!("close to tray set enabled={enabled}"));
    Ok(enabled)
}

#[cfg(test)]
mod tests {
    use super::WindowBehaviorSettings;

    #[test]
    fn closes_to_tray_by_default() {
        assert!(WindowBehaviorSettings::default().close_to_tray);
    }

    #[test]
    fn missing_or_malformed_settings_keep_the_default() {
        for contents in ["", "{}", "{\"close_to_tray\":\"no\"}"] {
            let parsed = serde_json::from_str::<WindowBehaviorSettings>(contents)
                .ok()
                .unwrap_or_default();
            assert!(parsed.close_to_tray, "contents={contents}");
        }
    }
}
