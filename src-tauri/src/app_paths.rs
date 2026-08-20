use std::path::PathBuf;

pub(crate) const DATA_DIRECTORY_NAME: &str = "BlackRack Overlay";

pub(crate) fn data_directory() -> PathBuf {
    #[cfg(target_os = "windows")]
    let base = std::env::var_os("APPDATA").map(PathBuf::from);

    #[cfg(not(target_os = "windows"))]
    let base = std::env::var_os("XDG_DATA_HOME").map(PathBuf::from);

    base.unwrap_or_else(std::env::temp_dir)
        .join(DATA_DIRECTORY_NAME)
}
