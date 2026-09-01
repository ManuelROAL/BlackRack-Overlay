use std::backtrace::Backtrace;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

static LOG_FILE: OnceLock<Mutex<File>> = OnceLock::new();
const LOG_DIRECTORY_NAME: &str = "diagnostics";
const MAX_SESSION_LOGS: usize = 20;
const REDACTED: &str = "[redacted]";

fn timestamp_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default()
}

fn log_directory() -> PathBuf {
    crate::app_paths::data_directory().join(LOG_DIRECTORY_NAME)
}

fn session_log_path(directory: &Path, started_at: u128, pid: u32, suffix: usize) -> PathBuf {
    let suffix = if suffix == 0 {
        String::new()
    } else {
        format!("-{suffix}")
    };
    directory.join(format!("startup-{started_at}-{pid}{suffix}.log"))
}

fn prune_session_logs(directory: &Path, keep: usize) {
    let Ok(entries) = fs::read_dir(directory) else {
        return;
    };
    let mut logs = entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let path = entry.path();
            let name = path.file_name()?.to_str()?;
            (path.is_file() && name.starts_with("startup-") && name.ends_with(".log"))
                .then_some(path)
        })
        .collect::<Vec<_>>();
    logs.sort();
    let remove_count = logs.len().saturating_sub(keep);
    for path in logs.into_iter().take(remove_count) {
        let _ = fs::remove_file(path);
    }
}

fn create_session_log(directory: &Path, started_at: u128, pid: u32) -> (PathBuf, Option<File>) {
    for suffix in 0..100 {
        let path = session_log_path(directory, started_at, pid, suffix);
        match OpenOptions::new().create_new(true).write(true).open(&path) {
            Ok(file) => return (path, Some(file)),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(_) => return (path, None),
        }
    }
    (session_log_path(directory, started_at, pid, 100), None)
}

pub(crate) fn initialize() -> PathBuf {
    let directory = log_directory();
    let _ = fs::create_dir_all(&directory);
    prune_session_logs(&directory, MAX_SESSION_LOGS.saturating_sub(1));

    let started_at = timestamp_millis();
    let pid = std::process::id();
    let (path, file) = create_session_log(&directory, started_at, pid);
    if let Some(file) = file {
        let _ = LOG_FILE.set(Mutex::new(file));
    }
    record(format!(
        "session begin version={} pid={} os={} arch={}",
        env!("CARGO_PKG_VERSION"),
        pid,
        std::env::consts::OS,
        std::env::consts::ARCH
    ));

    let previous_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let cause = info
            .payload()
            .downcast_ref::<&str>()
            .copied()
            .or_else(|| info.payload().downcast_ref::<String>().map(String::as_str))
            .unwrap_or("non-string panic payload");
        let location = info
            .location()
            .map(|location| {
                format!(
                    "{}:{}:{}",
                    location.file(),
                    location.line(),
                    location.column()
                )
            })
            .unwrap_or_else(|| "unknown".into());
        let backtrace = Backtrace::force_capture().to_string();
        record(format!(
            "session end status=crashed cause={} location={} backtrace={}",
            sanitize(cause, 4_000),
            sanitize(&location, 1_000),
            sanitize(&backtrace, 32_000)
        ));
        previous_hook(info);
    }));

    path
}

pub(crate) fn record(message: impl AsRef<str>) {
    let Some(file) = LOG_FILE.get() else {
        return;
    };
    let mut file = file.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let _ = writeln!(file, "{} {}", timestamp_millis(), message.as_ref());
    let _ = file.flush();
}

fn truncate_chars(value: &str, maximum: usize) -> String {
    value.chars().take(maximum).collect()
}

fn redact_marker(mut value: String, marker: &str) -> String {
    let mut search_from = 0;
    loop {
        let lowercase = value.to_ascii_lowercase();
        let Some(relative_start) = lowercase[search_from..].find(marker) else {
            return value;
        };
        let secret_start = search_from + relative_start + marker.len();
        if secret_start >= value.len() {
            return value;
        }
        if value[secret_start..].starts_with(REDACTED) {
            search_from = secret_start + REDACTED.len();
            continue;
        }
        let secret_end = value[secret_start..]
            .char_indices()
            .find_map(|(offset, character)| {
                (character.is_whitespace()
                    || matches!(character, '&' | ',' | ';' | '\"' | '\'' | '\\'))
                .then_some(secret_start + offset)
            })
            .unwrap_or(value.len());
        if secret_end == secret_start {
            search_from = secret_start + value[secret_start..].chars().next().unwrap().len_utf8();
            continue;
        }
        value.replace_range(secret_start..secret_end, REDACTED);
        search_from = secret_start + REDACTED.len();
    }
}

fn sanitize(value: &str, maximum: usize) -> String {
    let mut sanitized = truncate_chars(value, maximum)
        .replace('\r', "\\r")
        .replace('\n', "\\n");
    for marker in [
        "authorization: bearer ",
        "access_token=",
        "access_token\":\"",
        "ticket=",
        "ticket\":\"",
        "token=",
        "token\":\"",
        "bearer ",
    ] {
        sanitized = redact_marker(sanitized, marker);
    }
    sanitized
}

pub(crate) fn record_frontend_error(source: &str, kind: &str, message: &str, stack: Option<&str>) {
    let source = sanitize(source, 80);
    let kind = sanitize(kind, 32);
    let message = sanitize(message, 4_000);
    let stack = stack
        .filter(|value| !value.is_empty())
        .map(|value| format!(" stack={}", sanitize(value, 8_000)))
        .unwrap_or_default();
    record(format!(
        "frontend_error source={source} kind={kind} message={message}{stack}"
    ));
}

#[cfg(test)]
mod tests {
    use super::{prune_session_logs, sanitize, session_log_path};
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn session_logs_are_unique_and_never_replace_the_previous_run() {
        let directory = std::path::Path::new("diagnostics");
        assert_ne!(
            session_log_path(directory, 123, 456, 0),
            session_log_path(directory, 123, 456, 1)
        );
        assert_eq!(
            session_log_path(directory, 123, 456, 0),
            directory.join("startup-123-456.log")
        );
    }

    #[test]
    fn retention_keeps_the_newest_session_logs() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory = std::env::temp_dir().join(format!("blackrack-overlay-log-{unique}"));
        fs::create_dir_all(&directory).unwrap();
        for timestamp in 1..=4 {
            fs::write(
                directory.join(format!("startup-{timestamp:02}-1.log")),
                timestamp.to_string(),
            )
            .unwrap();
        }

        prune_session_logs(&directory, 2);

        let mut remaining = fs::read_dir(&directory)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        remaining.sort();
        assert_eq!(remaining, ["startup-03-1.log", "startup-04-1.log"]);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn frontend_diagnostics_redact_credentials_and_escape_lines() {
        let value = sanitize(
            "request ticket=secret-ticket&token=secret-token\nAuthorization: Bearer secret-auth",
            1_000,
        );

        assert_eq!(
            value,
            "request ticket=[redacted]&token=[redacted]\\nAuthorization: Bearer [redacted]"
        );
    }
}
