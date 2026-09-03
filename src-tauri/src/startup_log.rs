use std::backtrace::Backtrace;
use std::collections::HashMap;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

static LOG_FILE: OnceLock<Mutex<File>> = OnceLock::new();
const LOG_DIRECTORY_NAME: &str = "diagnostics";
const MAX_SESSION_LOGS: usize = 20;
const REDACTED: &str = "[redacted]";
/// A failing endpoint reports once when it breaks and once when it recovers; in
/// between it repeats at this cadence, so a request polled several times a
/// second cannot flood the log during an outage.
const FAILURE_REPORT_INTERVAL: Duration = Duration::from_secs(30);

static FAILING_REQUESTS: OnceLock<Mutex<HashMap<&'static str, EndpointFailure>>> = OnceLock::new();

struct EndpointFailure {
    consecutive: u32,
    last_reported: Instant,
}

fn failing_requests() -> &'static Mutex<HashMap<&'static str, EndpointFailure>> {
    FAILING_REQUESTS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Records that a request succeeded, reporting the recovery if the endpoint was
/// known to be failing. Silent otherwise: only transitions are worth a line.
pub(crate) fn record_request_success(endpoint: &'static str) {
    let removed = failing_requests()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .remove(endpoint);
    if let Some(failure) = removed {
        record(format!(
            "request_recovered endpoint={endpoint} after_failures={}",
            failure.consecutive
        ));
    }
}

/// Records that a request failed. The detail is redacted the way panics and
/// frontend errors are, because these endpoints carry bearer tokens and session
/// tickets that must never reach the log.
pub(crate) fn record_request_failure(endpoint: &'static str, error: &str) {
    let error = sanitize(error, 1_000);
    let mut failing = failing_requests()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    match failing.get_mut(endpoint) {
        Some(failure) => {
            failure.consecutive = failure.consecutive.saturating_add(1);
            if failure.last_reported.elapsed() >= FAILURE_REPORT_INTERVAL {
                failure.last_reported = Instant::now();
                let consecutive = failure.consecutive;
                drop(failing);
                record(format!(
                    "request_still_failing endpoint={endpoint} consecutive={consecutive} error={error}"
                ));
            }
        }
        None => {
            failing.insert(
                endpoint,
                EndpointFailure {
                    consecutive: 1,
                    last_reported: Instant::now(),
                },
            );
            drop(failing);
            record(format!("request_failed endpoint={endpoint} error={error}"));
        }
    }
}

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

/// Records the English detail behind a failure and returns the stable code that
/// travels to the webview. Command errors reach the user through the frontend
/// i18n catalogs, so backend `Result::Err` payloads carry a machine-readable
/// code and the diagnostics stay in this log rather than in the UI copy.
pub(crate) fn command_error(code: &'static str, detail: impl std::fmt::Display) -> String {
    record(format!("{code}: {detail}"));
    code.to_owned()
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

/// JSON Web Tokens are the one secret shape that carries no useful marker of its
/// own, and their `eyJ` prefix (a base64url `{"`) never appears in the symbol
/// names or paths that make up the rest of a backtrace.
fn redact_json_web_tokens(mut value: String) -> String {
    let is_token_char =
        |character: char| character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.');
    let mut search_from = 0;
    loop {
        let Some(relative_start) = value[search_from..].find("eyJ") else {
            return value;
        };
        let start = search_from + relative_start;
        let end = value[start..]
            .char_indices()
            .find_map(|(offset, character)| (!is_token_char(character)).then_some(start + offset))
            .unwrap_or(value.len());
        // A bare `eyJ` inside ordinary text is not a token; require the dotted
        // header.payload shape before redacting.
        if end - start >= 16 && value[start..end].contains('.') {
            value.replace_range(start..end, REDACTED);
            search_from = start + REDACTED.len();
        } else {
            search_from = end.max(start + 3);
        }
        if search_from >= value.len() {
            return value;
        }
    }
}

fn sanitize(value: &str, maximum: usize) -> String {
    let mut sanitized = truncate_chars(value, maximum)
        .replace('\r', "\\r")
        .replace('\n', "\\n");
    for marker in [
        "authorization: bearer ",
        "authorization: basic ",
        "access_token=",
        "access_token\":\"",
        "refresh_token=",
        "refresh_token\":\"",
        "ticket=",
        "ticket\":\"",
        "token=",
        "token\":\"",
        "password=",
        "password\":\"",
        "api_key=",
        "api_key\":\"",
        "apikey=",
        "apikey\":\"",
        "secret=",
        "secret\":\"",
        "bearer ",
    ] {
        sanitized = redact_marker(sanitized, marker);
    }
    redact_json_web_tokens(sanitized)
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

    #[test]
    fn credentials_without_a_known_marker_are_redacted_by_shape() {
        let value = sanitize(
            "POST failed with eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxIn0.dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1 attached",
            1_000,
        );

        assert_eq!(value, "POST failed with [redacted] attached");
    }

    #[test]
    fn ordinary_diagnostics_survive_the_shape_based_redaction() {
        for message in [
            "at blackrack_overlay_lib::telemetry::source::read (src/telemetry/mod.rs:3447)",
            "eyJ",
            "keyJoin failed",
        ] {
            assert_eq!(sanitize(message, 1_000), message);
        }
    }

    #[test]
    fn repeated_failures_collapse_and_a_success_clears_the_endpoint() {
        let endpoint = "test/endpoint-collapse";
        super::record_request_failure(endpoint, "decode: broken");
        super::record_request_failure(endpoint, "decode: broken");
        super::record_request_failure(endpoint, "decode: broken");

        {
            let failing = super::failing_requests().lock().unwrap();
            let failure = failing
                .get(endpoint)
                .expect("endpoint marcado como fallido");
            // Three failures, one reported: the rest wait for the interval.
            assert_eq!(failure.consecutive, 3);
        }

        super::record_request_success(endpoint);
        assert!(!super::failing_requests()
            .lock()
            .unwrap()
            .contains_key(endpoint));

        // A success on a healthy endpoint stays silent instead of re-reporting.
        super::record_request_success(endpoint);
        assert!(!super::failing_requests()
            .lock()
            .unwrap()
            .contains_key(endpoint));
    }

    #[test]
    fn new_credential_markers_are_covered() {
        assert_eq!(
            sanitize("login password=hunter2 api_key=abc123", 1_000),
            "login password=[redacted] api_key=[redacted]"
        );
    }
}
