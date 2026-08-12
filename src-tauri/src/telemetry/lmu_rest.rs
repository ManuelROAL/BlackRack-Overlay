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

pub(super) struct LocalRestResolver {
    standings_receiver: Option<Receiver<Vec<RestStanding>>>,
    supplement_receiver: Option<Receiver<SupplementUpdate>>,
    standings_by_slot: HashMap<i32, RestStanding>,
    standings_by_name: HashMap<String, RestStanding>,
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
            loop {
                if !standings_enabled.load(Ordering::Relaxed) {
                    thread::sleep(Duration::from_secs(1));
                    continue;
                }
                let started = Instant::now();
                if let Ok(standings) = fetch_json(&client, "/rest/watch/standings") {
                    if standings_sender.send(standings).is_err() {
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
        if let Some(receiver) = self.standings_receiver.as_ref() {
            while let Ok(standings) = receiver.try_recv() {
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

    pub(super) fn standing(&self, slot_id: i32, driver_name: &str) -> Option<&RestStanding> {
        if !is_fresh(self.standings_received_at, STANDINGS_MAX_AGE) {
            return None;
        }
        self.standings_by_slot
            .get(&slot_id)
            .or_else(|| self.standings_by_name.get(&normalized_name(driver_name)))
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

#[cfg(test)]
mod tests {
    use super::{normalized_name, RestPitStopEstimate, RestRepairAndRefuel, RestStanding};

    #[test]
    fn parses_rest_standings_fields_used_by_the_overlay() {
        let value: RestStanding = serde_json::from_str(
            r#"{"slotID":29,"driverName":"Test Driver","carNumber":"29","qualification":7,"serverScored":true,"finishStatus":"FSTAT_NONE","lapsBehindClassLeader":1,"timeBehindClassLeader":2.5,"lapsBehindNext":0,"timeBehindNext":1.2,"pitstops":2,"pitState":"REQUEST","pitting":true,"inGarageStall":false,"fuelFraction":0.5,"veFraction":0.75}"#,
        )
        .unwrap();
        assert_eq!(value.slot_id, 29);
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
}
