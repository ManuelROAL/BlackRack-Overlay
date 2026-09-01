use serde::{Deserialize, Serialize};
use std::fs;
use std::sync::Mutex;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};

use crate::{app_paths, startup_log, telemetry};

#[derive(Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WheelBinding {
    device_id: String,
    device_name: String,
    button: u16,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WheelInputStatus {
    available: bool,
    capturing: bool,
    binding: Option<WheelBinding>,
    error: Option<String>,
}

pub(crate) struct WheelInputControl(pub(crate) Mutex<WheelInputStatus>);

impl Default for WheelInputControl {
    fn default() -> Self {
        Self(Mutex::new(WheelInputStatus {
            available: false,
            capturing: false,
            binding: load_binding(),
            error: None,
        }))
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
struct NativeWheelButtonEvent {
    device_id: [u8; 16],
    device_name: [u16; 128],
    button: u16,
}

extern "C" {
    fn wheel_input_initialize(hwnd: isize) -> bool;
    fn wheel_input_poll(event: *mut NativeWheelButtonEvent) -> bool;
    fn wheel_input_shutdown();
}

fn binding_path() -> std::path::PathBuf {
    app_paths::data_directory().join("wheel-input.json")
}

fn load_binding() -> Option<WheelBinding> {
    fs::read(binding_path())
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
}

fn save_binding(binding: Option<&WheelBinding>) -> Result<(), String> {
    let path = binding_path();
    if let Some(directory) = path.parent() {
        fs::create_dir_all(directory).map_err(|error| error.to_string())?;
    }
    let bytes = serde_json::to_vec_pretty(&binding).map_err(|error| error.to_string())?;
    fs::write(path, bytes).map_err(|error| error.to_string())
}

fn device_id(bytes: &[u8; 16]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn decode_name(name: &[u16; 128]) -> String {
    let end = name
        .iter()
        .position(|character| *character == 0)
        .unwrap_or(name.len());
    String::from_utf16_lossy(&name[..end])
}

pub(crate) fn spawn(app: AppHandle, hwnd: isize) {
    let _ = std::thread::Builder::new()
        .name("wheel-input".into())
        .spawn(move || unsafe {
            if !wheel_input_initialize(hwnd) {
                let control = app.state::<WheelInputControl>();
                let snapshot = {
                    let mut status = control
                        .0
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner());
                    status.error = Some("initialization_failed".into());
                    status.clone()
                };
                let _ = app.emit("wheel-input://status", snapshot);
                startup_log::record("warning: DirectInput wheel initialization failed");
                return;
            }

            {
                let control = app.state::<WheelInputControl>();
                let snapshot = {
                    let mut status = control
                        .0
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner());
                    status.available = true;
                    status.clone()
                };
                let _ = app.emit("wheel-input://status", snapshot);
            }

            loop {
                let mut event = NativeWheelButtonEvent {
                    device_id: [0; 16],
                    device_name: [0; 128],
                    button: 0,
                };
                if wheel_input_poll(&mut event) {
                    let pressed = WheelBinding {
                        device_id: device_id(&event.device_id),
                        device_name: decode_name(&event.device_name),
                        button: event.button,
                    };
                    let control = app.state::<WheelInputControl>();
                    let mut status = control
                        .0
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner());
                    if status.capturing {
                        status.capturing = false;
                        status.binding = Some(pressed.clone());
                        match save_binding(Some(&pressed)) {
                            Ok(()) => status.error = None,
                            Err(error) => {
                                status.error = Some("persistence_failed".into());
                                startup_log::record(format!(
                                    "could not save wheel binding: {error}"
                                ));
                            }
                        }
                        let snapshot = status.clone();
                        drop(status);
                        let _ = app.emit("wheel-input://status", snapshot);
                    } else if status.binding.as_ref().is_some_and(|binding| {
                        binding.device_id == pressed.device_id && binding.button == pressed.button
                    }) {
                        drop(status);
                        if crate::overlay_is_enabled(&app, "delta") {
                            let mode = telemetry::cycle_delta_mode();
                            let _ = app.emit("delta://mode-changed", mode);
                        }
                    }
                }
                if app.webview_windows().is_empty() {
                    break;
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            wheel_input_shutdown();
        });
}

#[tauri::command]
pub(crate) fn get_wheel_input_status(
    control: tauri::State<'_, WheelInputControl>,
) -> WheelInputStatus {
    control
        .0
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone()
}

#[tauri::command]
pub(crate) fn capture_delta_wheel_button(
    control: tauri::State<'_, WheelInputControl>,
) -> Result<WheelInputStatus, String> {
    let mut status = control
        .0
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if !status.available {
        return Err(status.error.clone().unwrap_or_else(|| "unavailable".into()));
    }
    status.capturing = true;
    status.error = None;
    Ok(status.clone())
}

#[tauri::command]
pub(crate) fn cancel_delta_wheel_button_capture(
    control: tauri::State<'_, WheelInputControl>,
) -> WheelInputStatus {
    let mut status = control
        .0
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    status.capturing = false;
    status.clone()
}

#[tauri::command]
pub(crate) fn clear_delta_wheel_button(
    control: tauri::State<'_, WheelInputControl>,
) -> Result<WheelInputStatus, String> {
    save_binding(None).map_err(|_| "persistence_failed".to_string())?;
    let mut status = control
        .0
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    status.binding = None;
    status.capturing = false;
    status.error = None;
    Ok(status.clone())
}
