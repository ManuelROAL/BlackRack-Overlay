//! Telemetry source backed by the LMU shared memory.
//!
//! This file owns the pieces every overlay model reads from: the shared-memory
//! layout, the per-car trackers and the source struct itself. The models that
//! build on them live in the child modules below, one per overlay family, and
//! reach the private types and fields here the way any child module does.

use std::collections::{HashMap, HashSet, VecDeque};
use std::ffi::{c_char, c_int};
use std::time::{Duration, Instant};

use super::super::{SourceDescriptor, TelemetrySource};
use super::driver_ranks::DriverRankResolver;
use super::event_split::{DriverRankSettings, SessionSplitResolver};
use super::rest::{
    normalized_driver_identity, normalized_name, LocalRestResolver, RestCompoundCondition,
};
use crate::telemetry::consumption_profile::{ConsumptionProfiler, ProfileEstimate};
use crate::telemetry::fuel_strategy::{
    calculate_resource_strategy, calculate_stint_targets, next_stint_autonomy, FuelStrategies,
    ResourceStrategyInput,
};
use crate::telemetry::{
    FlagWarning, RejoinWarning, StandingEntry, TelemetryDemand, TelemetryFrame, TireLifeModel,
    TrackMapVehicle,
};

mod driver_rank;
mod frame;
mod fuel;
mod session;
mod standings;
#[cfg(test)]
mod tests;
mod vehicle;
mod warnings;
mod weather;

const MAX_VEHICLES: usize = 104;
const DRIVER_SWAP_SERVICE_SECONDS: f64 = 26.0;
const DRIVER_RANK_INTERNAL_SCALE: f64 = 3.0;
const DRIVER_RANK_QUALIFY_WEIGHT: f64 = 0.176_470_588_235_294;
const DRIVER_RANK_LOG_SCHEMA_VERSION: u32 = 2;
const DRIVER_RANK_FORMULA_VERSION: &str = "pairwise-elo-v2";

fn suspension_damage_by_wheel_percent(damage: Option<[f64; 4]>, detached: [u8; 4]) -> [f64; 4] {
    std::array::from_fn(|index| {
        damage
            .map(|value| (value[index] * 100.0).clamp(0.0, 100.0))
            .unwrap_or_else(|| if detached[index] != 0 { 100.0 } else { -1.0 })
    })
}

fn suspension_damage_percent(damage: Option<[f64; 4]>, detached: [u8; 4]) -> f64 {
    damage
        .map(|value| value.into_iter().fold(0.0_f64, f64::max) * 100.0)
        .unwrap_or_else(|| if detached.contains(&1) { 100.0 } else { -1.0 })
        .clamp(-1.0, 100.0)
}

fn rear_wing_detached(
    _aero_damage: Option<f64>,
    _part_detached: bool,
    _rear_center_severity: u8,
) -> bool {
    // LMU's shared-memory flag covers every detachable body part, while REST
    // aero wear is an aggregate that can exceed 200% with the wing attached.
    // Neither source identifies the rear wing reliably.
    false
}

fn fuel_energy_ratio(fuel_consumption: f64, energy_consumption: f64) -> f64 {
    if fuel_consumption.is_finite()
        && fuel_consumption > 0.0
        && energy_consumption.is_finite()
        && energy_consumption > 0.0
    {
        fuel_consumption / energy_consumption
    } else {
        0.0
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
struct LmuStandingEntry {
    vehicle_id: i32,
    position: i32,
    total_laps: i32,
    laps_behind_leader: i32,
    is_player: u32,
    in_pits: u32,
    in_garage: u32,
    lap_invalidated: u32,
    flag: u32,
    pit_state: u32,
    pit_stops: u32,
    penalties: u32,
    finish_status: u32,
    vehicle_class_id: u32,
    sector: i32,
    individual_phase: u32,
    steam_id: u64,
    time_behind_leader: f64,
    interval: f64,
    laps_behind_next: i32,
    time_into_lap: f64,
    estimated_lap_time: f64,
    best_lap_seconds: f64,
    last_lap_seconds: f64,
    lap_start_elapsed_seconds: f64,
    elapsed_seconds: f64,
    virtual_energy: f64,
    damage_percent: f64,
    track_limits_steps: u32,
    track_limits_available: u32,
    speed_kph: f64,
    path_lateral: f64,
    track_edge: f64,
    lap_distance: f64,
    world_x: f64,
    world_y: f64,
    driver_name: [c_char; 32],
    vehicle_class: [c_char; 32],
    team_name: [c_char; 24],
    vehicle_name: [c_char; 64],
    vehicle_filename: [c_char; 32],
    vehicle_model: [c_char; 30],
    tire_compound: [c_char; 18],
    rear_tire_compound: [c_char; 18],
    wheel_compounds: [u8; 4],
}

#[derive(Clone)]
struct VehicleIdentity {
    driver_name_raw: [c_char; 32],
    vehicle_class_raw: [c_char; 32],
    team_name_raw: [c_char; 24],
    vehicle_name_raw: [c_char; 64],
    vehicle_filename_raw: [c_char; 32],
    vehicle_model_raw: [c_char; 30],
    tire_compound_raw: [c_char; 18],
    rear_tire_compound_raw: [c_char; 18],
    driver_name: String,
    vehicle_class: String,
    team_name: String,
    vehicle_name: String,
    fallback_car_number: String,
    tire_compound: String,
}

impl VehicleIdentity {
    fn matches(&self, entry: &LmuStandingEntry) -> bool {
        self.driver_name_raw == entry.driver_name
            && self.vehicle_class_raw == entry.vehicle_class
            && self.team_name_raw == entry.team_name
            && self.vehicle_name_raw == entry.vehicle_name
            && self.vehicle_filename_raw == entry.vehicle_filename
            && self.vehicle_model_raw == entry.vehicle_model
            && self.tire_compound_raw == entry.tire_compound
            && self.rear_tire_compound_raw == entry.rear_tire_compound
    }
}

impl Default for LmuStandingEntry {
    fn default() -> Self {
        Self {
            vehicle_id: 0,
            position: 0,
            total_laps: 0,
            laps_behind_leader: 0,
            is_player: 0,
            in_pits: 0,
            in_garage: 0,
            lap_invalidated: 0,
            flag: 0,
            pit_state: 0,
            pit_stops: 0,
            penalties: 0,
            finish_status: 0,
            vehicle_class_id: u32::MAX,
            sector: 0,
            individual_phase: 0,
            steam_id: 0,
            time_behind_leader: 0.0,
            interval: 0.0,
            laps_behind_next: 0,
            time_into_lap: 0.0,
            estimated_lap_time: 0.0,
            best_lap_seconds: 0.0,
            last_lap_seconds: 0.0,
            lap_start_elapsed_seconds: 0.0,
            elapsed_seconds: 0.0,
            virtual_energy: 0.0,
            damage_percent: 0.0,
            track_limits_steps: 0,
            track_limits_available: 0,
            speed_kph: 0.0,
            path_lateral: 0.0,
            track_edge: 0.0,
            lap_distance: 0.0,
            world_x: 0.0,
            world_y: 0.0,
            driver_name: [0; 32],
            vehicle_class: [0; 32],
            team_name: [0; 24],
            vehicle_name: [0; 64],
            vehicle_filename: [0; 32],
            vehicle_model: [0; 30],
            tire_compound: [0; 18],
            rear_tire_compound: [0; 18],
            wheel_compounds: [0; 4],
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
struct LmuSnapshot {
    connected: u32,
    player_active: u32,
    game_in_foreground: u32,
    game_in_realtime: u32,
    player_in_garage: u32,
    player_offroad_wheels: u32,
    standings_count: u32,
    lap_number: i32,
    player_sector: i32,
    gear: i32,
    player_total_laps: i32,
    max_laps: i32,
    session_type: i32,
    game_phase: u32,
    yellow_sectors: u32,
    leader_total_laps: i32,
    speed_kph: f64,
    rpm: f64,
    max_rpm: f64,
    throttle: f64,
    brake: f64,
    brake_bias_percent: f64,
    track_limits_steps: u32,
    track_limits_steps_per_penalty: u32,
    tc_active: u32,
    abs_active: u32,
    lift_and_coast_progress: u32,
    steering: f64,
    steering_range_degrees: f64,
    force_feedback: f64,
    fuel_liters: f64,
    fuel_capacity_liters: f64,
    virtual_energy: f64,
    vehicle_class_id: u32,
    player_lap_valid: u32,
    current_lap_seconds: f64,
    player_lap_start_elapsed_seconds: f64,
    current_sector1_seconds: f64,
    current_sector2_seconds: f64,
    player_best_sector_ends: [f64; 3],
    session_best_sector_ends: [f64; 3],
    best_lap_seconds: f64,
    lap_delta_seconds: f64,
    session_time_remaining: f64,
    session_elapsed_seconds: f64,
    game_time_of_day_seconds: f64,
    session_end_seconds: f64,
    estimated_lap_time: f64,
    last_lap_seconds: f64,
    leader_lap_time: f64,
    leader_time_into_lap: f64,
    player_time_into_lap: f64,
    player_lap_distance: f64,
    track_length: f64,
    ambient_temperature_c: f64,
    track_temperature_c: f64,
    rain_percent: f64,
    track_wetness_percent: f64,
    track_wetness_min_percent: f64,
    track_wetness_max_percent: f64,
    track_grip_level: u8,
    cloud_coverage: u8,
    wind_x: f64,
    wind_y: f64,
    wind_z: f64,
    player_orientation_right_z: f64,
    player_orientation_forward_z: f64,
    player_tire_remaining_percent: f64,
    player_damage_percent: f64,
    player_engine_oil_temperature_c: f64,
    player_engine_water_temperature_c: f64,
    player_tire_temperature_c: [f64; 4],
    player_tire_temperature_by_zone_c: [[f64; 3]; 4],
    player_brake_temperature_c: [f64; 4],
    player_tire_remaining_by_wheel_percent: [f64; 4],
    player_tire_slip_ratio: [f64; 4],
    player_tire_sliding_fraction: [f64; 4],
    player_engine_overheating: u8,
    player_part_detached: u32,
    player_tire_compounds: [u8; 4],
    player_tire_flat: [u8; 4],
    player_tire_detached: [u8; 4],
    player_damage_severity: [u8; 8],
    battery_charge_percent: f64,
    hybrid_regen_kw: f64,
    hybrid_motor_temperature_c: f64,
    hybrid_motor_rpm: f64,
    player_position: i32,
    player_class_position: i32,
    player_class_size: i32,
    hybrid_motor_state: u8,
    engine_map: u8,
    engine_map_max: u8,
    traction_control_level: u8,
    traction_control_max: u8,
    traction_control_slip: u8,
    traction_control_slip_max: u8,
    traction_control_cut: u8,
    traction_control_cut_max: u8,
    anti_lock_brakes_level: u8,
    anti_lock_brakes_max: u8,
    brake_migration: u8,
    brake_migration_max: u8,
    front_anti_roll_bar: u8,
    front_anti_roll_bar_max: u8,
    rear_anti_roll_bar: u8,
    rear_anti_roll_bar_max: u8,
    speed_limiter_active: u8,
    headlights_on: u8,
    wiper_state: u8,
    vehicle_name: [c_char; 64],
    vehicle_model: [c_char; 30],
    track_name: [c_char; 64],
    standings: [LmuStandingEntry; MAX_VEHICLES],
}

impl Default for LmuSnapshot {
    fn default() -> Self {
        Self {
            connected: 0,
            player_active: 0,
            game_in_foreground: 0,
            game_in_realtime: 0,
            player_in_garage: 0,
            player_offroad_wheels: 0,
            standings_count: 0,
            lap_number: 0,
            player_sector: 0,
            gear: 0,
            player_total_laps: 0,
            max_laps: 0,
            session_type: 0,
            game_phase: 0,
            yellow_sectors: 0,
            leader_total_laps: 0,
            speed_kph: 0.0,
            rpm: 0.0,
            max_rpm: 1.0,
            throttle: 0.0,
            brake: 0.0,
            brake_bias_percent: 0.0,
            track_limits_steps: 0,
            track_limits_steps_per_penalty: 0,
            tc_active: 0,
            abs_active: 0,
            lift_and_coast_progress: 0,
            steering: 0.0,
            steering_range_degrees: 0.0,
            force_feedback: 0.0,
            fuel_liters: 0.0,
            fuel_capacity_liters: 1.0,
            virtual_energy: 0.0,
            vehicle_class_id: u32::MAX,
            player_lap_valid: 0,
            current_lap_seconds: 0.0,
            player_lap_start_elapsed_seconds: 0.0,
            current_sector1_seconds: 0.0,
            current_sector2_seconds: 0.0,
            player_best_sector_ends: [0.0; 3],
            session_best_sector_ends: [0.0; 3],
            best_lap_seconds: 0.0,
            lap_delta_seconds: 0.0,
            session_time_remaining: 0.0,
            session_elapsed_seconds: 0.0,
            game_time_of_day_seconds: 0.0,
            session_end_seconds: 0.0,
            estimated_lap_time: 0.0,
            last_lap_seconds: 0.0,
            leader_lap_time: 0.0,
            leader_time_into_lap: 0.0,
            player_time_into_lap: 0.0,
            player_lap_distance: 0.0,
            track_length: 0.0,
            ambient_temperature_c: 0.0,
            track_temperature_c: 0.0,
            rain_percent: 0.0,
            track_wetness_percent: 0.0,
            track_wetness_min_percent: 0.0,
            track_wetness_max_percent: 0.0,
            track_grip_level: 0,
            cloud_coverage: 0,
            wind_x: 0.0,
            wind_y: 0.0,
            wind_z: 0.0,
            player_orientation_right_z: 0.0,
            player_orientation_forward_z: 1.0,
            player_tire_remaining_percent: -1.0,
            player_damage_percent: 0.0,
            player_engine_oil_temperature_c: -1.0,
            player_engine_water_temperature_c: -1.0,
            player_tire_temperature_c: [-1.0; 4],
            player_tire_temperature_by_zone_c: [[-1.0; 3]; 4],
            player_brake_temperature_c: [-1.0; 4],
            player_tire_remaining_by_wheel_percent: [-1.0; 4],
            player_tire_slip_ratio: [0.0; 4],
            player_tire_sliding_fraction: [0.0; 4],
            player_engine_overheating: 0,
            player_part_detached: 0,
            player_tire_compounds: [0; 4],
            player_tire_flat: [0; 4],
            player_tire_detached: [0; 4],
            player_damage_severity: [0; 8],
            battery_charge_percent: 0.0,
            hybrid_regen_kw: 0.0,
            hybrid_motor_temperature_c: 0.0,
            hybrid_motor_rpm: 0.0,
            player_position: 0,
            player_class_position: 0,
            player_class_size: 0,
            hybrid_motor_state: 0,
            engine_map: 0,
            engine_map_max: 0,
            traction_control_level: 0,
            traction_control_max: 0,
            traction_control_slip: 0,
            traction_control_slip_max: 0,
            traction_control_cut: 0,
            traction_control_cut_max: 0,
            anti_lock_brakes_level: 0,
            anti_lock_brakes_max: 0,
            brake_migration: 0,
            brake_migration_max: 0,
            front_anti_roll_bar: 0,
            front_anti_roll_bar_max: 0,
            rear_anti_roll_bar: 0,
            rear_anti_roll_bar_max: 0,
            speed_limiter_active: 0,
            headlights_on: 0,
            wiper_state: 0,
            vehicle_name: [0; 64],
            vehicle_model: [0; 30],
            track_name: [0; 64],
            standings: [LmuStandingEntry::default(); MAX_VEHICLES],
        }
    }
}

extern "C" {
    fn lmu_read_snapshot(snapshot: *mut LmuSnapshot, spectator_vehicle_id: i32) -> c_int;
    fn lmu_snapshot_size() -> usize;
}

/// `lmu_read_snapshot` memsets and fills `sizeof(LmuSnapshot)` **native** bytes
/// into the caller-provided buffer. If the Rust and C++ struct definitions ever
/// drift, that write would run past the Rust allocation and corrupt memory, so
/// the FFI is refused entirely until the layouts match again.
fn ffi_snapshot_layout_ok() -> bool {
    use std::sync::OnceLock;
    static LAYOUT_OK: OnceLock<bool> = OnceLock::new();
    *LAYOUT_OK.get_or_init(|| {
        let rust_size = std::mem::size_of::<LmuSnapshot>();
        let native_size = unsafe { lmu_snapshot_size() };
        if rust_size == native_size {
            true
        } else {
            crate::startup_log::record(format!(
                "fatal: LmuSnapshot ABI mismatch rust={rust_size} native={native_size}; \
                 native telemetry disabled to avoid memory corruption"
            ));
            false
        }
    })
}

#[derive(Default)]
struct TireWearTracker {
    last_remaining: [Option<f64>; 4],
    flat_spot_wear: [f64; 4],
    lap_start_remaining: Option<[f64; 4]>,
    last_lap_wear: Option<[f64; 4]>,
}

impl TireWearTracker {
    const LOCK_SLIP_RATIO: f64 = -0.3;
    const MIN_SLIDING_FRACTION: f64 = 0.5;

    fn reset(&mut self) {
        self.last_remaining = [None; 4];
        self.flat_spot_wear = [0.0; 4];
        self.lap_start_remaining = None;
        self.last_lap_wear = None;
    }

    fn update(
        &mut self,
        remaining: [f64; 4],
        slip_ratio: [f64; 4],
        sliding_fraction: [f64; 4],
        brake: f64,
        in_pits: bool,
    ) -> [f64; 4] {
        for index in 0..4 {
            let current = remaining[index];
            if !current.is_finite() || current < 0.0 {
                self.last_remaining[index] = None;
                continue;
            }

            if let Some(previous) = self.last_remaining[index] {
                let wear = previous - current;
                if wear > 0.0
                    && slip_ratio[index] < Self::LOCK_SLIP_RATIO
                    && (brake > 0.02 || sliding_fraction[index] >= Self::MIN_SLIDING_FRACTION)
                {
                    self.flat_spot_wear[index] += wear;
                }

                // Un aumento de goma en boxes identifica un cambio de neumático.
                // Una caída imposible también descarta la muestra anterior.
                if in_pits && !(0.0..=1.0).contains(&wear) {
                    self.flat_spot_wear[index] = 0.0;
                    if wear < -1.0 {
                        self.last_lap_wear = None;
                        self.lap_start_remaining = None;
                    }
                }
            }
            self.last_remaining[index] = Some(current);
        }

        self.flat_spot_wear.map(|value| value.clamp(0.0, 100.0))
    }

    fn observe_lap(&mut self, remaining: [f64; 4], lap_changed: bool, completed_is_clean: bool) {
        if !lap_changed {
            return;
        }
        if completed_is_clean {
            self.last_lap_wear = self.lap_start_remaining.and_then(|start| {
                let wear = std::array::from_fn(|index| start[index] - remaining[index]);
                wear.into_iter()
                    .all(|value| value.is_finite() && (0.001..20.0).contains(&value))
                    .then_some(wear)
            });
        }
        self.lap_start_remaining = remaining
            .into_iter()
            .all(|value| value.is_finite() && (0.0..=100.0).contains(&value))
            .then_some(remaining);
    }

    fn life_model(&self, remaining: [f64; 4], full_stint_laps: f64) -> Option<TireLifeModel> {
        let wear = self.last_lap_wear?;
        if !full_stint_laps.is_finite()
            || full_stint_laps <= 0.0
            || !remaining
                .into_iter()
                .all(|value| value.is_finite() && value >= 0.0)
        {
            return None;
        }
        let remaining_laps = (0..4)
            .map(|index| remaining[index] / wear[index])
            .fold(f64::INFINITY, f64::min);
        let projected_remaining_percent = std::array::from_fn(|stint| {
            (0..4)
                .map(|index| remaining[index] - wear[index] * full_stint_laps * (stint + 1) as f64)
                .fold(f64::INFINITY, f64::min)
                .clamp(0.0, 100.0)
        });
        Some(TireLifeModel {
            wear_per_lap_percent: wear,
            full_stint_laps,
            remaining_laps,
            remaining_stints: remaining_laps / full_stint_laps,
            projected_remaining_percent,
        })
    }
}

#[derive(Default)]
struct CarHistory {
    last_total_laps: Option<i32>,
    energy_at_lap_start: Option<f64>,
    energy_previous_sample: Option<f64>,
    energy_added_this_lap: f64,
    lap_visited_pits: bool,
    lap_valid: bool,
    recent_lap_times: VecDeque<f64>,
    recent_energy_usage: VecDeque<f64>,
    best_lap_seconds: f64,
    last_lap_valid: bool,
    last_lap_seconds: f64,
    last_lap_start_elapsed_seconds: Option<f64>,
    current_lap_invalid: bool,
    last_lap_boundary_elapsed_seconds: Option<f64>,
    was_in_pits: bool,
    out_lap: bool,
    pit_stop_started_at: Option<Instant>,
    pit_stop_elapsed: Option<Duration>,
    pit_stop_lap_at_entry: Option<i32>,
    pit_stops_at_entry: Option<u32>,
    pit_stop_confirmed: bool,
    pit_exit_total_laps: Option<i32>,
    last_pit_stop_lap: Option<i32>,
    last_pit_stop_elapsed: Option<Duration>,
    last_observed_pit_stops: Option<u32>,
    rest_history_last_lap: Option<i32>,
}

impl CarHistory {
    fn push_recent(values: &mut VecDeque<f64>, value: f64) {
        if !value.is_finite() || value <= 0.0 {
            return;
        }
        if values.len() == 5 {
            values.pop_front();
        }
        values.push_back(value);
    }

    fn average(values: &VecDeque<f64>) -> f64 {
        if values.is_empty() {
            0.0
        } else {
            values.iter().sum::<f64>() / values.len() as f64
        }
    }

    fn seed_recent_lap_times(&mut self, current_total_laps: i32, laps: &[(i32, f64)]) {
        let Some(last_history_lap) = laps
            .iter()
            .rev()
            .find(|(lap, _)| *lap <= current_total_laps)
            .map(|(lap, _)| *lap)
        else {
            return;
        };
        if self.rest_history_last_lap == Some(last_history_lap) {
            return;
        }
        self.recent_lap_times.clear();
        let eligible = laps
            .iter()
            .filter(|(lap, _)| *lap <= current_total_laps)
            .collect::<Vec<_>>();
        for (_, lap_time) in eligible.iter().skip(eligible.len().saturating_sub(5)) {
            Self::push_recent(&mut self.recent_lap_times, *lap_time);
        }
        self.rest_history_last_lap = Some(last_history_lap);
    }

    fn update(&mut self, entry: &LmuStandingEntry, current_energy: f64) {
        self.update_at(entry, current_energy, Instant::now());
    }

    fn update_at(&mut self, entry: &LmuStandingEntry, current_energy: f64, now: Instant) {
        let in_pits = entry.in_pits != 0;
        let in_garage = entry.in_garage != 0;
        let first_sample = self.last_total_laps.is_none();
        let previous_pit_stops = self.last_observed_pit_stops.unwrap_or(entry.pit_stops);
        self.update_lap_timing(entry);
        if entry.best_lap_seconds.is_finite() && entry.best_lap_seconds > 0.0 {
            self.best_lap_seconds = entry.best_lap_seconds;
        }

        if in_garage {
            self.pit_stop_started_at = None;
            self.pit_stop_elapsed = None;
            self.pit_stop_lap_at_entry = None;
            self.pit_stops_at_entry = None;
            self.pit_stop_confirmed = false;
            self.pit_exit_total_laps = None;
        } else if !first_sample && !self.was_in_pits && in_pits {
            self.pit_stop_started_at = Some(now);
            self.pit_stop_elapsed = Some(Duration::ZERO);
            self.pit_stop_lap_at_entry = Some(entry.total_laps + 1);
            self.pit_stops_at_entry = Some(previous_pit_stops);
            self.pit_stop_confirmed = entry.pit_stops > previous_pit_stops;
            self.pit_exit_total_laps = None;
        } else if in_pits {
            if let Some(started) = self.pit_stop_started_at {
                self.pit_stop_elapsed = Some(now.saturating_duration_since(started));
            }
        } else if self.was_in_pits {
            if let Some(started) = self.pit_stop_started_at.take() {
                self.pit_stop_elapsed = Some(now.saturating_duration_since(started));
            }
        }

        if self
            .pit_stops_at_entry
            .is_some_and(|pit_stops| entry.pit_stops > pit_stops)
        {
            self.pit_stop_confirmed = true;
        }

        if self.was_in_pits && !in_pits && entry.in_garage == 0 {
            self.pit_exit_total_laps = Some(entry.total_laps);
            self.out_lap = self.pit_stop_confirmed;
        } else if !in_pits
            && self.pit_stop_confirmed
            && !self.out_lap
            && self
                .pit_exit_total_laps
                .is_some_and(|pit_exit_lap| entry.total_laps == pit_exit_lap)
        {
            self.out_lap = true;
        }
        if !in_pits && self.pit_stop_confirmed {
            if let (Some(lap), Some(elapsed)) = (self.pit_stop_lap_at_entry, self.pit_stop_elapsed)
            {
                self.last_pit_stop_lap = Some(lap);
                self.last_pit_stop_elapsed = Some(elapsed);
            }
        }
        if self
            .pit_exit_total_laps
            .is_some_and(|pit_exit_lap| entry.total_laps > pit_exit_lap)
        {
            self.out_lap = false;
            self.pit_stop_elapsed = None;
            self.pit_stop_lap_at_entry = None;
            self.pit_stops_at_entry = None;
            self.pit_stop_confirmed = false;
            self.pit_exit_total_laps = None;
        }
        if entry.in_garage != 0 {
            self.out_lap = false;
            self.pit_stop_elapsed = None;
        }
        if in_pits {
            if let Some(previous) = self.energy_previous_sample {
                let added = current_energy - previous;
                if added.is_finite() && added > 0.0 {
                    self.energy_added_this_lap += added;
                }
            }
        }

        match self.last_total_laps {
            None => {
                self.last_total_laps = Some(entry.total_laps);
                self.energy_at_lap_start = (current_energy > 0.0).then_some(current_energy);
                self.lap_visited_pits = in_pits;
                self.lap_valid = !self.current_lap_invalid;
            }
            Some(previous_laps) if entry.total_laps != previous_laps => {
                let completed_one_lap = entry.total_laps == previous_laps + 1;
                let clean_lap = completed_one_lap && self.lap_valid && !self.lap_visited_pits;
                if clean_lap {
                    if let Some(start) = self.energy_at_lap_start {
                        let consumed = start + self.energy_added_this_lap - current_energy;
                        Self::push_recent(&mut self.recent_energy_usage, consumed);
                    }
                }
                self.last_total_laps = Some(entry.total_laps);
                self.energy_at_lap_start = (current_energy > 0.0).then_some(current_energy);
                self.energy_added_this_lap = 0.0;
                self.lap_visited_pits = in_pits;
                self.lap_valid = !self.current_lap_invalid;
            }
            Some(_) => {
                self.lap_visited_pits |= in_pits;
                self.lap_valid &= !self.current_lap_invalid;
            }
        }
        self.energy_previous_sample = (current_energy > 0.0).then_some(current_energy);
        self.last_observed_pit_stops = Some(entry.pit_stops);
        self.was_in_pits = in_pits;
    }

    fn update_lap_timing(&mut self, entry: &LmuStandingEntry) {
        let lap_start = entry.lap_start_elapsed_seconds;
        let current_lap_seconds = entry.elapsed_seconds - lap_start;
        let official_last_lap = Self::normalize_official_lap(entry.last_lap_seconds);
        let official_last_lap_invalid = Self::official_lap_is_invalid(entry.last_lap_seconds);
        let stable_lap_start = lap_start.is_finite()
            && lap_start > 0.0
            && current_lap_seconds.is_finite()
            && current_lap_seconds > 1.0;

        if stable_lap_start {
            match self.last_lap_start_elapsed_seconds {
                None => {
                    self.last_lap_start_elapsed_seconds = Some(lap_start);
                    self.last_lap_seconds = official_last_lap;
                    self.last_lap_valid = !official_last_lap_invalid;
                    self.current_lap_invalid = entry.in_garage == 0 && entry.lap_invalidated != 0;
                }
                Some(previous_start) if previous_start != lap_start => {
                    if previous_start > 0.0 && previous_start < lap_start {
                        let reconstructed = lap_start - previous_start;
                        let official_result_valid = Self::completed_lap_result_is_valid(
                            entry.last_lap_seconds,
                            reconstructed,
                        );
                        self.last_lap_seconds = if Self::valid_lap_time(official_last_lap) {
                            official_last_lap
                        } else if (self.current_lap_invalid && Self::valid_lap_time(reconstructed))
                            || Self::plausible_reconstructed_lap(entry, reconstructed)
                        {
                            reconstructed
                        } else {
                            0.0
                        };
                        self.last_lap_valid = !(self.current_lap_invalid
                            || !official_result_valid
                            || (entry.in_garage == 0 && entry.lap_invalidated != 0));
                        Self::push_recent(&mut self.recent_lap_times, self.last_lap_seconds);
                        self.last_lap_boundary_elapsed_seconds = Some(entry.elapsed_seconds);
                        self.current_lap_invalid = false;
                    } else {
                        self.recent_lap_times.clear();
                        self.last_lap_seconds = 0.0;
                        self.last_lap_valid = true;
                        self.current_lap_invalid = false;
                        self.last_lap_boundary_elapsed_seconds = None;
                    }
                    self.last_lap_start_elapsed_seconds = Some(lap_start);
                }
                Some(_) => {}
            }
        }

        if official_last_lap_invalid && Self::valid_lap_time(official_last_lap) {
            self.last_lap_seconds = official_last_lap;
            self.last_lap_valid = false;
        }

        let within_boundary_holdoff =
            self.last_lap_boundary_elapsed_seconds
                .is_some_and(|boundary| {
                    entry.elapsed_seconds.is_finite()
                        && entry.elapsed_seconds >= boundary
                        && entry.elapsed_seconds - boundary <= 2.0
                });
        if entry.in_garage == 0 && entry.lap_invalidated != 0 && !within_boundary_holdoff {
            self.current_lap_invalid = true;
        }
    }

    fn valid_lap_time(lap_time: f64) -> bool {
        lap_time.is_finite() && lap_time > 20.0 && lap_time < 900.0
    }

    fn normalize_official_lap(lap_time: f64) -> f64 {
        let normalized = lap_time.abs();
        if Self::valid_lap_time(normalized) {
            normalized
        } else {
            0.0
        }
    }

    fn official_lap_is_invalid(lap_time: f64) -> bool {
        lap_time.is_finite() && lap_time > -900.0 && lap_time < -20.0
    }

    fn completed_lap_result_is_valid(official_lap_time: f64, reconstructed: f64) -> bool {
        !Self::official_lap_is_invalid(official_lap_time)
            && !(official_lap_time.is_finite()
                && official_lap_time < 0.0
                && Self::valid_lap_time(reconstructed))
    }

    fn plausible_reconstructed_lap(entry: &LmuStandingEntry, lap_time: f64) -> bool {
        if !lap_time.is_finite() || lap_time <= 1.0 {
            return false;
        }
        let reference = [entry.best_lap_seconds, entry.estimated_lap_time]
            .into_iter()
            .find(|value| value.is_finite() && *value > 1.0);
        reference.is_none_or(|reference| lap_time >= reference * 0.5)
    }

    fn average_lap_time(&self) -> f64 {
        if !self.best_lap_seconds.is_finite() || self.best_lap_seconds <= 0.0 {
            return 0.0;
        }

        // TinyPedal no filtra AVG 5 por la bandera de validez. Conserva las
        // cinco vueltas completadas y elimina tiempos imposibles usando la
        // mejor vuelta oficial válida y un margen máximo del 120 %.
        let valid_floor = self.best_lap_seconds - 0.01;
        let recent_best = self
            .recent_lap_times
            .iter()
            .copied()
            .filter(|lap_time| lap_time.is_finite() && *lap_time >= valid_floor)
            .reduce(f64::min)
            .unwrap_or(self.best_lap_seconds);
        let recent_floor = recent_best - 0.01;
        let margin = recent_best * 1.2;
        let mut sum = 0.0;
        let mut count = 0;
        for lap_time in self.recent_lap_times.iter().copied() {
            if lap_time.is_finite() && lap_time >= recent_floor && lap_time <= margin {
                sum += lap_time;
                count += 1;
            }
        }
        if count > 0 {
            sum / f64::from(count)
        } else {
            recent_best
        }
    }

    fn average_energy_usage(&self) -> f64 {
        Self::average(&self.recent_energy_usage)
    }

    fn is_last_lap_valid(&self) -> bool {
        self.last_lap_valid
    }

    fn last_lap_seconds(&self, official_last_lap_seconds: f64) -> f64 {
        let official = Self::normalize_official_lap(official_last_lap_seconds);
        if official > 0.0 {
            official
        } else {
            self.last_lap_seconds
        }
    }

    fn is_out_lap(&self) -> bool {
        self.out_lap
    }

    fn pit_stop_time_seconds(&self) -> Option<f64> {
        if self.was_in_pits {
            self.pit_stop_elapsed.map(|duration| duration.as_secs_f64())
        } else {
            self.last_pit_stop_elapsed
                .map(|duration| duration.as_secs_f64())
        }
    }

    fn pit_stop_lap(&self) -> Option<i32> {
        self.last_pit_stop_lap
    }
}

pub struct LmuTelemetrySource {
    current_session: Option<i32>,
    last_lap: i32,
    fuel_at_lap_start: Option<f64>,
    fuel_previous_sample: Option<f64>,
    fuel_added_this_lap: f64,
    fuel_per_lap: Option<f64>,
    fuel_clean_laps: u32,
    fuel_last_lap: Option<f64>,
    fuel_qualifying_lap: Option<f64>,
    energy_at_lap_start: Option<f64>,
    energy_previous_sample: Option<f64>,
    energy_added_this_lap: f64,
    energy_per_lap: Option<f64>,
    energy_clean_laps: u32,
    energy_last_lap: Option<f64>,
    energy_qualifying_lap: Option<f64>,
    qualifying_reference_time: Option<f64>,
    lap_time_pace: Option<f64>,
    lap_visited_pits: bool,
    lap_was_formation: bool,
    lap_was_valid: bool,
    lap_was_green: bool,
    tire_wear_tracker: TireWearTracker,
    consumption_profiler: ConsumptionProfiler,
    car_histories: HashMap<i32, CarHistory>,
    starting_positions: HashMap<i32, i32>,
    scored_finish_positions: HashMap<i32, i32>,
    race_qualifying_positions: HashMap<i32, i32>,
    vehicle_identities: HashMap<i32, VehicleIdentity>,
    driver_ranks: DriverRankResolver,
    driver_rank_prerace_scores: HashMap<String, f64>,
    session_split: SessionSplitResolver,
    local_rest: LocalRestResolver,
    rejoin_hold_frames: u16,
    rejoin_reason: &'static str,
    last_gap_log_at: Option<Instant>,
    last_standings_state_update: Option<Instant>,
    last_valid_standings: Vec<StandingEntry>,
    last_valid_standings_at: Option<Instant>,
    last_valid_snapshot: Option<LmuSnapshot>,
    last_valid_snapshot_at: Option<Instant>,
    player_lap_distance: PlayerLapDistanceEstimator,
    player_lap_times: PlayerLapTimeHistory,
    driver_rank_race_sequence: u64,
    driver_rank_validation: Option<DriverRankValidationState>,
    source_stage_performance: SourceStagePerformance,
}

#[derive(Default)]
struct PlayerLapTimeHistory {
    lap_start_elapsed_seconds: Option<f64>,
    reconstructed_last_lap_seconds: f64,
}

impl PlayerLapTimeHistory {
    fn reset(&mut self) {
        *self = Self::default();
    }

    fn update(&mut self, lap_start: f64, current_lap_seconds: f64) -> f64 {
        if !lap_start.is_finite()
            || lap_start <= 0.0
            || !current_lap_seconds.is_finite()
            || current_lap_seconds <= 1.0
        {
            return self.reconstructed_last_lap_seconds;
        }
        match self.lap_start_elapsed_seconds {
            None => self.lap_start_elapsed_seconds = Some(lap_start),
            Some(previous) if lap_start > previous => {
                let reconstructed = lap_start - previous;
                self.reconstructed_last_lap_seconds =
                    CarHistory::normalize_official_lap(reconstructed);
                self.lap_start_elapsed_seconds = Some(lap_start);
            }
            Some(previous) if lap_start < previous => self.reset(),
            Some(_) => {}
        }
        self.reconstructed_last_lap_seconds
    }
}

#[derive(Default)]
struct PlayerLapDistanceEstimator {
    initialized: bool,
    lap_number: i32,
    last_raw_distance: f64,
    estimated_distance: f64,
    last_lap_seconds: f64,
}

struct PlayerLapDistanceSample {
    raw_distance: f64,
    current_lap_seconds: f64,
    speed_kph: f64,
    gear: i32,
    lap_number: i32,
    track_length: f64,
    lap_changed: bool,
}

impl PlayerLapDistanceEstimator {
    fn reset(&mut self) {
        *self = Self::default();
    }

    fn update(&mut self, sample: PlayerLapDistanceSample) -> f64 {
        let PlayerLapDistanceSample {
            raw_distance,
            current_lap_seconds,
            speed_kph,
            gear,
            lap_number,
            track_length,
            lap_changed,
        } = sample;
        if !raw_distance.is_finite()
            || !current_lap_seconds.is_finite()
            || !track_length.is_finite()
            || track_length <= 1.0
        {
            self.reset();
            return 0.0;
        }

        let raw_distance = raw_distance.clamp(0.0, track_length);
        let must_initialize = !self.initialized
            || lap_changed
            || self.lap_number != lap_number
            || current_lap_seconds < self.last_lap_seconds
            || raw_distance + track_length * 0.5 < self.last_raw_distance;
        if must_initialize {
            self.initialized = true;
            self.lap_number = lap_number;
            self.last_raw_distance = raw_distance;
            self.estimated_distance = raw_distance;
            self.last_lap_seconds = current_lap_seconds;
            return raw_distance;
        }

        let elapsed = (current_lap_seconds - self.last_lap_seconds).clamp(0.0, 0.25);
        let signed_speed_mps = if gear < 0 {
            -speed_kph.max(0.0) / 3.6
        } else {
            speed_kph.max(0.0) / 3.6
        };
        let predicted =
            (self.estimated_distance + signed_speed_mps * elapsed).clamp(0.0, track_length);
        let raw_changed = (raw_distance - self.last_raw_distance).abs() >= 0.05;

        self.estimated_distance = if raw_changed {
            let correction = raw_distance - predicted;
            if correction.abs() > 10.0 {
                raw_distance
            } else {
                // Scoring es autoritativo pero llega a menor cadencia. Aplicar
                // parte de su pequeña corrección evita reintroducir un salto
                // visible en cada actualización.
                predicted + correction * 0.35
            }
        } else {
            predicted
        }
        .clamp(0.0, track_length);
        self.last_raw_distance = raw_distance;
        self.last_lap_seconds = current_lap_seconds;
        self.estimated_distance
    }
}

#[derive(Clone)]
struct DriverRankOpponentDiagnostic {
    vehicle_id: i32,
    visual_score: f64,
    internal_score: f64,
    race_position: i32,
    qualifying_position: i32,
    expected: f64,
    race_result: f64,
    qualifying_result: f64,
}

#[derive(Clone)]
struct DriverRankEstimateDiagnostic {
    status: &'static str,
    vehicle_id: i32,
    vehicle_class: String,
    driver_rank: String,
    driver_rank_progress: f64,
    visual_score: Option<f64>,
    race_position: i32,
    live_race_position: i32,
    race_position_source: &'static str,
    qualifying_position: i32,
    same_class_rivals: u32,
    rated_opponents: u32,
    race_result_total: f64,
    qualifying_result_total: f64,
    gain_factor: f64,
    estimated_gain: Option<f64>,
    opponents: Vec<DriverRankOpponentDiagnostic>,
}

struct DriverRankValidationInput<'a> {
    session_type: i32,
    game_phase: u32,
    event_id: &'a str,
    split_number: u32,
    sample: Option<&'a DriverRankEstimateDiagnostic>,
    player_raw_elo: Option<f64>,
    refresh_revision: u64,
}

struct DriverRankValidationState {
    race_sequence: u64,
    event_id: String,
    split_number: u32,
    player_vehicle_id: i32,
    player_class: String,
    before_raw_elo: Option<f64>,
    before_visual_score: Option<f64>,
    before_refresh_revision: u64,
    final_estimated_gain: Option<f64>,
    final_race_position: i32,
    final_qualifying_position: i32,
    final_position_source: &'static str,
    final_logged_signature: Option<String>,
}

struct SourceStagePerformance {
    period_started_at: Instant,
    cycles: u64,
    snapshot_us: u128,
    rest_us: u128,
    session_us: u128,
    standings_state_us: u128,
    standings_state_updates: u64,
    standings_build_us: u128,
    standings_builds: u64,
    warnings_us: u128,
    frame_us: u128,
}

impl SourceStagePerformance {
    fn new() -> Self {
        Self {
            period_started_at: Instant::now(),
            cycles: 0,
            snapshot_us: 0,
            rest_us: 0,
            session_us: 0,
            standings_state_us: 0,
            standings_state_updates: 0,
            standings_build_us: 0,
            standings_builds: 0,
            warnings_us: 0,
            frame_us: 0,
        }
    }

    fn record(&mut self, sample: SourceStageSample) {
        if crate::telemetry::analysis_logging_generation().is_none() {
            if self.cycles > 0 {
                *self = Self::new();
            }
            return;
        }
        self.cycles += 1;
        self.snapshot_us += sample.snapshot_us;
        self.rest_us += sample.rest_us;
        self.session_us += sample.session_us;
        self.warnings_us += sample.warnings_us;
        self.frame_us += sample.frame_us;
        if let Some(duration) = sample.standings_state_us {
            self.standings_state_updates += 1;
            self.standings_state_us += duration;
        }
        if let Some(duration) = sample.standings_build_us {
            self.standings_builds += 1;
            self.standings_build_us += duration;
        }

        let elapsed = self.period_started_at.elapsed();
        if elapsed < Duration::from_secs(5) {
            return;
        }
        let cycles = self.cycles.max(1) as u128;
        let state_updates = self.standings_state_updates.max(1) as u128;
        let builds = self.standings_builds.max(1) as u128;
        crate::telemetry::queue_analysis_event(serde_json::json!({
            "event": "source_stage_performance",
            "period_ms": elapsed.as_millis() as u64,
            "cycles": self.cycles,
            "average_snapshot_us": (self.snapshot_us / cycles) as u64,
            "average_rest_receive_us": (self.rest_us / cycles) as u64,
            "average_session_and_warnings_scan_us": (self.session_us / cycles) as u64,
            "average_standings_state_us": (self.standings_state_us / state_updates) as u64,
            "standings_state_updates": self.standings_state_updates,
            "average_standings_build_us": (self.standings_build_us / builds) as u64,
            "standings_builds": self.standings_builds,
            "average_flag_rejoin_us": (self.warnings_us / cycles) as u64,
            "average_frame_strategy_us": (self.frame_us / cycles) as u64,
        }));
        *self = Self::new();
    }
}

struct SourceStageSample {
    snapshot_us: u128,
    rest_us: u128,
    session_us: u128,
    standings_state_us: Option<u128>,
    standings_build_us: Option<u128>,
    warnings_us: u128,
    frame_us: u128,
}

impl LmuTelemetrySource {
    #[cfg(test)]
    pub fn new() -> Self {
        let mut source = Self::with_profile_directory(None);
        source.driver_ranks = DriverRankResolver::empty();
        source.session_split = SessionSplitResolver::empty();
        source
    }

    pub fn with_profile_directory(profile_directory: Option<std::path::PathBuf>) -> Self {
        Self {
            current_session: None,
            last_lap: -1,
            fuel_at_lap_start: None,
            fuel_previous_sample: None,
            fuel_added_this_lap: 0.0,
            fuel_per_lap: None,
            fuel_clean_laps: 0,
            fuel_last_lap: None,
            fuel_qualifying_lap: None,
            energy_at_lap_start: None,
            energy_previous_sample: None,
            energy_added_this_lap: 0.0,
            energy_per_lap: None,
            energy_clean_laps: 0,
            energy_last_lap: None,
            energy_qualifying_lap: None,
            qualifying_reference_time: None,
            lap_time_pace: None,
            lap_visited_pits: false,
            lap_was_formation: false,
            lap_was_valid: true,
            lap_was_green: true,
            tire_wear_tracker: TireWearTracker::default(),
            consumption_profiler: ConsumptionProfiler::new(profile_directory),
            car_histories: HashMap::new(),
            starting_positions: HashMap::new(),
            scored_finish_positions: HashMap::new(),
            race_qualifying_positions: HashMap::new(),
            vehicle_identities: HashMap::new(),
            driver_ranks: DriverRankResolver::discover(),
            driver_rank_prerace_scores: HashMap::new(),
            session_split: SessionSplitResolver::discover(),
            local_rest: {
                #[cfg(test)]
                {
                    LocalRestResolver::empty()
                }
                #[cfg(not(test))]
                {
                    LocalRestResolver::discover()
                }
            },
            rejoin_hold_frames: 0,
            rejoin_reason: "rejoin",
            last_gap_log_at: None,
            last_standings_state_update: None,
            last_valid_standings: Vec::new(),
            last_valid_standings_at: None,
            last_valid_snapshot: None,
            last_valid_snapshot_at: None,
            player_lap_distance: PlayerLapDistanceEstimator::default(),
            player_lap_times: PlayerLapTimeHistory::default(),
            driver_rank_race_sequence: 0,
            driver_rank_validation: None,
            source_stage_performance: SourceStagePerformance::new(),
        }
    }
}

fn synchronized_lap_progress(raw: f64, current_lap_seconds: f64, lap_changed: bool) -> f64 {
    let stale_previous_lap = raw > 0.9
        && (lap_changed
            || (current_lap_seconds.is_finite() && (0.0..2.0).contains(&current_lap_seconds)));
    if stale_previous_lap {
        0.0
    } else {
        raw.clamp(0.0, 1.0)
    }
}
