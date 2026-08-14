#[cfg(not(test))]
use reqwest::blocking::Client;
#[cfg(not(test))]
use serde::de::DeserializeOwned;
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(not(test))]
use std::sync::mpsc;
use std::sync::mpsc::Receiver;
use std::sync::Arc;
#[cfg(not(test))]
use std::thread;
use std::time::{Duration, Instant};

#[cfg(not(test))]
const LOCAL_API: &str = "http://127.0.0.1:6397";
#[cfg(not(test))]
const STANDINGS_INTERVAL: Duration = Duration::from_secs(1);
#[cfg(not(test))]
const HISTORY_INTERVAL: Duration = Duration::from_secs(5);
#[cfg(not(test))]
const SUPPLEMENT_INTERVAL: Duration = Duration::from_secs(1);
const STANDINGS_MAX_AGE: Duration = Duration::from_secs(1);
const SUPPLEMENT_MAX_AGE: Duration = Duration::from_secs(3);
#[cfg(not(test))]
const REQUEST_TIMEOUT: Duration = Duration::from_millis(400);

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub(super) struct RestStanding {
    #[serde(rename = "slotID")]
    pub slot_id: i32,
    pub driver_name: String,
    pub car_class: String,
    pub car_number: String,
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

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct RestVehicleDamage {
    pub aero: f64,
    pub suspension: [f64; 4],
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
struct RestRepairAndRefuel {
    wearables: RestWearables,
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

#[derive(Default)]
struct SupplementUpdate {
    pit_stop: Option<RestPitStopEstimate>,
    vehicle_damage: Option<RestVehicleDamage>,
}

#[derive(Default)]
struct StandingsUpdate {
    standings: Vec<RestStanding>,
    history: Option<HashMap<String, Vec<RestStandingHistory>>>,
}

pub(super) struct LocalRestResolver {
    standings_receiver: Option<Receiver<StandingsUpdate>>,
    supplement_receiver: Option<Receiver<SupplementUpdate>>,
    standings_by_slot: HashMap<i32, RestStanding>,
    standings_by_name: HashMap<String, RestStanding>,
    history_by_slot: HashMap<i32, Vec<RestStandingHistory>>,
    history_by_name: HashMap<String, Vec<RestStandingHistory>>,
    standings_received_at: Option<Instant>,
    pit_stop: RestPitStopEstimate,
    vehicle_damage: RestVehicleDamage,
    supplement_received_at: Option<Instant>,
    vehicle_damage_received_at: Option<Instant>,
    enabled: Arc<AtomicBool>,
}

impl LocalRestResolver {
    #[cfg(not(test))]
    pub(super) fn discover() -> Self {
        let enabled = Arc::new(AtomicBool::new(false));
        let (standings_sender, standings_receiver) = mpsc::channel();
        let standings_enabled = Arc::clone(&enabled);
        thread::spawn(move || {
            let Some(client) = http_client() else {
                return;
            };
            let mut history_received_at: Option<Instant> = None;
            loop {
                if !standings_enabled.load(Ordering::Relaxed) {
                    thread::sleep(Duration::from_secs(1));
                    continue;
                }
                let started = Instant::now();
                if let Ok(standings) = fetch_json(&client, "/rest/watch/standings") {
                    let history = if history_received_at
                        .is_none_or(|received| received.elapsed() >= HISTORY_INTERVAL)
                    {
                        let response = fetch_json(&client, "/rest/watch/standings/history").ok();
                        if response.is_some() {
                            history_received_at = Some(Instant::now());
                        }
                        response
                    } else {
                        None
                    };
                    if standings_sender
                        .send(StandingsUpdate { standings, history })
                        .is_err()
                    {
                        break;
                    }
                }
                sleep_remaining(started, STANDINGS_INTERVAL);
            }
        });

        let (supplement_sender, supplement_receiver) = mpsc::channel();
        let supplement_enabled = Arc::clone(&enabled);
        thread::spawn(move || {
            let Some(client) = http_client() else {
                return;
            };
            loop {
                if !supplement_enabled.load(Ordering::Relaxed) {
                    thread::sleep(Duration::from_secs(1));
                    continue;
                }
                let started = Instant::now();
                let update = SupplementUpdate {
                    pit_stop: fetch_json(&client, "/rest/strategy/pitstop-estimate").ok(),
                    vehicle_damage: fetch_json::<RestRepairAndRefuel>(
                        &client,
                        "/rest/garage/UIScreen/RepairAndRefuel",
                    )
                    .ok()
                    .map(|response| RestVehicleDamage {
                        aero: response.wearables.body.aero,
                        suspension: response.wearables.suspension,
                    }),
                };
                if supplement_sender.send(update).is_err() {
                    break;
                }
                sleep_remaining(started, SUPPLEMENT_INTERVAL);
            }
        });

        Self {
            standings_receiver: Some(standings_receiver),
            supplement_receiver: Some(supplement_receiver),
            standings_by_slot: HashMap::new(),
            standings_by_name: HashMap::new(),
            history_by_slot: HashMap::new(),
            history_by_name: HashMap::new(),
            standings_received_at: None,
            pit_stop: RestPitStopEstimate::default(),
            vehicle_damage: RestVehicleDamage::default(),
            supplement_received_at: None,
            vehicle_damage_received_at: None,
            enabled,
        }
    }

    #[cfg(test)]
    pub(super) fn empty() -> Self {
        Self {
            standings_receiver: None,
            supplement_receiver: None,
            standings_by_slot: HashMap::new(),
            standings_by_name: HashMap::new(),
            history_by_slot: HashMap::new(),
            history_by_name: HashMap::new(),
            standings_received_at: None,
            pit_stop: RestPitStopEstimate::default(),
            vehicle_damage: RestVehicleDamage::default(),
            supplement_received_at: None,
            vehicle_damage_received_at: None,
            enabled: Arc::new(AtomicBool::new(false)),
        }
    }

    pub(super) fn refresh(&mut self, active: bool) {
        self.enabled.store(active, Ordering::Relaxed);
        let standings_updates = self
            .standings_receiver
            .as_ref()
            .map(|receiver| receiver.try_iter().collect::<Vec<_>>())
            .unwrap_or_default();
        for update in standings_updates {
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

        if let Some(receiver) = self.supplement_receiver.as_ref() {
            while let Ok(update) = receiver.try_recv() {
                let mut received = false;
                if let Some(pit_stop) = update.pit_stop {
                    self.pit_stop = pit_stop;
                    received = true;
                }
                if let Some(vehicle_damage) = update.vehicle_damage {
                    self.vehicle_damage = vehicle_damage;
                    self.vehicle_damage_received_at = Some(Instant::now());
                }
                if received {
                    self.supplement_received_at = Some(Instant::now());
                }
            }
        }
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
        self.history_by_slot.clear();
        self.history_by_name.clear();
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

    pub(super) fn pit_stop(&self) -> Option<&RestPitStopEstimate> {
        is_fresh(self.supplement_received_at, SUPPLEMENT_MAX_AGE).then_some(&self.pit_stop)
    }

    pub(super) fn vehicle_damage(&self) -> Option<RestVehicleDamage> {
        is_fresh(self.vehicle_damage_received_at, SUPPLEMENT_MAX_AGE).then_some(self.vehicle_damage)
    }
}

#[cfg(not(test))]
fn http_client() -> Option<Client> {
    Client::builder()
        .connect_timeout(REQUEST_TIMEOUT)
        .timeout(REQUEST_TIMEOUT)
        .build()
        .ok()
}

#[cfg(not(test))]
fn fetch_json<T: DeserializeOwned>(client: &Client, path: &str) -> Result<T, ()> {
    client
        .get(format!("{LOCAL_API}{path}"))
        .send()
        .map_err(|_| ())?
        .error_for_status()
        .map_err(|_| ())?
        .json::<T>()
        .map_err(|_| ())
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

fn normalized_name(name: &str) -> String {
    name.trim().to_lowercase()
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
        normalized_name, LocalRestResolver, RestPitStopEstimate, RestRepairAndRefuel, RestStanding,
        RestStandingHistory,
    };
    use std::collections::HashMap;

    #[test]
    fn parses_rest_standings_fields_used_by_the_overlay() {
        let value: RestStanding = serde_json::from_str(
            r#"{"slotID":29,"driverName":"Test Driver","carClass":"LMP2_ELMS","carNumber":"29","qualification":7,"serverScored":true,"finishStatus":"FSTAT_NONE","lapsBehindClassLeader":1,"timeBehindClassLeader":2.5,"lapsBehindNext":0,"timeBehindNext":1.2,"pitstops":2,"pitState":"REQUEST","pitting":true,"inGarageStall":false,"fuelFraction":0.5,"veFraction":0.75}"#,
        )
        .unwrap();
        assert_eq!(value.slot_id, 29);
        assert_eq!(value.car_class, "LMP2_ELMS");
        assert_eq!(value.car_number, "29");
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
    fn parses_vehicle_damage_from_repair_and_refuel() {
        let response: RestRepairAndRefuel = serde_json::from_str(
            r#"{"wearables":{"body":{"aero":0.12},"suspension":[0.01,0.2,0.03,0.04]}}"#,
        )
        .unwrap();
        assert!((response.wearables.body.aero - 0.12).abs() < f64::EPSILON);
        assert_eq!(response.wearables.suspension, [0.01, 0.2, 0.03, 0.04]);
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
