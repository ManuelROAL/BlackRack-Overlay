use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{ErrorKind, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering};
use std::sync::{mpsc, Mutex, OnceLock, RwLock};
use std::thread::{self, JoinHandle};
use std::time::Duration;
use tauri::AppHandle;

use crate::telemetry::TelemetryFrame;

const ADDRESS: &str = "127.0.0.1:47636";
const BASE_URL: &str = "http://127.0.0.1:47636";
const BROWSER_INDEX_ENTRY: &str = "browser.html";
// Only loopback names are legitimate for a local browser source. Rejecting every
// other `Host` blocks DNS-rebinding pages that resolve an attacker domain to
// 127.0.0.1 and then read the telemetry stream as if they were same-origin.
const ALLOWED_HOSTS: [&str; 4] = [
    "127.0.0.1:47636",
    "localhost:47636",
    "127.0.0.1",
    "localhost",
];
const ALLOWED_ORIGINS: [&str; 2] = ["http://127.0.0.1:47636", "http://localhost:47636"];
const MAX_SUBSCRIBERS: usize = 32;
const MAX_REQUEST_HEAD_BYTES: usize = 16 * 1024;
const MAX_ACCEPTS_PER_ITERATION: usize = 2;
const REQUEST_READ_ATTEMPTS: usize = 8;
const REQUEST_READ_TIMEOUT: Duration = Duration::from_millis(50);
const HTML_SECURITY_POLICY: &str = "default-src 'self'; connect-src 'self' ws://localhost:6398 ws://127.0.0.1:6398; img-src 'self' data:; style-src 'self'; base-uri 'none'; form-action 'none'; frame-ancestors 'self'; object-src 'none'";
const BROWSER_OVERLAYS: [(&str, &str); 20] = [
    ("standings", "standings.html"),
    ("relative", "relative.html"),
    ("fuel", "fuel.html"),
    ("pitstop", "pitstop.html"),
    ("flags", "flags.html"),
    ("rejoin", "rejoin.html"),
    ("delta", "delta.html"),
    ("timing", "timing.html"),
    ("stinthistory", "stinthistory.html"),
    ("driving", "driving.html"),
    ("liftcoast", "liftcoast.html"),
    ("tires", "tires.html"),
    ("damage", "damage.html"),
    ("trackmap", "trackmap.html"),
    ("forecast", "forecast.html"),
    ("conditions", "conditions.html"),
    ("dashboard", "dashboard.html"),
    ("sessioninfo", "sessioninfo.html"),
    ("chat", "chat.html"),
    ("minimap", "minimap.html"),
];
const ALL_OVERLAY_DEMANDS: u32 = (1 << BROWSER_OVERLAYS.len()) - 1;

#[derive(Clone, Deserialize, Serialize)]
struct Preferences {
    enabled: bool,
}

#[derive(Clone, Serialize)]
pub(crate) struct BrowserSourceStatus {
    enabled: bool,
    running: bool,
    url: &'static str,
    clients: usize,
    dropped_frames: u64,
    error_kind: Option<BrowserSourceErrorKind>,
    error_detail: Option<String>,
}

#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
enum BrowserSourceErrorKind {
    NotInitialized,
    MissingAssets,
    AddressUnavailable,
    ServerConfiguration,
    ServerStartup,
    SettingsPersistence,
}

struct BrowserSourceFailure {
    kind: BrowserSourceErrorKind,
    detail: String,
}

struct ServerRuntime {
    frames: mpsc::SyncSender<String>,
    shutdown: mpsc::Sender<()>,
    thread: JoinHandle<()>,
}

struct ServiceState {
    settings_path: PathBuf,
    app: AppHandle,
    runtime: Option<ServerRuntime>,
    error: Option<BrowserSourceFailure>,
}

struct BrowserSourceService {
    enabled: AtomicBool,
    clients: AtomicUsize,
    frame_clients: AtomicUsize,
    overlay_demands: AtomicU32,
    // Frames published while the server thread is behind are dropped on purpose;
    // the counter keeps that visible in the browser source status.
    dropped_frames: AtomicU64,
    state: Mutex<ServiceState>,
    preferences: RwLock<serde_json::Value>,
    chat_settings_pending: AtomicBool,
}

static SERVICE: OnceLock<BrowserSourceService> = OnceLock::new();

fn service() -> Option<&'static BrowserSourceService> {
    SERVICE.get()
}

pub(crate) fn configure(app: &AppHandle) {
    let config_dir = crate::app_paths::data_directory();
    let settings_path = config_dir.join("browser-source.json");
    let enabled = fs::read(&settings_path)
        .ok()
        .and_then(|contents| serde_json::from_slice::<Preferences>(&contents).ok())
        .map(|settings| settings.enabled)
        .unwrap_or(false);

    let _ = SERVICE.set(BrowserSourceService {
        enabled: AtomicBool::new(false),
        clients: AtomicUsize::new(0),
        frame_clients: AtomicUsize::new(0),
        overlay_demands: AtomicU32::new(0),
        dropped_frames: AtomicU64::new(0),
        state: Mutex::new(ServiceState {
            settings_path,
            app: app.clone(),
            runtime: None,
            error: None,
        }),
        preferences: RwLock::new(serde_json::json!({})),
        chat_settings_pending: AtomicBool::new(false),
    });

    if enabled {
        let status = set_enabled(true);
        if !status.running {
            crate::startup_log::record(format!(
                "warning: local browser source could not start: {}",
                status
                    .error_detail
                    .unwrap_or_else(|| "unknown error".into())
            ));
        }
    }
}

pub(crate) fn status() -> BrowserSourceStatus {
    let Some(service) = service() else {
        return BrowserSourceStatus {
            enabled: false,
            running: false,
            url: BASE_URL,
            clients: 0,
            dropped_frames: 0,
            error_kind: Some(BrowserSourceErrorKind::NotInitialized),
            error_detail: None,
        };
    };
    let state = service
        .state
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    BrowserSourceStatus {
        enabled: service.enabled.load(Ordering::Relaxed),
        running: state.runtime.is_some(),
        url: BASE_URL,
        clients: service.clients.load(Ordering::Relaxed),
        dropped_frames: service.dropped_frames.load(Ordering::Relaxed),
        error_kind: state.error.as_ref().map(|error| error.kind),
        error_detail: state.error.as_ref().map(|error| error.detail.clone()),
    }
}

pub(crate) fn set_enabled(enabled: bool) -> BrowserSourceStatus {
    let Some(service) = service() else {
        return status();
    };
    let mut state = service
        .state
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());

    if enabled && state.runtime.is_none() {
        match start_server(state.app.clone()) {
            Ok(runtime) => {
                service.dropped_frames.store(0, Ordering::Relaxed);
                state.runtime = Some(runtime);
                state.error = None;
                service.enabled.store(true, Ordering::Relaxed);
                crate::startup_log::record(format!("local browser source active at {BASE_URL}"));
            }
            Err(error) => {
                state.error = Some(error);
                service.enabled.store(false, Ordering::Relaxed);
            }
        }
    } else if !enabled {
        service.enabled.store(false, Ordering::Relaxed);
        service.clients.store(0, Ordering::Relaxed);
        service.frame_clients.store(0, Ordering::Relaxed);
        service.overlay_demands.store(0, Ordering::Relaxed);
        if let Some(runtime) = state.runtime.take() {
            let _ = runtime.shutdown.send(());
            let _ = runtime.thread.join();
        }
        state.error = None;
        crate::startup_log::record(format!(
            "local browser source disabled after dropping {} frames",
            service.dropped_frames.load(Ordering::Relaxed)
        ));
    }

    let actual_enabled = service.enabled.load(Ordering::Relaxed);
    if let Some(parent) = state.settings_path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Ok(contents) = serde_json::to_vec_pretty(&Preferences {
        enabled: actual_enabled,
    }) {
        if let Err(error) = fs::write(&state.settings_path, contents) {
            state.error = Some(BrowserSourceFailure {
                kind: BrowserSourceErrorKind::SettingsPersistence,
                detail: error.to_string(),
            });
        }
    }
    drop(state);
    status()
}

pub(crate) fn set_preferences(preferences: serde_json::Value) {
    let Some(service) = service() else {
        return;
    };
    let chat_settings_changed = {
        let mut stored = service
            .preferences
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let changed = stored.get("chat") != preferences.get("chat");
        *stored = preferences;
        changed
    };
    if chat_settings_changed {
        // Preferences are coalesced out of band from droppable telemetry/chat
        // snapshots, so a busy frame queue cannot lose the latest setting.
        service.chat_settings_pending.store(true, Ordering::Release);
    }
}

pub(crate) fn publish_frame(frame: &TelemetryFrame) {
    let Some(service) = service() else {
        return;
    };
    if !service.enabled.load(Ordering::Relaxed)
        || service.frame_clients.load(Ordering::Relaxed) == 0
    {
        return;
    }
    let sender = {
        let state = service
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state.runtime.as_ref().map(|runtime| runtime.frames.clone())
    };
    let Some(sender) = sender else {
        return;
    };
    if let Ok(json) = serde_json::to_string(frame) {
        if sender.try_send(json).is_err() {
            service.dropped_frames.fetch_add(1, Ordering::Relaxed);
        }
    }
}

pub(crate) fn publish_chat_update(update: &crate::telemetry::ChatUpdate) {
    let Some(service) = service() else {
        return;
    };
    if !service.enabled.load(Ordering::Relaxed) || !overlay_has_clients("chat") {
        return;
    }
    let sender = service
        .state
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .runtime
        .as_ref()
        .map(|runtime| runtime.frames.clone());
    if let (Some(sender), Ok(json)) = (sender, serde_json::to_string(update)) {
        if sender.try_send(format!("event:chat\n{json}")).is_err() {
            service.dropped_frames.fetch_add(1, Ordering::Relaxed);
        }
    }
}

pub(crate) fn overlay_has_clients(label: &str) -> bool {
    let Some(index) = BROWSER_OVERLAYS
        .iter()
        .position(|(candidate, _)| *candidate == label)
    else {
        return false;
    };
    service().is_some_and(|service| {
        service.enabled.load(Ordering::Relaxed)
            && service.overlay_demands.load(Ordering::Relaxed) & (1 << index) != 0
    })
}

fn start_server(app: AppHandle) -> Result<ServerRuntime, BrowserSourceFailure> {
    if [BROWSER_INDEX_ENTRY, "standings.html"]
        .into_iter()
        .any(|entry| app.asset_resolver().get(entry.into()).is_none())
    {
        return Err(BrowserSourceFailure {
            kind: BrowserSourceErrorKind::MissingAssets,
            detail: "embedded browser-source assets are unavailable".into(),
        });
    }
    let listener = TcpListener::bind(ADDRESS).map_err(|error| BrowserSourceFailure {
        kind: BrowserSourceErrorKind::AddressUnavailable,
        detail: format!("{ADDRESS}: {error}"),
    })?;
    listener
        .set_nonblocking(true)
        .map_err(|error| BrowserSourceFailure {
            kind: BrowserSourceErrorKind::ServerConfiguration,
            detail: error.to_string(),
        })?;
    let (frame_tx, frame_rx) = mpsc::sync_channel::<String>(2);
    let (shutdown_tx, shutdown_rx) = mpsc::channel::<()>();
    let server_thread = thread::Builder::new()
        .name("lmu-browser-source".into())
        .spawn(move || server_loop(listener, app, frame_rx, shutdown_rx))
        .map_err(|error| BrowserSourceFailure {
            kind: BrowserSourceErrorKind::ServerStartup,
            detail: error.to_string(),
        })?;
    Ok(ServerRuntime {
        frames: frame_tx,
        shutdown: shutdown_tx,
        thread: server_thread,
    })
}

fn server_loop(
    listener: TcpListener,
    app: AppHandle,
    frames: mpsc::Receiver<String>,
    shutdown: mpsc::Receiver<()>,
) {
    struct Subscriber {
        stream: TcpStream,
        overlay_demand: u32,
        include_frames: bool,
    }

    // The loop parks on the frame channel, so a published frame wakes it at once
    // and an idle server only wakes to poll the non-blocking listener. A served
    // request usually has more queued behind it, so the burst keeps a tight poll.
    const IDLE_POLL_INTERVAL: Duration = Duration::from_millis(50);
    const BUSY_POLL_INTERVAL: Duration = Duration::from_millis(2);

    let publish_subscriber_demand = |subscribers: &[Subscriber]| {
        if let Some(service) = service() {
            service.clients.store(subscribers.len(), Ordering::Relaxed);
            service.frame_clients.store(
                subscribers
                    .iter()
                    .filter(|subscriber| subscriber.include_frames)
                    .count(),
                Ordering::Relaxed,
            );
            service.overlay_demands.store(
                subscribers
                    .iter()
                    .fold(0, |demands, subscriber| demands | subscriber.overlay_demand),
                Ordering::Relaxed,
            );
        }
    };

    let mut subscribers = Vec::<Subscriber>::new();
    'server: loop {
        if shutdown.try_recv().is_ok() {
            break;
        }
        let mut served_request = false;
        for _ in 0..MAX_ACCEPTS_PER_ITERATION {
            if shutdown.try_recv().is_ok() {
                break 'server;
            }
            let (mut stream, _) = match listener.accept() {
                Ok(connection) => connection,
                Err(error) if error.kind() == ErrorKind::WouldBlock => break,
                Err(_) => break,
            };
            served_request = true;
            let head = match read_request_head(&mut stream, &shutdown) {
                Ok(Some(head)) => head,
                Ok(None) => continue,
                Err(()) => break 'server,
            };
            if !head.host_allowed || !head.origin_allowed {
                write_error(&mut stream, 403, "Forbidden");
                continue;
            }
            let target = head.target;
            let path = target.split('?').next().unwrap_or(&target);
            if path == "/api/events" {
                if subscribers.len() >= MAX_SUBSCRIBERS {
                    write_error(&mut stream, 503, "Too many browser source clients");
                } else if write_sse_headers(&mut stream).is_ok()
                    && stream.set_nonblocking(true).is_ok()
                {
                    subscribers.push(Subscriber {
                        stream,
                        overlay_demand: event_overlay_demand(&target),
                        include_frames: !event_stream_is_chat_only(&target),
                    });
                }
            } else if path == "/api/trackmap" {
                serve_track_map(&mut stream, &target);
            } else {
                serve_request(&mut stream, path, &app);
            }
        }
        if served_request {
            publish_subscriber_demand(&subscribers);
        }
        let poll_interval = if served_request {
            BUSY_POLL_INTERVAL
        } else {
            IDLE_POLL_INTERVAL
        };

        let mut pending = match frames.recv_timeout(poll_interval) {
            Ok(frame) => Some(frame),
            Err(mpsc::RecvTimeoutError::Timeout) => None,
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        };
        let mut broadcast = false;
        while let Some(frame) = pending.take().or_else(|| frames.try_recv().ok()) {
            if let Some(json) = frame.strip_prefix("event:chat\n") {
                let message = format!("event: chat\ndata: {json}\n\n");
                let chat_bit = BROWSER_OVERLAYS
                    .iter()
                    .position(|(name, _)| *name == "chat")
                    .map_or(0, |index| 1 << index);
                subscribers.retain_mut(|subscriber| {
                    subscriber.overlay_demand & chat_bit == 0
                        || subscriber.stream.write_all(message.as_bytes()).is_ok()
                });
            } else {
                let message = format!("data: {frame}\n\n");
                subscribers.retain_mut(|subscriber| {
                    !subscriber.include_frames
                        || subscriber.stream.write_all(message.as_bytes()).is_ok()
                });
            }
            broadcast = true;
        }
        let chat_settings = service().and_then(|service| {
            service
                .chat_settings_pending
                .swap(false, Ordering::AcqRel)
                .then(|| {
                    service
                        .preferences
                        .read()
                        .unwrap_or_else(|poisoned| poisoned.into_inner())
                        .get("chat")
                        .cloned()
                })
                .flatten()
        });
        if let Some(settings) = chat_settings {
            if let Ok(json) = serde_json::to_string(&settings) {
                let message = format!("event: chat-settings\ndata: {json}\n\n");
                let chat_bit = BROWSER_OVERLAYS
                    .iter()
                    .position(|(name, _)| *name == "chat")
                    .map_or(0, |index| 1 << index);
                subscribers.retain_mut(|subscriber| {
                    subscriber.overlay_demand & chat_bit == 0
                        || subscriber.stream.write_all(message.as_bytes()).is_ok()
                });
                broadcast = true;
            }
        }
        if broadcast {
            publish_subscriber_demand(&subscribers);
        }
    }
    if let Some(service) = service() {
        service.clients.store(0, Ordering::Relaxed);
        service.frame_clients.store(0, Ordering::Relaxed);
        service.overlay_demands.store(0, Ordering::Relaxed);
    }
}

fn event_overlay_demand(target: &str) -> u32 {
    let overlay = target.split_once('?').and_then(|(_, query)| {
        query
            .split('&')
            .find_map(|part| part.strip_prefix("overlay="))
    });
    overlay
        .and_then(|label| {
            BROWSER_OVERLAYS
                .iter()
                .position(|(candidate, _)| *candidate == label)
        })
        .map_or(ALL_OVERLAY_DEMANDS, |index| 1 << index)
}

fn event_stream_is_chat_only(target: &str) -> bool {
    target
        .split_once('?')
        .is_some_and(|(_, query)| query.split('&').any(|part| part == "chat_only=1"))
}

struct RequestHead {
    target: String,
    host_allowed: bool,
    origin_allowed: bool,
}

fn host_allowed(value: &str) -> bool {
    ALLOWED_HOSTS
        .iter()
        .any(|allowed| value.eq_ignore_ascii_case(allowed))
}

fn origin_allowed(value: &str) -> bool {
    ALLOWED_ORIGINS
        .iter()
        .any(|allowed| value.eq_ignore_ascii_case(allowed))
}

/// Embedded asset keys are plain relative paths. Anything that could encode a
/// traversal — percent escapes, backslashes, `..`, an absolute path — is
/// rejected before it reaches the resolver rather than relying on a raw `..`
/// comparison against a target the client controls.
fn is_safe_asset_path(relative: &str) -> bool {
    !relative.is_empty()
        && !relative.contains("..")
        && !relative.contains('%')
        && !relative.contains('\\')
        && !relative.starts_with('/')
}

fn read_request_head(
    stream: &mut TcpStream,
    shutdown: &mpsc::Receiver<()>,
) -> Result<Option<RequestHead>, ()> {
    let _ = stream.set_read_timeout(Some(REQUEST_READ_TIMEOUT));
    let _ = stream.set_write_timeout(Some(Duration::from_millis(100)));
    // Header bytes can be split across TCP segments, so keep reading until the
    // blank line that ends the head. The attempt and size caps stop a slow or
    // oversized client from holding up the single-threaded accept loop.
    let mut buffer = Vec::with_capacity(2_048);
    let mut chunk = [0_u8; 2_048];
    for _ in 0..REQUEST_READ_ATTEMPTS {
        if shutdown.try_recv().is_ok() {
            return Err(());
        }
        let length = match stream.read(&mut chunk) {
            Ok(length) => length,
            Err(error) if matches!(error.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {
                continue;
            }
            Err(_) => return Ok(None),
        };
        if length == 0 {
            break;
        }
        buffer.extend_from_slice(&chunk[..length]);
        if buffer.len() >= MAX_REQUEST_HEAD_BYTES
            || buffer.windows(4).any(|window| window == b"\r\n\r\n")
        {
            break;
        }
    }

    if !buffer.windows(4).any(|window| window == b"\r\n\r\n") {
        return Ok(None);
    }
    let request = match std::str::from_utf8(&buffer) {
        Ok(request) => request,
        Err(_) => return Ok(None),
    };
    let mut lines = request.split("\r\n");
    let Some(request_line) = lines.next() else {
        return Ok(None);
    };
    let mut parts = request_line.split_whitespace();
    if parts.next() != Some("GET") {
        return Ok(None);
    }
    let target = parts.next().unwrap_or("/").to_string();

    let mut host_allowed = false;
    // A browser source navigates to the page directly and sends no `Origin`;
    // any cross-site request that does carry one is refused.
    let mut origin_allowed = true;
    for line in lines {
        if line.is_empty() {
            break;
        }
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        let value = value.trim();
        if name.eq_ignore_ascii_case("host") {
            host_allowed = self::host_allowed(value);
        } else if name.eq_ignore_ascii_case("origin") {
            origin_allowed = self::origin_allowed(value);
        }
    }

    Ok(Some(RequestHead {
        target,
        host_allowed,
        origin_allowed,
    }))
}

fn serve_track_map(stream: &mut TcpStream, target: &str) {
    let cache_key = target
        .split_once('?')
        .map(|(_, query)| query)
        .and_then(|query| query.split('&').find_map(|part| part.strip_prefix("key=")))
        .unwrap_or("");
    match crate::telemetry::track_map_geometry(cache_key) {
        Ok(geometry) => match serde_json::to_vec(&geometry) {
            Ok(body) => write_response(stream, 200, "application/json", &body),
            Err(error) => write_error(stream, 500, &error.to_string()),
        },
        Err(error) => write_error(stream, 503, &error),
    }
}

fn write_sse_headers(stream: &mut TcpStream) -> std::io::Result<()> {
    stream.write_all(
        b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-cache\r\nConnection: keep-alive\r\nX-Content-Type-Options: nosniff\r\n\r\n: connected\n\n",
    )
}

fn serve_request(stream: &mut TcpStream, request_path: &str, app: &AppHandle) {
    if request_path == "/browser-source-settings.js" {
        let settings = service()
            .map(|service| {
                service
                    .preferences
                    .read()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .clone()
            })
            .unwrap_or_else(|| serde_json::json!({}));
        let json = serde_json::to_string(&settings).unwrap_or_else(|_| "{}".into());
        let script = format!(
            "{base}(()=>{{const p={json};if(p.chat)localStorage.setItem('blackrack-overlay.chat-settings.v1',JSON.stringify(p.chat));}})();",
            base = browser_source_settings_script(&json),
            json = json
        );
        write_response(
            stream,
            200,
            "application/javascript; charset=utf-8",
            script.as_bytes(),
        );
        return;
    }

    let relative = if request_path == "/" {
        BROWSER_INDEX_ENTRY
    } else {
        browser_overlay_entry(request_path).unwrap_or_else(|| request_path.trim_start_matches('/'))
    };
    if !is_safe_asset_path(relative) {
        write_response(stream, 404, "text/plain; charset=utf-8", b"Not found");
        return;
    }
    let Some(asset) = app.asset_resolver().get(relative.to_string()) else {
        write_response(stream, 404, "text/plain; charset=utf-8", b"Not found");
        return;
    };
    let mut contents = asset.bytes;
    let mime = asset.mime_type;
    if mime.starts_with("text/html") {
        contents = String::from_utf8_lossy(&contents)
            .replace(
                "</head>",
                "<script src=\"/browser-source-settings.js\"></script></head>",
            )
            .into_bytes();
    }
    write_response(stream, 200, &mime, &contents);
}

fn browser_source_settings_script(json: &str) -> String {
    format!(
        "(()=>{{const p={json},q=new URLSearchParams(location.search).get('lang'),valid=l=>typeof l==='string'&&Array.isArray(p.supportedLocales)&&p.supportedLocales.includes(l);if(valid(q))document.documentElement.dataset.localeOverride=q;else if(valid(p.locale))localStorage.setItem('blackrack-overlay.locale.v1',p.locale);if(p.displayUnits)localStorage.setItem('blackrack-overlay.display-units.v1',JSON.stringify(p.displayUnits));if(p.standings)localStorage.setItem('blackrack-overlay.standings.v1',JSON.stringify(p.standings));if(p.relative)localStorage.setItem('blackrack-overlay.relative.v3',JSON.stringify(p.relative));if(p.driving)localStorage.setItem('blackrack-overlay.driving.v1',JSON.stringify(p.driving));if(p.delta)localStorage.setItem('blackrack-overlay.delta.v1',JSON.stringify(p.delta));if(p.timing)localStorage.setItem('blackrack-overlay.timing.v1',JSON.stringify(p.timing));if(p.trackMap)localStorage.setItem('blackrack-overlay.track-map-settings.v1',JSON.stringify(p.trackMap));if(p.transparency)localStorage.setItem('blackrack-overlay.background-transparency.v1',JSON.stringify(p.transparency));if(p.fontSize)localStorage.setItem('blackrack-overlay.font-size.v1',JSON.stringify(p.fontSize));if(p.fuel)localStorage.setItem('blackrack-overlay.fuel-strategy.v1',JSON.stringify(p.fuel));if(p.tires)localStorage.setItem('blackrack-overlay.tires.v1',JSON.stringify(p.tires));if(p.conditions)localStorage.setItem('blackrack-overlay.conditions.v1',JSON.stringify(p.conditions));if(p.dashboard)localStorage.setItem('blackrack-overlay.dashboard.v1',JSON.stringify(p.dashboard));if(p.sessionInfo)localStorage.setItem('blackrack-overlay.sessioninfo.v1',JSON.stringify(p.sessionInfo));if(p.liftCoast)localStorage.setItem('blackrack-overlay.liftcoast.v1',JSON.stringify(p.liftCoast));if(p.pitstop)localStorage.setItem('blackrack-overlay.pitstop.v1',JSON.stringify(p.pitstop));if(p.minimap)localStorage.setItem('blackrack-overlay.minimap-settings.v1',JSON.stringify(p.minimap));document.documentElement.dataset.browserSource='true';}})();"
    )
}

fn write_response(stream: &mut TcpStream, status: u16, mime: &str, body: &[u8]) {
    let reason = match status {
        200 => "OK",
        403 => "Forbidden",
        404 => "Not Found",
        503 => "Service Unavailable",
        _ => "Internal Server Error",
    };
    let mut headers = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: {mime}\r\nContent-Length: {}\r\nCache-Control: no-cache\r\nX-Content-Type-Options: nosniff\r\nReferrer-Policy: no-referrer\r\nConnection: close\r\n",
        body.len()
    );
    // Pages served here run outside the WebView, so they do not inherit the
    // application CSP from tauri.conf.json and need their own.
    if mime.starts_with("text/html") {
        headers.push_str("Content-Security-Policy: ");
        headers.push_str(HTML_SECURITY_POLICY);
        headers.push_str("\r\n");
    }
    headers.push_str("\r\n");
    let _ = stream.write_all(headers.as_bytes());
    let _ = stream.write_all(body);
}

fn write_error(stream: &mut TcpStream, status: u16, message: &str) {
    write_response(
        stream,
        status,
        "text/plain; charset=utf-8",
        message.as_bytes(),
    );
}

fn browser_overlay_entry(request_path: &str) -> Option<&'static str> {
    let route = request_path
        .strip_prefix('/')
        .unwrap_or(request_path)
        .trim_end_matches('/');
    BROWSER_OVERLAYS
        .iter()
        .find_map(|(candidate, entry)| (*candidate == route).then_some(*entry))
}

#[cfg(test)]
mod tests {
    use super::{
        browser_overlay_entry, browser_source_settings_script, event_overlay_demand,
        event_stream_is_chat_only, host_allowed, is_safe_asset_path, origin_allowed,
        BROWSER_OVERLAYS,
    };

    #[test]
    fn only_loopback_hosts_are_served() {
        assert!(host_allowed("127.0.0.1:47636"));
        assert!(host_allowed("localhost:47636"));
        assert!(host_allowed("LocalHost"));
        // A DNS-rebinding page reaches the socket but still sends its own name.
        assert!(!host_allowed("attacker.example:47636"));
        assert!(!host_allowed("127.0.0.1.nip.io:47636"));
        assert!(!host_allowed(""));
    }

    #[test]
    fn cross_site_origins_are_refused() {
        assert!(origin_allowed("http://127.0.0.1:47636"));
        assert!(origin_allowed("http://localhost:47636"));
        assert!(!origin_allowed("https://attacker.example"));
        assert!(!origin_allowed("null"));
    }

    #[test]
    fn asset_paths_reject_encoded_and_absolute_traversals() {
        assert!(is_safe_asset_path("standings.html"));
        assert!(is_safe_asset_path("assets/index-a1b2c3.js"));
        assert!(!is_safe_asset_path("../secrets.json"));
        assert!(!is_safe_asset_path("%2e%2e/secrets.json"));
        assert!(!is_safe_asset_path("..\\secrets.json"));
        assert!(!is_safe_asset_path("/etc/passwd"));
        assert!(!is_safe_asset_path(""));
    }

    #[test]
    fn every_browser_overlay_has_a_route() {
        for (route, entry) in BROWSER_OVERLAYS {
            assert_eq!(browser_overlay_entry(&format!("/{route}")), Some(entry));
            assert_eq!(browser_overlay_entry(&format!("/{route}/")), Some(entry));
        }
    }

    #[test]
    fn unknown_routes_are_not_treated_as_overlays() {
        assert_eq!(browser_overlay_entry("/unknown"), None);
    }

    #[test]
    fn browser_source_settings_use_aligned_keys() {
        let script = browser_source_settings_script("{}");

        let settings_keys = [
            ("standings", "blackrack-overlay.standings.v1"),
            ("relative", "blackrack-overlay.relative.v3"),
            ("driving", "blackrack-overlay.driving.v1"),
            ("delta", "blackrack-overlay.delta.v1"),
            ("timing", "blackrack-overlay.timing.v1"),
            ("trackMap", "blackrack-overlay.track-map-settings.v1"),
            (
                "transparency",
                "blackrack-overlay.background-transparency.v1",
            ),
            ("fontSize", "blackrack-overlay.font-size.v1"),
            ("fuel", "blackrack-overlay.fuel-strategy.v1"),
            ("tires", "blackrack-overlay.tires.v1"),
            ("conditions", "blackrack-overlay.conditions.v1"),
            ("dashboard", "blackrack-overlay.dashboard.v1"),
            ("sessionInfo", "blackrack-overlay.sessioninfo.v1"),
            ("liftCoast", "blackrack-overlay.liftcoast.v1"),
            ("pitstop", "blackrack-overlay.pitstop.v1"),
            ("minimap", "blackrack-overlay.minimap-settings.v1"),
        ];

        for (payload, key) in settings_keys {
            assert!(
                script.contains(&format!("if(p.{payload})localStorage.setItem('{key}'")),
                "missing browser-source settings mapping for {payload} -> {key}"
            );
        }
        assert!(script.contains("localStorage.setItem('blackrack-overlay.locale.v1',p.locale)"));
        assert!(!script.contains("blackrack-overlay.relative.v1"));
    }

    #[test]
    fn event_clients_declare_their_overlay_demand() {
        let standings = event_overlay_demand("/api/events?overlay=standings");
        let fuel = event_overlay_demand("/api/events?overlay=fuel&lang=es");
        assert_ne!(standings, fuel);
        assert_eq!(standings.count_ones(), 1);
        assert_eq!(fuel.count_ones(), 1);
    }

    #[test]
    fn chat_only_event_clients_skip_telemetry_frames() {
        assert!(event_stream_is_chat_only(
            "/api/events?overlay=chat&chat_only=1"
        ));
        assert!(!event_stream_is_chat_only("/api/events?overlay=chat"));
    }

    #[test]
    fn legacy_or_unknown_event_clients_keep_full_demand() {
        assert_eq!(
            event_overlay_demand("/api/events").count_ones(),
            BROWSER_OVERLAYS.len() as u32
        );
        assert_eq!(
            event_overlay_demand("/api/events?overlay=unknown").count_ones(),
            BROWSER_OVERLAYS.len() as u32
        );
    }
}
