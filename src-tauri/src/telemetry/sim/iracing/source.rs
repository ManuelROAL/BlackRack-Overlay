//! Telemetry source backed by the simulator's shared memory.
//!
//! This source fills the session, car and standings state the overlay host
//! needs, plus the Driving, Delta, Timing, fuel and player-tyre values. Other
//! areas remain at their documented sentinel until they are validated against
//! the simulator.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use super::super::{SourceDescriptor, TelemetrySource};
use super::irsdk::Connection;
use super::session::{self, Session};
use super::standings;
use super::{foreground, DESCRIPTOR};
use crate::telemetry::fuel_strategy::{
    calculate_lap_reference_strategy, projected_consumption, FuelStrategies, LapConsumptionWindow,
    ResourceAutonomy, ResourceStrategyInput,
};
use crate::telemetry::{TelemetryDemand, TelemetryFrame, TrackMapVehicle};

#[path = "warnings.rs"]
mod warnings;
#[path = "weather.rs"]
mod weather;

/// How long to wait before reopening a mapping that stopped answering, so a
/// simulator restart is picked up without probing on every cycle.
const RECONNECT_INTERVAL: Duration = Duration::from_millis(500);
/// The session string is large and is republished as results change. Parsing it
/// at most once a second keeps the 50 Hz loop away from that cost.
const SESSION_PARSE_INTERVAL: Duration = Duration::from_secs(1);
/// The foreground owner rarely changes, so it is sampled well below the loop
/// rate instead of once per frame.
const FOREGROUND_INTERVAL: Duration = Duration::from_millis(200);

const SURFACE_NOT_IN_WORLD: i32 = -1;
const SURFACE_OFF_TRACK: i32 = 0;

/// Full-course caution bits, which the frame reports as its own phase.
const FLAG_CAUTION: u32 = 0x0000_4000 | 0x0000_8000;

const TIRE_TEMP_NAMES: [[&str; 3]; 4] = [
    ["LFtempCL", "LFtempCM", "LFtempCR"],
    ["RFtempCL", "RFtempCM", "RFtempCR"],
    ["LRtempCL", "LRtempCM", "LRtempCR"],
    ["RRtempCL", "RRtempCM", "RRtempCR"],
];
const TIRE_WEAR_NAMES: [[&str; 3]; 4] = [
    ["LFwearL", "LFwearM", "LFwearR"],
    ["RFwearL", "RFwearM", "RFwearR"],
    ["LRwearL", "LRwearM", "LRwearR"],
    ["RRwearL", "RRwearM", "RRwearR"],
];

pub(super) struct IracingTelemetrySource {
    connection: Option<Connection>,
    minimum_tick_after_reconnect: Option<i32>,
    reconnect_at: Instant,
    session: Session,
    session_parsed_at: Option<Instant>,
    session_number: i32,
    last_laps_completed: i32,
    lap_valid: bool,
    fuel_at_lap_start: Option<f64>,
    fuel_previous_sample: Option<f64>,
    fuel_added_this_lap: f64,
    fuel_last_lap: Option<f64>,
    fuel_qualifying_lap: Option<f64>,
    fuel_consumption: LapConsumptionWindow,
    rejoin_hold_until: Option<Instant>,
    rejoin_reason: &'static str,
    observed_max_rpm: f64,
    foreground: bool,
    foreground_checked_at: Option<Instant>,
    starting_positions: HashMap<i32, i32>,
}

impl IracingTelemetrySource {
    pub(super) fn new() -> Self {
        Self {
            connection: None,
            minimum_tick_after_reconnect: None,
            reconnect_at: Instant::now(),
            session: Session::new(),
            session_parsed_at: None,
            session_number: i32::MIN,
            last_laps_completed: -1,
            lap_valid: true,
            fuel_at_lap_start: None,
            fuel_previous_sample: None,
            fuel_added_this_lap: 0.0,
            fuel_last_lap: None,
            fuel_qualifying_lap: None,
            fuel_consumption: LapConsumptionWindow::default(),
            rejoin_hold_until: None,
            rejoin_reason: "rejoin",
            observed_max_rpm: 0.0,
            foreground: false,
            foreground_checked_at: None,
            starting_positions: HashMap::new(),
        }
    }

    /// Drops everything learned inside a session. Called when the simulator
    /// disconnects and when it moves to another session of the event.
    fn reset(&mut self) {
        self.last_laps_completed = -1;
        self.lap_valid = true;
        self.fuel_at_lap_start = None;
        self.fuel_previous_sample = None;
        self.fuel_added_this_lap = 0.0;
        self.fuel_last_lap = None;
        self.fuel_qualifying_lap = None;
        self.fuel_consumption = LapConsumptionWindow::default();
        self.rejoin_hold_until = None;
        self.rejoin_reason = "rejoin";
        self.observed_max_rpm = 0.0;
        self.starting_positions.clear();
    }

    /// A new mapping is a new producer, even if iRacing reuses its session
    /// generation counter. Invalidate the document cache before reading it
    /// again from the reopened connection.
    fn reset_connection(&mut self) {
        self.reset();
        self.session = Session::new();
        self.session_parsed_at = None;
        self.session_number = i32::MIN;
    }

    fn waiting(&self, connected: bool) -> TelemetryFrame {
        let mut frame = TelemetryFrame::waiting_for_simulator(connected);
        frame.source = DESCRIPTOR.id;
        frame.source_name = DESCRIPTOR.display_name;
        frame.capabilities = DESCRIPTOR.capabilities;
        frame
    }

    fn simulator_has_focus(&mut self) -> bool {
        let due = self
            .foreground_checked_at
            .is_none_or(|checked| checked.elapsed() >= FOREGROUND_INTERVAL);
        if due {
            self.foreground = foreground::simulator_has_focus();
            self.foreground_checked_at = Some(Instant::now());
        }
        self.foreground
    }
}

impl TelemetrySource for IracingTelemetrySource {
    fn descriptor(&self) -> SourceDescriptor {
        DESCRIPTOR
    }

    fn next_frame(&mut self, demand: TelemetryDemand) -> TelemetryFrame {
        if self.connection.is_none() {
            let now = Instant::now();
            if now < self.reconnect_at {
                return self.waiting(false);
            }
            self.reconnect_at = now + RECONNECT_INTERVAL;
            self.connection = Connection::open_after_tick(self.minimum_tick_after_reconnect);
            if self.connection.is_some() {
                self.minimum_tick_after_reconnect = None;
            }
        }
        let last_tick_before_refresh = self.connection.as_ref().and_then(Connection::tick);
        if !self.connection.as_mut().is_some_and(Connection::refresh) {
            // The mapping outlives a session but not the process, so a silent
            // one is closed and reopened instead of polled forever.
            self.minimum_tick_after_reconnect = last_tick_before_refresh;
            self.connection = None;
            self.reconnect_at = Instant::now() + RECONNECT_INTERVAL;
            self.reset_connection();
            return self.waiting(false);
        }
        // Held outside `self` while the frame is assembled so the readers can
        // take the source mutably; it goes straight back afterwards.
        let sdk = self.connection.take().expect("la conexión está abierta");
        let frame = self.build(&sdk, demand);
        self.connection = Some(sdk);
        frame
    }
}

impl IracingTelemetrySource {
    fn build(&mut self, sdk: &Connection, demand: TelemetryDemand) -> TelemetryFrame {
        if self
            .session_parsed_at
            .is_none_or(|parsed| parsed.elapsed() >= SESSION_PARSE_INTERVAL)
            && self
                .session
                .refresh(sdk.session_generation(), sdk.session_text())
        {
            self.session_parsed_at = Some(Instant::now());
        }

        let session_number = sdk.integer("SessionNum").unwrap_or(0);
        if session_number != self.session_number {
            self.reset();
            self.session_number = session_number;
        }
        let schedule = self.session.scheduled(session_number).cloned();
        let session_name = schedule.as_ref().map_or("", |entry| entry.name.as_str());
        let session_state = sdk.integer("SessionState").unwrap_or(0);

        let in_garage = sdk.flag("IsInGarage").unwrap_or(false);
        let on_track = sdk.flag("IsOnTrack").unwrap_or(false);
        let on_pit_road = sdk.flag("OnPitRoad").unwrap_or(false);
        let session_flags = sdk.bits("SessionFlags");
        let surface = sdk
            .integer("PlayerTrackSurface")
            .unwrap_or(SURFACE_NOT_IN_WORLD);
        let lap_progress = sdk.number("LapDistPct").unwrap_or(0.0).clamp(0.0, 1.0);
        let laps_completed = sdk.integer("LapCompleted").unwrap_or(0).max(0);
        let lap_changed = laps_completed != self.last_laps_completed;
        let previous_lap_known = self.last_laps_completed >= 0;
        let previous_lap_valid = self.lap_valid && !on_pit_road;

        // A lap is not marked invalid by the simulator, so it is tracked from
        // the surface the car has been on since the last crossing.
        if surface == SURFACE_OFF_TRACK || on_pit_road {
            self.lap_valid = false;
        }
        if laps_completed != self.last_laps_completed {
            self.last_laps_completed = laps_completed;
            self.lap_valid = !on_pit_road;
        }

        let has_focus = self.simulator_has_focus();
        let mut frame = self.waiting(true);

        frame.player_active = on_track && !in_garage;
        frame.game_in_foreground = has_focus;
        frame.game_in_realtime = !in_garage && session_state != 0;
        frame.player_in_garage = in_garage;
        frame.session_type = session::session_type_code(session_name);
        frame.game_phase = game_phase(session_state, session_flags);
        frame.session_max_laps = schedule.as_ref().map_or(0, |entry| entry.laps);
        frame.session_time_remaining = finite_duration(sdk.number("SessionTimeRemain"));
        frame.session_elapsed_seconds = sdk.number("SessionTime").unwrap_or(0.0).max(0.0);
        frame.game_time_of_day_seconds = sdk.number("SessionTimeOfDay").unwrap_or(0.0).max(0.0);
        frame.session_max_time_seconds = schedule.as_ref().map_or(0.0, |entry| entry.seconds);
        frame.track_name = self.session.track_name.clone();
        frame.player_vehicle_name = self.session.player_car_name.clone();
        frame.player_vehicle_livery_name = self.session.player_car_name.clone();
        frame.track_length_meters = self.session.track_length_meters;

        if demand.include_standings {
            frame.standings = standings::build(
                &self.session,
                sdk,
                session_number,
                frame.session_type,
                session_state,
                &mut self.starting_positions,
            );
            frame.leader_total_laps = frame
                .standings
                .iter()
                .map(|entry| entry.total_laps)
                .max()
                .unwrap_or(0);
            if let Some(player) = frame.standings.iter().find(|entry| entry.is_player) {
                frame.player_position = player.overall_position;
                frame.player_class_position = player.position;
                frame.player_class_size = frame
                    .standings
                    .iter()
                    .filter(|entry| entry.vehicle_class == player.vehicle_class)
                    .count() as i32;
            }
        }

        frame.lap_number = sdk.integer("Lap").unwrap_or(0).max(0);
        frame.player_total_laps = laps_completed;
        frame.player_sector = self.session.sector(lap_progress);
        frame.player_lap_valid = self.lap_valid;
        frame.player_in_pits = on_pit_road;
        frame.lap_progress = lap_progress;
        frame.current_lap_seconds = sdk.number("LapCurrentLapTime").unwrap_or(0.0).max(0.0);
        let (last_lap_seconds, last_lap_valid) = normalized_last_lap(sdk.number("LapLastLapTime"));
        frame.last_lap_seconds = last_lap_seconds;
        frame.last_lap_valid = last_lap_valid;
        frame.best_lap_seconds = normalized_lap_time(sdk.number("LapBestLapTime"));
        let (lap_delta_seconds, lap_delta_available) = native_lap_delta(sdk);
        frame.lap_delta_seconds = lap_delta_seconds;
        frame.lap_delta_available = lap_delta_available;

        frame.speed_kph = sdk.number("Speed").unwrap_or(0.0).max(0.0) * 3.6;
        frame.gear = sdk.integer("Gear").unwrap_or(0).clamp(-1, 9) as i8;
        frame.rpm = sdk.number("RPM").unwrap_or(0.0).max(0.0);
        self.observed_max_rpm = self.observed_max_rpm.max(frame.rpm);
        // The published redline is the reliable ceiling; the highest reading of
        // the session stands in until the session string has been read.
        frame.max_rpm = [self.session.redline_rpm, self.observed_max_rpm, 1.0]
            .into_iter()
            .find(|value| *value > 1.0)
            .unwrap_or(1.0);
        frame.throttle = sdk.number("Throttle").unwrap_or(0.0).clamp(0.0, 1.0);
        frame.brake = sdk.number("Brake").unwrap_or(0.0).clamp(0.0, 1.0);
        frame.clutch = sdk.number("Clutch").unwrap_or(0.0).clamp(0.0, 1.0);
        frame.brake_bias_percent = sdk.number("dcBrakeBias").unwrap_or(0.0);
        frame.abs_active = sdk.flag("BrakeABSactive").unwrap_or(false);
        // Traction control has no published state, so the indicator stays off
        // rather than guessing one from throttle and slip.
        frame.tc_active = false;
        frame.steering_angle_degrees = sdk
            .number("SteeringWheelAngle")
            .map(f64::to_degrees)
            .unwrap_or(0.0);
        frame.force_feedback = sdk.number("SteeringWheelPctTorque").unwrap_or(0.0);

        let fuel_capacity = fuel_capacity(&self.session, sdk);
        let fuel_level = fuel_level(sdk, fuel_capacity);
        let _fuel_used_current_lap = self.observe_fuel(
            fuel_level,
            lap_changed,
            previous_lap_known,
            previous_lap_valid,
            frame.game_phase == 5,
            frame.session_type == 5,
        );
        let fuel_average = self.fuel_consumption.average().unwrap_or(0.0);
        let fuel_last = self.fuel_last_lap.unwrap_or(0.0);
        let fuel_qualifying = self.fuel_qualifying_lap.unwrap_or(0.0);
        let fuel_per_lap =
            projected_consumption([fuel_average, fuel_last, fuel_qualifying, 0.0, 0.0]);
        let lap_seconds = [
            frame.last_lap_seconds,
            frame.best_lap_seconds,
            frame.current_lap_seconds,
        ]
        .into_iter()
        .find(|value| value.is_finite() && *value > 0.0)
        .unwrap_or(0.0);
        let session_laps_remaining = remaining_laps(
            schedule.as_ref(),
            frame.session_time_remaining,
            laps_completed,
            lap_progress,
            lap_seconds,
            frame.game_phase,
        );
        let strategy_input = |consumption| ResourceStrategyInput {
            current: fuel_level.unwrap_or(0.0),
            capacity: fuel_capacity,
            consumption,
            laps_remaining: session_laps_remaining,
            lap_progress,
            completed_laps: laps_completed,
            pit_cycle_consumption: 0.0,
            pit_out_consumption: 0.0,
            pit_out_lap: false,
            pit_requested: false,
        };
        let strategy = |consumption| {
            fuel_level
                .is_some()
                .then(|| {
                    calculate_lap_reference_strategy(
                        strategy_input(consumption),
                        lap_seconds,
                        0,
                        crate::telemetry::fuel_refuel_margin(),
                    )
                })
                .flatten()
        };
        let active_strategy = strategy(fuel_per_lap);
        let fuel_strategies = FuelStrategies {
            active: active_strategy,
            fuel: active_strategy,
            average: strategy(fuel_average),
            qualifying: strategy(fuel_qualifying),
            last: strategy(fuel_last),
            ..FuelStrategies::default()
        }
        .with_qualifying_guidance();
        let resource_autonomy =
            ResourceAutonomy::calculate(fuel_level, fuel_per_lap, None, 0.0, false);
        let mut auxiliary_vehicles = if demand.include_track_map
            || demand.include_flag_warning
            || demand.include_rejoin_warning
        {
            build_auxiliary_vehicles(
                &self.session,
                sdk,
                session_number,
                frame.track_length_meters,
            )
        } else {
            Vec::new()
        };
        let inferred_yellow_id = (demand.include_track_map || demand.include_flag_warning)
            .then_some(session_flags)
            .filter(|flags| warnings::yellow_flag_active(*flags))
            .and_then(|_| {
                warnings::inferred_yellow_culprit_id(&auxiliary_vehicles, frame.track_length_meters)
            });
        if let Some(vehicle_id) = inferred_yellow_id {
            for vehicle in &mut auxiliary_vehicles {
                vehicle.map.causing_yellow = vehicle.map.vehicle_id == vehicle_id;
            }
        }
        let flag_warning = if demand.include_flag_warning {
            let warning = warnings::flag_warning(
                &auxiliary_vehicles,
                session_flags,
                session_state,
                frame.game_phase,
                frame.track_length_meters,
            );
            warning
        } else {
            crate::telemetry::FlagWarning::default()
        };
        let rejoin_warning = if demand.include_rejoin_warning {
            self.update_rejoin_warning(
                &auxiliary_vehicles,
                frame.speed_kph,
                surface,
                on_pit_road,
                frame.track_length_meters,
            )
        } else {
            crate::telemetry::RejoinWarning::default()
        };
        let weather = weather::read(sdk);
        let tire_temperatures = read_wheel_values(sdk, &TIRE_TEMP_NAMES);
        let tire_wear = read_wheel_values(sdk, &TIRE_WEAR_NAMES);
        let oil_temperature = optional_value(sdk.number("OilTemp"));
        let water_temperature = optional_value(sdk.number("WaterTemp"));
        let engine_warnings = sdk.bits("EngineWarnings");

        frame.fuel_liters = fuel_level.unwrap_or(-1.0);
        frame.fuel_added_this_lap = self.fuel_added_this_lap;
        frame.fuel_capacity_liters = fuel_capacity;
        frame.fuel_per_lap = fuel_per_lap;
        frame.fuel_last_lap = fuel_last;
        frame.fuel_qualifying_lap = fuel_qualifying;
        frame.fuel_reference_per_lap = fuel_per_lap;
        frame.fuel_projected_lap = fuel_per_lap;
        frame.estimated_fuel_laps = fuel_level
            .zip((fuel_per_lap > 0.0).then_some(fuel_per_lap))
            .map_or(0.0, |(level, consumption)| level / consumption);
        frame.session_laps_remaining = session_laps_remaining;
        frame.session_laps_remaining_estimated = session_laps_remaining;
        frame.session_lap_equivalents_remaining = session_laps_remaining;
        frame.session_total_laps_estimated =
            laps_completed as f64 + lap_progress + session_laps_remaining;
        frame.fuel_needed_liters = fuel_per_lap * session_laps_remaining;
        frame.fuel_to_add_liters = active_strategy.map_or(0.0, |plan| plan.total_additional);
        frame.resource_autonomy = resource_autonomy;
        frame.fuel_strategies = fuel_strategies;
        frame.rest_weather_available = weather.available;
        frame.ambient_temperature_c = weather.ambient_temperature_c;
        frame.track_temperature_c = weather.track_temperature_c;
        frame.rain_percent = weather.rain_percent;
        frame.track_wetness_percent = weather.track_wetness_percent;
        frame.track_wetness_min_percent = weather.track_wetness_percent;
        frame.track_wetness_max_percent = weather.track_wetness_percent;
        frame.current_humidity_percent = weather.current_humidity_percent;
        frame.wind_speed_ms = weather.wind_speed_ms;
        frame.wind_direction_degrees = weather.wind_direction_degrees;
        frame.wind_relative_direction_degrees = weather.wind_relative_direction_degrees;
        frame.player_grip_percent = -1.0;
        frame.track_rubber_percent = -1.0;
        frame.track_grip_state = weather.track_grip_state;
        frame.cloud_coverage = weather.cloud_coverage;
        frame.flag_warning = flag_warning;
        frame.rejoin_warning = rejoin_warning;
        frame.track_map_vehicles = if demand.include_track_map {
            auxiliary_vehicles
                .iter()
                .map(|vehicle| vehicle.map.clone())
                .collect()
        } else {
            Vec::new()
        };
        frame.player_damage_percent = -1.0;
        frame.player_suspension_damage_percent = -1.0;
        frame.player_suspension_damage_by_wheel_percent = [-1.0; 4];
        frame.player_body_damage_percent = -1.0;
        frame.player_tire_temperature_c =
            std::array::from_fn(|index| average_value(tire_temperatures[index]));
        frame.player_tire_temperature_by_zone_c = tire_temperatures;
        frame.player_tire_remaining_by_wheel_percent =
            std::array::from_fn(|index| minimum_value(tire_wear[index]));
        frame.player_tire_flat_spot_percent = [-1.0; 4];
        frame.player_brake_temperature_c = [-1.0; 4];
        frame.player_tire_sliding_fraction = [0.0; 4];
        frame.player_engine_oil_temperature_c = oil_temperature;
        frame.player_engine_water_temperature_c = water_temperature;
        frame.player_engine_overheating = engine_warnings & 0x0000_0001 != 0;
        frame
    }

    fn observe_fuel(
        &mut self,
        fuel_level: Option<f64>,
        lap_changed: bool,
        previous_lap_known: bool,
        previous_lap_valid: bool,
        previous_lap_green: bool,
        qualifying: bool,
    ) -> f64 {
        let Some(level) = fuel_level else {
            return 0.0;
        };
        if let Some(previous) = self.fuel_previous_sample {
            if level > previous {
                self.fuel_added_this_lap += level - previous;
            }
        }
        if self.fuel_at_lap_start.is_none() {
            self.fuel_at_lap_start = Some(level);
        }
        if lap_changed && previous_lap_known {
            let used = self
                .fuel_at_lap_start
                .map(|start| start + self.fuel_added_this_lap - level)
                .unwrap_or(0.0)
                .max(0.0);
            self.fuel_last_lap = (used > 0.0).then_some(used);
            if previous_lap_valid && previous_lap_green && used > 0.0 {
                self.fuel_consumption.push(used);
                if qualifying {
                    self.fuel_qualifying_lap = Some(
                        self.fuel_qualifying_lap
                            .map_or(used, |current| current.min(used)),
                    );
                }
            }
            self.fuel_at_lap_start = Some(level);
            self.fuel_added_this_lap = 0.0;
        }
        self.fuel_previous_sample = Some(level);
        self.fuel_at_lap_start
            .map(|start| (start + self.fuel_added_this_lap - level).max(0.0))
            .unwrap_or(0.0)
    }
}

#[derive(Clone)]
struct AuxiliaryVehicle {
    map: TrackMapVehicle,
    speed_kph: f64,
    speed_available: bool,
    pace_seconds: f64,
    pace_available: bool,
    progress: f64,
    track_surface: i32,
}

fn build_auxiliary_vehicles(
    session: &Session,
    sdk: &Connection,
    session_number: i32,
    track_length: f64,
) -> Vec<AuxiliaryVehicle> {
    if track_length <= 100.0 || !track_length.is_finite() {
        return Vec::new();
    }
    let results = session.results(session_number);
    let mut vehicles = session
        .cars()
        .iter()
        .filter(|driver| {
            (0..64).contains(&driver.car_idx) && !driver.is_spectator && !driver.is_pace_car
        })
        .filter_map(|driver| {
            let index = usize::try_from(driver.car_idx).ok()?;
            let result = results
                .iter()
                .find(|result| result.car_idx == driver.car_idx);
            let fraction = sdk
                .number_at("CarIdxLapDistPct", index)
                .or_else(|| {
                    (driver.car_idx == session.player_car_idx)
                        .then(|| sdk.number("LapDistPct"))
                        .flatten()
                })
                .filter(|value| value.is_finite() && (0.0..=1.0).contains(value))?;
            let laps = sdk
                .integer_at("CarIdxLapCompleted", index)
                .filter(|value| *value >= 0)
                .or_else(|| {
                    sdk.integer_at("CarIdxLap", index)
                        .filter(|value| *value >= 0)
                })
                .or_else(|| {
                    result
                        .map(|result| result.laps_complete)
                        .filter(|value| *value >= 0)
                })
                .unwrap_or(0);
            let surface = sdk
                .integer_at("CarIdxTrackSurface", index)
                .or_else(|| {
                    (driver.car_idx == session.player_car_idx)
                        .then(|| sdk.integer("PlayerTrackSurface"))
                        .flatten()
                })
                .unwrap_or(SURFACE_NOT_IN_WORLD);
            let in_pits = sdk.integer_at("CarIdxOnPitRoad", index).map_or_else(
                || {
                    driver.car_idx == session.player_car_idx
                        && sdk.flag("OnPitRoad").unwrap_or(false)
                },
                |value| value != 0,
            );
            let in_garage = surface == SURFACE_NOT_IN_WORLD && !in_pits;
            let best_lap = sdk
                .number_at("CarIdxBestLapTime", index)
                .filter(|value| value.is_finite() && *value > 1.0)
                .or_else(|| {
                    result
                        .map(|result| result.fastest_lap_time)
                        .filter(|value| *value > 1.0)
                });
            let last_lap = sdk
                .number_at("CarIdxLastLapTime", index)
                .filter(|value| value.is_finite() && *value > 1.0)
                .or_else(|| {
                    result
                        .map(|result| result.last_lap_time)
                        .filter(|value| *value > 1.0)
                });
            let pace = best_lap.or(last_lap);
            let pace_available = pace.is_some();
            let pace_seconds = pace.unwrap_or(100.0);
            let speed = sdk
                .number_at("CarIdxSpeed", index)
                .filter(|value| value.is_finite() && *value >= 0.0)
                .map(|value| value * 3.6);
            let speed_available = speed.is_some();
            let speed_kph = speed.unwrap_or(0.0);
            let overall_position = sdk
                .integer_at("CarIdxPosition", index)
                .filter(|value| *value > 0)
                .or_else(|| result.and_then(|result| result.overall_position))
                .unwrap_or(0);
            let vehicle_class = if driver.car_class_short_name.trim().is_empty() {
                "OTHER".to_owned()
            } else {
                driver.car_class_short_name.trim().to_owned()
            };
            let progress = f64::from(laps) + fraction;
            Some(AuxiliaryVehicle {
                map: TrackMapVehicle {
                    vehicle_id: driver.car_idx,
                    overall_position,
                    vehicle_class,
                    world_x: 0.0,
                    world_y: 0.0,
                    world_position_available: false,
                    lap_distance: fraction * track_length,
                    total_laps: laps,
                    in_pits,
                    in_garage,
                    causing_yellow: false,
                    sector: session.sector(fraction),
                    is_player: driver.car_idx == session.player_car_idx,
                },
                speed_kph,
                speed_available,
                pace_seconds,
                pace_available,
                progress,
                track_surface: surface,
            })
        })
        .collect::<Vec<_>>();
    vehicles.sort_by(|left, right| {
        let left_position = if left.map.overall_position > 0 {
            left.map.overall_position
        } else {
            i32::MAX
        };
        let right_position = if right.map.overall_position > 0 {
            right.map.overall_position
        } else {
            i32::MAX
        };
        left_position
            .cmp(&right_position)
            .then_with(|| right.progress.total_cmp(&left.progress))
            .then_with(|| left.map.vehicle_id.cmp(&right.map.vehicle_id))
    });
    let mut next_position = 1;
    for vehicle in &mut vehicles {
        if vehicle.map.overall_position <= 0 {
            vehicle.map.overall_position = next_position;
        }
        next_position = next_position.max(vehicle.map.overall_position.saturating_add(1));
    }
    vehicles
}

fn fuel_capacity(session: &Session, sdk: &Connection) -> f64 {
    let configured = session.fuel_capacity_liters;
    if configured > 0.0 {
        return configured;
    }
    let percent = sdk.number("FuelLevelPct").and_then(normalized_percent);
    let level = sdk
        .number("FuelLevel")
        .filter(|value| value.is_finite() && *value >= 0.0);
    level
        .zip(percent)
        .and_then(|(level, percent)| (percent > 0.0).then_some(level / percent))
        .filter(|capacity| capacity.is_finite() && *capacity > 0.0)
        .unwrap_or(-1.0)
}

fn fuel_level(sdk: &Connection, capacity: f64) -> Option<f64> {
    sdk.number("FuelLevel")
        .filter(|value| value.is_finite() && *value >= 0.0)
        .or_else(|| {
            sdk.number("FuelLevelPct")
                .and_then(normalized_percent)
                .zip((capacity > 0.0).then_some(capacity))
                .map(|(percent, capacity)| percent * capacity)
        })
}

fn normalized_percent(value: f64) -> Option<f64> {
    if !value.is_finite() || value < 0.0 {
        return None;
    }
    Some(if value <= 1.0 { value } else { value / 100.0 }.clamp(0.0, 1.0))
}

fn remaining_laps(
    schedule: Option<&super::session::Schedule>,
    time_remaining: f64,
    completed_laps: i32,
    progress: f64,
    lap_seconds: f64,
    phase: u32,
) -> f64 {
    if phase >= 8 {
        return (1.0 - progress).max(0.0);
    }
    if let Some(laps) = schedule.map(|entry| entry.laps).filter(|laps| *laps > 0) {
        return (f64::from(laps - completed_laps) - progress).max(0.0);
    }
    if time_remaining > 0.0 && lap_seconds > 0.0 {
        return (time_remaining / lap_seconds + progress).ceil() - progress;
    }
    0.0
}

fn read_wheel_values(sdk: &Connection, names: &[[&str; 3]; 4]) -> [[f64; 3]; 4] {
    std::array::from_fn(|wheel| {
        std::array::from_fn(|zone| optional_value(sdk.number(names[wheel][zone])))
    })
}

fn optional_value(value: Option<f64>) -> f64 {
    value
        .filter(|value| value.is_finite() && *value >= 0.0)
        .unwrap_or(-1.0)
}

fn average_value(values: [f64; 3]) -> f64 {
    let (sum, count) = values
        .into_iter()
        .filter(|value| *value >= 0.0)
        .fold((0.0, 0), |(sum, count), value| (sum + value, count + 1));
    (count > 0)
        .then_some(sum / f64::from(count))
        .unwrap_or(-1.0)
}

fn minimum_value(values: [f64; 3]) -> f64 {
    values
        .into_iter()
        .filter(|value| *value >= 0.0)
        .min_by(f64::total_cmp)
        .unwrap_or(-1.0)
}

/// iRacing uses a negative official time for an invalid completed lap and a
/// small negative sentinel before the first result exists. Preserve the time
/// for Timing while only the real lap-time range can mark a result invalid.
fn normalized_last_lap(value: Option<f64>) -> (f64, bool) {
    let Some(value) = value.filter(|value| value.is_finite()) else {
        return (0.0, true);
    };
    let absolute = value.abs();
    if (20.0..900.0).contains(&absolute) {
        (absolute, value >= 0.0)
    } else {
        (0.0, true)
    }
}

fn normalized_lap_time(value: Option<f64>) -> f64 {
    value
        .filter(|value| value.is_finite() && (20.0..900.0).contains(value))
        .unwrap_or(0.0)
}

/// `LapDeltaToBestLap` is the player's current-session comparison, matching
/// the reference carried by `best_lap_seconds` and the shared Timing model.
/// The companion `_OK` flag prevents a zero/uninitialised value from becoming
/// a false live delta. Older sessions without the flag remain compatible when
/// the delta itself is in the documented lap-time range.
fn native_lap_delta(sdk: &Connection) -> (f64, bool) {
    native_lap_delta_value(
        sdk.number("LapDeltaToBestLap"),
        sdk.flag("LapDeltaToBestLap_OK"),
    )
}

fn native_lap_delta_value(value: Option<f64>, valid_flag: Option<bool>) -> (f64, bool) {
    let Some(value) = value else {
        return (0.0, false);
    };
    let finite = value.is_finite() && value.abs() < 900.0;
    let valid = valid_flag.unwrap_or(true) && finite;
    (if valid { value } else { 0.0 }, valid)
}

/// The frame numbers session phases the way the domain models read them: three
/// is formation, five is green and six is a full-course caution.
fn game_phase(session_state: i32, flags: u32) -> u32 {
    if flags & FLAG_CAUTION != 0 {
        return 6;
    }
    match session_state {
        1 => 2,
        2 => 1,
        3 => 3,
        4 => 5,
        5 | 6 => 8,
        _ => 0,
    }
}

/// A session without a clock publishes a week of remaining time. Anything from
/// that sentinel upwards becomes the zero the frame expects for "no clock",
/// which still admits the longest real session of a day.
fn finite_duration(value: Option<f64>) -> f64 {
    const UNLIMITED_SECONDS: f64 = 7.0 * 24.0 * 3_600.0;
    value
        .filter(|seconds| seconds.is_finite() && (0.0..UNLIMITED_SECONDS).contains(seconds))
        .unwrap_or(0.0)
}

#[cfg(test)]
mod tests {
    use super::super::session::Schedule;
    use super::{
        finite_duration, game_phase, minimum_value, native_lap_delta_value, normalized_lap_time,
        normalized_last_lap, normalized_percent, remaining_laps, IracingTelemetrySource,
    };

    #[test]
    fn maps_session_state_and_flags_to_phases() {
        assert_eq!(game_phase(0, 0), 0);
        assert_eq!(game_phase(3, 0), 3);
        assert_eq!(game_phase(4, 0), 5);
        assert_eq!(game_phase(5, 0), 8);
        assert_eq!(game_phase(4, 0x0000_4000), 6);
        assert_eq!(game_phase(4, 0x0000_8000), 6);
    }

    #[test]
    fn treats_an_unlimited_clock_as_no_clock() {
        assert_eq!(finite_duration(Some(1_800.0)), 1_800.0);
        assert_eq!(finite_duration(Some(604_800.0)), 0.0);
        assert_eq!(finite_duration(Some(-1.0)), 0.0);
        assert_eq!(finite_duration(None), 0.0);
    }

    #[test]
    fn preserves_invalid_iracing_lap_time_without_treating_the_sentinel_as_a_result() {
        assert_eq!(normalized_last_lap(Some(92.5)), (92.5, true));
        assert_eq!(normalized_last_lap(Some(-92.5)), (92.5, false));
        assert_eq!(normalized_last_lap(Some(-1.0)), (0.0, true));
        assert_eq!(normalized_lap_time(Some(-92.5)), 0.0);
        assert_eq!(normalized_lap_time(Some(0.0)), 0.0);
    }

    #[test]
    fn only_uses_a_native_delta_when_iracing_marks_it_valid() {
        assert_eq!(
            native_lap_delta_value(Some(-0.42), Some(true)),
            (-0.42, true)
        );
        assert_eq!(
            native_lap_delta_value(Some(-0.42), Some(false)),
            (0.0, false)
        );
        assert_eq!(native_lap_delta_value(Some(0.0), None), (0.0, true));
        assert_eq!(native_lap_delta_value(None, Some(true)), (0.0, false));
    }

    #[test]
    fn normalizes_fuel_percent_from_fraction_or_percent_units() {
        assert_eq!(normalized_percent(0.5), Some(0.5));
        assert_eq!(normalized_percent(50.0), Some(0.5));
        assert_eq!(normalized_percent(-1.0), None);
        assert_eq!(normalized_percent(f64::NAN), None);
    }

    #[test]
    fn uses_fixed_lap_target_and_caution_finish_distance() {
        let schedule = Schedule {
            laps: 25,
            ..Schedule::default()
        };
        assert!((remaining_laps(Some(&schedule), 0.0, 7, 0.25, 100.0, 5) - 17.75).abs() < 1e-9);
        assert!((remaining_laps(None, 0.0, 7, 0.25, 100.0, 8) - 0.75).abs() < 1e-9);
    }

    #[test]
    fn records_fuel_used_at_the_next_lap_boundary() {
        let mut source = IracingTelemetrySource::new();
        assert_eq!(
            source.observe_fuel(Some(50.0), true, false, true, true, false),
            0.0
        );
        assert_eq!(
            source.observe_fuel(Some(48.0), false, true, true, true, false),
            2.0
        );
        assert_eq!(
            source.observe_fuel(Some(40.0), true, true, true, true, false),
            0.0
        );
        assert_eq!(source.fuel_last_lap, Some(10.0));
        assert_eq!(source.fuel_consumption.average(), Some(10.0));
    }

    #[test]
    fn tire_wear_uses_the_lowest_remaining_band() {
        assert_eq!(minimum_value([91.0, 84.0, 88.0]), 84.0);
        assert_eq!(minimum_value([-1.0, -1.0, -1.0]), -1.0);
    }
}
