//! Whether the simulator owns the foreground window.
//!
//! The telemetry interface does not say it, and the overlay host needs it to
//! stay hidden while the user is in another application. The check is a
//! read-only look at the foreground window's process image name.

use std::ffi::OsString;
use std::os::windows::ffi::OsStringExt;

use windows_sys::Win32::Foundation::CloseHandle;
use windows_sys::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};

/// The simulator runs under more than one executable — the graphics client and
/// the UI shell it returns to between sessions — and their exact names have
/// changed across releases. Matching the family rather than a list of names
/// keeps a rename from hiding every overlay, which is the costly failure here.
const SIMULATOR_EXECUTABLE_FAMILY: &str = "iracing";

pub(super) fn simulator_has_focus() -> bool {
    foreground_image_name().is_some_and(|image| {
        image
            .to_ascii_lowercase()
            .contains(SIMULATOR_EXECUTABLE_FAMILY)
    })
}

fn foreground_image_name() -> Option<String> {
    let window = unsafe { GetForegroundWindow() };
    if window.is_null() {
        return None;
    }
    let mut process_id = 0_u32;
    unsafe { GetWindowThreadProcessId(window, &mut process_id) };
    if process_id == 0 {
        return None;
    }
    let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, process_id) };
    if process.is_null() {
        return None;
    }
    let mut buffer = [0_u16; 260];
    let mut length = buffer.len() as u32;
    let read = unsafe {
        QueryFullProcessImageNameW(
            process,
            PROCESS_NAME_WIN32,
            buffer.as_mut_ptr(),
            &mut length,
        )
    };
    unsafe { CloseHandle(process) };
    if read == 0 || length == 0 {
        return None;
    }
    let path = OsString::from_wide(&buffer[..length as usize]);
    let path = path.to_string_lossy();
    Some(
        path.rsplit(['\\', '/'])
            .next()
            .unwrap_or(path.as_ref())
            .to_owned(),
    )
}
