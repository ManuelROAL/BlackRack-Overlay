use serde_json::Value;
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::{Duration, Instant, SystemTime};

const RACECONTROL_EVENT_OVERVIEW_URL: &str = "https://raceos.gg/api/v1/event/overview";
const RACECONTROL_MY_SPLIT_URL: &str = "https://raceos.gg/api/v1/event/my-split/daily";
/// Endpoint names the diagnostics log reports these two requests under.
const OVERVIEW_ENDPOINT: &str = "raceos/event-overview";
const MY_SPLIT_ENDPOINT: &str = "raceos/my-split";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(5);
const RETRY_INTERVAL: Duration = Duration::from_secs(10);
/// An attempt that leaves the split unresolved doubles the wait, up to this
/// ceiling: RaceOS answering 500 for a stale event ID would otherwise be asked
/// twice every ten seconds for the rest of the session. The first retry keeps
/// the normal interval, so a single failed request still costs nothing.
const MAX_RETRY_INTERVAL: Duration = Duration::from_secs(5 * 60);
const MAX_BACKOFF_STEPS: u32 = 5;
const TRACE_CHUNK_BYTES: u64 = 2 * 1024 * 1024;
const EVENT_MARKER: &str = "server for online event ";

#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct SessionSplit {
    pub number: u32,
    pub count: u32,
    pub event_id: String,
    pub driver_rank_settings: DriverRankSettings,
    pub player_driver_elo: Option<f64>,
    profiles: HashMap<String, EventDriverProfile>,
    profiles_checked: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct EventDriverProfile {
    pub nationality: String,
    pub badge: String,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct DriverRankSettings {
    pub multiplier: f64,
    pub k: f64,
    pub distance: f64,
    pub logarithm: f64,
}

impl Default for DriverRankSettings {
    fn default() -> Self {
        Self {
            multiplier: 1.0,
            k: 30.0,
            distance: 420.0,
            logarithm: 10.0,
        }
    }
}

/// One resolution attempt. The event ID travels with the outcome because the
/// resolver backs off per event and only the worker reads it from the trace.
struct SplitAttempt {
    event_id: String,
    result: Result<SessionSplit, String>,
}

pub(super) struct SessionSplitResolver {
    current: SessionSplit,
    receiver: Option<Receiver<SplitAttempt>>,
    last_attempt: Option<Instant>,
    last_event_id: String,
    failures: u32,
    request_revision: u64,
    resolved_request_revision: u64,
}

impl SessionSplitResolver {
    pub(super) fn discover() -> Self {
        Self {
            current: SessionSplit::default(),
            receiver: None,
            last_attempt: None,
            last_event_id: String::new(),
            failures: 0,
            request_revision: 0,
            resolved_request_revision: 0,
        }
    }

    #[cfg(test)]
    pub(super) fn empty() -> Self {
        Self::discover()
    }

    pub(super) fn refresh(&mut self) {
        self.receive_result();
        if self.receiver.is_some() {
            return;
        }

        let now = Instant::now();
        let interval = self.retry_interval();
        if self
            .last_attempt
            .is_some_and(|attempt| now.duration_since(attempt) < interval)
        {
            return;
        }

        self.last_attempt = Some(now);
        self.request_revision = self.request_revision.saturating_add(1);
        let (sender, receiver) = mpsc::channel();
        self.receiver = Some(receiver);
        let current = self.current.clone();
        thread::spawn(move || {
            let event_id = latest_online_event_id().unwrap_or_default();
            let result = fetch_session_split(current, &event_id);
            let _ = sender.send(SplitAttempt { event_id, result });
        });
    }

    /// The normal interval until RaceOS starts leaving the split unresolved,
    /// then twice as long per further failure until the ceiling.
    fn retry_interval(&self) -> Duration {
        let steps = self.failures.saturating_sub(1).min(MAX_BACKOFF_STEPS);
        RETRY_INTERVAL
            .saturating_mul(1u32 << steps)
            .min(MAX_RETRY_INTERVAL)
    }

    /// Counts how the attempt went for the backoff above. Only an event that
    /// RaceOS refuses to resolve counts as a failure: being out of an online
    /// event is the idle case, and a new event ID starts the count over because
    /// the previous failures said nothing about this one.
    fn note_attempt(&mut self, attempt: &SplitAttempt) {
        if attempt.event_id != self.last_event_id {
            self.last_event_id = attempt.event_id.clone();
            self.failures = 0;
        }
        let settled = attempt
            .result
            .as_ref()
            .is_ok_and(|split| split.is_settled());
        if attempt.event_id.is_empty() || settled {
            self.failures = 0;
        } else {
            self.failures = self.failures.saturating_add(1);
        }
    }

    pub(super) fn value(&self) -> &SessionSplit {
        &self.current
    }

    pub(super) fn request_revisions(&self) -> (u64, u64) {
        (self.request_revision, self.resolved_request_revision)
    }

    fn receive_result(&mut self) {
        let Some(receiver) = self.receiver.as_ref() else {
            return;
        };
        let Ok(attempt) = receiver.try_recv() else {
            return;
        };
        self.receiver = None;
        self.note_attempt(&attempt);

        match attempt.result {
            Ok(split) => {
                // Only one request can be in flight; refresh receives it before
                // incrementing request_revision for the next request. This is
                // the revision at request start, not a count of responses.
                self.resolved_request_revision = self.request_revision;
                if split != self.current {
                    crate::telemetry::queue_analysis_event(serde_json::json!({
                        "event": "session_split",
                        "source": "racecontrol_event_overview",
                        "status": if split.number > 0 { "resolved" } else { "not_available" },
                        "event_id": split.event_id,
                        "split_number": split.number,
                        "split_count": split.count,
                        "driver_rank_settings": {
                            "multiplier": split.driver_rank_settings.multiplier,
                            "k": split.driver_rank_settings.k,
                            "distance": split.driver_rank_settings.distance,
                            "logarithm": split.driver_rank_settings.logarithm,
                        },
                    }));
                }
                self.current = split;
            }
            Err(error) => crate::telemetry::queue_analysis_event(serde_json::json!({
                "event": "session_split",
                "source": "racecontrol_event_overview",
                "status": "failed",
                "error": error,
            })),
        }
    }
}

impl SessionSplit {
    pub(super) fn profile(&self, driver_name: &str) -> Option<&EventDriverProfile> {
        self.profiles
            .get(&super::driver_ranks::normalized_name(driver_name))
    }

    /// Both halves of the split are known and the roster has been read, which is
    /// exactly the state that lets the resolver stop asking RaceOS.
    fn is_settled(&self) -> bool {
        self.number > 0 && self.count > 0 && self.profiles_checked
    }
}

fn fetch_session_split(current: SessionSplit, event_id: &str) -> Result<SessionSplit, String> {
    if event_id.is_empty() {
        return Ok(SessionSplit::default());
    }
    if current.event_id.eq_ignore_ascii_case(event_id) && current.is_settled() {
        return Ok(current);
    }

    let cached = cached_event_split(event_id).unwrap_or_else(|| SessionSplit {
        event_id: event_id.to_owned(),
        ..SessionSplit::default()
    });

    let client = reqwest::blocking::Client::builder()
        .timeout(REQUEST_TIMEOUT)
        .build()
        .map_err(|_| "http_client_initialization_failed".to_owned())?;
    let access_token = match super::racecontrol::authenticate(&client) {
        Ok(token) => token,
        Err(_) if cached.number > 0 || cached.count > 0 => return Ok(cached),
        Err(error) => return Err(error),
    };
    let split_response = client
        .post(RACECONTROL_EVENT_OVERVIEW_URL)
        .header("Game-Authorization", format!("Bearer {access_token}"))
        .json(&event_overview_request(event_id))
        .send()
        .map_err(|_| {
            let error = "racecontrol_event_overview_unavailable".to_owned();
            crate::startup_log::record_request_failure(OVERVIEW_ENDPOINT, &error);
            error
        })?;
    if !split_response.status().is_success() {
        let error = format!(
            "racecontrol_event_overview_http_{}",
            split_response.status().as_u16()
        );
        crate::startup_log::record_request_failure(OVERVIEW_ENDPOINT, &error);
        return fetch_direct_split(&client, &access_token, event_id, cached)
            .map_err(|direct_error| format!("{error}__{direct_error}"));
    }
    let split_json = split_response.json::<Value>().map_err(|_| {
        let error = "racecontrol_event_overview_invalid_json".to_owned();
        crate::startup_log::record_request_failure(OVERVIEW_ENDPOINT, &error);
        error
    })?;
    crate::startup_log::record_request_success(OVERVIEW_ENDPOINT);
    let mut split = parse_event_split(&split_json, event_id);
    if split.number == 0 {
        split.number = cached.number;
    }
    if split.count == 0 {
        split.count = cached.count;
    }
    fetch_direct_split(&client, &access_token, event_id, split)
}

fn fetch_direct_split(
    client: &reqwest::blocking::Client,
    access_token: &str,
    event_id: &str,
    mut fallback: SessionSplit,
) -> Result<SessionSplit, String> {
    let response = client
        .get(format!("{RACECONTROL_MY_SPLIT_URL}/{event_id}"))
        .header("Game-Authorization", format!("Bearer {access_token}"))
        .send()
        .map_err(|_| {
            let error = "racecontrol_my_split_unavailable".to_owned();
            crate::startup_log::record_request_failure(MY_SPLIT_ENDPOINT, &error);
            error
        })?;
    if !response.status().is_success() {
        let error = format!("racecontrol_my_split_http_{}", response.status().as_u16());
        crate::startup_log::record_request_failure(MY_SPLIT_ENDPOINT, &error);
        // A cached split still stands, so the caller is not failed over a
        // refresh that could not happen.
        return if fallback.number > 0 || fallback.count > 0 {
            Ok(fallback)
        } else {
            Err(error)
        };
    }
    let json = response.json::<Value>().map_err(|_| {
        let error = "racecontrol_my_split_invalid_json".to_owned();
        crate::startup_log::record_request_failure(MY_SPLIT_ENDPOINT, &error);
        error
    })?;
    crate::startup_log::record_request_success(MY_SPLIT_ENDPOINT);
    let direct = parse_event_split(&json, event_id);
    if direct.number > 0 {
        fallback.number = direct.number;
    }
    if direct.count > 0 {
        fallback.count = direct.count;
    }
    if direct.player_driver_elo.is_some() {
        fallback.player_driver_elo = direct.player_driver_elo;
    }
    fallback.event_id = event_id.to_owned();
    fallback.profiles = parse_event_profiles(&json);
    fallback.profiles_checked = true;
    if find_object_key(&json, "drSettings").is_some() {
        fallback.driver_rank_settings = direct.driver_rank_settings;
    }
    Ok(fallback)
}

fn parse_event_profiles(value: &Value) -> HashMap<String, EventDriverProfile> {
    fn visit(value: &Value, profiles: &mut HashMap<String, EventDriverProfile>) {
        match value {
            Value::Object(object) => {
                if let Some(drivers) = object
                    .iter()
                    .find(|(key, _)| key.eq_ignore_ascii_case("drivers"))
                    .and_then(|(_, value)| value.as_array())
                {
                    for registration in drivers {
                        let driver = registration.get("driver").unwrap_or(registration);
                        let nationality = super::driver_ranks::profile_nationality(driver);
                        let badge = super::driver_ranks::profile_badge(driver);
                        if nationality.is_empty() && badge.is_empty() {
                            continue;
                        }
                        let profile = EventDriverProfile { nationality, badge };
                        for name in ["name", "username"]
                            .into_iter()
                            .filter_map(|key| driver.get(key).and_then(Value::as_str))
                            .map(str::trim)
                            .filter(|name| !name.is_empty())
                        {
                            profiles.insert(
                                super::driver_ranks::normalized_name(name),
                                profile.clone(),
                            );
                        }
                    }
                }
                for child in object.values() {
                    visit(child, profiles);
                }
            }
            Value::Array(items) => {
                for child in items {
                    visit(child, profiles);
                }
            }
            _ => {}
        }
    }

    let mut profiles = HashMap::new();
    visit(value, &mut profiles);
    profiles
}

fn event_overview_request(event_id: &str) -> Value {
    serde_json::json!({
        "game": "lmu",
        "eventType": "daily",
        "eventId": event_id,
    })
}

fn parse_event_split(value: &Value, event_id: &str) -> SessionSplit {
    let overview = find_event_overview(value, event_id).unwrap_or(value);
    let number = object_value_from_value(overview, "split")
        .and_then(|split| {
            positive_u32(split).or_else(|| find_positive_key(split, &["splitNo", "splitNumber"]))
        })
        .or_else(|| find_direct_positive_key(overview, &["splitNo", "splitNumber"]))
        .or_else(|| find_positive_key(overview, &["splitNo", "splitNumber"]))
        .unwrap_or(0);
    let explicit_count = find_positive_key(
        overview,
        &[
            "splitCount",
            "totalSplits",
            "totalSplitCount",
            "numberOfSplits",
        ],
    )
    .unwrap_or(0);
    let splits_count = find_array_len(overview, "splits").unwrap_or(0);
    let max_players = find_positive_key(overview, &["maxPlayers"]).unwrap_or(0);
    let registrations = find_array_len(overview, "registrations").unwrap_or(0);
    let estimated_count = if max_players > 0 && registrations > 0 {
        registrations.div_ceil(max_players)
    } else {
        0
    };
    let count = explicit_count.max(splits_count).max(estimated_count);

    SessionSplit {
        number,
        count,
        event_id: event_id.to_owned(),
        driver_rank_settings: parse_driver_rank_settings(overview),
        player_driver_elo: parse_authenticated_driver_elo(overview),
        profiles: HashMap::new(),
        profiles_checked: false,
    }
}

fn parse_authenticated_driver_elo(overview: &Value) -> Option<f64> {
    let split = object_value_from_value(overview, "split")?;
    let drivers = object_value_from_value(split, "drivers")?.as_array()?;
    drivers.iter().find_map(|registration| {
        let driver = object_value_from_value(registration, "driver").unwrap_or(registration);
        object_value_from_value(driver, "driverRank").and_then(super::driver_ranks::rank_elo)
    })
}

fn find_event_overview<'a>(value: &'a Value, event_id: &str) -> Option<&'a Value> {
    match value {
        Value::Object(object) => {
            let event_identifier = ["eventId", "id"]
                .iter()
                .find_map(|key| object_value(object, key).and_then(Value::as_str));
            let matches_event =
                event_identifier.is_some_and(|candidate| candidate.eq_ignore_ascii_case(event_id));
            if matches_event
                && ["split", "splitNo", "splitNumber", "totalSplits", "splits"]
                    .iter()
                    .any(|key| object_value(object, key).is_some())
            {
                return Some(value);
            }
            if event_identifier.is_none()
                && object_value(object, "split").is_some()
                && ["totalSplits", "splitCount", "splits"]
                    .iter()
                    .any(|key| object_value(object, key).is_some())
            {
                return Some(value);
            }
            object
                .values()
                .find_map(|child| find_event_overview(child, event_id))
        }
        Value::Array(items) => items
            .iter()
            .find_map(|child| find_event_overview(child, event_id)),
        _ => None,
    }
}

fn parse_driver_rank_settings(value: &Value) -> DriverRankSettings {
    let Some(settings) = find_object_key(value, "drSettings") else {
        return DriverRankSettings::default();
    };
    let defaults = DriverRankSettings::default();
    DriverRankSettings {
        multiplier: positive_f64(object_value(settings, "base")).unwrap_or(defaults.multiplier),
        k: positive_f64(object_value(settings, "k")).unwrap_or(defaults.k),
        distance: positive_f64(object_value(settings, "d")).unwrap_or(defaults.distance),
        logarithm: positive_f64(object_value(settings, "log"))
            .filter(|value| *value > 1.0)
            .unwrap_or(defaults.logarithm),
    }
}

fn find_object_key<'a>(value: &'a Value, key: &str) -> Option<&'a serde_json::Map<String, Value>> {
    match value {
        Value::Object(object) => object
            .iter()
            .find(|(candidate, _)| candidate.eq_ignore_ascii_case(key))
            .map(|(_, value)| value)
            .and_then(Value::as_object)
            .or_else(|| {
                object
                    .values()
                    .find_map(|child| find_object_key(child, key))
            }),
        Value::Array(items) => items.iter().find_map(|child| find_object_key(child, key)),
        _ => None,
    }
}

fn object_value<'a>(object: &'a serde_json::Map<String, Value>, key: &str) -> Option<&'a Value> {
    object
        .iter()
        .find(|(candidate, _)| candidate.eq_ignore_ascii_case(key))
        .map(|(_, value)| value)
}

fn object_value_from_value<'a>(value: &'a Value, key: &str) -> Option<&'a Value> {
    object_value(value.as_object()?, key)
}

fn positive_f64(value: Option<&Value>) -> Option<f64> {
    let number = value?.as_f64().or_else(|| value?.as_str()?.parse().ok())?;
    (number.is_finite() && number > 0.0).then_some(number)
}

fn find_positive_key(value: &Value, keys: &[&str]) -> Option<u32> {
    match value {
        Value::Object(object) => {
            for key in keys {
                if let Some(number) = object.get(*key).and_then(positive_u32) {
                    return Some(number);
                }
            }
            object
                .values()
                .find_map(|child| find_positive_key(child, keys))
        }
        Value::Array(items) => items
            .iter()
            .find_map(|child| find_positive_key(child, keys)),
        _ => None,
    }
}

fn find_direct_positive_key(value: &Value, keys: &[&str]) -> Option<u32> {
    let object = value.as_object()?;
    keys.iter()
        .find_map(|key| object_value(object, key).and_then(positive_u32))
}

fn find_array_len(value: &Value, key: &str) -> Option<u32> {
    match value {
        Value::Object(object) => object
            .get(key)
            .and_then(Value::as_array)
            .and_then(|items| u32::try_from(items.len()).ok())
            .or_else(|| object.values().find_map(|child| find_array_len(child, key))),
        Value::Array(items) => items.iter().find_map(|child| find_array_len(child, key)),
        _ => None,
    }
}

fn positive_u32(value: &Value) -> Option<u32> {
    let number = value
        .as_u64()
        .and_then(|number| u32::try_from(number).ok())
        .or_else(|| value.as_str().and_then(|number| number.parse::<u32>().ok()))?;
    (number > 0).then_some(number)
}

fn latest_online_event_id() -> Option<String> {
    let latest = super::install::installations()
        .into_iter()
        .flat_map(|installation| {
            fs::read_dir(installation.join("UserData/Log"))
                .into_iter()
                .flatten()
                .filter_map(Result::ok)
        })
        .filter(|entry| {
            entry.file_name().to_string_lossy().starts_with("trace_")
                && entry
                    .path()
                    .extension()
                    .is_some_and(|extension| extension == "txt")
        })
        .max_by_key(|entry| {
            entry
                .metadata()
                .and_then(|metadata| metadata.modified())
                .unwrap_or(SystemTime::UNIX_EPOCH)
        })?;
    event_id_from_trace(&latest.path())
}

fn event_id_from_trace(path: &Path) -> Option<String> {
    let mut file = File::open(path).ok()?;
    let length = file.metadata().ok()?.len();
    let overlap = EVENT_MARKER.len() + 36;
    let mut end = length;
    let mut following_prefix = Vec::new();

    while end > 0 {
        let start = end.saturating_sub(TRACE_CHUNK_BYTES);
        file.seek(SeekFrom::Start(start)).ok()?;
        let mut bytes = vec![0; (end - start) as usize];
        file.read_exact(&mut bytes).ok()?;
        bytes.extend_from_slice(&following_prefix);
        if let Some(event_id) = event_id_from_text(&String::from_utf8_lossy(&bytes)) {
            return Some(event_id);
        }
        following_prefix = bytes[..bytes.len().min(overlap)].to_vec();
        end = start;
    }
    None
}

fn event_id_from_text(text: &str) -> Option<String> {
    text.rmatch_indices(EVENT_MARKER).find_map(|(marker, _)| {
        let start = marker + EVENT_MARKER.len();
        let candidate = text.get(start..start + 36)?;
        is_guid(candidate).then(|| candidate.to_owned())
    })
}

fn cached_event_split(event_id: &str) -> Option<SessionSplit> {
    super::install::installations()
        .into_iter()
        .flat_map(|installation| {
            [
                installation.join("UserData/Replays/coherent_local_storage.json"),
                installation.join("coherent_local_storage.json"),
            ]
        })
        .filter_map(|installation| {
            let modified = fs::metadata(&installation).ok()?.modified().ok()?;
            let json = fs::read_to_string(installation).ok()?;
            Some((modified, json))
        })
        .collect::<Vec<_>>()
        .into_iter()
        .max_by_key(|(modified, _)| *modified)
        .and_then(|(_, json)| parse_cached_event_split(&json, event_id))
}

fn parse_cached_event_split(json: &str, event_id: &str) -> Option<SessionSplit> {
    let outer: Value = serde_json::from_str(json).ok()?;
    let stored = find_value_key(&outer, "lmu.cs.registeredEvents")?.as_str()?;
    let registered: Value = serde_json::from_str(stored).ok()?;
    let event = find_object_with_event_id(&registered, event_id)?;
    Some(parse_event_split(&Value::Object(event.clone()), event_id))
}

fn find_value_key<'a>(value: &'a Value, key: &str) -> Option<&'a Value> {
    match value {
        Value::Object(object) => object
            .iter()
            .find(|(candidate, _)| candidate.eq_ignore_ascii_case(key))
            .map(|(_, value)| value)
            .or_else(|| object.values().find_map(|child| find_value_key(child, key))),
        Value::Array(items) => items.iter().find_map(|child| find_value_key(child, key)),
        _ => None,
    }
}

fn find_object_with_event_id<'a>(
    value: &'a Value,
    event_id: &str,
) -> Option<&'a serde_json::Map<String, Value>> {
    match value {
        Value::Object(object) => {
            let matches = ["eventId", "id"].iter().any(|key| {
                object
                    .get(*key)
                    .and_then(Value::as_str)
                    .is_some_and(|candidate| candidate.eq_ignore_ascii_case(event_id))
            });
            matches.then_some(object).or_else(|| {
                object
                    .values()
                    .find_map(|child| find_object_with_event_id(child, event_id))
            })
        }
        Value::Array(items) => items
            .iter()
            .find_map(|child| find_object_with_event_id(child, event_id)),
        _ => None,
    }
}

fn is_guid(value: &str) -> bool {
    value.len() == 36
        && value.bytes().enumerate().all(|(index, byte)| match index {
            8 | 13 | 18 | 23 => byte == b'-',
            _ => byte.is_ascii_hexdigit(),
        })
}

#[cfg(test)]
mod tests {
    use super::{
        event_id_from_text, event_overview_request, parse_cached_event_split, parse_event_profiles,
        parse_event_split, DriverRankSettings, SessionSplit, SessionSplitResolver, SplitAttempt,
        MAX_RETRY_INTERVAL, RETRY_INTERVAL,
    };

    fn failed_attempt(event_id: &str) -> SplitAttempt {
        SplitAttempt {
            event_id: event_id.into(),
            result: Err("racecontrol_my_split_http_500".into()),
        }
    }

    fn settled_attempt(event_id: &str) -> SplitAttempt {
        SplitAttempt {
            event_id: event_id.into(),
            result: Ok(SessionSplit {
                number: 2,
                count: 8,
                event_id: event_id.into(),
                profiles_checked: true,
                ..SessionSplit::default()
            }),
        }
    }

    #[test]
    fn resolved_split_is_rechecked_for_a_new_event() {
        let mut resolver = SessionSplitResolver::discover();
        resolver.current = SessionSplit {
            number: 2,
            count: 8,
            event_id: "event-id".into(),
            driver_rank_settings: DriverRankSettings::default(),
            ..SessionSplit::default()
        };

        resolver.refresh();

        assert!(resolver.receiver.is_some());
        assert!(resolver.last_attempt.is_some());
        assert_eq!(resolver.request_revisions().0, 1);
    }

    #[test]
    fn resolution_revision_identifies_the_successful_request_not_failed_responses() {
        let mut resolver = SessionSplitResolver::empty();
        for (revision, attempt, expected_resolved) in [
            (1, settled_attempt("old-event"), 1),
            (2, failed_attempt("new-event"), 1),
            (3, settled_attempt("new-event"), 3),
        ] {
            let (sender, receiver) = std::sync::mpsc::channel();
            resolver.receiver = Some(receiver);
            resolver.request_revision = revision;
            sender.send(attempt).unwrap();
            resolver.receive_result();
            assert_eq!(resolver.request_revisions(), (revision, expected_resolved));
        }
    }

    #[test]
    fn split_without_total_keeps_retrying() {
        let mut resolver = SessionSplitResolver::discover();
        resolver.current = SessionSplit {
            number: 2,
            count: 0,
            event_id: "event-id".into(),
            driver_rank_settings: DriverRankSettings::default(),
            ..SessionSplit::default()
        };

        resolver.refresh();

        assert!(resolver.receiver.is_some());
    }

    #[test]
    fn repeated_failures_stretch_the_retry_interval() {
        let mut resolver = SessionSplitResolver::discover();
        assert_eq!(resolver.retry_interval(), RETRY_INTERVAL);

        resolver.note_attempt(&failed_attempt("event-id"));
        assert_eq!(resolver.retry_interval(), RETRY_INTERVAL);

        resolver.note_attempt(&failed_attempt("event-id"));
        assert_eq!(resolver.retry_interval(), RETRY_INTERVAL * 2);

        for _ in 0..20 {
            resolver.note_attempt(&failed_attempt("event-id"));
        }
        assert_eq!(resolver.retry_interval(), MAX_RETRY_INTERVAL);
    }

    #[test]
    fn a_resolved_split_returns_to_the_normal_interval() {
        let mut resolver = SessionSplitResolver::discover();
        for _ in 0..5 {
            resolver.note_attempt(&failed_attempt("event-id"));
        }
        assert!(resolver.retry_interval() > RETRY_INTERVAL);

        resolver.note_attempt(&settled_attempt("event-id"));

        assert_eq!(resolver.retry_interval(), RETRY_INTERVAL);
    }

    #[test]
    fn a_new_event_starts_the_backoff_over() {
        let mut resolver = SessionSplitResolver::discover();
        for _ in 0..5 {
            resolver.note_attempt(&failed_attempt("stale-event-id"));
        }
        assert!(resolver.retry_interval() > RETRY_INTERVAL);

        resolver.note_attempt(&failed_attempt("fresh-event-id"));

        assert_eq!(resolver.retry_interval(), RETRY_INTERVAL);
    }

    #[test]
    fn no_online_event_is_not_a_failure() {
        let mut resolver = SessionSplitResolver::discover();
        for _ in 0..5 {
            resolver.note_attempt(&failed_attempt("event-id"));
        }

        resolver.note_attempt(&SplitAttempt {
            event_id: String::new(),
            result: Ok(SessionSplit::default()),
        });

        assert_eq!(resolver.retry_interval(), RETRY_INTERVAL);
    }

    #[test]
    fn an_unresolved_answer_counts_as_a_failure() {
        let mut resolver = SessionSplitResolver::discover();

        for _ in 0..2 {
            resolver.note_attempt(&SplitAttempt {
                event_id: "event-id".into(),
                result: Ok(SessionSplit {
                    number: 2,
                    count: 8,
                    event_id: "event-id".into(),
                    ..SessionSplit::default()
                }),
            });
        }

        assert_eq!(resolver.retry_interval(), RETRY_INTERVAL * 2);
    }

    #[test]
    fn finds_latest_event_id_in_text() {
        let text = "server for online event 11111111-1111-1111-1111-111111111111\n\
                    server for online event 22222222-2222-2222-2222-222222222222";
        assert_eq!(
            event_id_from_text(text).as_deref(),
            Some("22222222-2222-2222-2222-222222222222")
        );
    }

    #[test]
    fn parses_split_from_lmu_local_storage() {
        let registered = serde_json::json!({
            "state": {
                "registeredOnlineEvents": [{
                    "eventId": "event-id",
                    "splitNo": "3",
                    "event": { "id": "event-id", "totalSplits": 9 }
                }]
            }
        });
        let outer = serde_json::json!({
            "http://127.0.0.1:6397": {
                "lmu.cs.registeredEvents": registered.to_string()
            }
        });

        let split = parse_cached_event_split(&outer.to_string(), "event-id").unwrap();
        assert_eq!(split.number, 3);
        assert_eq!(split.count, 9);
    }

    #[test]
    fn parses_direct_my_split_response() {
        let response = serde_json::json!({
            "eventId": "event-id",
            "splitNo": "4",
            "totalSplits": 12
        });

        let split = parse_event_split(&response, "event-id");

        assert_eq!(split.number, 4);
        assert_eq!(split.count, 12);
    }

    #[test]
    fn parses_split_number_and_estimated_total() {
        let event = serde_json::json!({
            "configuration": { "settings": { "maxPlayers": 40 } },
            "registrations": vec![serde_json::json!({}); 81],
            "splits": [{ "splitNo": 2, "drivers": [] }]
        });
        assert_eq!(
            parse_event_split(&event, "event-id"),
            SessionSplit {
                number: 2,
                count: 3,
                event_id: "event-id".into(),
                driver_rank_settings: DriverRankSettings::default(),
                ..SessionSplit::default()
            }
        );
    }

    #[test]
    fn builds_dox_event_overview_request() {
        assert_eq!(
            event_overview_request("event-id"),
            serde_json::json!({
                "game": "lmu",
                "eventType": "daily",
                "eventId": "event-id",
            })
        );
    }

    #[test]
    fn parses_dox_event_overview_response() {
        let response = serde_json::json!([{
            "id": "event-id",
            "split": { "splitNo": "3" },
            "totalSplits": 12,
            "splits": [
                { "splitNo": 1 },
                { "splitNo": 2 },
                { "splitNo": 3 }
            ]
        }]);

        assert_eq!(
            parse_event_split(&response, "event-id"),
            SessionSplit {
                number: 3,
                count: 12,
                event_id: "event-id".into(),
                driver_rank_settings: DriverRankSettings::default(),
                ..SessionSplit::default()
            }
        );
    }

    #[test]
    fn selects_requested_event_overview() {
        let response = serde_json::json!([
            {
                "id": "other-event",
                "split": { "splitNo": 1 },
                "totalSplits": 2
            },
            {
                "id": "event-id",
                "split": { "splitNo": 4 },
                "totalSplits": 9
            }
        ]);

        let split = parse_event_split(&response, "event-id");
        assert_eq!(split.number, 4);
        assert_eq!(split.count, 9);
    }

    #[test]
    fn accepts_server_split_as_a_string() {
        let event = serde_json::json!({
            "splits": [{ "splitNo": 0, "server": { "splitNo": "4" } }]
        });
        assert_eq!(parse_event_split(&event, "event-id").number, 4);
    }

    #[test]
    fn accepts_wrapped_racecontrol_split_response() {
        let response = serde_json::json!({
            "data": {
                "split": { "splitNumber": "3" },
                "totalSplits": 12
            }
        });
        assert_eq!(
            parse_event_split(&response, "event-id"),
            SessionSplit {
                number: 3,
                count: 12,
                event_id: "event-id".into(),
                driver_rank_settings: DriverRankSettings::default(),
                ..SessionSplit::default()
            }
        );
    }

    #[test]
    fn parses_driver_profiles_from_direct_split_roster() {
        let response = serde_json::json!({
            "drivers": [{
                "driver": {
                    "name": "Charles-Antoine Wipf",
                    "username": "dark_revan",
                    "profile": { "nationality": "fr", "badge": "sr-clean" }
                }
            }]
        });

        let profiles = parse_event_profiles(&response);
        for key in ["charles-antoine wipf", "dark_revan"] {
            assert_eq!(profiles[key].nationality, "FR");
            assert_eq!(profiles[key].badge, "sr-clean");
        }
    }

    #[test]
    fn parses_event_driver_rank_parameters() {
        let response = serde_json::json!({
            "configuration": {
                "ratings": {
                    "drSettings": { "base": 1.25, "k": 24, "d": 360, "log": 8 }
                }
            }
        });
        assert_eq!(
            parse_event_split(&response, "event-id").driver_rank_settings,
            DriverRankSettings {
                multiplier: 1.25,
                k: 24.0,
                distance: 360.0,
                logarithm: 8.0,
            }
        );
    }

    #[test]
    fn parses_authenticated_driver_elo_from_event_overview() {
        let response = serde_json::json!({
            "id": "event-id",
            "split": {
                "splitNo": 2,
                "drivers": [{
                    "driver": {
                        "username": "player",
                        "driverRank": { "elo": 1456.25, "rank": "Silver", "tier": 1 }
                    }
                }]
            },
            "totalSplits": 5,
            "sof": { "elo": 1800 }
        });

        assert_eq!(
            parse_event_split(&response, "event-id").player_driver_elo,
            Some(1456.25)
        );
    }
}
