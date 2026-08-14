use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::{Duration, Instant};

const RACECONTROL_PLAYERS_URL: &str = "https://raceos.gg/api/v1/players";
const RETRY_INTERVAL: Duration = Duration::from_secs(10);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Clone, Debug, PartialEq)]
pub(super) struct DriverRanks {
    pub driver: String,
    pub driver_progress: f64,
    pub driver_elo: f64,
    pub safety: String,
    pub safety_progress: f64,
    pub nationality: String,
    pub badge: String,
}

impl Default for DriverRanks {
    fn default() -> Self {
        Self {
            driver: String::new(),
            driver_progress: -1.0,
            driver_elo: -1.0,
            safety: String::new(),
            safety_progress: -1.0,
            nationality: String::new(),
            badge: String::new(),
        }
    }
}

struct RankFetchResult {
    requested_names: Vec<String>,
    fixed_session: bool,
    result: Result<HashMap<String, DriverRanks>, String>,
}

pub(super) struct DriverRankResolver {
    ranks: HashMap<String, DriverRanks>,
    queried_names: HashSet<String>,
    resolved_roster_names: HashSet<String>,
    lookup_keys: HashMap<String, String>,
    receiver: Option<Receiver<RankFetchResult>>,
    last_attempt: Option<Instant>,
    fixed_session_attempted: bool,
    logging_generation: Option<u64>,
    logged_refresh: Option<String>,
    logged_lookups: HashMap<String, String>,
}

impl DriverRankResolver {
    pub(super) fn discover() -> Self {
        Self {
            ranks: HashMap::new(),
            queried_names: HashSet::new(),
            resolved_roster_names: HashSet::new(),
            lookup_keys: HashMap::new(),
            receiver: None,
            last_attempt: None,
            fixed_session_attempted: false,
            logging_generation: None,
            logged_refresh: None,
            logged_lookups: HashMap::new(),
        }
    }

    #[cfg(test)]
    pub(super) fn empty() -> Self {
        Self::discover()
    }

    pub(super) fn begin_session(&mut self) {
        self.receiver = None;
        self.last_attempt = None;
        self.fixed_session_attempted = false;
        // Keep the resolved profiles as an immediate fallback, but allow one
        // fresh roster request in the new session. DR changes after every rated
        // result, so treating a name queried in an earlier session as current
        // can noticeably skew the live estimate.
        self.queried_names.clear();
        self.resolved_roster_names.clear();
        self.logged_refresh = None;
        self.logged_lookups.clear();
    }

    fn logging_active(&mut self) -> bool {
        let Some(generation) = super::analysis_logging_generation() else {
            return false;
        };
        if self.logging_generation != Some(generation) {
            self.logging_generation = Some(generation);
            self.logged_refresh = None;
            self.logged_lookups.clear();
        }
        true
    }

    pub(super) fn refresh(&mut self, driver_names: &[&str], session_type: i32) {
        self.receive_result();

        if self.receiver.is_some() {
            return;
        }

        let practice_session = (0..=4).contains(&session_type);
        if !practice_session && self.fixed_session_attempted {
            return;
        }

        if driver_names
            .iter()
            .filter(|name| !name.trim().is_empty())
            .all(|name| self.resolved_roster_names.contains(name.trim()))
        {
            return;
        }

        let mut names = driver_names
            .iter()
            .map(|name| name.trim())
            .filter(|name| !name.is_empty())
            .map(str::to_owned)
            .collect::<Vec<_>>();
        names.sort_by_key(|name| normalized_name(name));
        names.dedup_by(|left, right| normalized_name(left) == normalized_name(right));
        if names.is_empty() {
            return;
        }

        let now = Instant::now();
        let contains_new_name = names
            .iter()
            .any(|name| !self.queried_names.contains(&normalized_name(name)));
        if !contains_new_name {
            return;
        }
        if self
            .last_attempt
            .is_some_and(|attempt| now.duration_since(attempt) < RETRY_INTERVAL)
        {
            return;
        }

        self.last_attempt = Some(now);
        let (sender, receiver) = mpsc::channel();
        self.receiver = Some(receiver);
        let requested_names = names.clone();
        thread::spawn(move || {
            let result = fetch_driver_ranks(&requested_names);
            let _ = sender.send(RankFetchResult {
                requested_names,
                fixed_session: !practice_session,
                result,
            });
        });

        self.log_refresh(serde_json::json!({
            "event": "driver_rank_refresh",
            "source": "racecontrol_players_by_username",
            "status": "request_started",
            "requested_count": names.len(),
            "cache_entries": self.ranks.len(),
        }));
    }

    fn receive_result(&mut self) {
        let Some(receiver) = self.receiver.as_ref() else {
            return;
        };
        let Ok(response) = receiver.try_recv() else {
            return;
        };
        self.receiver = None;

        match response.result {
            Ok(discovered) => {
                if !discovered.is_empty() {
                    for name in &response.requested_names {
                        if discovered.contains_key(&normalized_name(name)) {
                            self.queried_names.insert(normalized_name(name));
                            self.resolved_roster_names.insert(name.trim().to_owned());
                        }
                    }
                    if response.fixed_session
                        && response
                            .requested_names
                            .iter()
                            .all(|name| discovered.contains_key(&normalized_name(name)))
                    {
                        self.fixed_session_attempted = true;
                    }
                }
                let received = discovered.len();
                self.ranks.extend(discovered);
                self.logged_lookups.clear();
                self.log_refresh(serde_json::json!({
                    "event": "driver_rank_refresh",
                    "source": "racecontrol_players_by_username",
                    "status": "resolved",
                    "requested_count": response.requested_names.len(),
                    "profiles_received": received,
                    "cache_entries": self.ranks.len(),
                }));
            }
            Err(error) => self.log_refresh(serde_json::json!({
                "event": "driver_rank_refresh",
                "source": "racecontrol_players_by_username",
                "status": "failed",
                "requested_count": response.requested_names.len(),
                "error": error,
                "cache_entries": self.ranks.len(),
            })),
        }
    }

    fn log_refresh(&mut self, diagnostic: Value) {
        if !self.logging_active() {
            return;
        }
        let signature = diagnostic.to_string();
        if self.logged_refresh.as_deref() != Some(&signature) {
            super::queue_analysis_event(diagnostic);
            self.logged_refresh = Some(signature);
        }
    }

    pub(super) fn lookup(&mut self, driver_name: &str) -> Option<DriverRanks> {
        if !self.lookup_keys.contains_key(driver_name) {
            self.lookup_keys
                .insert(driver_name.to_owned(), normalized_name(driver_name));
        }
        let (ranks, was_queried) = {
            let key = &self.lookup_keys[driver_name];
            (
                self.ranks.get(key).cloned(),
                self.queried_names.contains(key),
            )
        };
        if self.logging_active() {
            let status = if ranks.is_some() {
                "resolved"
            } else if self.receiver.is_some() {
                "pending"
            } else if was_queried {
                "not_found"
            } else {
                "not_queried"
            };
            let diagnostic = serde_json::json!({
                "event": "driver_rank_lookup",
                "driver_name": driver_name,
                "status": status,
                "driver_rank": ranks.as_ref().map(|rank| rank.driver.as_str()).unwrap_or(""),
                "driver_rank_progress": ranks.as_ref().map(|rank| rank.driver_progress).unwrap_or(-1.0),
                "driver_elo": ranks.as_ref().map(|rank| rank.driver_elo).unwrap_or(-1.0),
                "safety_rank": ranks.as_ref().map(|rank| rank.safety.as_str()).unwrap_or(""),
                "safety_rank_progress": ranks.as_ref().map(|rank| rank.safety_progress).unwrap_or(-1.0),
                "nationality": ranks.as_ref().map(|rank| rank.nationality.as_str()).unwrap_or(""),
                "driver_badge": ranks.as_ref().map(|rank| rank.badge.as_str()).unwrap_or(""),
                "cache_entries": self.ranks.len(),
            });
            let signature = diagnostic.to_string();
            if self.logged_lookups.get(driver_name) != Some(&signature) {
                super::queue_analysis_event(diagnostic);
                self.logged_lookups
                    .insert(driver_name.to_owned(), signature);
            }
        }
        ranks
    }
}

fn fetch_driver_ranks(driver_names: &[String]) -> Result<HashMap<String, DriverRanks>, String> {
    let client = reqwest::blocking::Client::builder()
        .timeout(REQUEST_TIMEOUT)
        .build()
        .map_err(|_| "http_client_initialization_failed".to_owned())?;

    let access_token = super::racecontrol::authenticate(&client)?;

    let profiles_response = client
        .post(RACECONTROL_PLAYERS_URL)
        .header("Game-Authorization", format!("Bearer {access_token}"))
        .json(&serde_json::json!({ "usernames": driver_names }))
        .send()
        .map_err(|_| "racecontrol_players_unavailable".to_owned())?;
    if !profiles_response.status().is_success() {
        return Err(format!(
            "racecontrol_players_http_{}",
            profiles_response.status().as_u16()
        ));
    }
    let profiles_json = profiles_response
        .json::<Value>()
        .map_err(|_| "racecontrol_players_invalid_json".to_owned())?;
    collect_profiles(&profiles_json)
}

fn collect_profiles(value: &Value) -> Result<HashMap<String, DriverRanks>, String> {
    let profiles = value
        .as_array()
        .or_else(|| value.get("players").and_then(Value::as_array))
        .ok_or_else(|| "racecontrol_players_invalid_shape".to_owned())?;
    let mut ranks = HashMap::new();
    for profile in profiles {
        let Some(username) = profile
            .get("username")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|name| !name.is_empty())
        else {
            continue;
        };
        let driver = profile.get("driverRank").and_then(rank_code);
        let driver_progress = profile
            .get("driverRank")
            .and_then(rank_progress)
            .unwrap_or(-1.0);
        let driver_elo = profile.get("driverRank").and_then(rank_elo).unwrap_or(-1.0);
        let safety = profile.get("safetyRank").and_then(rank_code);
        let safety_progress = profile
            .get("safetyRank")
            .and_then(rank_progress)
            .unwrap_or(-1.0);
        let nationality = profile_nationality(profile);
        let badge = profile_badge(profile);
        if driver.is_none() && safety.is_none() && nationality.is_empty() && badge.is_empty() {
            continue;
        }
        ranks.insert(
            normalized_name(username),
            DriverRanks {
                driver: driver.unwrap_or_default(),
                driver_progress,
                driver_elo,
                safety: safety.unwrap_or_default(),
                safety_progress,
                nationality,
                badge,
            },
        );
    }
    Ok(ranks)
}

fn rank_elo(value: &Value) -> Option<f64> {
    let elo = value
        .get("elo")
        .and_then(|value| value.as_f64().or_else(|| value.as_str()?.parse().ok()))?;
    (elo.is_finite() && elo > 0.0).then_some(elo)
}

pub(super) fn profile_badge(value: &Value) -> String {
    fn badge_field(value: &Value) -> Option<&str> {
        value
            .get("badge")
            .or_else(|| value.get("driverBadge"))
            .and_then(Value::as_str)
    }

    if let Some(badge) = value
        .get("profile")
        .and_then(badge_field)
        .or_else(|| badge_field(value))
    {
        return badge.trim().to_ascii_lowercase();
    }

    for candidate in [value.get("profile"), value.get("metadata")]
        .into_iter()
        .flatten()
    {
        let Some(parsed) = candidate
            .as_str()
            .and_then(|json| serde_json::from_str::<Value>(json).ok())
        else {
            continue;
        };
        if let Some(badge) = parsed
            .get("profile")
            .and_then(badge_field)
            .or_else(|| badge_field(&parsed))
        {
            return badge.trim().to_ascii_lowercase();
        }
    }

    String::new()
}

pub(super) fn profile_nationality(value: &Value) -> String {
    fn nationality_field(value: &Value) -> Option<&str> {
        ["nationality", "countryCode", "country_code", "country"]
            .into_iter()
            .find_map(|key| {
                let field = value.get(key)?;
                field.as_str().or_else(|| {
                    ["code", "iso2", "alpha2"]
                        .into_iter()
                        .find_map(|nested| field.get(nested).and_then(Value::as_str))
                })
            })
    }

    fn parsed_object(value: &Value) -> Option<Value> {
        value
            .as_str()
            .and_then(|json| serde_json::from_str::<Value>(json).ok())
    }

    let direct = value
        .get("profile")
        .and_then(|profile| nationality_field(profile))
        .or_else(|| nationality_field(value));
    if let Some(nationality) = direct {
        return nationality.trim().to_uppercase();
    }

    for candidate in [
        value.get("profile"),
        value.get("metadata"),
        value.pointer("/user/metadata"),
    ]
    .into_iter()
    .flatten()
    {
        let Some(parsed) = parsed_object(candidate) else {
            continue;
        };
        if let Some(nationality) = parsed
            .get("profile")
            .and_then(nationality_field)
            .or_else(|| nationality_field(&parsed))
        {
            return nationality.trim().to_uppercase();
        }
    }

    String::new()
}

fn rank_progress(value: &Value) -> Option<f64> {
    let progress = value.get("progress")?.as_f64()?;
    if !progress.is_finite() || progress < 0.0 {
        return None;
    }
    Some(if progress <= 1.0 && progress > 0.0 {
        progress * 100.0
    } else {
        progress.min(100.0)
    })
}

pub(super) fn normalized_name(value: &str) -> String {
    value.trim().to_lowercase()
}

fn rank_code(value: &Value) -> Option<String> {
    let object = value.as_object()?;
    let name = object.get("rank")?.as_str()?.trim();
    if name.is_empty() {
        return None;
    }
    let prefix = match name.to_ascii_lowercase().as_str() {
        "bronze" => "B",
        "silver" => "S",
        "gold" => "G",
        "platinum" => "P",
        _ => name.get(..1)?,
    };
    let tier = object.get("tier").and_then(Value::as_i64);
    Some(tier.map_or_else(|| prefix.to_owned(), |tier| format!("{prefix}{tier}")))
}

#[cfg(test)]
mod tests {
    use super::{collect_profiles, DriverRankResolver, DriverRanks, RankFetchResult};
    use std::collections::HashMap;
    use std::sync::mpsc;

    #[test]
    fn keeps_profile_cache_between_sessions() {
        let mut resolver = DriverRankResolver::empty();
        resolver.queried_names.insert("cached driver".to_owned());
        resolver
            .resolved_roster_names
            .insert("Cached Driver".to_owned());
        resolver.ranks.insert(
            "cached driver".to_owned(),
            DriverRanks {
                driver: "S1".to_owned(),
                ..DriverRanks::default()
            },
        );

        resolver.begin_session();

        assert!(resolver.queried_names.is_empty());
        assert!(resolver.resolved_roster_names.is_empty());
        assert_eq!(resolver.lookup("Cached Driver").unwrap().driver, "S1");
    }

    #[test]
    fn fixed_session_retries_after_failed_or_empty_profile_request() {
        let mut resolver = DriverRankResolver::empty();

        for result in [
            Err("temporary failure".to_owned()),
            Ok(HashMap::<String, DriverRanks>::new()),
        ] {
            let (sender, receiver) = mpsc::channel();
            sender
                .send(RankFetchResult {
                    requested_names: vec!["Test Driver".to_owned()],
                    fixed_session: true,
                    result,
                })
                .unwrap();
            resolver.receiver = Some(receiver);
            resolver.receive_result();

            assert!(!resolver.fixed_session_attempted);
            assert!(resolver.queried_names.is_empty());
        }
    }

    #[test]
    fn fixed_session_stops_after_profiles_are_received() {
        let mut resolver = DriverRankResolver::empty();
        let (sender, receiver) = mpsc::channel();
        sender
            .send(RankFetchResult {
                requested_names: vec!["Test Driver".to_owned()],
                fixed_session: true,
                result: Ok(HashMap::from([(
                    "test driver".to_owned(),
                    DriverRanks::default(),
                )])),
            })
            .unwrap();
        resolver.receiver = Some(receiver);
        resolver.receive_result();

        assert!(resolver.fixed_session_attempted);
        assert!(resolver.queried_names.contains("test driver"));
    }

    #[test]
    fn fixed_session_retries_names_missing_from_a_partial_response() {
        let mut resolver = DriverRankResolver::empty();
        let (sender, receiver) = mpsc::channel();
        sender
            .send(RankFetchResult {
                requested_names: vec!["Resolved Driver".to_owned(), "Missing Driver".to_owned()],
                fixed_session: true,
                result: Ok(HashMap::from([(
                    "resolved driver".to_owned(),
                    DriverRanks::default(),
                )])),
            })
            .unwrap();
        resolver.receiver = Some(receiver);
        resolver.receive_result();

        assert!(!resolver.fixed_session_attempted);
        assert!(resolver.queried_names.contains("resolved driver"));
        assert!(!resolver.queried_names.contains("missing driver"));
    }

    #[test]
    fn extracts_ranks_from_racecontrol_profiles() {
        let response = serde_json::json!([{
            "username": "Test Driver",
            "profile": { "nationality": "fr" },
            "badge": "sr-clean",
            "driverRank": { "rank": "Silver", "tier": 1, "elo": 1234, "progress": 50 },
            "safetyRank": { "rank": "Gold", "tier": 2, "rating": 80, "progress": 25 }
        }]);
        let ranks = collect_profiles(&response).unwrap();
        assert_eq!(
            ranks.get("test driver"),
            Some(&DriverRanks {
                driver: "S1".into(),
                driver_progress: 50.0,
                driver_elo: 1234.0,
                safety: "G2".into(),
                safety_progress: 25.0,
                nationality: "FR".into(),
                badge: "sr-clean".into(),
            })
        );
    }

    #[test]
    fn accepts_wrapped_players_response() {
        let response = serde_json::json!({
            "players": [{
                "username": "Another Driver",
                "driverRank": { "rank": "Bronze", "tier": 3 }
            }]
        });
        assert_eq!(
            collect_profiles(&response).unwrap()["another driver"].driver,
            "B3"
        );
    }

    #[test]
    fn extracts_nationality_from_serialized_profile() {
        let response = serde_json::json!([{
            "username": "Serialized Driver",
            "profile": "{\"nationality\":\"es\"}",
            "driverRank": { "rank": "Silver", "tier": 2 }
        }]);
        assert_eq!(
            collect_profiles(&response).unwrap()["serialized driver"].nationality,
            "ES"
        );
    }

    #[test]
    fn extracts_nationality_from_nakama_metadata() {
        let response = serde_json::json!([{
            "username": "Metadata Driver",
            "metadata": "{\"profile\":{\"nationality\":\"gb\"}}",
            "safetyRank": { "rank": "Platinum", "tier": 3 }
        }]);
        assert_eq!(
            collect_profiles(&response).unwrap()["metadata driver"].nationality,
            "GB"
        );
    }

    #[test]
    fn extracts_badge_from_serialized_profile() {
        let response = serde_json::json!([{
            "username": "Trusted Driver",
            "profile": "{\"badge\":\"sr-saint\"}"
        }]);
        assert_eq!(
            collect_profiles(&response).unwrap()["trusted driver"].badge,
            "sr-saint"
        );
    }
}
