use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{mpsc, Mutex, OnceLock, RwLock};
use std::thread::{self, JoinHandle};
use std::time::Duration;
use tauri::{AppHandle, Manager};

use crate::telemetry::TelemetryFrame;

const ADDRESS: &str = "127.0.0.1:47636";
const BASE_URL: &str = "http://127.0.0.1:47636";
const BROWSER_OVERLAYS: [(&str, &str, &str); 13] = [
    ("standings", "standings.html", "Standings"),
    ("relative", "relative.html", "Relative"),
    ("fuel", "fuel.html", "Combustible / energía"),
    ("pitstop", "pitstop.html", "Parada estimada"),
    ("flags", "flags.html", "Banderas"),
    ("rejoin", "rejoin.html", "Rejoin"),
    ("dashboard", "dashboard.html", "Dashboard"),
    ("delta", "delta.html", "Delta"),
    ("timing", "timing.html", "Timing compacto"),
    ("driving", "driving.html", "Trailing + Pedal"),
    ("tires", "tires.html", "Daños + neumáticos"),
    ("damage", "damage.html", "Daño detallado"),
    ("trackmap", "trackmap.html", "Mapa del circuito"),
];

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
    error: Option<String>,
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
    error: Option<String>,
}

struct BrowserSourceService {
    enabled: AtomicBool,
    clients: AtomicUsize,
    state: Mutex<ServiceState>,
    preferences: RwLock<serde_json::Value>,
}

static SERVICE: OnceLock<BrowserSourceService> = OnceLock::new();

fn service() -> Option<&'static BrowserSourceService> {
    SERVICE.get()
}

pub(crate) fn configure(app: &AppHandle) {
    let config_dir = app
        .path()
        .app_config_dir()
        .unwrap_or_else(|_| PathBuf::from("."));
    let settings_path = config_dir.join("browser-source.json");
    let enabled = fs::read(&settings_path)
        .ok()
        .and_then(|contents| serde_json::from_slice::<Preferences>(&contents).ok())
        .map(|settings| settings.enabled)
        .unwrap_or(false);

    let _ = SERVICE.set(BrowserSourceService {
        enabled: AtomicBool::new(false),
        clients: AtomicUsize::new(0),
        state: Mutex::new(ServiceState {
            settings_path,
            app: app.clone(),
            runtime: None,
            error: None,
        }),
        preferences: RwLock::new(serde_json::json!({})),
    });

    if enabled {
        let status = set_enabled(true);
        if !status.running {
            crate::startup_log::record(format!(
                "warning: local browser source could not start: {}",
                status.error.unwrap_or_else(|| "unknown error".into())
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
            error: Some("El servidor todavia no se ha inicializado".into()),
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
        error: state.error.clone(),
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
        if let Some(runtime) = state.runtime.take() {
            let _ = runtime.shutdown.send(());
            let _ = runtime.thread.join();
        }
        state.error = None;
        crate::startup_log::record("local browser source disabled");
    }

    let actual_enabled = service.enabled.load(Ordering::Relaxed);
    if let Some(parent) = state.settings_path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Ok(contents) = serde_json::to_vec_pretty(&Preferences {
        enabled: actual_enabled,
    }) {
        if let Err(error) = fs::write(&state.settings_path, contents) {
            state.error = Some(format!("No se pudo guardar la configuracion: {error}"));
        }
    }
    drop(state);
    status()
}

pub(crate) fn set_preferences(preferences: serde_json::Value) {
    let Some(service) = service() else {
        return;
    };
    *service
        .preferences
        .write()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = preferences;
}

pub(crate) fn publish_frame(frame: &TelemetryFrame) {
    let Some(service) = service() else {
        return;
    };
    if !service.enabled.load(Ordering::Relaxed) || service.clients.load(Ordering::Relaxed) == 0 {
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
        let _ = sender.try_send(json);
    }
}

pub(crate) fn has_clients() -> bool {
    service().is_some_and(|service| {
        service.enabled.load(Ordering::Relaxed) && service.clients.load(Ordering::Relaxed) > 0
    })
}

fn start_server(app: AppHandle) -> Result<ServerRuntime, String> {
    if app.asset_resolver().get("standings.html".into()).is_none() {
        return Err("No se encontraron los recursos web incrustados".into());
    }
    let listener = TcpListener::bind(ADDRESS)
        .map_err(|error| format!("No se pudo abrir {ADDRESS}: {error}"))?;
    listener
        .set_nonblocking(true)
        .map_err(|error| format!("No se pudo configurar el servidor local: {error}"))?;
    let (frame_tx, frame_rx) = mpsc::sync_channel::<String>(2);
    let (shutdown_tx, shutdown_rx) = mpsc::channel::<()>();
    let server_thread = thread::Builder::new()
        .name("lmu-browser-source".into())
        .spawn(move || server_loop(listener, app, frame_rx, shutdown_rx))
        .map_err(|error| format!("No se pudo iniciar el servidor local: {error}"))?;
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
    let mut subscribers = Vec::<TcpStream>::new();
    loop {
        if shutdown.try_recv().is_ok() {
            break;
        }
        while let Ok((mut stream, _)) = listener.accept() {
            if let Some(target) = request_path(&mut stream) {
                let path = target.split('?').next().unwrap_or(&target);
                if path == "/api/events" {
                    if write_sse_headers(&mut stream).is_ok() {
                        subscribers.push(stream);
                    }
                } else if path == "/api/trackmap" {
                    serve_track_map(&mut stream, &target);
                } else {
                    serve_request(&mut stream, path, &app);
                }
            }
        }

        while let Ok(frame) = frames.try_recv() {
            let message = format!("data: {frame}\n\n");
            subscribers.retain_mut(|stream| stream.write_all(message.as_bytes()).is_ok());
        }
        if let Some(service) = service() {
            service.clients.store(subscribers.len(), Ordering::Relaxed);
        }
        thread::sleep(Duration::from_millis(10));
    }
    if let Some(service) = service() {
        service.clients.store(0, Ordering::Relaxed);
    }
}

fn request_path(stream: &mut TcpStream) -> Option<String> {
    let _ = stream.set_read_timeout(Some(Duration::from_millis(500)));
    let _ = stream.set_write_timeout(Some(Duration::from_millis(100)));
    let mut buffer = [0_u8; 8192];
    let length = stream.read(&mut buffer).ok()?;
    let request = std::str::from_utf8(&buffer[..length]).ok()?;
    let line = request.lines().next()?;
    let mut parts = line.split_whitespace();
    (parts.next()? == "GET").then(|| parts.next().unwrap_or("/").to_string())
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
    if request_path == "/" {
        let body = browser_source_index();
        write_response(stream, 200, "text/html; charset=utf-8", body.as_bytes());
        return;
    }
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
            "(()=>{{const p={json};if(p.standings)localStorage.setItem('lmu-overlay.standings.v1',JSON.stringify(p.standings));if(p.relative)localStorage.setItem('lmu-overlay.relative.v1',JSON.stringify(p.relative));if(p.driving)localStorage.setItem('lmu-overlay.driving.v1',JSON.stringify(p.driving));if(p.delta)localStorage.setItem('lmu-overlay.delta.v1',JSON.stringify(p.delta));if(p.timing)localStorage.setItem('lmu-overlay.timing.v1',JSON.stringify(p.timing));if(p.transparency)localStorage.setItem('lmu-overlay.background-transparency.v1',JSON.stringify(p.transparency));if(p.fuel)localStorage.setItem('lmu-overlay.fuel-strategy.v1',JSON.stringify(p.fuel));document.documentElement.dataset.browserSource='true';}})();"
        );
        write_response(
            stream,
            200,
            "application/javascript; charset=utf-8",
            script.as_bytes(),
        );
        return;
    }

    let relative =
        browser_overlay_entry(request_path).unwrap_or_else(|| request_path.trim_start_matches('/'));
    if relative.contains("..") || relative.is_empty() {
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

fn write_response(stream: &mut TcpStream, status: u16, mime: &str, body: &[u8]) {
    let reason = match status {
        200 => "OK",
        404 => "Not Found",
        503 => "Service Unavailable",
        _ => "Internal Server Error",
    };
    let headers = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: {mime}\r\nContent-Length: {}\r\nCache-Control: no-cache\r\nX-Content-Type-Options: nosniff\r\nConnection: close\r\n\r\n",
        body.len()
    );
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

fn browser_source_index() -> String {
    let links = BROWSER_OVERLAYS
        .iter()
        .map(|(route, _, label)| format!("<a href=\"/{route}\">{label}</a>"))
        .collect::<String>();
    format!(
        "<!doctype html><html><head><meta charset=\"utf-8\"><title>LMU Overlay · OBS</title><style>body{{margin:40px;background:#080b0f;color:#f4f7fa;font:16px sans-serif}}a{{display:block;width:max-content;margin:12px 0;color:#d8ff3e}}</style></head><body><h1>LMU Overlay · Fuentes de navegador</h1>{links}<p>Servidor local: {BASE_URL}</p></body></html>"
    )
}

fn browser_overlay_entry(request_path: &str) -> Option<&'static str> {
    let route = request_path
        .strip_prefix('/')
        .unwrap_or(request_path)
        .trim_end_matches('/');
    BROWSER_OVERLAYS
        .iter()
        .find_map(|(candidate, entry, _)| (*candidate == route).then_some(*entry))
}

#[cfg(test)]
mod tests {
    use super::{browser_overlay_entry, browser_source_index, BROWSER_OVERLAYS};

    #[test]
    fn every_browser_overlay_has_a_route_and_index_link() {
        let index = browser_source_index();
        for (route, entry, label) in BROWSER_OVERLAYS {
            assert_eq!(browser_overlay_entry(&format!("/{route}")), Some(entry));
            assert_eq!(browser_overlay_entry(&format!("/{route}/")), Some(entry));
            assert!(index.contains(&format!("href=\"/{route}\"")));
            assert!(index.contains(label));
        }
    }

    #[test]
    fn unknown_routes_are_not_treated_as_overlays() {
        assert_eq!(browser_overlay_entry("/unknown"), None);
    }
}
