use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::{self, File, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

static ENABLED: AtomicBool = AtomicBool::new(false);
static DIRECTORY: OnceLock<PathBuf> = OnceLock::new();
static SETTINGS_PATH: OnceLock<PathBuf> = OnceLock::new();
static RUN_FILE: OnceLock<PathBuf> = OnceLock::new();
static ACTIVE_FILE: Mutex<Option<PathBuf>> = Mutex::new(None);
static EVENTS: Mutex<Vec<serde_json::Value>> = Mutex::new(Vec::new());
static LAST_SIGNATURES: Mutex<Option<HashMap<String, serde_json::Value>>> = Mutex::new(None);

#[derive(Clone, Deserialize, Serialize)]
struct Preferences {
    enabled: bool,
}

#[derive(Clone, Serialize)]
pub(crate) struct DriverRankEstimateLoggingStatus {
    enabled: bool,
    directory: String,
    active_file: Option<String>,
}

pub(super) fn configure(app_data_directory: &Path) {
    let directory = app_data_directory.join("dr-estimate-logs");
    let settings = app_data_directory.join("dr-estimate-logging.json");
    let enabled = fs::read(&settings)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Preferences>(&bytes).ok())
        .map(|preferences| preferences.enabled)
        .unwrap_or(false);
    let _ = DIRECTORY.set(directory);
    let _ = SETTINGS_PATH.set(settings);
    ENABLED.store(enabled, Ordering::Relaxed);
    if let Some(directory) = DIRECTORY.get() {
        if fs::create_dir_all(directory).is_ok() {
            // Un solo fichero por ejecución de la aplicación. Volver a activar
            // el registro continúa ese log, pero los arranques anteriores se conservan.
            let started_ms = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis();
            for suffix in 0_u16..=u16::MAX {
                let suffix = if suffix == 0 {
                    String::new()
                } else {
                    format!("-{suffix}")
                };
                let path = directory.join(format!(
                    "dr-estimate-{started_ms}-{}{suffix}.jsonl",
                    std::process::id()
                ));
                match OpenOptions::new().write(true).create_new(true).open(&path) {
                    Ok(_) => {
                        let _ = RUN_FILE.set(path);
                        break;
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                    Err(_) => break,
                }
            }
        }
    }
}

pub(crate) fn status() -> DriverRankEstimateLoggingStatus {
    let active_file = ACTIVE_FILE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .as_ref()
        .map(|path| path.display().to_string());
    DriverRankEstimateLoggingStatus {
        enabled: ENABLED.load(Ordering::Relaxed),
        directory: DIRECTORY
            .get()
            .map(|path| path.display().to_string())
            .unwrap_or_default(),
        active_file,
    }
}

pub(crate) fn set_enabled(enabled: bool) -> Result<DriverRankEstimateLoggingStatus, String> {
    ENABLED.store(enabled, Ordering::Relaxed);
    if !enabled {
        *LAST_SIGNATURES
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = None;
        EVENTS
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clear();
    }
    if let Some(path) = SETTINGS_PATH.get() {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        let bytes = serde_json::to_vec_pretty(&Preferences { enabled })
            .map_err(|error| error.to_string())?;
        fs::write(path, bytes).map_err(|error| error.to_string())?;
    }
    Ok(status())
}

pub(super) fn enabled() -> bool {
    ENABLED.load(Ordering::Relaxed)
}

pub(super) fn queue(event: serde_json::Value) {
    if !enabled() {
        return;
    }

    let signature = normalized_signature(&event);
    let signature_key = event
        .get("event")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("unknown")
        .to_owned();
    let mut last_signatures = LAST_SIGNATURES
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let signatures = last_signatures.get_or_insert_with(HashMap::new);
    if signatures.get(&signature_key) == Some(&signature) {
        return;
    }
    signatures.insert(signature_key, signature);
    EVENTS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .push(event);
}

fn normalized_signature(value: &serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Array(values) => {
            serde_json::Value::Array(values.iter().map(normalized_signature).collect())
        }
        serde_json::Value::Object(object) => serde_json::Value::Object(
            object
                .iter()
                .map(|(key, value)| (key.clone(), normalized_signature(value)))
                .collect(),
        ),
        serde_json::Value::Number(number) if number.is_f64() => number
            .as_f64()
            .and_then(|value| serde_json::Number::from_f64((value * 1_000.0).round() / 1_000.0))
            .map(serde_json::Value::Number)
            .unwrap_or_else(|| value.clone()),
        _ => value.clone(),
    }
}

fn take_events() -> Vec<serde_json::Value> {
    std::mem::take(
        &mut *EVENTS
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()),
    )
}

pub(super) struct DriverRankEstimateLogger {
    writer: Option<BufWriter<File>>,
}

impl DriverRankEstimateLogger {
    pub(super) fn new() -> Self {
        Self { writer: None }
    }

    pub(super) fn record(&mut self) {
        self.sync();
        let Some(writer) = self.writer.as_mut() else {
            return;
        };
        let mut wrote = false;
        for event in take_events() {
            let timestamp_ms = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64;
            let mut entry = event;
            if let Some(object) = entry.as_object_mut() {
                object.insert("timestamp_ms".into(), timestamp_ms.into());
            }
            wrote |=
                serde_json::to_writer(&mut *writer, &entry).is_ok() && writeln!(writer).is_ok();
        }
        if wrote {
            let _ = writer.flush();
        }
    }

    fn sync(&mut self) {
        if enabled() {
            if self.writer.is_none() {
                self.open();
            }
        } else if let Some(mut writer) = self.writer.take() {
            let _ = writer.flush();
            *ACTIVE_FILE
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner()) = None;
        }
    }

    fn open(&mut self) {
        let Some(path) = RUN_FILE.get().cloned() else {
            return;
        };
        let Some(directory) = path.parent() else {
            return;
        };
        if fs::create_dir_all(directory).is_err() {
            return;
        }
        let Ok(file) = OpenOptions::new().create(true).append(true).open(&path) else {
            return;
        };
        self.writer = Some(BufWriter::new(file));
        *ACTIVE_FILE
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(path);
    }
}

#[cfg(test)]
mod tests {
    use super::{configure, queue, set_enabled, DriverRankEstimateLogger};
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn writes_only_queued_driver_rank_estimate_events() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let app_data = std::env::temp_dir().join(format!(
            "blackrack-overlay-dr-estimate-log-{}-{unique}",
            std::process::id()
        ));
        configure(&app_data);
        set_enabled(true).unwrap();
        queue(serde_json::json!({
            "event": "driver_rank_estimate_sample",
            "status": "estimated",
            "calculation": { "estimated_gain": 1.2344 }
        }));
        queue(serde_json::json!({
            "event": "driver_rank_race_final",
            "status": "final"
        }));
        queue(serde_json::json!({
            "event": "driver_rank_estimate_sample",
            "status": "estimated",
            "calculation": { "estimated_gain": 1.23449 }
        }));
        queue(serde_json::json!({
            "event": "driver_rank_estimate_sample",
            "status": "estimated",
            "calculation": { "estimated_gain": 1.236 }
        }));

        let mut logger = DriverRankEstimateLogger::new();
        logger.record();
        set_enabled(false).unwrap();
        logger.record();
        set_enabled(true).unwrap();
        queue(serde_json::json!({
            "event": "driver_rank_estimate_sample",
            "status": "estimated_after_reenable"
        }));
        logger.record();
        set_enabled(false).unwrap();
        logger.record();

        let files = fs::read_dir(app_data.join("dr-estimate-logs"))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(files.len(), 1);
        let path = files[0].path();
        let file_name = path.file_name().unwrap().to_string_lossy();
        assert!(file_name.starts_with("dr-estimate-"));
        assert!(file_name.ends_with(".jsonl"));
        assert_ne!(file_name, "dr-estimate.jsonl");
        let contents = fs::read_to_string(path).unwrap();
        let entries = contents.lines().collect::<Vec<_>>();
        assert_eq!(entries.len(), 4);
        let entry: serde_json::Value = serde_json::from_str(entries[0]).unwrap();
        assert_eq!(entry["event"], "driver_rank_estimate_sample");
        assert_eq!(entry["status"], "estimated");
        assert!(entry["timestamp_ms"].is_u64());
        let final_entry: serde_json::Value = serde_json::from_str(entries[1]).unwrap();
        assert_eq!(final_entry["event"], "driver_rank_race_final");
        let changed: serde_json::Value = serde_json::from_str(entries[2]).unwrap();
        assert_eq!(changed["calculation"]["estimated_gain"], 1.236);
        let reenabled: serde_json::Value = serde_json::from_str(entries[3]).unwrap();
        assert_eq!(reenabled["status"], "estimated_after_reenable");

        let _ = fs::remove_dir_all(app_data);
    }
}
