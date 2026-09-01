use std::path::PathBuf;

pub(crate) const DATA_DIRECTORY_NAME: &str = "BlackRack Overlay";
pub(crate) const WEBVIEW_DATA_DIRECTORY_NAME: &str = "BlackRackOverlay";

pub(crate) fn data_directory() -> PathBuf {
    // Diagnostics, telemetry logs and settings live here, so fall back to the
    // user profile before the shared temporary directory: on Linux `temp_dir()`
    // is world-readable `/tmp`.
    #[cfg(target_os = "windows")]
    let base = std::env::var_os("APPDATA").map(PathBuf::from).or_else(|| {
        std::env::var_os("USERPROFILE")
            .map(|profile| PathBuf::from(profile).join("AppData").join("Roaming"))
    });

    #[cfg(not(target_os = "windows"))]
    let base = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local").join("share"))
        });

    base.unwrap_or_else(std::env::temp_dir)
        .join(DATA_DIRECTORY_NAME)
}

/// Shared WebView2 user data directory (holds the `EBWebView` profile). It lives
/// under the user's local app data under a readable `BlackRackOverlay` name,
/// independent of the technical bundle identifier, so it must be set explicitly
/// on every webview window.
pub(crate) fn webview_data_directory() -> PathBuf {
    #[cfg(target_os = "windows")]
    let base = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("USERPROFILE")
                .map(|profile| PathBuf::from(profile).join("AppData").join("Local"))
        });

    #[cfg(not(target_os = "windows"))]
    let base = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local").join("share"))
        });

    base.unwrap_or_else(std::env::temp_dir)
        .join(WEBVIEW_DATA_DIRECTORY_NAME)
}
