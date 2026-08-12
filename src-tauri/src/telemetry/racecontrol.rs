use reqwest::blocking::Client;
use serde_json::Value;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

const LOCAL_AUTH_URL: &str = "http://127.0.0.1:6397/rest/profile/getAuthSessionTicket";
const RACECONTROL_AUTH_URL: &str = "https://raceos.gg/authenticate";
const ACCESS_TOKEN_CACHE_DURATION: Duration = Duration::from_secs(5 * 60);

struct CachedAccessToken {
    value: String,
    expires_at: Instant,
}

static ACCESS_TOKEN: OnceLock<Mutex<Option<CachedAccessToken>>> = OnceLock::new();

pub(super) fn authenticate(client: &Client) -> Result<String, String> {
    let cache = ACCESS_TOKEN.get_or_init(|| Mutex::new(None));
    let mut cached = cache
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some(token) = cached.as_ref() {
        if Instant::now() < token.expires_at {
            return Ok(token.value.clone());
        }
    }

    let ticket_response = client
        .get(LOCAL_AUTH_URL)
        .send()
        .map_err(|_| "lmu_local_api_unavailable".to_owned())?;
    if !ticket_response.status().is_success() {
        return Err(format!(
            "lmu_auth_ticket_http_{}",
            ticket_response.status().as_u16()
        ));
    }
    let ticket_json = ticket_response
        .json::<Value>()
        .map_err(|_| "lmu_auth_ticket_invalid_json".to_owned())?;
    let ticket = ticket_json
        .get("authSessionTicket")
        .and_then(Value::as_str)
        .filter(|ticket| !ticket.is_empty())
        .ok_or_else(|| "lmu_auth_ticket_missing".to_owned())?;

    let auth_response = client
        .post(RACECONTROL_AUTH_URL)
        .json(&serde_json::json!({
            "token": ticket,
            "game": "lmu",
            "platform": "steam",
        }))
        .send()
        .map_err(|_| "racecontrol_auth_unavailable".to_owned())?;
    if !auth_response.status().is_success() {
        return Err(format!(
            "racecontrol_auth_http_{}",
            auth_response.status().as_u16()
        ));
    }
    let auth_json = auth_response
        .json::<Value>()
        .map_err(|_| "racecontrol_auth_invalid_json".to_owned())?;
    let access_token = auth_json
        .get("accessToken")
        .and_then(Value::as_str)
        .filter(|token| !token.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| "racecontrol_access_token_missing".to_owned())?;
    *cached = Some(CachedAccessToken {
        value: access_token.clone(),
        expires_at: Instant::now() + ACCESS_TOKEN_CACHE_DURATION,
    });
    Ok(access_token)
}
