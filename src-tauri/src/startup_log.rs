use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

static LOG_FILE: OnceLock<Mutex<File>> = OnceLock::new();
const REDACTED: &str = "[redacted]";

fn timestamp_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default()
}

fn default_path() -> PathBuf {
    crate::app_paths::data_directory().join("startup.log")
}

fn previous_path(path: &std::path::Path) -> PathBuf {
    path.with_file_name("startup.previous.log")
}

fn preserve_previous(path: &std::path::Path) {
    if path.is_file() {
        let _ = fs::copy(path, previous_path(path));
    }
}

pub(crate) fn initialize() -> PathBuf {
    let path = default_path();
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    preserve_previous(&path);

    if let Ok(file) = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(&path)
    {
        let _ = LOG_FILE.set(Mutex::new(file));
    }
    record(format!(
        "startup begin version={} pid={} os={} arch={}",
        env!("CARGO_PKG_VERSION"),
        std::process::id(),
        std::env::consts::OS,
        std::env::consts::ARCH
    ));

    let previous_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        record(format!("panic: {info}"));
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
    use super::{preserve_previous, previous_path, sanitize};
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn preserves_the_previous_startup_log() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory =
            std::env::temp_dir().join(format!("blackrack-overlay-startup-log-{unique}"));
        fs::create_dir_all(&directory).unwrap();
        let path = directory.join("startup.log");
        fs::write(&path, "previous failure").unwrap();

        preserve_previous(&path);

        assert_eq!(
            fs::read_to_string(previous_path(&path)).unwrap(),
            "previous failure"
        );
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
