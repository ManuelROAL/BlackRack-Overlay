use crate::telemetry::WeatherForecastNode;
#[cfg(not(test))]
use reqwest::blocking::Client;
#[cfg(not(test))]
use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicU8, Ordering};
#[cfg(not(test))]
use std::sync::mpsc;
use std::sync::mpsc::Receiver;
use std::sync::{Arc, Mutex};
#[cfg(not(test))]
use std::thread;
use std::time::{Duration, Instant};

const STANDINGS_DEMAND: u8 = 1 << 0;
const SUPPLEMENT_DEMAND: u8 = 1 << 1;
const WEATHER_DEMAND: u8 = 1 << 2;
const CHAT_DEMAND: u8 = 1 << 3;

/// `connected` only says the game is running with its plugin loaded, which is
/// already true at the main menu. The supplement worker additionally needs the
/// player to have a car, because every endpoint it polls lives under
/// `/rest/garage/` and answers about a vehicle: with none loaded LMU either
/// 404s them or, in the case that took the game down, never answers at all.
fn rest_demand(
    connected: bool,
    player_has_vehicle: bool,
    standings_requested: bool,
    supplement_requested: bool,
    weather_requested: bool,
    chat_requested: bool,
) -> u8 {
    if connected {
        (u8::from(standings_requested) * STANDINGS_DEMAND)
            | (u8::from(supplement_requested && player_has_vehicle) * SUPPLEMENT_DEMAND)
            | (u8::from(weather_requested) * WEATHER_DEMAND)
            | (u8::from(chat_requested) * CHAT_DEMAND)
    } else {
        0
    }
}

#[cfg(not(test))]
const LOCAL_API: &str = "http://127.0.0.1:6397";
#[cfg(not(test))]
const STANDINGS_INTERVAL: Duration = Duration::from_secs(1);
#[cfg(not(test))]
const HISTORY_INTERVAL: Duration = Duration::from_secs(5);
#[cfg(not(test))]
const SUPPLEMENT_INTERVAL: Duration = Duration::from_secs(1);
const STANDINGS_MAX_AGE: Duration = Duration::from_secs(1);
const FOCUS_MAX_AGE: Duration = Duration::from_secs(3);
const SUPPLEMENT_MAX_AGE: Duration = Duration::from_secs(3);
const WEATHER_MAX_AGE: Duration = Duration::from_secs(5);
#[cfg(not(test))]
const GARAGE_INTERVAL: Duration = Duration::from_secs(5);
#[cfg(not(test))]
const WEATHER_INTERVAL: Duration = Duration::from_secs(1);
#[cfg(not(test))]
const CHAT_INTERVAL: Duration = Duration::from_secs(2);
#[cfg(not(test))]
const REQUEST_TIMEOUT: Duration = Duration::from_millis(400);
/// The compounds a car carries and their optimal temperatures do not change
/// within a session, so this ~8 KB response is refreshed rarely and only to
/// catch a car that was not loaded yet when the session began.
#[cfg(not(test))]
const TIRE_COMPOUND_INTERVAL: Duration = Duration::from_secs(30);

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub(super) struct RestStanding {
    #[serde(rename = "slotID")]
    pub slot_id: i32,
    pub driver_name: String,
    pub focus: bool,
    pub has_focus: bool,
    pub car_class: String,
    pub car_number: String,
    pub position: i32,
    pub qualification: i32,
    pub server_scored: bool,
    pub finish_status: String,
    pub laps_behind_class_leader: i32,
    pub time_behind_class_leader: f64,
    pub laps_behind_next: i32,
    pub time_behind_next: f64,
    pub pitstops: u32,
    pub pit_state: String,
    pub pitting: bool,
    pub in_garage_stall: bool,
    pub fuel_fraction: f64,
    pub ve_fraction: f64,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub(super) struct RestStandingHistory {
    pub car_class: String,
    pub driver_name: String,
    pub lap_time: f64,
    #[serde(rename = "slotID")]
    pub slot_id: i32,
    pub total_laps: i32,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub(super) struct RestPitStopEstimate {
    pub brake_ducts: f64,
    pub brakes: f64,
    pub damage: f64,
    pub driver_swap: f64,
    pub fuel: f64,
    pub penalties: f64,
    pub tires: f64,
    pub total: f64,
    pub ve: f64,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct RestSessionInfo {
    max_time: f64,
    player_name: String,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
struct RestGarageData {
    #[serde(rename = "VM_STEER_LOCK")]
    steering_lock: RestGarageSetting,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct RestGarageSetting {
    string_value: String,
}

#[derive(Clone, Copy, Debug, Default)]
struct RestVehicleDamage {
    aero: f64,
    suspension: [f64; 4],
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct RestTireManagement {
    optimal_compound_conditions: RestOptimalCompoundConditions,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct RestOptimalCompoundConditions {
    compounds: Vec<RestCompoundCondition>,
}

/// The compounds the current car actually carries and the optimal temperature
/// of each. The order is the car's own and has nothing to do with
/// `mCompoundType`, which is a fixed soft/medium/hard/wet enum: an LMP2
/// carrying only Medium and Wet reports 1 for its mediums, and indexing this
/// list with that 1 lands on the Wet entry. It is searched by compound.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub(super) struct RestCompoundCondition {
    #[serde(rename = "type")]
    pub compound_type: String,
    pub optimal_temperature: f64,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
struct RestRepairAndRefuel {
    #[serde(rename = "teamInfo")]
    team_info: RestTeamInfo,
    wearables: RestWearables,
    #[serde(rename = "pitMenu")]
    pit_menu: RestPitMenu,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct RestTeamInfo {
    #[serde(deserialize_with = "deserialize_driver_names")]
    driver_names: Vec<String>,
    team_name: String,
    vehicle_name: String,
}

/// LMU does not serialise a driver name as a string: it dumps the raw
/// fixed-size C buffer, an array of character codes with NUL padding. A plain
/// `Vec<String>` rejects that, and because serde fails the whole response, one
/// unreadable name used to cost the entire `RepairAndRefuel` payload — aero
/// damage, pit menu and team identity included.
#[derive(Clone, Debug, Deserialize)]
#[serde(untagged)]
enum RestDriverName {
    Text(String),
    Buffer(Vec<u32>),
}

impl RestDriverName {
    fn into_name(self) -> String {
        match self {
            Self::Text(text) => text.trim().to_owned(),
            Self::Buffer(codes) => {
                let bytes: Vec<u8> = codes
                    .into_iter()
                    .take_while(|code| *code != 0)
                    .filter_map(|code| u8::try_from(code).ok())
                    .collect();
                String::from_utf8_lossy(&bytes).trim().to_owned()
            }
        }
    }
}

fn deserialize_driver_names<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Ok(Vec::<RestDriverName>::deserialize(deserializer)?
        .into_iter()
        .map(RestDriverName::into_name)
        .filter(|name| !name.is_empty())
        .collect())
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct RestPitMenu {
    pit_menu: Vec<RestPitMenuItem>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct RestPitMenuItem {
    name: String,
    current_setting: usize,
    #[serde(rename = "default")]
    default_setting: Option<usize>,
    settings: Vec<RestPitMenuSetting>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
struct RestPitMenuSetting {
    text: String,
}

#[derive(Clone, Debug, Default)]
pub(super) struct RestPitMenuChange {
    pub(super) label: String,
    pub(super) value: String,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
struct RestWearables {
    body: RestBodyWear,
    suspension: [f64; 4],
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
struct RestBodyWear {
    aero: f64,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub(super) struct RestWeatherSession {
    #[serde(rename = "START")]
    pub start: RestWeatherNode,
    #[serde(rename = "NODE_25")]
    pub node_25: RestWeatherNode,
    #[serde(rename = "NODE_50")]
    pub node_50: RestWeatherNode,
    #[serde(rename = "NODE_75")]
    pub node_75: RestWeatherNode,
    #[serde(rename = "FINISH")]
    pub finish: RestWeatherNode,
}

impl RestWeatherSession {
    pub(super) fn forecast_nodes(&self) -> Vec<WeatherForecastNode> {
        [
            &self.start,
            &self.node_25,
            &self.node_50,
            &self.node_75,
            &self.finish,
        ]
        .iter()
        .map(|node| WeatherForecastNode {
            sky: node.sky.current_value.round() as i32,
            sky_label: node.sky.string_value.clone(),
            temperature_c: node.temperature.current_value,
            rain_chance_percent: node.rain_chance.current_value.clamp(0.0, 100.0),
            humidity_percent: node
                .humidity
                .as_ref()
                .and_then(|humidity| humidity.current_value)
                .filter(|value| value.is_finite())
                .map(|value| value.clamp(0.0, 100.0)),
            minutes_from_now: None,
        })
        .collect()
    }

    pub(super) fn wind_at(&self, index: usize) -> Option<(f64, f64)> {
        let node = [
            &self.start,
            &self.node_25,
            &self.node_50,
            &self.node_75,
            &self.finish,
        ]
        .get(index)
        .copied()?;
        let speed_ms = node.wind_speed.current_value;
        let direction_index = node.wind_direction.current_value;
        (speed_ms.is_finite() && speed_ms > 0.0 && direction_index.is_finite()).then_some((
            speed_ms,
            (direction_index.round().rem_euclid(8.0) * 45.0) % 360.0,
        ))
    }
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub(super) struct RestWeatherNode {
    #[serde(rename = "WNV_TEMPERATURE")]
    pub temperature: RestWeatherMetric,
    #[serde(rename = "WNV_RAIN_CHANCE")]
    pub rain_chance: RestWeatherMetric,
    #[serde(rename = "WNV_SKY")]
    pub sky: RestWeatherMetric,
    #[serde(rename = "WNV_HUMIDITY")]
    pub humidity: Option<RestWeatherHumidityMetric>,
    #[serde(rename = "WNV_WINDSPEED")]
    pub wind_speed: RestWeatherMetric,
    #[serde(rename = "WNV_WINDDIRECTION")]
    pub wind_direction: RestWeatherMetric,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub(super) struct RestWeatherMetric {
    pub current_value: f64,
    pub string_value: String,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub(super) struct RestWeatherHumidityMetric {
    pub current_value: Option<f64>,
    pub string_value: String,
}

#[derive(Default)]
struct SupplementUpdate {
    generation: u64,
    pit_stop: Option<RestPitStopEstimate>,
    pit_menu_changes: Option<Vec<RestPitMenuChange>>,
    compound_conditions: Option<Vec<RestCompoundCondition>>,
    vehicle_damage: Option<RestVehicleDamage>,
    fuel_ratio_assigned: Option<f64>,
    pit_refill_targets: Option<RestPitRefillTargets>,
    session_info: Option<RestSessionInfo>,
    team_info: Option<RestTeamInfo>,
    steering_range_degrees: Option<f64>,
}

#[derive(Default)]
struct StandingsUpdate {
    generation: u64,
    standings: Vec<RestStanding>,
    history: Option<HashMap<String, Vec<RestStandingHistory>>>,
}

struct WeatherUpdate {
    generation: u64,
    session: String,
    weather: RestWeatherSession,
}

#[derive(PartialEq, Eq, PartialOrd, Ord)]
enum ChatTimestampOrder {
    Number(i64),
    Text(String),
    Missing(usize),
}

fn parse_chat_messages(value: &Value) -> Option<Vec<crate::telemetry::ChatMessage>> {
    let entries = value.as_array().or_else(|| {
        value
            .get("Messages")
            .or_else(|| value.get("messages"))
            .and_then(Value::as_array)
    })?;
    let mut messages = Vec::new();
    for (index, entry) in entries.iter().enumerate() {
        let Some(raw) = entry
            .get("Message")
            .or_else(|| entry.get("message"))
            .and_then(Value::as_str)
        else {
            continue;
        };
        let Some((name, text)) = raw.split_once(':') else {
            continue;
        };
        let (name, text) = (name.trim(), text.trim());
        if name.is_empty() || text.is_empty() {
            continue;
        }
        let timestamp_value = entry.get("Timestamp").or_else(|| entry.get("timestamp"));
        let timestamp_text = timestamp_value.and_then(Value::as_str);
        let timestamp_number = timestamp_value
            .and_then(Value::as_i64)
            .or_else(|| timestamp_text.and_then(|text| text.parse::<i64>().ok()));
        let timestamp_order = timestamp_number
            .map(ChatTimestampOrder::Number)
            .or_else(|| timestamp_text.map(|text| ChatTimestampOrder::Text(text.to_owned())))
            .unwrap_or(ChatTimestampOrder::Missing(index));
        let id = timestamp_value
            .and_then(|value| {
                value
                    .as_i64()
                    .map(|number| number.to_string())
                    .or_else(|| value.as_str().map(ToOwned::to_owned))
            })
            .unwrap_or_else(|| index.to_string());
        messages.push((
            timestamp_order,
            index,
            crate::telemetry::ChatMessage {
                id,
                name: name.to_owned(),
                text: text.to_owned(),
            },
        ));
    }
    messages.sort_by(|(left, left_index, _), (right, right_index, _)| {
        left.cmp(right).then_with(|| left_index.cmp(right_index))
    });
    let mut messages = messages
        .into_iter()
        .map(|(_, _, message)| message)
        .collect::<Vec<_>>();
    if messages.len() > 8 {
        messages.drain(..messages.len() - 8);
    }
    Some(messages)
}

#[derive(Default)]
pub(super) struct LocalRestResolver {
    standings_receiver: Option<Receiver<StandingsUpdate>>,
    supplement_receiver: Option<Receiver<SupplementUpdate>>,
    weather_receiver: Option<Receiver<WeatherUpdate>>,
    chat_receiver: Option<Receiver<crate::telemetry::ChatUpdate>>,
    chat_updates: Vec<crate::telemetry::ChatUpdate>,
    chat_snapshot_nonempty: bool,
    weather_session: Arc<Mutex<String>>,
    standings_by_slot: HashMap<i32, RestStanding>,
    standings_by_name: HashMap<String, RestStanding>,
    history_by_slot: HashMap<i32, Vec<RestStandingHistory>>,
    history_by_name: HashMap<String, Vec<RestStandingHistory>>,
    standings_received_at: Option<Instant>,
    pit_stop: RestPitStopEstimate,
    compound_conditions: Vec<RestCompoundCondition>,
    vehicle_damage: Option<RestVehicleDamage>,
    session_max_time_seconds: f64,
    steering_range_degrees: Option<f64>,
    fuel_ratio_assigned: f64,
    pit_refill_targets: RestPitRefillTargets,
    pit_menu_changes: Vec<RestPitMenuChange>,
    pit_menu_received_at: Option<Instant>,
    team_driver_names: Vec<String>,
    team_name: String,
    team_vehicle_name: String,
    pit_stop_received_at: Option<Instant>,
    weather_nodes: RestWeatherSession,
    weather_received_at: Option<Instant>,
    demand: Arc<AtomicU8>,
    session_generation: Arc<AtomicU64>,
    stop: Arc<AtomicBool>,
    #[cfg(not(test))]
    workers: Vec<thread::JoinHandle<()>>,
}

impl Drop for LocalRestResolver {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        #[cfg(not(test))]
        for worker in self.workers.drain(..) {
            let _ = worker.join();
        }
    }
}

impl LocalRestResolver {
    #[cfg(not(test))]
    pub(super) fn discover() -> Self {
        let demand = Arc::new(AtomicU8::new(0));
        let session_generation = Arc::new(AtomicU64::new(0));
        let stop = Arc::new(AtomicBool::new(false));
        let mut workers = Vec::with_capacity(4);
        let (standings_sender, standings_receiver) = mpsc::channel();
        let standings_demand = Arc::clone(&demand);
        let standings_generation = Arc::clone(&session_generation);
        let standings_stop = Arc::clone(&stop);
        workers.push(thread::spawn(move || {
            let Some(client) = http_client() else {
                return;
            };
            let mut history_received_at: Option<Instant> = None;
            loop {
                if standings_stop.load(Ordering::Relaxed) {
                    break;
                }
                if standings_demand.load(Ordering::Relaxed) & STANDINGS_DEMAND == 0 {
                    thread::sleep(Duration::from_secs(1));
                    continue;
                }
                let started = Instant::now();
                let generation = standings_generation.load(Ordering::Acquire);
                if let Some(standings) = fetch_tracked(&client, "/rest/watch/standings") {
                    let history = if history_received_at
                        .is_none_or(|received| received.elapsed() >= HISTORY_INTERVAL)
                    {
                        let response = fetch_tracked(&client, "/rest/watch/standings/history");
                        if response.is_some() {
                            history_received_at = Some(Instant::now());
                        }
                        response
                    } else {
                        None
                    };
                    if standings_sender
                        .send(StandingsUpdate {
                            generation,
                            standings,
                            history,
                        })
                        .is_err()
                    {
                        break;
                    }
                }
                sleep_remaining(started, STANDINGS_INTERVAL);
            }
        }));

        let (chat_sender, chat_receiver) = mpsc::sync_channel(1);
        let chat_demand = Arc::clone(&demand);
        let chat_stop = Arc::clone(&stop);
        workers.push(thread::spawn(move || {
            let Some(client) = http_client() else {
                return;
            };
            loop {
                if chat_stop.load(Ordering::Relaxed) {
                    break;
                }
                if chat_demand.load(Ordering::Relaxed) & CHAT_DEMAND == 0 {
                    thread::sleep(Duration::from_secs(1));
                    continue;
                }
                let started = Instant::now();
                if let Ok(response) = client.get(format!("{LOCAL_API}/rest/chat/")).send() {
                    if let Ok(value) = response.json::<Value>() {
                        if let Some(messages) = parse_chat_messages(&value) {
                            if chat_demand.load(Ordering::Relaxed) & CHAT_DEMAND == 0 {
                                sleep_remaining(started, CHAT_INTERVAL);
                                continue;
                            }
                            let update = crate::telemetry::ChatUpdate {
                                replace: true,
                                messages,
                            };
                            match chat_sender.try_send(update) {
                                Ok(()) | Err(mpsc::TrySendError::Full(_)) => {}
                                Err(mpsc::TrySendError::Disconnected(_)) => break,
                            }
                        }
                    }
                }
                sleep_remaining(started, CHAT_INTERVAL);
            }
        }));

        let (supplement_sender, supplement_receiver) = mpsc::channel();
        let supplement_demand = Arc::clone(&demand);
        let supplement_generation = Arc::clone(&session_generation);
        let supplement_stop = Arc::clone(&stop);
        workers.push(thread::spawn(move || {
            let Some(client) = http_client() else {
                return;
            };
            let mut garage_received_at: Option<Instant> = None;
            let mut compounds_received_at: Option<Instant> = None;
            loop {
                if supplement_stop.load(Ordering::Relaxed) {
                    break;
                }
                if supplement_demand.load(Ordering::Relaxed) & SUPPLEMENT_DEMAND == 0 {
                    thread::sleep(Duration::from_secs(1));
                    continue;
                }
                let started = Instant::now();
                let generation = supplement_generation.load(Ordering::Acquire);
                let steering_range_degrees = if garage_received_at
                    .is_none_or(|received| received.elapsed() >= GARAGE_INTERVAL)
                {
                    let range = fetch_tracked::<RestGarageData>(
                        &client,
                        "/rest/garage/getPlayerGarageData",
                    )
                    .and_then(|response| steering_range(&response.steering_lock.string_value));
                    if range.is_some() {
                        garage_received_at = Some(Instant::now());
                    }
                    range
                } else {
                    None
                };
                let compound_conditions = if compounds_received_at
                    .is_none_or(|received| received.elapsed() >= TIRE_COMPOUND_INTERVAL)
                {
                    let compounds = fetch_tracked::<RestTireManagement>(
                        &client,
                        "/rest/garage/UIScreen/TireManagement",
                    )
                    .map(|response| response.optimal_compound_conditions.compounds)
                    .filter(|compounds| !compounds.is_empty());
                    if compounds.is_some() {
                        compounds_received_at = Some(Instant::now());
                    }
                    compounds
                } else {
                    None
                };
                let repair_and_refuel = fetch_tracked::<RestRepairAndRefuel>(
                    &client,
                    "/rest/garage/UIScreen/RepairAndRefuel",
                );
                let session_info: Option<RestSessionInfo> =
                    fetch_tracked(&client, "/rest/watch/sessionInfo");
                let team_info = repair_and_refuel.as_ref().and_then(|response| {
                    session_info.as_ref().and_then(|session| {
                        team_info_for_player(&response.team_info, &session.player_name)
                    })
                });
                let update = SupplementUpdate {
                    generation,
                    pit_stop: fetch_tracked(&client, "/rest/strategy/pitstop-estimate"),
                    pit_menu_changes: repair_and_refuel.as_ref().map(pit_menu_changes),
                    compound_conditions,
                    vehicle_damage: repair_and_refuel
                        .as_ref()
                        .map(|response| RestVehicleDamage {
                            aero: response.wearables.body.aero,
                            suspension: response.wearables.suspension,
                        }),
                    fuel_ratio_assigned: repair_and_refuel
                        .as_ref()
                        .map(|response| fuel_ratio_assigned(response).unwrap_or(0.0)),
                    pit_refill_targets: repair_and_refuel.as_ref().map(pit_refill_targets),
                    session_info,
                    team_info,
                    steering_range_degrees,
                };
                if supplement_sender.send(update).is_err() {
                    break;
                }
                sleep_remaining(started, SUPPLEMENT_INTERVAL);
            }
        }));

        let (weather_sender, weather_receiver) = mpsc::channel();
        let weather_demand = Arc::clone(&demand);
        let weather_generation = Arc::clone(&session_generation);
        let weather_session = Arc::new(Mutex::new(String::new()));
        let weather_session_slot = Arc::clone(&weather_session);
        let weather_stop = Arc::clone(&stop);
        workers.push(thread::spawn(move || {
            let Some(client) = http_client() else {
                return;
            };
            loop {
                if weather_stop.load(Ordering::Relaxed) {
                    break;
                }
                if weather_demand.load(Ordering::Relaxed) & WEATHER_DEMAND == 0 {
                    thread::sleep(Duration::from_secs(1));
                    continue;
                }
                let session = weather_session_slot
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .clone();
                if session.is_empty() {
                    thread::sleep(Duration::from_secs(1));
                    continue;
                }
                let started = Instant::now();
                let generation = weather_generation.load(Ordering::Acquire);
                if let Some(weather_sessions) = fetch_tracked::<HashMap<String, RestWeatherSession>>(
                    &client,
                    "/rest/sessions/weather",
                ) {
                    if let Some(weather) = weather_sessions.get(&session) {
                        if weather_sender
                            .send(WeatherUpdate {
                                generation,
                                session,
                                weather: weather.clone(),
                            })
                            .is_err()
                        {
                            break;
                        }
                    }
                }
                sleep_remaining(started, WEATHER_INTERVAL);
            }
        }));

        // Everything except the worker handles starts empty, so only the fields
        // wired to the threads spawned above are named here.
        let mut resolver = Self::default();
        resolver.standings_receiver = Some(standings_receiver);
        resolver.supplement_receiver = Some(supplement_receiver);
        resolver.weather_receiver = Some(weather_receiver);
        resolver.chat_receiver = Some(chat_receiver);
        resolver.weather_session = weather_session;
        resolver.demand = demand;
        resolver.session_generation = session_generation;
        resolver.stop = stop;
        resolver.workers = workers;
        resolver
    }

    #[cfg(test)]
    pub(super) fn empty() -> Self {
        Self::default()
    }

    pub(super) fn refresh(
        &mut self,
        connected: bool,
        player_has_vehicle: bool,
        standings_requested: bool,
        supplement_requested: bool,
        weather_requested: bool,
        chat_requested: bool,
        weather_session: &str,
    ) {
        let demand = rest_demand(
            connected,
            player_has_vehicle,
            standings_requested,
            supplement_requested,
            weather_requested,
            chat_requested,
        );
        self.demand.store(demand, Ordering::Relaxed);
        if let Some(receiver) = self.chat_receiver.as_ref() {
            for update in receiver.try_iter() {
                self.chat_snapshot_nonempty = !update.messages.is_empty();
                self.chat_updates.clear();
                self.chat_updates.push(update);
            }
        }
        {
            let mut slot = self
                .weather_session
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            if *slot != weather_session {
                *slot = weather_session.to_owned();
            }
        }
        let weather_updates = self
            .weather_receiver
            .as_ref()
            .map(|receiver| receiver.try_iter().collect::<Vec<_>>())
            .unwrap_or_default();
        for update in weather_updates {
            if update.generation == self.session_generation.load(Ordering::Acquire)
                && update.session == weather_session
            {
                self.weather_nodes = update.weather;
                self.weather_received_at = Some(Instant::now());
            }
        }
        let standings_updates = self
            .standings_receiver
            .as_ref()
            .map(|receiver| receiver.try_iter().collect::<Vec<_>>())
            .unwrap_or_default();
        for update in standings_updates {
            if update.generation != self.session_generation.load(Ordering::Acquire) {
                continue;
            }
            self.standings_by_slot.clear();
            self.standings_by_name.clear();
            for standing in update.standings {
                if standing.slot_id != 0 {
                    self.standings_by_slot
                        .insert(standing.slot_id, standing.clone());
                }
                let name = normalized_name(&standing.driver_name);
                if !name.is_empty() {
                    self.standings_by_name.insert(name, standing);
                }
            }
            if let Some(history) = update.history {
                self.replace_history(history);
            }
            self.standings_received_at = Some(Instant::now());
        }

        let supplement_updates = self
            .supplement_receiver
            .as_ref()
            .map(|receiver| receiver.try_iter().collect::<Vec<_>>())
            .unwrap_or_default();
        for update in supplement_updates {
            if update.generation != self.session_generation.load(Ordering::Acquire) {
                continue;
            }
            if let Some(pit_stop) = update.pit_stop {
                self.pit_stop = pit_stop;
                self.pit_stop_received_at = Some(Instant::now());
            }
            if let Some(compound_conditions) = update.compound_conditions {
                self.compound_conditions = compound_conditions;
            }
            if let Some(vehicle_damage) = update.vehicle_damage {
                self.vehicle_damage = Some(vehicle_damage);
            }
            if let Some(session_info) = update.session_info {
                self.latch_session_max_time(session_info.max_time);
            }
            if let Some(team_info) = update.team_info {
                self.team_driver_names = team_info.driver_names;
                self.team_name = team_info.team_name;
                self.team_vehicle_name = team_info.vehicle_name;
            }
            if let Some(fuel_ratio_assigned) = update.fuel_ratio_assigned {
                self.fuel_ratio_assigned = fuel_ratio_assigned;
                self.pit_menu_received_at = Some(Instant::now());
            }
            if let Some(pit_menu_changes) = update.pit_menu_changes {
                self.pit_menu_changes = pit_menu_changes;
                self.pit_menu_received_at = Some(Instant::now());
            }
            if let Some(pit_refill_targets) = update.pit_refill_targets {
                self.pit_refill_targets = pit_refill_targets;
                self.pit_menu_received_at = Some(Instant::now());
            }
            if let Some(steering_range_degrees) = update.steering_range_degrees {
                self.steering_range_degrees = Some(steering_range_degrees);
            }
        }
        if !connected {
            self.compound_conditions.clear();
            self.vehicle_damage = None;
        }
    }

    pub(super) fn take_chat_updates(&mut self) -> Vec<crate::telemetry::ChatUpdate> {
        std::mem::take(&mut self.chat_updates)
    }

    #[cfg(test)]
    pub(super) fn seed_standings(&mut self, standings: Vec<RestStanding>) {
        self.standings_by_slot.clear();
        self.standings_by_name.clear();
        for standing in standings {
            if standing.slot_id != 0 {
                self.standings_by_slot
                    .insert(standing.slot_id, standing.clone());
            }
            let name = normalized_name(&standing.driver_name);
            if !name.is_empty() {
                self.standings_by_name.insert(name, standing);
            }
        }
        self.standings_received_at = Some(Instant::now());
    }

    #[cfg(test)]
    pub(super) fn seed_history(&mut self, history: HashMap<String, Vec<RestStandingHistory>>) {
        self.replace_history(history);
    }

    #[cfg(test)]
    pub(super) fn seed_team_reference(
        &mut self,
        driver_names: Vec<String>,
        team_name: &str,
        vehicle_name: &str,
    ) {
        self.team_driver_names = driver_names;
        self.team_name = team_name.to_owned();
        self.team_vehicle_name = vehicle_name.to_owned();
    }

    fn replace_history(&mut self, history: HashMap<String, Vec<RestStandingHistory>>) {
        self.history_by_slot.clear();
        self.history_by_name.clear();
        for (key, entries) in history {
            if entries.is_empty() {
                continue;
            }
            let slot_id = key.parse::<i32>().unwrap_or(entries[0].slot_id);
            if slot_id >= 0 {
                self.history_by_slot.insert(slot_id, entries.clone());
            }
            for name in entries
                .iter()
                .map(|entry| normalized_name(&entry.driver_name))
                .filter(|name| !name.is_empty())
                .collect::<std::collections::HashSet<_>>()
            {
                self.history_by_name.insert(name, entries.clone());
            }
        }
    }

    pub(super) fn reset_session_history(&mut self) {
        if self.chat_snapshot_nonempty {
            self.chat_updates.clear();
            self.chat_updates.push(crate::telemetry::ChatUpdate {
                replace: true,
                messages: Vec::new(),
            });
        } else if !self
            .chat_updates
            .iter()
            .any(|update| update.messages.is_empty())
        {
            self.chat_updates.clear();
        }
        self.chat_snapshot_nonempty = false;
        self.session_generation.fetch_add(1, Ordering::AcqRel);
        self.standings_by_slot.clear();
        self.standings_by_name.clear();
        self.standings_received_at = None;
        self.history_by_slot.clear();
        self.history_by_name.clear();
        self.session_max_time_seconds = 0.0;
        self.steering_range_degrees = None;
        self.fuel_ratio_assigned = 0.0;
        self.pit_refill_targets = RestPitRefillTargets::default();
        self.pit_menu_changes.clear();
        self.pit_menu_received_at = None;
        self.pit_stop = RestPitStopEstimate::default();
        self.pit_stop_received_at = None;
        self.team_driver_names.clear();
        self.team_name.clear();
        self.team_vehicle_name.clear();
        self.compound_conditions.clear();
        self.vehicle_damage = None;
        self.weather_nodes = RestWeatherSession::default();
        self.weather_received_at = None;
    }

    fn latch_session_max_time(&mut self, seconds: f64) {
        if self.session_max_time_seconds <= 0.0
            && seconds.is_finite()
            && seconds > 0.0
            && seconds < u32::MAX as f64
        {
            self.session_max_time_seconds = seconds;
        }
    }

    pub(super) fn history(
        &self,
        slot_id: i32,
        driver_name: &str,
    ) -> Option<&[RestStandingHistory]> {
        let name = normalized_name(driver_name);
        self.history_by_name
            .get(&name)
            .or_else(|| {
                self.history_by_slot.get(&slot_id).filter(|entries| {
                    name.is_empty()
                        || entries
                            .iter()
                            .any(|entry| normalized_name(&entry.driver_name) == name)
                })
            })
            .map(Vec::as_slice)
    }

    pub(super) fn initial_class_count(&self, vehicle_class: &str) -> usize {
        let class = normalized_class(vehicle_class);
        self.history_by_slot
            .values()
            .filter_map(|entries| entries.first())
            .filter(|entry| normalized_class(&entry.car_class) == class)
            .count()
    }

    pub(super) fn starting_class_position(
        &self,
        slot_id: i32,
        driver_name: &str,
        vehicle_class: &str,
    ) -> Option<i32> {
        let name = normalized_name(driver_name);
        let target = self.standings_by_name.get(&name).or_else(|| {
            self.standings_by_slot.get(&slot_id).filter(|standing| {
                name.is_empty() || normalized_name(&standing.driver_name) == name
            })
        })?;
        if target.qualification <= 0 {
            return None;
        }
        let class = normalized_class(vehicle_class);
        Some(
            1 + self
                .standings_by_name
                .values()
                .filter(|standing| {
                    standing.qualification > 0
                        && normalized_class(&standing.car_class) == class
                        && standing.qualification < target.qualification
                })
                .count() as i32,
        )
    }

    pub(super) fn standing(&self, slot_id: i32, driver_name: &str) -> Option<&RestStanding> {
        if !is_fresh(self.standings_received_at, STANDINGS_MAX_AGE) {
            return None;
        }
        let name = normalized_name(driver_name);
        self.standings_by_name.get(&name).or_else(|| {
            self.standings_by_slot.get(&slot_id).filter(|standing| {
                name.is_empty() || normalized_name(&standing.driver_name) == name
            })
        })
    }

    pub(super) fn focused_standing(&self) -> Option<&RestStanding> {
        if !is_fresh(self.standings_received_at, FOCUS_MAX_AGE) {
            return None;
        }
        self.standings_by_name
            .values()
            .find(|standing| standing.focus || standing.has_focus)
    }

    pub(super) fn team_reference(&self) -> Option<(&[String], &str, &str)> {
        (!self.team_driver_names.is_empty()).then_some((
            self.team_driver_names.as_slice(),
            self.team_name.as_str(),
            self.team_vehicle_name.as_str(),
        ))
    }

    pub(super) fn pit_stop(&self) -> Option<&RestPitStopEstimate> {
        is_fresh(self.pit_stop_received_at, SUPPLEMENT_MAX_AGE).then_some(&self.pit_stop)
    }

    /// The car's compounds in `mCompoundType` index order, empty until the
    /// garage screen answers.
    pub(super) fn compound_conditions(&self) -> &[RestCompoundCondition] {
        &self.compound_conditions
    }

    pub(super) fn aero_damage(&self) -> Option<f64> {
        self.vehicle_damage.map(|damage| damage.aero)
    }

    /// Read from the `RepairAndRefuel` wearables. `getVehicleCondition` carries
    /// the same per-corner values in a fraction of the payload, but asking for
    /// it is what crashed the game — see the endpoint list in TELEMETRY.md.
    pub(super) fn suspension_damage(&self) -> Option<[f64; 4]> {
        self.vehicle_damage.map(|damage| damage.suspension)
    }

    pub(super) fn session_max_time_seconds(&self) -> f64 {
        self.session_max_time_seconds
    }

    pub(super) fn steering_range_degrees(&self) -> Option<f64> {
        self.steering_range_degrees
    }

    pub(super) fn fuel_ratio_assigned(&self) -> f64 {
        if is_fresh(self.pit_menu_received_at, SUPPLEMENT_MAX_AGE) {
            self.fuel_ratio_assigned
        } else {
            0.0
        }
    }

    pub(super) fn pit_refill_target(&self, virtual_energy: bool) -> Option<f64> {
        if !is_fresh(self.pit_menu_received_at, SUPPLEMENT_MAX_AGE) {
            return None;
        }
        if virtual_energy {
            self.pit_refill_targets.virtual_energy
        } else {
            self.pit_refill_targets.fuel
        }
    }

    pub(super) fn pit_menu_changes(&self) -> Option<&[RestPitMenuChange]> {
        is_fresh(self.pit_menu_received_at, SUPPLEMENT_MAX_AGE)
            .then_some(self.pit_menu_changes.as_slice())
    }

    pub(super) fn weather_forecast(&self) -> Option<&RestWeatherSession> {
        is_fresh(self.weather_received_at, WEATHER_MAX_AGE).then_some(&self.weather_nodes)
    }
}

fn fuel_ratio_assigned(response: &RestRepairAndRefuel) -> Option<f64> {
    let item = response
        .pit_menu
        .pit_menu
        .iter()
        .find(|item| item.name.trim().eq_ignore_ascii_case("FUEL RATIO:"))?;
    let ratio = item
        .settings
        .get(item.current_setting)?
        .text
        .trim()
        .parse::<f64>()
        .ok()?;
    (ratio.is_finite() && ratio > 0.0).then_some(ratio)
}

fn pit_menu_changes(response: &RestRepairAndRefuel) -> Vec<RestPitMenuChange> {
    response
        .pit_menu
        .pit_menu
        .iter()
        .filter_map(|item| {
            let default_setting = item.default_setting?;
            if item.current_setting == default_setting {
                return None;
            }
            let label = item.name.trim().trim_end_matches(':').trim().to_owned();
            if label.is_empty() {
                return None;
            }
            let value = item
                .settings
                .get(item.current_setting)
                .map(|setting| setting.text.trim().to_owned())
                .filter(|text| !text.is_empty())
                .unwrap_or_else(|| item.current_setting.to_string());
            Some(RestPitMenuChange { label, value })
        })
        .collect()
}

#[derive(Clone, Copy, Debug, Default)]
struct RestPitRefillTargets {
    fuel: Option<f64>,
    virtual_energy: Option<f64>,
}

fn pit_refill_targets(response: &RestRepairAndRefuel) -> RestPitRefillTargets {
    let mut targets = RestPitRefillTargets::default();
    for item in &response.pit_menu.pit_menu {
        let name = item.name.trim();
        if name.eq_ignore_ascii_case("VIRTUAL ENERGY:") {
            let value = item.current_setting as f64;
            targets.virtual_energy = (value > 0.0).then_some(value);
        } else if name.eq_ignore_ascii_case("FUEL:") {
            targets.fuel = item
                .settings
                .get(item.current_setting)
                .and_then(|setting| first_positive_number(&setting.text));
        }
    }
    targets
}

fn first_positive_number(value: &str) -> Option<f64> {
    let number = value
        .split(|character: char| !(character.is_ascii_digit() || character == '.'))
        .find(|part| !part.is_empty())?
        .parse::<f64>()
        .ok()?;
    (number.is_finite() && number > 0.0).then_some(number)
}

fn team_info_for_player(info: &RestTeamInfo, player_name: &str) -> Option<RestTeamInfo> {
    let player = normalized_driver_identity(player_name);
    (!player.is_empty()
        && info
            .driver_names
            .iter()
            .any(|name| normalized_driver_identity(name) == player))
    .then(|| info.clone())
}

fn steering_range(value: &str) -> Option<f64> {
    let range = value
        .split(|character: char| !(character.is_ascii_digit() || character == '.'))
        .find(|part| !part.is_empty())?
        .parse::<f64>()
        .ok()?;
    (range.is_finite() && range > 0.0).then_some(range)
}

#[cfg(not(test))]
fn http_client() -> Option<Client> {
    Client::builder()
        .connect_timeout(REQUEST_TIMEOUT)
        .timeout(REQUEST_TIMEOUT)
        .build()
        .ok()
}

/// Reports the outcome of a local REST fetch, so a failure surfaces in the
/// diagnostics log instead of disappearing into a discarded `Result`.
#[cfg(not(test))]
fn fetch_tracked<T: DeserializeOwned>(client: &Client, path: &'static str) -> Option<T> {
    match fetch_json(client, path) {
        Ok(value) => {
            crate::startup_log::record_request_success(path);
            Some(value)
        }
        Err(error) => {
            crate::startup_log::record_request_failure(path, &error);
            None
        }
    }
}

/// The error string names the stage that failed, because a request that times
/// out and a response whose shape stopped matching need very different fixes.
#[cfg(not(test))]
fn fetch_json<T: DeserializeOwned>(client: &Client, path: &str) -> Result<T, String> {
    let response = client
        .get(format!("{LOCAL_API}{path}"))
        .send()
        .map_err(|error| format!("request: {error}"))?
        .error_for_status()
        .map_err(|error| format!("status: {error}"))?;
    response
        .json::<T>()
        .map_err(|error| format!("decode: {error}"))
}

#[cfg(not(test))]
fn sleep_remaining(started: Instant, interval: Duration) {
    let elapsed = started.elapsed();
    if elapsed < interval {
        thread::sleep(interval - elapsed);
    }
}

fn is_fresh(received_at: Option<Instant>, maximum_age: Duration) -> bool {
    received_at.is_some_and(|received| received.elapsed() <= maximum_age)
}

pub(super) fn normalized_name(name: &str) -> String {
    name.trim().to_lowercase()
}

pub(super) fn normalized_driver_identity(name: &str) -> String {
    let normalized = normalized_name(name);
    normalized
        .rsplit_once('#')
        .filter(|(_, suffix)| {
            !suffix.is_empty() && suffix.chars().all(|value| value.is_ascii_digit())
        })
        .map_or_else(
            || normalized.clone(),
            |(base, _)| base.trim_end().to_owned(),
        )
}

fn normalized_class(value: &str) -> String {
    let compact = value
        .trim()
        .to_ascii_uppercase()
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .collect::<String>();
    if compact.contains("GT3") {
        "GT3".to_owned()
    } else if compact.contains("HYPERCAR") || compact.contains("GTP") {
        "HYPERCAR".to_owned()
    } else if compact.contains("LMP2") {
        "LMP2".to_owned()
    } else if compact.contains("LMP3") {
        "LMP3".to_owned()
    } else {
        compact
    }
}

#[cfg(test)]
mod tests {
    use super::{
        fuel_ratio_assigned, normalized_driver_identity, normalized_name, pit_menu_changes,
        pit_refill_targets, rest_demand, steering_range, team_info_for_player, LocalRestResolver,
        RestGarageData, RestPitStopEstimate, RestRepairAndRefuel, RestSessionInfo, RestStanding,
        RestStandingHistory, RestTeamInfo, RestTireManagement, RestVehicleDamage,
        RestWeatherSession, StandingsUpdate, CHAT_DEMAND, STANDINGS_DEMAND, SUPPLEMENT_DEMAND,
        SUPPLEMENT_MAX_AGE, WEATHER_DEMAND,
    };
    use std::collections::HashMap;
    use std::time::{Duration, Instant};

    #[test]
    fn rest_workers_follow_independent_connected_demands() {
        assert_eq!(rest_demand(false, true, true, true, true, true), 0);
        assert_eq!(rest_demand(true, true, false, false, false, false), 0);
        assert_eq!(
            rest_demand(true, true, false, false, false, true),
            CHAT_DEMAND
        );
        assert_eq!(
            rest_demand(true, true, true, false, false, false).count_ones(),
            1
        );
        assert_eq!(
            rest_demand(true, true, false, true, true, false).count_ones(),
            2
        );
    }

    #[test]
    fn chat_parser_splits_first_colon_trims_caps_and_preserves_int64_ids() {
        let value = serde_json::json!([
            {"Message":" Driver: hello: world ","Timestamp":9223372036854775000_i64},
            {"Message":" Empty:   ","Timestamp":2},
            {"Message":"missing separator","Timestamp":3}
        ]);
        let parsed = super::parse_chat_messages(&value).unwrap();
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].name, "Driver");
        assert_eq!(parsed[0].text, "hello: world");
        assert_eq!(parsed[0].id, "9223372036854775000");
        assert!(super::parse_chat_messages(&serde_json::json!({"unexpected": []})).is_none());

        let camel_case = serde_json::json!([
            {"message":"Driver: hello", "timestamp":10}
        ]);
        let parsed_camel_case = super::parse_chat_messages(&camel_case).unwrap();
        assert_eq!(parsed_camel_case[0].id, "10");
        assert_eq!(parsed_camel_case[0].name, "Driver");
        assert_eq!(parsed_camel_case[0].text, "hello");

        let unsorted = serde_json::json!([
            {"Message":"nine: n","Timestamp":9},
            {"Message":"one: n","Timestamp":1},
            {"Message":"two: n","Timestamp":2}
        ]);
        let sorted = super::parse_chat_messages(&unsorted).unwrap();
        assert_eq!(
            sorted
                .iter()
                .map(|message| message.id.as_str())
                .collect::<Vec<_>>(),
            vec!["1", "2", "9"]
        );
    }

    /// Every endpoint the supplement worker polls lives under `/rest/garage/`
    /// and asks LMU about a vehicle. At the main menu there is none, and asking
    /// anyway is what closed the game, so being connected is not enough.
    #[test]
    fn the_supplement_worker_waits_for_the_player_to_have_a_vehicle() {
        let without_vehicle = rest_demand(true, false, true, true, true, false);
        let with_vehicle = rest_demand(true, true, true, true, true, false);
        assert_eq!(without_vehicle & SUPPLEMENT_DEMAND, 0);
        assert_ne!(with_vehicle & SUPPLEMENT_DEMAND, 0);
        // Standings and weather answer about the session, not the car, so they
        // keep polling in the menus.
        assert_eq!(
            without_vehicle & (STANDINGS_DEMAND | WEATHER_DEMAND),
            with_vehicle & (STANDINGS_DEMAND | WEATHER_DEMAND)
        );
    }

    #[test]
    fn parses_rest_standings_fields_used_by_the_overlay() {
        let value: RestStanding = serde_json::from_str(
            r#"{"slotID":29,"driverName":"Test Driver","focus":true,"hasFocus":true,"carClass":"LMP2_ELMS","carNumber":"29","position":4,"qualification":7,"serverScored":true,"finishStatus":"FSTAT_NONE","lapsBehindClassLeader":1,"timeBehindClassLeader":2.5,"lapsBehindNext":0,"timeBehindNext":1.2,"pitstops":2,"pitState":"REQUEST","pitting":true,"inGarageStall":false,"fuelFraction":0.5,"veFraction":0.75}"#,
        )
        .unwrap();
        assert_eq!(value.slot_id, 29);
        assert!(value.focus);
        assert!(value.has_focus);
        assert_eq!(value.car_class, "LMP2_ELMS");
        assert_eq!(value.car_number, "29");
        assert_eq!(value.position, 4);
        assert_eq!(value.qualification, 7);
        assert!(value.server_scored);
        assert_eq!(value.finish_status, "FSTAT_NONE");
        assert_eq!(value.laps_behind_class_leader, 1);
        assert_eq!(value.pitstops, 2);
        assert!((value.ve_fraction - 0.75).abs() < f64::EPSILON);
    }

    #[test]
    fn parses_pit_stop_estimate() {
        let pit: RestPitStopEstimate =
            serde_json::from_str(r#"{"fuel":5.0,"tires":12.0,"total":17.0,"ve":3.0}"#).unwrap();
        assert_eq!(pit.total, 17.0);
    }

    #[test]
    fn pit_stop_freshness_is_independent_from_the_fuel_menu() {
        let mut resolver = LocalRestResolver::empty();
        resolver.pit_stop = RestPitStopEstimate {
            total: 17.0,
            ..RestPitStopEstimate::default()
        };
        resolver.pit_stop_received_at = Some(
            Instant::now()
                .checked_sub(SUPPLEMENT_MAX_AGE + Duration::from_secs(1))
                .unwrap(),
        );
        resolver.pit_menu_received_at = Some(Instant::now());

        assert!(resolver.pit_stop().is_none());

        resolver.pit_stop_received_at = Some(Instant::now());
        assert_eq!(
            resolver.pit_stop().map(|estimate| estimate.total),
            Some(17.0)
        );
    }

    #[test]
    fn session_reset_discards_standings_and_pit_stop_cache() {
        let mut resolver = LocalRestResolver::empty();
        resolver.seed_standings(vec![RestStanding {
            slot_id: 4,
            ..RestStanding::default()
        }]);
        resolver.pit_stop = RestPitStopEstimate {
            total: 17.0,
            ..RestPitStopEstimate::default()
        };
        resolver.pit_stop_received_at = Some(Instant::now());

        resolver.reset_session_history();

        assert!(resolver.standing(4, "").is_none());
        assert!(resolver.pit_stop().is_none());
    }

    #[test]
    fn session_reset_discards_a_queued_update_from_the_previous_generation() {
        let mut resolver = LocalRestResolver::empty();
        let (sender, receiver) = std::sync::mpsc::channel();
        resolver.standings_receiver = Some(receiver);
        sender
            .send(StandingsUpdate {
                generation: 0,
                standings: vec![RestStanding {
                    slot_id: 42,
                    driver_name: "Old Session".to_owned(),
                    ..RestStanding::default()
                }],
                history: None,
            })
            .unwrap();

        resolver.reset_session_history();
        resolver.refresh(true, true, true, false, false, false, "");

        assert!(resolver.standings_by_slot.is_empty());
        assert!(resolver.standings_by_name.is_empty());
    }

    #[test]
    fn parses_official_session_max_time() {
        let session: RestSessionInfo =
            serde_json::from_str(r#"{"maxTime":14400,"playerName":"Manuel Rodriguez Alvarez"}"#)
                .unwrap();
        assert_eq!(session.max_time, 14_400.0);
        assert_eq!(session.player_name, "Manuel Rodriguez Alvarez");
    }

    #[test]
    fn accepts_only_team_info_containing_the_local_player() {
        let info = RestTeamInfo {
            driver_names: vec!["Manuel Rodriguez Alvarez#1234".into(), "Compañero".into()],
            team_name: "BlackRack Racing".into(),
            vehicle_name: "Ferrari 296 #29".into(),
        };
        assert!(team_info_for_player(&info, "Manuel Rodriguez Alvarez").is_some());
        assert!(team_info_for_player(&info, "Otro piloto").is_none());
        assert_eq!(normalized_driver_identity(" Piloto#9006 "), "piloto");
    }

    #[test]
    fn parses_active_steering_range_from_garage_text() {
        let garage: RestGarageData =
            serde_json::from_str(r#"{"VM_STEER_LOCK":{"stringValue":"360 (13.3) deg"}}"#).unwrap();
        assert_eq!(
            steering_range(&garage.steering_lock.string_value),
            Some(360.0)
        );
        assert_eq!(steering_range("719 deg"), Some(719.0));
        assert_eq!(steering_range("unavailable"), None);
    }

    #[test]
    fn session_max_time_is_latched_until_session_reset() {
        let mut resolver = LocalRestResolver::empty();
        resolver.latch_session_max_time(14_400.0);
        resolver.latch_session_max_time(14_399.0);
        assert_eq!(resolver.session_max_time_seconds(), 14_400.0);

        resolver.reset_session_history();
        resolver.latch_session_max_time(7_200.0);
        assert_eq!(resolver.session_max_time_seconds(), 7_200.0);
    }

    #[test]
    fn parses_vehicle_damage_from_repair_and_refuel() {
        let response: RestRepairAndRefuel = serde_json::from_str(
            r#"{"wearables":{"body":{"aero":0.12},"suspension":[0.01,0.2,0.03,0.04]}}"#,
        )
        .unwrap();
        assert!((response.wearables.body.aero - 0.12).abs() < f64::EPSILON);
        assert_eq!(response.wearables.suspension, [0.01, 0.2, 0.03, 0.04]);
    }

    #[test]
    fn parses_the_cars_compounds_and_their_optimal_temperatures() {
        let response: RestTireManagement = serde_json::from_str(
            r#"{"optimalCompoundConditions":{"compounds":[{"optimalTemperature":89.0,"type":"Medium"},{"optimalTemperature":52.0,"type":"Wet"}]},"pitMenu":{"pitMenu":[]}}"#,
        )
        .unwrap();

        let compounds = response.optimal_compound_conditions.compounds;
        assert_eq!(compounds.len(), 2);
        assert_eq!(compounds[0].compound_type, "Medium");
        assert_eq!(compounds[0].optimal_temperature, 89.0);
        assert_eq!(compounds[1].compound_type, "Wet");
        assert_eq!(compounds[1].optimal_temperature, 52.0);
    }

    /// The live response that exposed this: a driver name arrives as its raw
    /// 32-byte C buffer, and rejecting it used to discard the whole payload.
    #[test]
    fn driver_names_arriving_as_character_buffers_do_not_sink_the_response() {
        let response: RestRepairAndRefuel = serde_json::from_str(
            r#"{"teamInfo":{"driverNames":[[77,97,110,117,101,108,32,82,111,100,0,0,0,0]],"teamName":"Lucus Racing Team Green","vehicleName":"Oreca 07"},"wearables":{"body":{"aero":0.234},"suspension":[0.0,0.0,0.0,0.158]}}"#,
        )
        .unwrap();

        assert_eq!(response.team_info.driver_names, ["Manuel Rod"]);
        assert!((response.wearables.body.aero - 0.234).abs() < f64::EPSILON);
        assert_eq!(response.wearables.suspension, [0.0, 0.0, 0.0, 0.158]);
    }

    #[test]
    fn driver_names_are_still_accepted_as_plain_strings() {
        let response: RestTeamInfo =
            serde_json::from_str(r#"{"driverNames":["Manuel Rod",""],"teamName":"Lucus"}"#)
                .unwrap();
        assert_eq!(response.driver_names, ["Manuel Rod"]);
    }

    #[test]
    fn vehicle_damage_is_latched_until_disconnect_or_session_reset() {
        let mut resolver = LocalRestResolver::empty();
        resolver.vehicle_damage = Some(RestVehicleDamage {
            aero: 0.28,
            suspension: [0.2, 0.79, 0.4, 0.6],
        });

        resolver.refresh(true, true, false, true, false, false, "RACE");
        assert_eq!(resolver.aero_damage(), Some(0.28));

        resolver.refresh(false, false, false, false, false, false, "");
        assert!(resolver.aero_damage().is_none());
        assert!(resolver.suspension_damage().is_none());

        resolver.vehicle_damage = Some(RestVehicleDamage::default());
        resolver.reset_session_history();
        assert!(resolver.aero_damage().is_none());
    }

    /// Both halves of the damage reading come from the one `RepairAndRefuel`
    /// response, so they appear and disappear together.
    #[test]
    fn suspension_and_aero_damage_both_come_from_the_wearables() {
        let mut resolver = LocalRestResolver::empty();
        resolver.vehicle_damage = Some(RestVehicleDamage {
            aero: 0.11,
            suspension: [0.2, 0.0, 0.0, 0.0],
        });
        assert_eq!(resolver.suspension_damage(), Some([0.2, 0.0, 0.0, 0.0]));
        assert_eq!(resolver.aero_damage(), Some(0.11));

        resolver.vehicle_damage = None;
        assert!(resolver.suspension_damage().is_none());
        assert!(resolver.aero_damage().is_none());
    }

    #[test]
    fn parses_selected_official_fuel_ratio_from_pit_menu() {
        let response: RestRepairAndRefuel = serde_json::from_str(
            r#"{"pitMenu":{"pitMenu":[{"name":"FUEL RATIO:","currentSetting":2,"default":1,"settings":[{"text":"0.91"},{"text":"0.92"},{"text":"0.93"}]}]}}"#,
        )
        .unwrap();
        assert_eq!(fuel_ratio_assigned(&response), Some(0.93));
    }

    #[test]
    fn keeps_only_non_default_pit_menu_changes() {
        let response: RestRepairAndRefuel = serde_json::from_str(
            r#"{"pitMenu":{"pitMenu":[
                {"name":"FUEL RATIO:","currentSetting":2,"default":1,"settings":[{"text":"0.91"},{"text":"0.92"},{"text":"0.93"}]},
                {"name":"TYRES:","currentSetting":0,"default":0,"settings":[{"text":"NO CHANGE"}]},
                {"name":"VIRTUAL ENERGY:","currentSetting":82,"default":0,"settings":[]}
            ]}}"#,
        )
        .unwrap();

        let changes = pit_menu_changes(&response);
        assert_eq!(changes.len(), 2);
        assert_eq!(changes[0].label, "FUEL RATIO");
        assert_eq!(changes[0].value, "0.93");
        assert_eq!(changes[1].label, "VIRTUAL ENERGY");
        assert_eq!(changes[1].value, "82");
    }

    #[test]
    fn parses_absolute_fuel_and_energy_targets_from_pit_menu() {
        let response: RestRepairAndRefuel = serde_json::from_str(
            r#"{"pitMenu":{"pitMenu":[{"name":"FUEL:","currentSetting":1,"settings":[{"text":"40 L"},{"text":"75 L"}]},{"name":"VIRTUAL ENERGY:","currentSetting":82,"settings":[]}]}}"#,
        )
        .unwrap();

        let targets = pit_refill_targets(&response);
        assert_eq!(targets.fuel, Some(75.0));
        assert_eq!(targets.virtual_energy, Some(82.0));
    }

    #[test]
    fn parses_weather_forecast_nodes_into_serialized_nodes() {
        let weather: RestWeatherSession = serde_json::from_str(
            r#"{
              "START": {"WNV_SKY": {"currentValue": 0, "stringValue": "Sunny"}, "WNV_TEMPERATURE": {"currentValue": 22.4, "stringValue": "22.4 C"}, "WNV_RAIN_CHANCE": {"currentValue": 5, "stringValue": "5 %"}, "WNV_HUMIDITY": {"currentValue": 40, "stringValue": "40 %"}, "WNV_WINDSPEED": {"currentValue": 7, "stringValue": "25.2 km/h"}, "WNV_WINDDIRECTION": {"currentValue": 2, "stringValue": "East"}},
              "NODE_25": {"WNV_SKY": {"currentValue": 2, "stringValue": "Cloudy"}, "WNV_TEMPERATURE": {"currentValue": 21.1, "stringValue": "21.1 C"}, "WNV_RAIN_CHANCE": {"currentValue": 30, "stringValue": "30 %"}, "WNV_HUMIDITY": {"currentValue": 55, "stringValue": "55 %"}},
              "NODE_50": {"WNV_SKY": {"currentValue": 4, "stringValue": "Rain"}, "WNV_TEMPERATURE": {"currentValue": 19.6, "stringValue": "19.6 C"}, "WNV_RAIN_CHANCE": {"currentValue": 85, "stringValue": "85 %"}, "WNV_HUMIDITY": {"currentValue": 90, "stringValue": "90 %"}},
              "NODE_75": {"WNV_SKY": {"currentValue": 5, "stringValue": "Storm"}, "WNV_TEMPERATURE": {"currentValue": 18.2, "stringValue": "18.2 C"}, "WNV_RAIN_CHANCE": {"currentValue": 95, "stringValue": "95 %"}, "WNV_HUMIDITY": {"currentValue": 95, "stringValue": "95 %"}},
              "FINISH": {"WNV_SKY": {"currentValue": 1, "stringValue": "Partly Cloudy"}, "WNV_TEMPERATURE": {"currentValue": 20.1, "stringValue": "20.1 C"}, "WNV_RAIN_CHANCE": {"currentValue": 20, "stringValue": "20 %"}, "WNV_HUMIDITY": {"currentValue": 60, "stringValue": "60 %"}}
            }"#,
        )
        .unwrap();
        let nodes = weather.forecast_nodes();
        assert_eq!(nodes.len(), 5);
        assert_eq!(nodes[0].sky, 0);
        assert_eq!(nodes[0].temperature_c, 22.4);
        assert_eq!(nodes[0].rain_chance_percent, 5.0);
        assert_eq!(nodes[2].sky, 4);
        assert_eq!(nodes[2].rain_chance_percent, 85.0);
        assert_eq!(nodes[0].humidity_percent, Some(40.0));
        assert_eq!(nodes[4].humidity_percent, Some(60.0));
        assert_eq!(weather.wind_at(0), Some((7.0, 90.0)));
        assert_eq!(weather.wind_at(1), None);
    }

    #[test]
    fn weather_humidity_preserves_zero_and_marks_missing_metrics_unavailable() {
        let weather: RestWeatherSession = serde_json::from_str(
            r#"{
              "START": {"WNV_HUMIDITY": {"currentValue": 0, "stringValue": "0 %"}},
              "NODE_25": {"WNV_HUMIDITY": {}},
              "NODE_50": {}, "NODE_75": {}, "FINISH": {}
            }"#,
        )
        .unwrap();

        let nodes = weather.forecast_nodes();

        assert_eq!(nodes[0].humidity_percent, Some(0.0));
        assert_eq!(nodes[1].humidity_percent, None);
        assert_eq!(nodes[2].humidity_percent, None);
    }

    #[test]
    fn normalizes_driver_names_for_fallback_matching() {
        assert_eq!(normalized_name("  Test DRIVER "), "test driver");
    }

    #[test]
    fn recovers_qualification_grid_and_lap_history_from_rest() {
        let history_entry =
            |slot_id, driver_name: &str, car_class: &str, lap_time| RestStandingHistory {
                slot_id,
                driver_name: driver_name.to_owned(),
                car_class: car_class.to_owned(),
                total_laps: i32::from(lap_time > 0.0),
                lap_time,
            };
        let mut resolver = LocalRestResolver::empty();
        resolver.seed_history(HashMap::from([
            (
                "4".to_owned(),
                vec![
                    history_entry(4, "Driver B", "GT3", -1.0),
                    history_entry(4, "Driver B", "GT3", 102.4),
                ],
            ),
            (
                "7".to_owned(),
                vec![history_entry(7, "Driver A", "GT3", -1.0)],
            ),
            (
                "9".to_owned(),
                vec![history_entry(9, "Prototype", "HYPERCAR", -1.0)],
            ),
        ]));
        resolver.seed_standings(vec![
            RestStanding {
                slot_id: 4,
                driver_name: "Driver B".to_owned(),
                car_class: "GT3".to_owned(),
                qualification: 9,
                ..RestStanding::default()
            },
            RestStanding {
                slot_id: 7,
                driver_name: "Driver A".to_owned(),
                car_class: "GT3".to_owned(),
                qualification: 3,
                ..RestStanding::default()
            },
            RestStanding {
                slot_id: 9,
                driver_name: "Prototype".to_owned(),
                car_class: "HYPERCAR".to_owned(),
                qualification: 1,
                ..RestStanding::default()
            },
        ]);

        assert_eq!(resolver.initial_class_count("LMGT3"), 2);
        assert_eq!(
            resolver.starting_class_position(4, "Driver B", "GT3"),
            Some(2)
        );
        assert_eq!(
            resolver.starting_class_position(4, "Driver A", "GT3"),
            Some(1),
            "a shared-memory vehicle id that collides with another REST slot must match by driver"
        );
        assert_eq!(resolver.history(4, "Driver A").unwrap()[0].slot_id, 7);
        assert_eq!(resolver.history(4, "Driver B").unwrap()[1].lap_time, 102.4);
    }
}
