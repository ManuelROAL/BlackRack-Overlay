use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

const UPDATE_MANIFEST_URL: &str =
    "https://github.com/ManuelROAL/BlackRack-Overlay/releases/latest/download/manifest.json";
const UPDATE_SCHEMA_VERSION: u32 = 1;
const MAX_MANIFEST_BYTES: u64 = 256 * 1024;
const MAX_UPDATE_PROGRESS_INTERVAL_BYTES: u64 = 256 * 1024;
const UPDATE_DIRECTORY_NAME: &str = "updates";
const UPDATE_INSTALLER_NAME: &str = "installer.exe";
const UPDATE_RUN_DIRECTORY_PREFIX: &str = "run-";
const UPDATE_FAILURE_FILE_NAME: &str = "last-failure.txt";
const UPDATE_COORDINATOR_NAME: &str = "coordinator.exe";
#[cfg(windows)]
const UPDATE_MUTEX_NAME: &str = "Local\\BlackRackOverlayUpdateLock";
#[cfg(windows)]
const UPDATE_ACTIVE_EVENT_NAME: &str = "Local\\BlackRackOverlayUpdateActive";
#[cfg(windows)]
const UPDATE_COORDINATOR_READY_EVENT_NAME: &str = "Local\\BlackRackOverlayUpdateCoordinatorReady";

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct UpdateInfo {
    pub version: String,
    pub release_page_url: Option<String>,
    pub update_title: Option<String>,
    pub full_title: Option<String>,
    pub changelog: Vec<String>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct UpdateCheckResponse {
    pub current_version: String,
    pub available: Option<UpdateInfo>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct UpdateProgress {
    pub stage: &'static str,
    pub downloaded_bytes: u64,
    pub total_bytes: Option<u64>,
    pub percent: Option<u8>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateManifest {
    schema_version: u32,
    version: String,
    package_url: String,
    sha256: String,
    file_name: String,
    #[serde(default)]
    release_page_url: Option<String>,
    #[serde(default)]
    update_title: Option<String>,
    #[serde(default)]
    full_title: Option<String>,
    #[serde(default)]
    changelog: Option<ManifestChangelog>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum ManifestChangelog {
    Text(String),
    Items(Vec<String>),
}

impl ManifestChangelog {
    fn into_lines(self) -> Vec<String> {
        match self {
            Self::Text(value) => value
                .lines()
                .map(str::trim)
                .filter(|line| !line.is_empty())
                .map(str::to_owned)
                .collect(),
            Self::Items(values) => values
                .into_iter()
                .map(|value| value.trim().to_owned())
                .filter(|value| !value.is_empty())
                .collect(),
        }
    }
}

#[tauri::command]
pub(crate) async fn check_for_update(
    window: tauri::WebviewWindow,
) -> Result<UpdateCheckResponse, String> {
    crate::require_control_window(&window)?;

    #[cfg(windows)]
    {
        let response = tauri::async_runtime::spawn_blocking(|| {
            let manifest = fetch_manifest()?;
            Ok::<_, String>(UpdateCheckResponse {
                current_version: env!("CARGO_PKG_VERSION").to_owned(),
                available: is_newer_version(&manifest.version, env!("CARGO_PKG_VERSION"))
                    .then(|| update_info(&manifest)),
            })
        })
        .await
        .map_err(|_| "update_worker_failed")??;
        return Ok(response);
    }

    #[cfg(not(windows))]
    {
        Err("updates_windows_only".into())
    }
}

#[tauri::command]
pub(crate) async fn download_and_install_update(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
) -> Result<(), String> {
    crate::require_control_window(&window)?;

    #[cfg(windows)]
    {
        let worker_app = app.clone();
        tauri::async_runtime::spawn_blocking(move || {
            let _update_lock = UpdateLock::acquire()?;
            let manifest = fetch_manifest()?;
            if !is_newer_version(&manifest.version, env!("CARGO_PKG_VERSION")) {
                return Err("update_not_available".into());
            }

            let installer = download_installer(&worker_app, &manifest)?;
            emit_progress(
                &worker_app,
                UpdateProgress {
                    stage: "installing",
                    downloaded_bytes: 0,
                    total_bytes: None,
                    percent: None,
                },
            );
            spawn_update_helper(installer, &manifest.sha256)
        })
        .await
        .map_err(|_| "update_worker_failed")??;
        app.exit(0);
        return Ok(());
    }

    #[cfg(not(windows))]
    {
        let _ = app;
        Err("updates_windows_only".into())
    }
}

#[tauri::command]
pub(crate) fn get_update_status(window: tauri::WebviewWindow) -> Result<Option<String>, String> {
    crate::require_control_window(&window)?;

    #[cfg(windows)]
    {
        let path = update_failure_path();
        let status = std::fs::read_to_string(&path)
            .ok()
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty());
        if status.is_some() {
            let _ = std::fs::remove_file(path);
        }
        return Ok(status);
    }

    #[cfg(not(windows))]
    {
        Ok(None)
    }
}

#[cfg(windows)]
fn http_client() -> Result<reqwest::blocking::Client, String> {
    reqwest::blocking::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(5))
        .timeout(std::time::Duration::from_secs(30))
        .redirect(secure_redirect_policy())
        .user_agent(format!("BlackRack-Overlay/{}", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|_| "update_client_failed".into())
}

#[cfg(windows)]
fn secure_redirect_policy() -> reqwest::redirect::Policy {
    reqwest::redirect::Policy::custom(|attempt| {
        if is_secure_url(attempt.url()) {
            attempt.follow()
        } else {
            attempt.stop()
        }
    })
}

#[cfg(windows)]
fn fetch_manifest() -> Result<UpdateManifest, String> {
    let manifest_url = validate_https_url(UPDATE_MANIFEST_URL)?;
    let response = http_client()?
        .get(manifest_url)
        .send()
        .map_err(|_| "update_manifest_unavailable")?
        .error_for_status()
        .map_err(|_| "update_manifest_unavailable")?;

    if response
        .content_length()
        .is_some_and(|size| size > MAX_MANIFEST_BYTES)
    {
        return Err("update_manifest_invalid".into());
    }
    let body = read_limited_manifest(response)?;

    let manifest: UpdateManifest =
        serde_json::from_slice(&body).map_err(|_| "update_manifest_invalid")?;
    validate_manifest(&manifest)?;
    Ok(manifest)
}

fn read_limited_manifest<R: std::io::Read>(reader: R) -> Result<Vec<u8>, String> {
    use std::io::Read;

    let mut body = Vec::new();
    reader
        .take(MAX_MANIFEST_BYTES.saturating_add(1))
        .read_to_end(&mut body)
        .map_err(|_| "update_manifest_unavailable")?;
    if body.len() as u64 > MAX_MANIFEST_BYTES {
        return Err("update_manifest_invalid".into());
    }
    Ok(body)
}

#[cfg(windows)]
fn validate_manifest(manifest: &UpdateManifest) -> Result<(), String> {
    if manifest.schema_version != UPDATE_SCHEMA_VERSION
        || parse_version(&manifest.version).is_none()
        || !is_sha256(&manifest.sha256)
        || !is_safe_installer_name(&manifest.file_name)
    {
        return Err("update_manifest_invalid".into());
    }

    validate_https_url(&manifest.package_url)?;
    if let Some(url) = &manifest.release_page_url {
        validate_https_url(url)?;
    }
    Ok(())
}

#[cfg(windows)]
fn validate_https_url(value: &str) -> Result<reqwest::Url, String> {
    let url = reqwest::Url::parse(value).map_err(|_| "update_manifest_invalid")?;
    if !is_secure_url(&url) {
        return Err("update_manifest_invalid".into());
    }
    Ok(url)
}

#[cfg(windows)]
fn is_secure_url(url: &reqwest::Url) -> bool {
    url.scheme() == "https"
        && url.host_str().is_some()
        && url.username().is_empty()
        && url.password().is_none()
        && url.fragment().is_none()
}

#[cfg(windows)]
fn is_safe_installer_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value.to_ascii_lowercase().ends_with(".exe")
        && !value.contains(['/', '\\', ':'])
}

#[cfg(windows)]
fn is_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

#[cfg(windows)]
fn download_installer(
    app: &tauri::AppHandle,
    manifest: &UpdateManifest,
) -> Result<PathBuf, String> {
    use reqwest::Url;
    use sha2::{Digest, Sha256};
    use std::fs::{self, File};
    use std::io::{Read, Write};

    let package_url = validate_https_url(&manifest.package_url)?;
    let update_directory = crate::app_paths::data_directory().join(UPDATE_DIRECTORY_NAME);
    let operation_directory = create_update_operation_directory(&update_directory)?;
    let partial_path = operation_directory.join("installer.exe.part");
    let installer_path = operation_directory.join(UPDATE_INSTALLER_NAME);

    let mut response = http_client()?
        .get(Url::clone(&package_url))
        .send()
        .map_err(|_| "update_download_failed")?
        .error_for_status()
        .map_err(|_| "update_download_failed")?;
    let total_bytes = response.content_length();
    let mut file = File::create(&partial_path).map_err(|_| "update_storage_failed")?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    let mut downloaded_bytes = 0_u64;
    let mut last_progress = 0_u64;

    emit_progress(
        app,
        UpdateProgress {
            stage: "downloading",
            downloaded_bytes,
            total_bytes,
            percent: progress_percent(downloaded_bytes, total_bytes),
        },
    );

    loop {
        let read = response
            .read(&mut buffer)
            .map_err(|_| "update_download_failed")?;
        if read == 0 {
            break;
        }
        file.write_all(&buffer[..read])
            .map_err(|_| "update_storage_failed")?;
        hasher.update(&buffer[..read]);
        downloaded_bytes = downloaded_bytes.saturating_add(read as u64);
        if downloaded_bytes.saturating_sub(last_progress) >= MAX_UPDATE_PROGRESS_INTERVAL_BYTES {
            last_progress = downloaded_bytes;
            emit_progress(
                app,
                UpdateProgress {
                    stage: "downloading",
                    downloaded_bytes,
                    total_bytes,
                    percent: progress_percent(downloaded_bytes, total_bytes),
                },
            );
        }
    }
    file.flush().map_err(|_| "update_storage_failed")?;
    drop(file);

    emit_progress(
        app,
        UpdateProgress {
            stage: "verifying",
            downloaded_bytes,
            total_bytes,
            percent: Some(100),
        },
    );
    let actual_hash = hex::encode(hasher.finalize());
    if !actual_hash.eq_ignore_ascii_case(&manifest.sha256) {
        let _ = fs::remove_file(&partial_path);
        return Err("update_hash_mismatch".into());
    }

    let _ = fs::remove_file(&installer_path);
    fs::rename(&partial_path, &installer_path).map_err(|_| "update_storage_failed")?;
    Ok(installer_path)
}

#[cfg(windows)]
fn create_update_operation_directory(parent: &PathBuf) -> Result<PathBuf, String> {
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fs::create_dir_all(parent).map_err(|_| "update_storage_failed")?;
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| "update_storage_failed")?
        .as_nanos();
    let directory = parent.join(format!(
        "{UPDATE_RUN_DIRECTORY_PREFIX}{}-{stamp}",
        std::process::id()
    ));
    fs::create_dir(&directory).map_err(|_| "update_storage_failed")?;
    Ok(directory)
}

#[cfg(windows)]
fn progress_percent(downloaded: u64, total: Option<u64>) -> Option<u8> {
    total
        .filter(|value| *value > 0)
        .map(|value| ((downloaded.saturating_mul(100) / value).min(100)) as u8)
}

#[cfg(windows)]
fn emit_progress(app: &tauri::AppHandle, progress: UpdateProgress) {
    use tauri::Emitter;
    let _ = app.emit("update://progress", progress);
}

#[cfg(windows)]
struct UpdateLock {
    handle: windows_sys::Win32::Foundation::HANDLE,
}

#[cfg(windows)]
impl UpdateLock {
    fn acquire() -> Result<Self, String> {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::System::Threading::{CreateMutexW, WaitForSingleObject};

        let name: Vec<u16> = std::ffi::OsStr::new(UPDATE_MUTEX_NAME)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        let handle = unsafe { CreateMutexW(std::ptr::null(), 0, name.as_ptr()) };
        if handle.is_null() {
            return Err("update_lock_failed".into());
        }

        let wait_result = unsafe { WaitForSingleObject(handle, u32::MAX) };
        if wait_result != 0 && wait_result != 0x80 {
            unsafe {
                windows_sys::Win32::Foundation::CloseHandle(handle);
            }
            return Err("update_lock_failed".into());
        }
        Ok(Self { handle })
    }
}

#[cfg(windows)]
impl Drop for UpdateLock {
    fn drop(&mut self) {
        unsafe {
            windows_sys::Win32::System::Threading::ReleaseMutex(self.handle);
            windows_sys::Win32::Foundation::CloseHandle(self.handle);
        }
    }
}

#[cfg(windows)]
fn named_event(name: &str) -> Result<windows_sys::Win32::Foundation::HANDLE, String> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::System::Threading::CreateEventW;

    let name: Vec<u16> = std::ffi::OsStr::new(name)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let handle = unsafe { CreateEventW(std::ptr::null(), 0, 0, name.as_ptr()) };
    if handle.is_null() {
        return Err("update_event_failed".into());
    }
    Ok(handle)
}

#[cfg(windows)]
fn open_named_event(name: &str) -> Result<windows_sys::Win32::Foundation::HANDLE, String> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::System::Threading::OpenEventW;

    let name: Vec<u16> = std::ffi::OsStr::new(name)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let handle = unsafe { OpenEventW(0x001F_0000 | 0x0002, 0, name.as_ptr()) };
    if handle.is_null() {
        return Err("update_event_unavailable".into());
    }
    Ok(handle)
}

#[cfg(windows)]
fn close_handle(handle: windows_sys::Win32::Foundation::HANDLE) {
    unsafe {
        windows_sys::Win32::Foundation::CloseHandle(handle);
    }
}

#[cfg(windows)]
fn ensure_no_active_update() -> Result<(), String> {
    use std::thread;
    use std::time::Duration;

    for _ in 0..20 {
        match open_named_event(UPDATE_ACTIVE_EVENT_NAME) {
            Ok(handle) => {
                let wait_result = unsafe {
                    windows_sys::Win32::System::Threading::WaitForSingleObject(handle, 0)
                };
                close_handle(handle);
                if wait_result == 0 {
                    thread::sleep(Duration::from_millis(100));
                    continue;
                }
                return Err("update_in_progress".into());
            }
            Err(_) => return Ok(()),
        }
    }
    Err("update_in_progress".into())
}

#[cfg(windows)]
fn spawn_update_helper(installer: PathBuf, expected_hash: &str) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    use std::process::Command;
    use windows_sys::Win32::System::Threading::{CREATE_NEW_PROCESS_GROUP, DETACHED_PROCESS};

    let current_exe = std::env::current_exe().map_err(|_| "update_helper_failed")?;
    let pid = std::process::id().to_string();
    let installer_arg = installer.to_string_lossy().into_owned();
    Command::new(current_exe)
        .args([
            "--blackrack-update-helper",
            "--pid",
            pid.as_str(),
            "--installer",
            installer_arg.as_str(),
            "--sha256",
            expected_hash,
        ])
        .creation_flags(CREATE_NEW_PROCESS_GROUP | DETACHED_PROCESS)
        .spawn()
        .map(|_| ())
        .map_err(|_| "update_helper_failed".into())
}

pub(crate) fn run_helper_if_requested() -> bool {
    #[cfg(windows)]
    {
        let args: Vec<String> = std::env::args().collect();
        if args
            .iter()
            .any(|arg| arg == "--blackrack-update-coordinator")
        {
            if let Err(error) = run_update_coordinator(&args) {
                record_helper_failure(&error);
                eprintln!("BlackRack update coordinator failed: {error}");
            }
            return true;
        }
        if !args.iter().any(|arg| arg == "--blackrack-update-helper") {
            return false;
        }
        if let Err(error) = run_helper(&args) {
            record_helper_failure(&error);
            eprintln!("BlackRack update helper failed: {error}");
        }
        return true;
    }

    #[cfg(not(windows))]
    {
        false
    }
}

#[cfg(windows)]
fn update_failure_path() -> PathBuf {
    crate::app_paths::data_directory()
        .join(UPDATE_DIRECTORY_NAME)
        .join(UPDATE_FAILURE_FILE_NAME)
}

#[cfg(windows)]
fn record_helper_failure(error: &str) {
    use std::fs;

    let path = update_failure_path();
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let _ = fs::write(path, error);
}

#[cfg(windows)]
fn run_helper(args: &[String]) -> Result<(), String> {
    use std::fs;
    use std::os::windows::process::CommandExt;
    use std::process::Command;
    use windows_sys::Win32::System::Threading::{CREATE_NEW_PROCESS_GROUP, DETACHED_PROCESS};

    let value_for = |name: &str| {
        args.windows(2)
            .find(|pair| pair[0] == name)
            .map(|pair| pair[1].clone())
    };
    let pid = value_for("--pid")
        .and_then(|value| value.parse::<u32>().ok())
        .filter(|value| *value != 0)
        .ok_or("update_helper_arguments")?;
    let installer = PathBuf::from(value_for("--installer").ok_or("update_helper_arguments")?);
    let expected_hash = value_for("--sha256").ok_or("update_helper_arguments")?;
    if !is_sha256(&expected_hash) {
        return Err("update_helper_arguments".into());
    }

    // Open and wait on the parent before taking the update lock. This captures
    // the original process object while it is still alive, so a later PID reuse
    // cannot make the update wait on an unrelated process.
    match wait_for_process(
        pid,
        windows_sys::Win32::System::Threading::PROCESS_SYNCHRONIZE,
    ) {
        Ok(parent_process) => close_handle(parent_process),
        Err(error) if error == "update_process_gone" => {}
        Err(error) => return Err(error),
    }

    let _update_lock = UpdateLock::acquire()?;
    ensure_no_active_update()?;
    let active_event = named_event(UPDATE_ACTIVE_EVENT_NAME)?;
    let ready_event = named_event(UPDATE_COORDINATOR_READY_EVENT_NAME)?;
    let (_, operation_directory) = validate_installer_path(&installer)?;
    let current_exe = std::env::current_exe().map_err(|_| "update_coordinator_failed")?;
    let coordinator_path = operation_directory.join(UPDATE_COORDINATOR_NAME);
    fs::copy(&current_exe, &coordinator_path).map_err(|_| "update_coordinator_failed")?;
    let installer_arg = installer.to_string_lossy().into_owned();
    let mut coordinator_process = Command::new(&coordinator_path)
        .args([
            "--blackrack-update-coordinator",
            "--installer",
            installer_arg.as_str(),
            "--sha256",
            expected_hash.as_str(),
        ])
        .creation_flags(CREATE_NEW_PROCESS_GROUP | DETACHED_PROCESS)
        .spawn()
        .map_err(|_| "update_coordinator_failed")?;

    let coordinator_ready =
        unsafe { windows_sys::Win32::System::Threading::WaitForSingleObject(ready_event, 5_000) };
    close_handle(ready_event);
    close_handle(active_event);
    if coordinator_ready != 0 {
        let _ = coordinator_process.kill();
        let _ = coordinator_process.wait();
        return Err("update_coordinator_failed".into());
    }
    Ok(())
}

#[cfg(windows)]
fn validate_installer_path(installer: &Path) -> Result<(PathBuf, PathBuf), String> {
    let update_directory = crate::app_paths::data_directory().join(UPDATE_DIRECTORY_NAME);
    let canonical_directory = update_directory
        .canonicalize()
        .map_err(|_| "update_helper_storage")?;
    let canonical_installer = installer
        .canonicalize()
        .map_err(|_| "update_helper_storage")?;
    let canonical_operation_directory = canonical_installer
        .parent()
        .ok_or("update_helper_storage")?;
    if canonical_operation_directory.parent() != Some(canonical_directory.as_path())
        || !canonical_operation_directory
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with(UPDATE_RUN_DIRECTORY_PREFIX))
        || canonical_installer
            .file_name()
            .and_then(|name| name.to_str())
            != Some(UPDATE_INSTALLER_NAME)
    {
        return Err("update_helper_storage".into());
    }
    let operation_directory = canonical_operation_directory.to_path_buf();
    Ok((canonical_installer, operation_directory))
}

#[cfg(windows)]
fn wait_for_process(
    pid: u32,
    access: u32,
) -> Result<windows_sys::Win32::Foundation::HANDLE, String> {
    let process = open_process(pid, access)?;
    let wait_result =
        unsafe { windows_sys::Win32::System::Threading::WaitForSingleObject(process, u32::MAX) };
    if wait_result == 0 || wait_result == 0x80 {
        return Ok(process);
    }
    close_handle(process);
    Err("update_process_wait_failed".into())
}

#[cfg(windows)]
fn open_process(pid: u32, access: u32) -> Result<windows_sys::Win32::Foundation::HANDLE, String> {
    use std::thread;
    use std::time::Duration;
    use windows_sys::Win32::Foundation::GetLastError;
    use windows_sys::Win32::System::Threading::OpenProcess;

    for _ in 0..20 {
        let process = unsafe { OpenProcess(access, 0, pid) };
        if !process.is_null() {
            return Ok(process);
        }
        if unsafe { GetLastError() } == 87 {
            return Err("update_process_gone".into());
        }
        thread::sleep(Duration::from_millis(100));
    }
    Err("update_process_unavailable".into())
}

#[cfg(windows)]
struct ActiveUpdateEvent {
    handle: windows_sys::Win32::Foundation::HANDLE,
}

#[cfg(windows)]
impl Drop for ActiveUpdateEvent {
    fn drop(&mut self) {
        unsafe {
            windows_sys::Win32::System::Threading::SetEvent(self.handle);
        }
        close_handle(self.handle);
    }
}

#[cfg(windows)]
fn run_update_coordinator(args: &[String]) -> Result<(), String> {
    use sha2::{Digest, Sha256};
    use std::fs::OpenOptions;
    use std::io::Read;
    use std::os::windows::fs::OpenOptionsExt;
    use std::os::windows::process::CommandExt;
    use std::process::Command;
    use windows_sys::Win32::System::Threading::{CREATE_NEW_PROCESS_GROUP, DETACHED_PROCESS};

    let value_for = |name: &str| {
        args.windows(2)
            .find(|pair| pair[0] == name)
            .map(|pair| pair[1].clone())
    };
    let installer = PathBuf::from(value_for("--installer").ok_or("update_coordinator_arguments")?);
    let expected_hash = value_for("--sha256").ok_or("update_coordinator_arguments")?;
    if !is_sha256(&expected_hash) {
        return Err("update_coordinator_arguments".into());
    }

    let active_event = open_named_event(UPDATE_ACTIVE_EVENT_NAME)?;
    let _active_update = ActiveUpdateEvent {
        handle: active_event,
    };
    let ready_event = open_named_event(UPDATE_COORDINATOR_READY_EVENT_NAME)?;
    let ready = unsafe { windows_sys::Win32::System::Threading::SetEvent(ready_event) };
    close_handle(ready_event);
    if ready == 0 {
        return Err("update_event_failed".into());
    }

    let (canonical_installer, _) = validate_installer_path(&installer)?;
    let mut file = OpenOptions::new()
        .read(true)
        // Keep the verified file stable until CreateProcess has opened it.
        // FILE_SHARE_READ deliberately excludes write and delete sharing.
        .share_mode(0x0000_0001)
        .open(&canonical_installer)
        .map_err(|_| "update_helper_storage")?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|_| "update_helper_storage")?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    if !hex::encode(hasher.finalize()).eq_ignore_ascii_case(&expected_hash) {
        return Err("update_hash_mismatch".into());
    }

    let mut installer_process = Command::new(&canonical_installer)
        .args(["/UPDATE", "/P", "/R"])
        .creation_flags(CREATE_NEW_PROCESS_GROUP | DETACHED_PROCESS)
        .spawn()
        .map_err(|_| "update_installer_failed")?;
    drop(file);
    let status = installer_process
        .wait()
        .map_err(|_| "update_installer_wait_failed")?;
    if !status.success() {
        return Err(format!(
            "update_installer_exit_{}",
            status.code().unwrap_or(-1)
        ));
    }
    Ok(())
}

fn parse_version(value: &str) -> Option<[u64; 4]> {
    let parts: Vec<u64> = value
        .split('.')
        .map(str::parse)
        .collect::<Result<_, _>>()
        .ok()?;
    if parts.is_empty() || parts.len() > 4 {
        return None;
    }
    let mut normalized = [0_u64; 4];
    normalized[..parts.len()].copy_from_slice(&parts);
    Some(normalized)
}

fn is_newer_version(candidate: &str, current: &str) -> bool {
    parse_version(candidate)
        .zip(parse_version(current))
        .is_some_and(|(candidate, current)| candidate > current)
}

#[cfg(windows)]
fn update_info(manifest: &UpdateManifest) -> UpdateInfo {
    UpdateInfo {
        version: manifest.version.clone(),
        release_page_url: manifest.release_page_url.clone(),
        update_title: manifest.update_title.clone(),
        full_title: manifest.full_title.clone(),
        changelog: manifest
            .changelog
            .as_ref()
            .map(|value| match value {
                ManifestChangelog::Text(text) => ManifestChangelog::Text(text.clone()),
                ManifestChangelog::Items(items) => ManifestChangelog::Items(items.clone()),
            })
            .map(ManifestChangelog::into_lines)
            .unwrap_or_default(),
    }
}

#[cfg(test)]
mod tests {
    use super::{is_newer_version, parse_version, read_limited_manifest, MAX_MANIFEST_BYTES};

    #[test]
    fn versions_compare_as_numeric_components() {
        assert!(is_newer_version("0.7.9", "0.7.8"));
        assert!(is_newer_version("1.0", "0.99.99"));
        assert!(!is_newer_version("0.7.8", "0.7.8"));
        assert!(!is_newer_version("0.7.7", "0.7.8"));
    }

    #[test]
    fn malformed_versions_are_rejected() {
        assert!(parse_version("0.7.beta").is_none());
        assert!(parse_version("1.2.3.4.5").is_none());
        assert!(parse_version("").is_none());
    }

    #[test]
    fn manifest_body_is_bounded_without_content_length() {
        let body = vec![0_u8; MAX_MANIFEST_BYTES as usize + 1];
        assert_eq!(
            read_limited_manifest(std::io::Cursor::new(body)).unwrap_err(),
            "update_manifest_invalid"
        );
    }
}
