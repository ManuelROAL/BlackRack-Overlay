//! Launch at Windows sign-in through the current user's `Run` registry key.
//!
//! The value name matches the NSIS `${PRODUCTNAME}`, so the uninstaller removes
//! the entry and updates keep it. The entry passes `AUTOSTART_ARG`, which starts
//! the application with the control panel hidden in the tray.

use crate::require_control_window;
#[cfg(windows)]
use crate::startup_log;
use tauri::WebviewWindow;

pub(crate) const AUTOSTART_ARG: &str = "--autostart";

/// Whether this process was started by the sign-in entry.
pub(crate) fn launched_at_login() -> bool {
    std::env::args().any(|arg| arg == AUTOSTART_ARG)
}

#[cfg(windows)]
mod registry {
    use super::AUTOSTART_ARG;
    use std::ptr::null_mut;
    use windows_sys::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS};
    use windows_sys::Win32::System::Registry::{
        RegDeleteKeyValueW, RegGetValueW, RegSetKeyValueW, HKEY_CURRENT_USER, REG_SZ,
        RRF_RT_REG_BINARY, RRF_RT_REG_SZ,
    };

    const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
    /// Where Task Manager and Settings record a startup entry the user turned off.
    const STARTUP_APPROVED_KEY: &str =
        r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run";
    const VALUE_NAME: &str = "BlackRack Overlay";

    fn wide(value: &str) -> Vec<u16> {
        value.encode_utf16().chain(std::iter::once(0)).collect()
    }

    fn value_exists(key: &str, flags: u32) -> bool {
        let (key, name) = (wide(key), wide(VALUE_NAME));
        let mut size = 0u32;
        let status = unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                key.as_ptr(),
                name.as_ptr(),
                flags,
                null_mut(),
                null_mut(),
                &mut size,
            )
        };
        status == ERROR_SUCCESS
    }

    /// Task Manager keeps the `Run` value and marks it disabled here: an odd
    /// first byte means the user turned the entry off.
    fn disabled_by_user() -> bool {
        let (key, name) = (wide(STARTUP_APPROVED_KEY), wide(VALUE_NAME));
        let mut data = [0u8; 12];
        let mut size = data.len() as u32;
        let status = unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                key.as_ptr(),
                name.as_ptr(),
                RRF_RT_REG_BINARY,
                null_mut(),
                data.as_mut_ptr().cast(),
                &mut size,
            )
        };
        status == ERROR_SUCCESS && size > 0 && is_disabled_marker(data[0])
    }

    pub(super) fn is_disabled_marker(first_byte: u8) -> bool {
        first_byte & 1 == 1
    }

    fn delete_value(key: &str) -> Result<(), u32> {
        let (key, name) = (wide(key), wide(VALUE_NAME));
        let status = unsafe { RegDeleteKeyValueW(HKEY_CURRENT_USER, key.as_ptr(), name.as_ptr()) };
        match status {
            ERROR_SUCCESS | ERROR_FILE_NOT_FOUND => Ok(()),
            error => Err(error),
        }
    }

    pub(super) fn command_line(executable: &std::path::Path) -> String {
        format!("\"{}\" {AUTOSTART_ARG}", executable.display())
    }

    pub(super) fn is_enabled() -> bool {
        value_exists(RUN_KEY, RRF_RT_REG_SZ) && !disabled_by_user()
    }

    pub(super) fn set_enabled(enabled: bool) -> Result<(), String> {
        // Clearing the Task Manager marker too makes the user's choice here
        // the one Windows honors, in either direction.
        delete_value(STARTUP_APPROVED_KEY).map_err(|error| format!("registry error {error}"))?;
        if !enabled {
            return delete_value(RUN_KEY).map_err(|error| format!("registry error {error}"));
        }
        let executable = std::env::current_exe().map_err(|error| error.to_string())?;
        let data = wide(&command_line(&executable));
        let (key, name) = (wide(RUN_KEY), wide(VALUE_NAME));
        let status = unsafe {
            RegSetKeyValueW(
                HKEY_CURRENT_USER,
                key.as_ptr(),
                name.as_ptr(),
                REG_SZ,
                data.as_ptr().cast(),
                (data.len() * std::mem::size_of::<u16>()) as u32,
            )
        };
        if status == ERROR_SUCCESS {
            Ok(())
        } else {
            Err(format!("registry error {status}"))
        }
    }
}

#[tauri::command]
pub(crate) fn get_launch_at_login(window: WebviewWindow) -> Result<bool, String> {
    require_control_window(&window)?;
    #[cfg(windows)]
    return Ok(registry::is_enabled());
    #[cfg(not(windows))]
    Err("launch_at_login_unsupported".into())
}

#[tauri::command]
pub(crate) fn set_launch_at_login(window: WebviewWindow, enabled: bool) -> Result<bool, String> {
    require_control_window(&window)?;
    #[cfg(windows)]
    {
        registry::set_enabled(enabled)
            .map_err(|error| startup_log::command_error("launch_at_login_failed", error))?;
        startup_log::record(format!("launch at login set enabled={enabled}"));
        Ok(registry::is_enabled())
    }
    #[cfg(not(windows))]
    {
        let _ = enabled;
        Err("launch_at_login_unsupported".into())
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::registry::{command_line, is_disabled_marker};
    use std::path::Path;

    #[test]
    fn quotes_the_executable_and_passes_the_autostart_flag() {
        assert_eq!(
            command_line(Path::new(
                r"C:\Program Files\BlackRack\blackrack-overlay.exe"
            )),
            r#""C:\Program Files\BlackRack\blackrack-overlay.exe" --autostart"#
        );
    }

    #[test]
    fn reads_task_manager_startup_markers() {
        assert!(!is_disabled_marker(0x02));
        assert!(!is_disabled_marker(0x06));
        assert!(is_disabled_marker(0x03));
        assert!(is_disabled_marker(0x07));
    }
}
