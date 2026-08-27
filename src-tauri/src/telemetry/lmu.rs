use std::collections::{HashMap, HashSet, VecDeque};
use std::ffi::{c_char, c_int};
use std::time::{Duration, Instant};

use super::consumption_profile::{ConsumptionProfiler, ProfileEstimate};
use super::driver_ranks::DriverRankResolver;
use super::event_split::{DriverRankSettings, SessionSplitResolver};
use super::fuel_strategy::{calculate_resource_strategy, FuelStrategies, ResourceStrategyInput};
use super::lmu_rest::{LocalRestResolver, RestVehicleDamage};
use super::{StandingEntry, TelemetryFrame, TelemetrySource, TrackMapVehicle};

const MAX_VEHICLES: usize = 104;
const DRIVER_RANK_INTERNAL_SCALE: f64 = 3.0;
const DRIVER_RANK_QUALIFY_WEIGHT: f64 = 0.176_470_588_235_294;

fn suspension_damage_by_wheel_percent(
    damage: Option<RestVehicleDamage>,
    detached: [u8; 4],
) -> [f64; 4] {
    std::array::from_fn(|index| {
        if detached[index] != 0 {
            100.0
        } else {
            damage
                .map(|value| (value.suspension[index] * 100.0).clamp(0.0, 100.0))
                .unwrap_or(-1.0)
        }
    })
}

fn rear_wing_detached(
    _damage: Option<RestVehicleDamage>,
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
    steering: f64,
    steering_range_degrees: f64,
    force_feedback: f64,
    fuel_liters: f64,
    fuel_capacity_liters: f64,
    virtual_energy: f64,
    vehicle_class_id: u32,
    player_lap_valid: u32,
    current_lap_seconds: f64,
    current_sector1_seconds: f64,
    current_sector2_seconds: f64,
    player_best_sector_ends: [f64; 3],
    session_best_sector_ends: [f64; 3],
    best_lap_seconds: f64,
    lap_delta_seconds: f64,
    session_time_remaining: f64,
    session_elapsed_seconds: f64,
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
    player_tire_remaining_percent: f64,
    player_damage_percent: f64,
    player_tire_temperature_c: [f64; 4],
    player_brake_temperature_c: [f64; 4],
    player_tire_remaining_by_wheel_percent: [f64; 4],
    player_tire_slip_ratio: [f64; 4],
    player_tire_sliding_fraction: [f64; 4],
    player_part_detached: u32,
    player_tire_compounds: [u8; 4],
    player_tire_flat: [u8; 4],
    player_tire_detached: [u8; 4],
    player_damage_severity: [u8; 8],
    vehicle_name: [c_char; 64],
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
            steering: 0.0,
            steering_range_degrees: 0.0,
            force_feedback: 0.0,
            fuel_liters: 0.0,
            fuel_capacity_liters: 1.0,
            virtual_energy: 0.0,
            vehicle_class_id: u32::MAX,
            player_lap_valid: 0,
            current_lap_seconds: 0.0,
            current_sector1_seconds: 0.0,
            current_sector2_seconds: 0.0,
            player_best_sector_ends: [0.0; 3],
            session_best_sector_ends: [0.0; 3],
            best_lap_seconds: 0.0,
            lap_delta_seconds: 0.0,
            session_time_remaining: 0.0,
            session_elapsed_seconds: 0.0,
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
            player_tire_remaining_percent: -1.0,
            player_damage_percent: 0.0,
            player_tire_temperature_c: [-1.0; 4],
            player_brake_temperature_c: [-1.0; 4],
            player_tire_remaining_by_wheel_percent: [-1.0; 4],
            player_tire_slip_ratio: [0.0; 4],
            player_tire_sliding_fraction: [0.0; 4],
            player_part_detached: 0,
            player_tire_compounds: [0; 4],
            player_tire_flat: [0; 4],
            player_tire_detached: [0; 4],
            player_damage_severity: [0; 8],
            vehicle_name: [0; 64],
            track_name: [0; 64],
            standings: [LmuStandingEntry::default(); MAX_VEHICLES],
        }
    }
}

extern "C" {
    fn lmu_read_snapshot(snapshot: *mut LmuSnapshot) -> c_int;
    #[cfg(test)]
    fn lmu_snapshot_size() -> usize;
}

#[derive(Default)]
struct TireWearTracker {
    last_remaining: [Option<f64>; 4],
    flat_spot_wear: [f64; 4],
}

impl TireWearTracker {
    const LOCK_SLIP_RATIO: f64 = -0.3;
    const MIN_SLIDING_FRACTION: f64 = 0.5;

    fn reset(&mut self) {
        self.last_remaining = [None; 4];
        self.flat_spot_wear = [0.0; 4];
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
                if in_pits && (wear < 0.0 || wear > 1.0) {
                    self.flat_spot_wear[index] = 0.0;
                }
            }
            self.last_remaining[index] = Some(current);
        }

        self.flat_spot_wear.map(|value| value.clamp(0.0, 100.0))
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
    pit_stops_at_entry: Option<u32>,
    pit_stop_confirmed: bool,
    pit_exit_total_laps: Option<i32>,
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
            self.pit_stops_at_entry = None;
            self.pit_stop_confirmed = false;
            self.pit_exit_total_laps = None;
        } else if !first_sample && !self.was_in_pits && in_pits {
            self.pit_stop_started_at = Some(now);
            self.pit_stop_elapsed = Some(Duration::ZERO);
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
        if self
            .pit_exit_total_laps
            .is_some_and(|pit_exit_lap| entry.total_laps > pit_exit_lap)
        {
            self.out_lap = false;
            self.pit_stop_elapsed = None;
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
                        self.last_lap_seconds = if Self::valid_lap_time(official_last_lap) {
                            official_last_lap
                        } else if Self::plausible_reconstructed_lap(entry, reconstructed) {
                            reconstructed
                        } else {
                            0.0
                        };
                        self.last_lap_valid = !(self.current_lap_invalid
                            || official_last_lap_invalid
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
        Self::valid_lap_time(normalized)
            .then_some(normalized)
            .unwrap_or(0.0)
    }

    fn official_lap_is_invalid(lap_time: f64) -> bool {
        lap_time.is_finite() && lap_time > -900.0 && lap_time < -20.0
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
        (self.was_in_pits || self.out_lap)
            .then(|| self.pit_stop_elapsed.map(|duration| duration.as_secs_f64()))
            .flatten()
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
    vehicle_identities: HashMap<i32, VehicleIdentity>,
    driver_ranks: DriverRankResolver,
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
    source_stage_performance: SourceStagePerformance,
}

#[derive(Default)]
struct PlayerLapDistanceEstimator {
    initialized: bool,
    lap_number: i32,
    last_raw_distance: f64,
    estimated_distance: f64,
    last_lap_seconds: f64,
}

impl PlayerLapDistanceEstimator {
    fn reset(&mut self) {
        *self = Self::default();
    }

    fn update(
        &mut self,
        raw_distance: f64,
        current_lap_seconds: f64,
        speed_kph: f64,
        gear: i32,
        lap_number: i32,
        track_length: f64,
        lap_changed: bool,
    ) -> f64 {
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
    internal_rating_gain: Option<f64>,
    estimated_gain: Option<f64>,
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
        if super::analysis_logging_generation().is_none() {
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
        super::queue_analysis_event(serde_json::json!({
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
    fn weather_session_key(session_type: i32) -> &'static str {
        if (0..=4).contains(&session_type) {
            "PRACTICE"
        } else if (5..=8).contains(&session_type) {
            "QUALIFY"
        } else {
            "RACE"
        }
    }

    fn live_weather_icon(cloud_coverage: u8, rain_percent: f64) -> i32 {
        if !rain_percent.is_finite() || rain_percent <= 0.0 {
            return i32::from(cloud_coverage.min(4));
        }
        if rain_percent <= 10.0 {
            5
        } else if rain_percent <= 15.0 {
            6
        } else if rain_percent <= 20.0 {
            7
        } else if rain_percent <= 40.0 {
            8
        } else if rain_percent <= 60.0 {
            9
        } else {
            10
        }
    }

    fn track_grip_percent(track_grip_level: u8) -> f64 {
        match track_grip_level {
            1 => 25.0,
            2 => 50.0,
            3 => 75.0,
            4 => 90.0,
            _ => 0.0,
        }
    }

    fn track_rubber_percent(snapshot: &LmuSnapshot) -> f64 {
        const MEDIAN_LAPS: f64 = 2_000.0;

        let starting_rubber = if (0..=4).contains(&snapshot.session_type) {
            0.25
        } else {
            0.50
        };
        let starting_laps = starting_rubber * MEDIAN_LAPS / 0.75;
        let count = (snapshot.standings_count as usize).min(MAX_VEHICLES);
        let completed_laps = snapshot.standings[..count]
            .iter()
            .filter(|entry| entry.vehicle_id > 0 && entry.position > 0)
            .map(|entry| entry.total_laps)
            .filter(|laps| (0..10_000).contains(laps))
            .map(f64::from)
            .sum::<f64>();
        let equivalent_laps = starting_laps + completed_laps;

        if equivalent_laps >= MEDIAN_LAPS * 2.0 {
            100.0
        } else if equivalent_laps > MEDIAN_LAPS {
            (75.0 + (equivalent_laps - MEDIAN_LAPS) / MEDIAN_LAPS * 25.0).clamp(0.0, 100.0)
        } else {
            (equivalent_laps / MEDIAN_LAPS * 75.0).clamp(0.0, 100.0)
        }
    }

    fn track_surface_state(track_wetness_percent: f64) -> &'static str {
        if !track_wetness_percent.is_finite() || track_wetness_percent < 1.0 {
            "dry"
        } else if track_wetness_percent < 15.0 {
            "damp"
        } else if track_wetness_percent < 40.0 {
            "wet"
        } else if track_wetness_percent < 70.0 {
            "heavy"
        } else {
            "saturated"
        }
    }

    fn resolve_wind(wind_x: f64, wind_z: f64, rest_wind: Option<(f64, f64)>) -> (f64, f64) {
        let shared_speed_ms = f64::hypot(wind_x, wind_z);
        if shared_speed_ms.is_finite() && shared_speed_ms > 0.0 {
            let direction_degrees = (wind_z.atan2(wind_x).to_degrees() + 360.0) % 360.0;
            (shared_speed_ms, direction_degrees)
        } else {
            rest_wind.unwrap_or((0.0, 0.0))
        }
    }

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
            vehicle_identities: HashMap::new(),
            driver_ranks: DriverRankResolver::discover(),
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
            source_stage_performance: SourceStagePerformance::new(),
        }
    }

    fn is_qualifying(session_type: i32) -> bool {
        (5..=8).contains(&session_type)
    }

    fn clear_qualifying_reference(&mut self) {
        self.fuel_qualifying_lap = None;
        self.energy_qualifying_lap = None;
        self.qualifying_reference_time = None;
    }

    fn update_session(&mut self, session_type: i32) {
        if self.current_session == Some(session_type) {
            return;
        }

        let previous_session = self.current_session.replace(session_type);
        self.last_lap = -1;
        self.fuel_at_lap_start = None;
        self.fuel_previous_sample = None;
        self.fuel_added_this_lap = 0.0;
        self.fuel_per_lap = None;
        self.fuel_clean_laps = 0;
        self.fuel_last_lap = None;
        self.energy_at_lap_start = None;
        self.energy_previous_sample = None;
        self.energy_added_this_lap = 0.0;
        self.energy_per_lap = None;
        self.energy_clean_laps = 0;
        self.energy_last_lap = None;
        self.lap_time_pace = None;
        self.lap_visited_pits = false;
        self.lap_was_formation = false;
        self.lap_was_valid = true;
        self.lap_was_green = true;
        self.tire_wear_tracker.reset();
        self.consumption_profiler.reset_lap();
        self.car_histories.clear();
        self.starting_positions.clear();
        self.scored_finish_positions.clear();
        if previous_session.is_some() {
            self.local_rest.reset_session_history();
        }
        self.vehicle_identities.clear();
        self.driver_ranks.begin_session();
        self.rejoin_hold_frames = 0;
        self.last_gap_log_at = None;
        self.last_standings_state_update = None;
        self.last_valid_standings.clear();
        self.last_valid_standings_at = None;

        let entered_qualifying = Self::is_qualifying(session_type)
            && previous_session.is_none_or(|previous| !Self::is_qualifying(previous));
        let entered_practice = (0..=4).contains(&session_type);
        if entered_qualifying || entered_practice {
            self.clear_qualifying_reference();
        }
    }

    fn player_in_pits(snapshot: &LmuSnapshot) -> bool {
        let count = (snapshot.standings_count as usize).min(MAX_VEHICLES);
        snapshot.standings[..count]
            .iter()
            .any(|entry| entry.is_player != 0 && entry.in_pits != 0)
    }

    fn update_clean_average(average: Option<f64>, samples: &mut u32, consumed: f64) -> Option<f64> {
        if !consumed.is_finite() || consumed <= 0.0 {
            return average;
        }
        *samples = samples.saturating_add(1);
        let weight = 1.0 / (*samples).min(8) as f64;
        Some(average.map_or(consumed, |value| value + (consumed - value) * weight))
    }

    fn update_fuel_estimate(
        &mut self,
        snapshot: &LmuSnapshot,
        lap_changed: bool,
        completed_is_clean: bool,
    ) -> (f64, f64) {
        if Self::player_in_pits(snapshot) {
            if let Some(previous) = self.fuel_previous_sample {
                let added = snapshot.fuel_liters - previous;
                if added.is_finite() && added > 0.0 {
                    self.fuel_added_this_lap += added;
                }
            }
        }
        self.fuel_previous_sample = Some(snapshot.fuel_liters);

        if lap_changed {
            if let Some(previous_start) = self.fuel_at_lap_start {
                let consumed = previous_start + self.fuel_added_this_lap - snapshot.fuel_liters;
                if (0.1..30.0).contains(&consumed) {
                    self.fuel_last_lap = Some(consumed);
                    if completed_is_clean {
                        self.fuel_per_lap = Self::update_clean_average(
                            self.fuel_per_lap,
                            &mut self.fuel_clean_laps,
                            consumed,
                        );
                    }
                }
            }
            self.fuel_at_lap_start = Some(snapshot.fuel_liters);
            self.fuel_added_this_lap = 0.0;
        }

        let fuel_per_lap = self.fuel_per_lap.unwrap_or(0.0);
        let estimated_laps = self
            .fuel_per_lap
            .filter(|consumption| *consumption > 0.0)
            .map(|consumption| snapshot.fuel_liters / consumption)
            .unwrap_or(0.0);
        (fuel_per_lap, estimated_laps)
    }

    fn uses_virtual_energy(snapshot: &LmuSnapshot) -> bool {
        // Valores de IP_VehicleClass del SDK oficial: Hypercar = 0, GT3 = 6.
        matches!(snapshot.vehicle_class_id, 0 | 6)
    }

    fn driver_assists(snapshot: &LmuSnapshot) -> (bool, bool) {
        (snapshot.tc_active != 0, snapshot.abs_active != 0)
    }

    fn steering_and_force(snapshot: &LmuSnapshot, rest_range_degrees: Option<f64>) -> (f64, f64) {
        let steering = if snapshot.steering.is_finite() {
            snapshot.steering.clamp(-1.0, 1.0)
        } else {
            0.0
        };
        let range = rest_range_degrees
            .filter(|range| range.is_finite() && *range > 0.0)
            .unwrap_or_else(|| {
                if snapshot.steering_range_degrees.is_finite() {
                    snapshot.steering_range_degrees.max(0.0)
                } else {
                    0.0
                }
            });
        let force = if snapshot.force_feedback.is_finite() {
            snapshot.force_feedback.clamp(-1.0, 1.0)
        } else {
            0.0
        };
        (steering * range * 0.5, force)
    }

    fn virtual_energy_percent(raw: f64) -> f64 {
        if !raw.is_finite() || raw <= 0.0 {
            return 0.0;
        }

        // El SDK ha expuesto este valor normalizado en distintas iteraciones.
        // Admitimos tanto fracción [0, 1] como porcentaje [0, 100].
        if raw <= 1.25 {
            (raw * 100.0).clamp(0.0, 100.0)
        } else {
            raw.clamp(0.0, 100.0)
        }
    }

    fn update_energy_estimate(
        &mut self,
        snapshot: &LmuSnapshot,
        lap_changed: bool,
        completed_is_clean: bool,
    ) -> (f64, f64, f64) {
        let current = Self::virtual_energy_percent(snapshot.virtual_energy);
        if !Self::uses_virtual_energy(snapshot) {
            self.energy_at_lap_start = None;
            self.energy_previous_sample = None;
            self.energy_added_this_lap = 0.0;
            self.energy_per_lap = None;
            return (current, 0.0, 0.0);
        }

        if Self::player_in_pits(snapshot) {
            if let Some(previous) = self.energy_previous_sample {
                let added = current - previous;
                if added.is_finite() && added > 0.0 {
                    self.energy_added_this_lap += added;
                }
            }
        }
        self.energy_previous_sample = Some(current);

        if lap_changed {
            if let Some(previous_start) = self.energy_at_lap_start {
                let consumed = previous_start + self.energy_added_this_lap - current;
                if (0.05..100.0).contains(&consumed) {
                    self.energy_last_lap = Some(consumed);
                    if completed_is_clean {
                        self.energy_per_lap = Self::update_clean_average(
                            self.energy_per_lap,
                            &mut self.energy_clean_laps,
                            consumed,
                        );
                    }
                }
            }
            // LMU puede limitar la EV inicial en carreras cortas. La referencia
            // de la vuelta siempre es la lectura real, nunca un 100 % teórico.
            self.energy_at_lap_start = Some(current);
            self.energy_added_this_lap = 0.0;
        }

        let energy_per_lap = self.energy_per_lap.unwrap_or(0.0);
        let estimated_laps = self
            .energy_per_lap
            .filter(|consumption| *consumption > 0.0)
            .map(|consumption| current / consumption)
            .unwrap_or(0.0);
        (current, energy_per_lap, estimated_laps)
    }

    fn update_qualifying_reference(
        &mut self,
        snapshot: &LmuSnapshot,
        lap_changed: bool,
        completed_is_clean: bool,
    ) {
        if !lap_changed
            || !completed_is_clean
            || !Self::is_qualifying(snapshot.session_type)
            || snapshot.last_lap_seconds <= 0.0
            || (self.fuel_last_lap.is_none() && self.energy_last_lap.is_none())
        {
            return;
        }

        // mBestLapTime es la referencia oficial de la sesión y no se actualiza
        // con una vuelta invalidada. Admitimos una pequeña tolerancia de lectura.
        if snapshot.best_lap_seconds > 0.0
            && (snapshot.last_lap_seconds - snapshot.best_lap_seconds).abs() > 0.05
        {
            return;
        }

        let should_update = self
            .qualifying_reference_time
            .map(|reference| snapshot.last_lap_seconds < reference)
            .unwrap_or(true);

        if should_update {
            self.qualifying_reference_time = Some(snapshot.last_lap_seconds);
            if let Some(consumption) = self.fuel_last_lap {
                self.fuel_qualifying_lap = Some(consumption);
            }
            if let Some(consumption) = self.energy_last_lap {
                self.energy_qualifying_lap = Some(consumption);
            }
        }
    }

    fn string_from_chars(chars: &[c_char]) -> String {
        let bytes = chars
            .iter()
            .take_while(|value| **value != 0)
            .map(|value| *value as u8)
            .collect::<Vec<_>>();
        String::from_utf8_lossy(&bytes).trim().to_owned()
    }

    fn car_number(vehicle_name: &str, vehicle_filename: &str) -> String {
        if let Some(number) = vehicle_name
            .split_once('#')
            .and_then(|(_, suffix)| {
                suffix
                    .split(|character: char| !character.is_ascii_digit())
                    .next()
            })
            .filter(|number| !number.is_empty())
        {
            return number.to_owned();
        }

        vehicle_filename
            .split(|character: char| !character.is_ascii_digit())
            .rfind(|part| !part.is_empty())
            .filter(|part| part.len() <= 3)
            .unwrap_or("--")
            .to_owned()
    }

    fn tire_compound(wheel_types: &[u8; 4]) -> String {
        let letters: Vec<&str> = wheel_types
            .iter()
            .map(|&compound| match compound {
                0 => "S",
                1 => "M",
                2 => "H",
                3 => "W",
                _ => "?",
            })
            .collect();
        let mut unique: Vec<&str> = Vec::new();
        for letter in letters {
            if !unique.contains(&letter) {
                unique.push(letter);
            }
        }
        if unique.len() == 1 {
            unique[0].to_string()
        } else {
            unique.join("/")
        }
    }

    fn tire_compounds(wheel_types: &[u8; 4]) -> [String; 4] {
        std::array::from_fn(|index| match wheel_types[index] {
            0 => "S".into(),
            1 => "M".into(),
            2 => "H".into(),
            3 => "W".into(),
            _ => "?".into(),
        })
    }

    fn ensure_vehicle_identity(&mut self, entry: &LmuStandingEntry) {
        if let Some(identity) = self.vehicle_identities.get(&entry.vehicle_id) {
            if identity.matches(entry) {
                return;
            }
        }

        let driver_name = Self::string_from_chars(&entry.driver_name);
        let vehicle_class = Self::string_from_chars(&entry.vehicle_class);
        let scoring_vehicle_name = Self::string_from_chars(&entry.vehicle_name);
        let vehicle_filename = Self::string_from_chars(&entry.vehicle_filename);
        let vehicle_model = Self::string_from_chars(&entry.vehicle_model);
        let raw_team_name = Self::string_from_chars(&entry.team_name);
        let tire_compound = Self::tire_compound(&entry.wheel_compounds);
        let identity = VehicleIdentity {
            driver_name_raw: entry.driver_name,
            vehicle_class_raw: entry.vehicle_class,
            team_name_raw: entry.team_name,
            vehicle_name_raw: entry.vehicle_name,
            vehicle_filename_raw: entry.vehicle_filename,
            vehicle_model_raw: entry.vehicle_model,
            tire_compound_raw: entry.tire_compound,
            rear_tire_compound_raw: entry.rear_tire_compound,
            driver_name,
            vehicle_class,
            team_name: if raw_team_name.is_empty() {
                scoring_vehicle_name.clone()
            } else {
                raw_team_name
            },
            vehicle_name: if vehicle_model.is_empty() {
                scoring_vehicle_name.clone()
            } else {
                vehicle_model
            },
            fallback_car_number: Self::car_number(&scoring_vehicle_name, &vehicle_filename),
            tire_compound,
        };
        self.vehicle_identities.insert(entry.vehicle_id, identity);
    }

    fn rest_finish_status(value: &str) -> Option<u32> {
        match value.trim().to_ascii_uppercase().as_str() {
            "FSTAT_FINISHED" | "FINISHED" => Some(1),
            "FSTAT_DNF" | "DNF" => Some(2),
            "FSTAT_DQ" | "FSTAT_DISQUALIFIED" | "DQ" | "DISQUALIFIED" => Some(3),
            _ => None,
        }
    }

    fn class_rank(class_name: &str) -> u8 {
        let normalized = class_name.to_ascii_uppercase();
        if normalized.contains("HYPER") || normalized.contains("GTP") {
            0
        } else if normalized.contains("LMP2") {
            1
        } else if normalized.contains("LMGT3") || normalized.contains("GT3") {
            2
        } else {
            3
        }
    }

    fn driver_rank_score(rank: &str, progress: f64) -> Option<f64> {
        if progress < 0.0 || !progress.is_finite() {
            return None;
        }
        let mut characters = rank.trim().chars();
        let level = characters.next()?.to_ascii_uppercase();
        let tier = characters.as_str().parse::<i32>().ok()?;
        let level_offset = match level {
            'B' => 0,
            'S' => 3,
            'G' => 6,
            'P' => 9,
            _ => return None,
        };
        Some(((level_offset + tier).max(0) as f64 * 100.0) + progress.clamp(0.0, 100.0))
    }

    fn head_to_head(position: i32, opponent_position: i32) -> f64 {
        match position.cmp(&opponent_position) {
            std::cmp::Ordering::Less => 1.0,
            std::cmp::Ordering::Equal => 0.5,
            std::cmp::Ordering::Greater => 0.0,
        }
    }

    fn driver_rank_expected(
        rating: f64,
        opponent_rating: f64,
        settings: DriverRankSettings,
    ) -> f64 {
        1.0 / (1.0
            + settings
                .logarithm
                .powf((opponent_rating - rating) / settings.distance))
    }

    fn driver_rank_gain(
        race_result_total: f64,
        qualify_result_total: f64,
        opponent_count: u32,
        settings: DriverRankSettings,
    ) -> f64 {
        let gain_factor = settings.multiplier * settings.k * 2.0 / f64::from(opponent_count);
        let internal_rating_gain =
            gain_factor * (race_result_total + DRIVER_RANK_QUALIFY_WEIGHT * qualify_result_total);
        internal_rating_gain / DRIVER_RANK_INTERNAL_SCALE
    }

    fn scored_class_positions(
        entries: &[StandingEntry],
        scored_overall_positions: &HashMap<i32, i32>,
    ) -> HashMap<i32, i32> {
        let mut classes = HashMap::<&str, Vec<&StandingEntry>>::new();
        for entry in entries {
            classes
                .entry(entry.vehicle_class.as_str())
                .or_default()
                .push(entry);
        }

        let mut positions = HashMap::new();
        for mut class_entries in classes.into_values() {
            if !class_entries.iter().all(|entry| {
                scored_overall_positions
                    .get(&entry.vehicle_id)
                    .is_some_and(|position| *position > 0)
            }) {
                continue;
            }
            class_entries.sort_by_key(|entry| {
                (
                    scored_overall_positions[&entry.vehicle_id],
                    entry.vehicle_id,
                )
            });
            for (index, entry) in class_entries.into_iter().enumerate() {
                positions.insert(entry.vehicle_id, index as i32 + 1);
            }
        }
        positions
    }

    fn update_driver_rank_estimates(
        entries: &mut [StandingEntry],
        rank_scores: &HashMap<i32, f64>,
        qualifying_positions: &HashMap<i32, i32>,
        scored_class_positions: &HashMap<i32, i32>,
        session_type: i32,
        settings: DriverRankSettings,
    ) -> Option<DriverRankEstimateDiagnostic> {
        if (0..=8).contains(&session_type) {
            let entry = entries.iter().find(|entry| entry.is_player)?;
            let visual_score = rank_scores.get(&entry.vehicle_id).copied();
            return Some(DriverRankEstimateDiagnostic {
                status: if visual_score.is_some() {
                    "current_rank"
                } else {
                    "player_rank_unavailable"
                },
                vehicle_id: entry.vehicle_id,
                vehicle_class: entry.vehicle_class.clone(),
                driver_rank: entry.driver_rank.clone(),
                driver_rank_progress: entry.driver_rank_progress,
                visual_score,
                race_position: 0,
                live_race_position: 0,
                race_position_source: "not_applicable",
                qualifying_position: 0,
                same_class_rivals: 0,
                rated_opponents: 0,
                race_result_total: 0.0,
                qualifying_result_total: 0.0,
                gain_factor: 0.0,
                internal_rating_gain: None,
                estimated_gain: None,
            });
        }
        if !(10..=13).contains(&session_type) {
            return None;
        }

        // RaceControl no publica el cambio en directo. La puntuación visual se
        // convierte a la escala interna usada por el estimador y se aplican los
        // parámetros de DR configurados para el evento.
        let mut gains = Vec::<(usize, f64)>::new();
        let mut player_diagnostic = None;
        for (index, entry) in entries.iter().enumerate() {
            let race_position = scored_class_positions
                .get(&entry.vehicle_id)
                .copied()
                .unwrap_or(entry.position);
            let race_position_source = if scored_class_positions.contains_key(&entry.vehicle_id) {
                "rest_server_scored"
            } else {
                "shared_memory_live"
            };
            let starting_position = qualifying_positions
                .get(&entry.vehicle_id)
                .copied()
                .unwrap_or(entry.position + entry.position_change);
            let same_class_rivals = entries
                .iter()
                .enumerate()
                .filter(|(other_index, opponent)| {
                    *other_index != index && opponent.vehicle_class == entry.vehicle_class
                })
                .count() as u32;
            let rated_opponents = entries
                .iter()
                .enumerate()
                .filter(|(other_index, opponent)| {
                    *other_index != index
                        && opponent.vehicle_class == entry.vehicle_class
                        && rank_scores.contains_key(&opponent.vehicle_id)
                })
                .count() as u32;
            let Some(rank_score) = rank_scores.get(&entry.vehicle_id).copied() else {
                if entry.is_player {
                    player_diagnostic = Some(DriverRankEstimateDiagnostic {
                        status: "player_rank_unavailable",
                        vehicle_id: entry.vehicle_id,
                        vehicle_class: entry.vehicle_class.clone(),
                        driver_rank: entry.driver_rank.clone(),
                        driver_rank_progress: entry.driver_rank_progress,
                        visual_score: None,
                        race_position,
                        live_race_position: entry.position,
                        race_position_source,
                        qualifying_position: starting_position,
                        same_class_rivals,
                        rated_opponents,
                        race_result_total: 0.0,
                        qualifying_result_total: 0.0,
                        gain_factor: 0.0,
                        internal_rating_gain: None,
                        estimated_gain: None,
                    });
                }
                continue;
            };
            let mut race_result_total = 0.0;
            let mut qualify_result_total = 0.0;
            let mut opponent_count = 0_u32;
            for (other_index, opponent) in entries.iter().enumerate() {
                if other_index == index || opponent.vehicle_class != entry.vehicle_class {
                    continue;
                }
                let Some(opponent_rank_score) = rank_scores.get(&opponent.vehicle_id).copied()
                else {
                    continue;
                };
                let rating = rank_score * DRIVER_RANK_INTERNAL_SCALE;
                let opponent_rating = opponent_rank_score * DRIVER_RANK_INTERNAL_SCALE;
                let expected = Self::driver_rank_expected(rating, opponent_rating, settings);
                let opponent_race_position = scored_class_positions
                    .get(&opponent.vehicle_id)
                    .copied()
                    .unwrap_or(opponent.position);
                let race_result = Self::head_to_head(race_position, opponent_race_position);
                let opponent_start = qualifying_positions
                    .get(&opponent.vehicle_id)
                    .copied()
                    .unwrap_or(opponent.position + opponent.position_change);
                let qualify_result = Self::head_to_head(starting_position, opponent_start);
                race_result_total += race_result - expected;
                qualify_result_total += qualify_result - expected;
                opponent_count += 1;
            }
            if opponent_count == 0 {
                if entry.is_player {
                    player_diagnostic = Some(DriverRankEstimateDiagnostic {
                        status: "opponent_ranks_unavailable",
                        vehicle_id: entry.vehicle_id,
                        vehicle_class: entry.vehicle_class.clone(),
                        driver_rank: entry.driver_rank.clone(),
                        driver_rank_progress: entry.driver_rank_progress,
                        visual_score: Some(rank_score),
                        race_position,
                        live_race_position: entry.position,
                        race_position_source,
                        qualifying_position: starting_position,
                        same_class_rivals,
                        rated_opponents,
                        race_result_total,
                        qualifying_result_total: qualify_result_total,
                        gain_factor: 0.0,
                        internal_rating_gain: None,
                        estimated_gain: None,
                    });
                }
                continue;
            }
            let gain_factor = settings.multiplier * settings.k * 2.0 / f64::from(opponent_count);
            let gain = Self::driver_rank_gain(
                race_result_total,
                qualify_result_total,
                opponent_count,
                settings,
            );
            if entry.is_player {
                player_diagnostic = Some(DriverRankEstimateDiagnostic {
                    status: "estimated",
                    vehicle_id: entry.vehicle_id,
                    vehicle_class: entry.vehicle_class.clone(),
                    driver_rank: entry.driver_rank.clone(),
                    driver_rank_progress: entry.driver_rank_progress,
                    visual_score: Some(rank_score),
                    race_position,
                    live_race_position: entry.position,
                    race_position_source,
                    qualifying_position: starting_position,
                    same_class_rivals,
                    rated_opponents,
                    race_result_total,
                    qualifying_result_total: qualify_result_total,
                    gain_factor,
                    internal_rating_gain: Some(gain * DRIVER_RANK_INTERNAL_SCALE),
                    estimated_gain: Some(gain),
                });
            }
            gains.push((index, gain));
        }

        for (index, gain) in gains {
            entries[index].estimated_driver_rank_gain = gain;
            entries[index].estimated_driver_rank_gain_available = true;
        }
        player_diagnostic
    }

    fn update_standings_state(&mut self, snapshot: &LmuSnapshot) {
        let count = (snapshot.standings_count as usize).min(MAX_VEHICLES);
        let mut raw_entries = snapshot.standings[..count]
            .iter()
            .filter(|entry| entry.position > 0)
            .collect::<Vec<_>>();
        raw_entries.sort_by_key(|entry| entry.position);

        let mut class_positions = HashMap::<[c_char; 32], i32>::new();
        for entry in raw_entries {
            self.ensure_vehicle_identity(entry);
            let current_energy = Self::virtual_energy_percent(entry.virtual_energy);
            self.car_histories
                .entry(entry.vehicle_id)
                .or_default()
                .update(entry, current_energy);

            let class_position = class_positions.entry(entry.vehicle_class).or_default();
            *class_position += 1;
            if (10..=13).contains(&snapshot.session_type) {
                if snapshot.game_phase <= 4 {
                    self.starting_positions
                        .insert(entry.vehicle_id, *class_position);
                } else {
                    self.starting_positions
                        .entry(entry.vehicle_id)
                        .or_insert(*class_position);
                }
            }
        }
    }

    fn standings(
        &mut self,
        snapshot: &LmuSnapshot,
        yellow_culprits: &HashSet<i32>,
    ) -> Vec<StandingEntry> {
        let count = (snapshot.standings_count as usize).min(MAX_VEHICLES);
        let mut raw_entries = snapshot.standings[..count]
            .iter()
            .filter(|entry| entry.position > 0)
            .collect::<Vec<_>>();
        raw_entries.sort_by_key(|entry| entry.position);

        // Mantiene el método autocontenido para las pruebas y para cualquier
        // refresco forzado. En el ciclo normal son comparaciones de arrays ya
        // cacheados, sin volver a convertir cadenas.
        for entry in &raw_entries {
            self.ensure_vehicle_identity(entry);
        }
        let driver_names = raw_entries
            .iter()
            .filter_map(|entry| self.vehicle_identities.get(&entry.vehicle_id))
            .map(|identity| identity.driver_name.as_str())
            .collect::<Vec<_>>();
        self.driver_ranks.refresh(&driver_names);

        let player_class = raw_entries
            .iter()
            .find(|entry| entry.is_player != 0)
            .and_then(|entry| self.vehicle_identities.get(&entry.vehicle_id))
            .map(|identity| identity.vehicle_class.clone());
        let now = Instant::now();
        let log_gap_sample = super::analysis_logging_generation().is_some()
            && self
                .last_gap_log_at
                .is_none_or(|previous| now.duration_since(previous) >= Duration::from_secs(1));
        if log_gap_sample {
            self.last_gap_log_at = Some(now);
        }
        let log_driver_rank_sample = super::dr_estimate_log::enabled();
        let mut gap_sample = Vec::new();

        let mut fastest_by_class = HashMap::<String, f64>::new();
        for entry in &raw_entries {
            if entry.best_lap_seconds <= 0.0 {
                continue;
            }
            let identity = self
                .vehicle_identities
                .get(&entry.vehicle_id)
                .expect("identidad de vehículo inicializada");
            fastest_by_class
                .entry(identity.vehicle_class.clone())
                .and_modify(|best| *best = best.min(entry.best_lap_seconds))
                .or_insert(entry.best_lap_seconds);
        }

        let mut class_positions = HashMap::<String, i32>::new();
        let mut class_leaders = HashMap::<String, &LmuStandingEntry>::new();
        let mut previous_in_class = HashMap::<String, &LmuStandingEntry>::new();
        let mut driver_rank_scores = HashMap::<i32, f64>::new();
        let mut driver_qualifying_positions = HashMap::<i32, i32>::new();
        let mut scored_overall_positions = HashMap::<i32, i32>::new();
        let player_entry = raw_entries
            .iter()
            .find(|entry| entry.is_player != 0)
            .copied();
        let mut entries = Vec::with_capacity(raw_entries.len());

        for entry in raw_entries {
            let identity = self
                .vehicle_identities
                .get(&entry.vehicle_id)
                .expect("identidad de vehículo inicializada");
            let vehicle_class = identity.vehicle_class.clone();
            let rest = self
                .local_rest
                .standing(entry.vehicle_id, &identity.driver_name)
                .cloned();
            if snapshot.game_phase == 8 {
                if let Some(position) = rest
                    .as_ref()
                    .filter(|standing| standing.server_scored)
                    .map(|standing| standing.position)
                    .filter(|position| *position > 0)
                {
                    scored_overall_positions.insert(entry.vehicle_id, position);
                }
            }
            let class_position = class_positions.entry(vehicle_class.clone()).or_default();
            *class_position += 1;
            let class_position = *class_position;

            let current_energy = Self::virtual_energy_percent(entry.virtual_energy);
            let historical_lap_times = self
                .local_rest
                .history(entry.vehicle_id, &identity.driver_name)
                .map(|history| {
                    history
                        .iter()
                        .filter_map(|lap| {
                            (lap.total_laps > 0 && lap.lap_time.is_finite() && lap.lap_time > 0.0)
                                .then_some((lap.total_laps, lap.lap_time))
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            let history = self.car_histories.entry(entry.vehicle_id).or_default();
            history.seed_recent_lap_times(entry.total_laps, &historical_lap_times);
            let average_lap_seconds = history.average_lap_time();
            let average_energy_usage = history.average_energy_usage();
            let is_out_lap = history.is_out_lap();
            let pit_stop_time_seconds = history.pit_stop_time_seconds();

            let starting_position = self
                .local_rest
                .starting_class_position(
                    entry.vehicle_id,
                    &identity.driver_name,
                    &identity.vehicle_class,
                )
                .or_else(|| self.starting_positions.get(&entry.vehicle_id).copied())
                .unwrap_or(class_position);
            let initial_class_count = self
                .local_rest
                .initial_class_count(&identity.vehicle_class)
                .max(class_position as usize);

            let leader = *class_leaders.entry(vehicle_class.clone()).or_insert(entry);
            let (laps_behind_leader, time_behind_leader) =
                Self::class_relative_gap(leader, entry, snapshot.track_length);
            let previous = previous_in_class
                .insert(vehicle_class.clone(), entry)
                .unwrap_or(entry);
            let (laps_behind_next, interval) = if class_position > 1 {
                Self::class_relative_gap(previous, entry, snapshot.track_length)
            } else {
                (0, 0.0)
            };

            if log_gap_sample && player_class.as_deref() == Some(vehicle_class.as_str()) {
                let leader_progress =
                    leader.total_laps as f64 + leader.lap_distance / snapshot.track_length.max(1.0);
                let entry_progress =
                    entry.total_laps as f64 + entry.lap_distance / snapshot.track_length.max(1.0);
                gap_sample.push(serde_json::json!({
                    "vehicle_id": entry.vehicle_id,
                    "driver": identity.driver_name.as_str(),
                    "overall_position": entry.position,
                    "class_position": class_position,
                    "is_player": entry.is_player != 0,
                    "total_laps": entry.total_laps,
                    "lap_distance": entry.lap_distance,
                    "time_into_lap": entry.time_into_lap,
                    "estimated_lap_time": entry.estimated_lap_time,
                    "raw_laps_behind_overall_leader": entry.laps_behind_leader,
                    "raw_time_behind_overall_leader": entry.time_behind_leader,
                    "raw_time_behind_overall_next": entry.interval,
                    "class_leader_vehicle_id": leader.vehicle_id,
                    "class_leader_total_laps": leader.total_laps,
                    "class_leader_lap_distance": leader.lap_distance,
                    "class_leader_time_into_lap": leader.time_into_lap,
                    "class_leader_estimated_lap_time": leader.estimated_lap_time,
                    "class_progress_diff": (leader_progress - entry_progress),
                    "gap_laps": laps_behind_leader,
                    "gap_seconds": time_behind_leader,
                    "interval_laps": laps_behind_next,
                    "interval_seconds": interval,
                }));
            }

            let virtual_energy_active = matches!(entry.vehicle_class_id, 0 | 6);
            let fastest_lap = fastest_by_class.get(&vehicle_class).copied().unwrap_or(0.0);
            let mut ranks = self
                .driver_ranks
                .lookup(&identity.driver_name)
                .unwrap_or_default();
            if let Some(event_profile) = self.session_split.value().profile(&identity.driver_name) {
                if ranks.nationality.is_empty() || ranks.nationality == "XX" {
                    ranks.nationality = event_profile.nationality.clone();
                }
                if ranks.badge.is_empty() {
                    ranks.badge = event_profile.badge.clone();
                }
            }
            if let Some(score) = Self::driver_rank_score(&ranks.driver, ranks.driver_progress) {
                driver_rank_scores.insert(entry.vehicle_id, score);
            }
            if let Some(qualification) = rest
                .as_ref()
                .map(|standing| standing.qualification)
                .filter(|qualification| *qualification > 0)
            {
                driver_qualifying_positions.insert(entry.vehicle_id, qualification);
            }
            let (relative_ahead_seconds, relative_behind_seconds) = player_entry
                .map(|player| Self::relative_gaps_seconds(&player, entry))
                .unwrap_or((0.0, 0.0));
            let relative_gap_seconds = if relative_ahead_seconds.abs() <= relative_behind_seconds {
                relative_ahead_seconds
            } else {
                relative_behind_seconds
            };
            let laps_relative_to_player = player_entry
                .map(|player| Self::laps_relative_to_player(player, entry))
                .unwrap_or(0);

            entries.push(StandingEntry {
                vehicle_id: entry.vehicle_id,
                overall_position: entry.position,
                position: class_position,
                position_change: starting_position - class_position,
                car_number: rest
                    .as_ref()
                    .map(|standing| standing.car_number.trim())
                    .filter(|number| !number.is_empty())
                    .map(str::to_owned)
                    .unwrap_or_else(|| identity.fallback_car_number.clone()),
                driver_name: identity.driver_name.clone(),
                driver_rank: ranks.driver,
                driver_rank_progress: ranks.driver_progress,
                estimated_driver_rank_gain: 0.0,
                estimated_driver_rank_gain_available: false,
                safety_rank: ranks.safety,
                safety_rank_progress: ranks.safety_progress,
                nationality: ranks.nationality,
                driver_badge: ranks.badge,
                team_name: identity.team_name.clone(),
                vehicle_name: identity.vehicle_name.clone(),
                vehicle_class,
                initial_class_count,
                laps_relative_to_player,
                total_laps: entry.total_laps,
                laps_behind_leader,
                laps_behind_next,
                time_behind_leader,
                interval,
                relative_gap_seconds,
                relative_ahead_seconds,
                relative_behind_seconds,
                best_lap_seconds: entry.best_lap_seconds.max(0.0),
                last_lap_seconds: history.last_lap_seconds(entry.last_lap_seconds),
                average_lap_seconds,
                virtual_energy_active,
                virtual_energy_percent: if virtual_energy_active {
                    rest.as_ref()
                        .map(|standing| standing.ve_fraction * 100.0)
                        .filter(|value| value.is_finite() && *value >= 0.0 && *value <= 100.0)
                        .unwrap_or(current_energy)
                } else {
                    0.0
                },
                virtual_energy_per_lap: if matches!(entry.vehicle_class_id, 0 | 6) {
                    average_energy_usage
                } else {
                    0.0
                },
                damage_percent: entry.damage_percent.clamp(0.0, 100.0),
                track_limits_steps: (entry.track_limits_available != 0)
                    .then_some(entry.track_limits_steps),
                pit_stops: rest
                    .as_ref()
                    .map(|standing| standing.pitstops)
                    .unwrap_or(entry.pit_stops),
                pit_stop_requested: entry.pit_state == 1
                    || rest.as_ref().is_some_and(|standing| {
                        matches!(
                            standing.pit_state.trim().to_ascii_uppercase().as_str(),
                            "REQUEST" | "REQUESTED"
                        )
                    }),
                pit_stop_time_seconds,
                tire_compound: identity.tire_compound.clone(),
                tire_compounds: Self::tire_compounds(&entry.wheel_compounds),
                flag: entry.flag,
                causing_yellow: yellow_culprits.contains(&entry.vehicle_id),
                has_fastest_lap: fastest_lap > 0.0
                    && (entry.best_lap_seconds - fastest_lap).abs() <= 0.001,
                in_pits: entry.in_pits != 0
                    || rest.as_ref().is_some_and(|standing| standing.pitting),
                in_garage: entry.in_garage != 0
                    || rest
                        .as_ref()
                        .is_some_and(|standing| standing.in_garage_stall),
                is_out_lap,
                last_lap_valid: history.is_last_lap_valid(),
                penalty_count: entry.penalties,
                finish_status: rest
                    .as_ref()
                    .and_then(|standing| Self::rest_finish_status(&standing.finish_status))
                    .unwrap_or(entry.finish_status),
                is_player: entry.is_player != 0,
            });
        }

        let latest_scored_positions =
            Self::scored_class_positions(&entries, &scored_overall_positions);
        self.scored_finish_positions.extend(latest_scored_positions);
        let driver_rank_diagnostic = Self::update_driver_rank_estimates(
            &mut entries,
            &driver_rank_scores,
            &driver_qualifying_positions,
            &self.scored_finish_positions,
            snapshot.session_type,
            self.session_split.value().driver_rank_settings,
        );

        if log_driver_rank_sample {
            let split = self.session_split.value();
            if let Some(sample) = driver_rank_diagnostic {
                if (0..=8).contains(&snapshot.session_type) {
                    super::dr_estimate_log::queue(serde_json::json!({
                        "event": "driver_rank_current_sample",
                        "event_id": split.event_id,
                        "session_type": snapshot.session_type,
                        "status": sample.status,
                        "player": {
                            "vehicle_id": sample.vehicle_id,
                            "vehicle_class": sample.vehicle_class,
                            "driver_rank": sample.driver_rank,
                            "driver_rank_progress": sample.driver_rank_progress,
                            "visual_score": sample.visual_score,
                            "internal_score": sample.visual_score.map(|score| score * DRIVER_RANK_INTERNAL_SCALE),
                        },
                    }));
                } else {
                    super::dr_estimate_log::queue(serde_json::json!({
                        "event": "driver_rank_estimate_sample",
                        "event_id": split.event_id,
                        "session_type": snapshot.session_type,
                        "game_phase": snapshot.game_phase,
                        "status": sample.status,
                        "player": {
                            "vehicle_id": sample.vehicle_id,
                            "vehicle_class": sample.vehicle_class,
                            "driver_rank": sample.driver_rank,
                            "driver_rank_progress": sample.driver_rank_progress,
                            "visual_score": sample.visual_score,
                            "internal_score": sample.visual_score.map(|score| score * DRIVER_RANK_INTERNAL_SCALE),
                            "race_position": sample.race_position,
                            "live_race_position": sample.live_race_position,
                            "race_position_source": sample.race_position_source,
                            "qualifying_position": sample.qualifying_position,
                        },
                        "coverage": {
                            "same_class_rivals": sample.same_class_rivals,
                            "rated_opponents": sample.rated_opponents,
                            "missing_profiles": sample.same_class_rivals.saturating_sub(sample.rated_opponents),
                        },
                        "calculation": {
                            "race_result_total": sample.race_result_total,
                            "qualifying_result_total": sample.qualifying_result_total,
                            "qualifying_weight": DRIVER_RANK_QUALIFY_WEIGHT,
                            "gain_factor": sample.gain_factor,
                            "internal_rating_gain": sample.internal_rating_gain,
                            "estimated_gain": sample.estimated_gain,
                        },
                        "settings": {
                            "multiplier": split.driver_rank_settings.multiplier,
                            "k": split.driver_rank_settings.k,
                            "distance": split.driver_rank_settings.distance,
                            "logarithm": split.driver_rank_settings.logarithm,
                        },
                    }));
                }
            }
        }

        if !gap_sample.is_empty() {
            super::queue_analysis_event(serde_json::json!({
                "event": "standings_gap_sample",
                "session_type": snapshot.session_type,
                "game_phase": snapshot.game_phase,
                "track_length": snapshot.track_length,
                "player_class": player_class.unwrap_or_default(),
                "cars": gap_sample,
            }));
        }

        entries.sort_by(|left, right| {
            Self::class_rank(&left.vehicle_class)
                .cmp(&Self::class_rank(&right.vehicle_class))
                .then_with(|| left.vehicle_class.cmp(&right.vehicle_class))
                .then_with(|| left.position.cmp(&right.position))
        });
        entries
    }

    fn stable_standings(&mut self, standings: Vec<StandingEntry>) -> Vec<StandingEntry> {
        const TRANSIENT_EMPTY_HOLD: Duration = Duration::from_secs(2);

        if !standings.is_empty() {
            self.last_valid_standings.clone_from(&standings);
            self.last_valid_standings_at = Some(Instant::now());
            return standings;
        }

        if self.last_valid_standings_at.is_some_and(|received| {
            received.elapsed() <= TRANSIENT_EMPTY_HOLD && !self.last_valid_standings.is_empty()
        }) {
            return self.last_valid_standings.clone();
        }

        self.last_valid_standings.clear();
        self.last_valid_standings_at = None;
        standings
    }

    /// TinyPedal considera que un coche está provocando (o a punto de provocar)
    /// una amarilla cuando circula a menos de 8 m/s. En standings el indicador
    /// es deliberadamente preventivo: no depende de que LMU ya haya activado la
    /// bandera sectorial.
    fn slow_yellow_vehicles(snapshot: &LmuSnapshot) -> HashSet<i32> {
        const YELLOW_SPEED_THRESHOLD_KPH: f64 = 8.0 * 3.6;
        let count = (snapshot.standings_count as usize).min(MAX_VEHICLES);

        snapshot.standings[..count]
            .iter()
            .filter(|entry| {
                entry.vehicle_id != 0
                    && entry.in_pits == 0
                    && entry.in_garage == 0
                    && entry.pit_state < 2
                    && entry.speed_kph.is_finite()
                    && entry.speed_kph < YELLOW_SPEED_THRESHOLD_KPH
            })
            .map(|entry| entry.vehicle_id)
            .collect()
    }

    fn class_relative_gap(
        ahead: &LmuStandingEntry,
        behind: &LmuStandingEntry,
        track_length: f64,
    ) -> (i32, f64) {
        if !track_length.is_finite() || track_length <= 0.0 {
            return (0, 0.0);
        }
        let ahead_progress = ahead.total_laps as f64 + ahead.lap_distance / track_length;
        let behind_progress = behind.total_laps as f64 + behind.lap_distance / track_length;
        let lap_diff = ahead_progress - behind_progress;
        if lap_diff >= 1.0 {
            return (lap_diff as i32, 0.0);
        }
        let mut time_gap = ahead.time_into_lap - behind.time_into_lap;
        if time_gap < 0.0 && lap_diff > 0.0 {
            time_gap += behind.estimated_lap_time.max(1.0);
        }
        (0, time_gap.abs().max(0.0))
    }

    fn relative_gaps_seconds(player: &LmuStandingEntry, entry: &LmuStandingEntry) -> (f64, f64) {
        let lap_time = player.estimated_lap_time;
        if player.vehicle_id == entry.vehicle_id
            || entry.in_garage != 0
            || !lap_time.is_finite()
            || lap_time <= 1.0
            || !player.time_into_lap.is_finite()
            || !entry.time_into_lap.is_finite()
        {
            return (0.0, 0.0);
        }

        // TinyPedal-style circular timing: every opponent has one possible
        // occurrence ahead and another behind. The displayed convention is
        // negative ahead and positive behind.
        let ahead = (entry.time_into_lap - player.time_into_lap).rem_euclid(lap_time);
        if ahead <= f64::EPSILON {
            return (0.0, 0.0);
        }
        (-ahead, lap_time - ahead)
    }

    fn laps_relative_to_player(player: &LmuStandingEntry, entry: &LmuStandingEntry) -> i32 {
        let lap_time = player.estimated_lap_time;
        if player.vehicle_id == entry.vehicle_id
            || !lap_time.is_finite()
            || lap_time <= 1.0
            || !player.time_into_lap.is_finite()
            || !entry.time_into_lap.is_finite()
        {
            return 0;
        }

        // Completed laps alone briefly differ when only one car has crossed the
        // timing line. Adding the continuous phase difference removes that false
        // lap before rounding to the actual race-lap relationship.
        let completed_delta = f64::from(entry.total_laps - player.total_laps);
        let phase_delta = (entry.time_into_lap - player.time_into_lap) / lap_time;
        (completed_delta + phase_delta).round() as i32
    }

    fn track_distance(from: f64, to: f64, track_length: f64) -> f64 {
        if !from.is_finite() || !to.is_finite() || track_length <= 1.0 {
            return 0.0;
        }
        (to - from).rem_euclid(track_length)
    }

    fn flag_warning(snapshot: &LmuSnapshot, yellow_culprits: &HashSet<i32>) -> super::FlagWarning {
        let count = (snapshot.standings_count as usize).min(MAX_VEHICLES);
        let raw = &snapshot.standings[..count];
        let Some(player) = raw.iter().find(|entry| entry.is_player != 0) else {
            return super::FlagWarning::default();
        };

        // La bandera a cuadros pertenece al estado de sesión/coche, no a
        // mFlag (el SDK documenta ese campo solamente para verde y azul).
        // Debe prevalecer sobre cualquier amarilla o azul todavía activa.
        if player.finish_status == 1 || snapshot.game_phase == 8 {
            return super::FlagWarning {
                kind: "checkered",
                active: true,
                distance_meters: 0.0,
                car_position: player.position,
                vehicle_class: String::new(),
            };
        }

        // TinyPedal solo enseña el aviso si LMU confirma una amarilla sectorial.
        // Busca primero un coche lento hasta 500 m por delante y, si no existe,
        // uno hasta 50 m por detrás.
        if snapshot.yellow_sectors != 0 {
            let ahead = raw
                .iter()
                .filter(|entry| yellow_culprits.contains(&entry.vehicle_id))
                .map(|entry| {
                    (
                        entry.vehicle_id,
                        Self::track_distance(
                            player.lap_distance,
                            entry.lap_distance,
                            snapshot.track_length,
                        ),
                    )
                })
                .filter(|(_, distance)| *distance <= 500.0)
                .min_by(|left, right| left.1.total_cmp(&right.1));

            if let Some((vehicle_id, distance)) = ahead {
                return Self::warning_for_car("yellow", vehicle_id, distance, snapshot);
            }

            let behind = raw
                .iter()
                .filter(|entry| yellow_culprits.contains(&entry.vehicle_id))
                .map(|entry| {
                    (
                        entry.vehicle_id,
                        Self::track_distance(
                            entry.lap_distance,
                            player.lap_distance,
                            snapshot.track_length,
                        ),
                    )
                })
                .filter(|(_, distance)| *distance <= 50.0)
                .min_by(|left, right| left.1.total_cmp(&right.1));

            if let Some((vehicle_id, distance)) = behind {
                // Igual que TinyPedal: positivo indica delante y negativo detrás.
                return Self::warning_for_car("yellow", vehicle_id, -distance, snapshot);
            }
        }

        if player.flag != 6 {
            return super::FlagWarning::default();
        }

        // mFlag confirma que el jugador recibe azul, pero no identifica al coche.
        // Elegimos el coche más cercano por detrás; priorizamos al que lleva una
        // vuelta de ventaja o tiene ritmo claramente superior.
        let mut candidates = raw
            .iter()
            .filter(|entry| {
                entry.vehicle_id != player.vehicle_id
                    && entry.in_pits == 0
                    && entry.in_garage == 0
                    && entry.finish_status == 0
            })
            .filter_map(|entry| {
                let distance = Self::track_distance(
                    entry.lap_distance,
                    player.lap_distance,
                    snapshot.track_length,
                );
                (distance > 1.0 && distance <= 2_000.0).then_some((entry, distance))
            })
            .collect::<Vec<_>>();
        candidates.sort_by(|left, right| left.1.total_cmp(&right.1));
        let is_plausible_blue_car = |entry: &LmuStandingEntry| {
            entry.total_laps > player.total_laps
                || (entry.best_lap_seconds > 0.0
                    && player.best_lap_seconds > 0.0
                    && entry.best_lap_seconds < player.best_lap_seconds * 0.98)
        };
        let target = candidates
            .iter()
            .find(|(entry, _)| is_plausible_blue_car(entry))
            .or_else(|| candidates.first());

        target.map_or_else(super::FlagWarning::default, |(entry, distance)| {
            Self::warning_for_car("blue", entry.vehicle_id, *distance, snapshot)
        })
    }

    fn warning_for_car(
        kind: &'static str,
        vehicle_id: i32,
        distance_meters: f64,
        snapshot: &LmuSnapshot,
    ) -> super::FlagWarning {
        let (car_position, vehicle_class) = Self::warning_car_details(snapshot, vehicle_id);
        super::FlagWarning {
            kind,
            active: true,
            distance_meters: if distance_meters.is_finite() {
                distance_meters
            } else {
                0.0
            },
            car_position,
            vehicle_class,
        }
    }

    fn warning_car_details(snapshot: &LmuSnapshot, vehicle_id: i32) -> (i32, String) {
        let count = (snapshot.standings_count as usize).min(MAX_VEHICLES);
        let raw = &snapshot.standings[..count];
        let Some(target) = raw.iter().find(|entry| entry.vehicle_id == vehicle_id) else {
            return (0, String::new());
        };
        let class_position = raw
            .iter()
            .filter(|entry| {
                entry.position > 0
                    && entry.position <= target.position
                    && entry.vehicle_class == target.vehicle_class
            })
            .count() as i32;
        (
            class_position,
            Self::string_from_chars(&target.vehicle_class),
        )
    }

    fn rejoin_safety(distance: f64, time_to_arrival: f64) -> &'static str {
        if !time_to_arrival.is_finite() {
            return "safe";
        }
        if distance <= 100.0 || time_to_arrival <= 6.0 {
            "danger"
        } else if distance <= 250.0 || time_to_arrival <= 12.0 {
            "caution"
        } else {
            "safe"
        }
    }

    fn update_rejoin_warning(&mut self, snapshot: &LmuSnapshot) -> super::RejoinWarning {
        // Coincide con los valores por defecto del aviso Traffic de TinyPedal:
        // 8 m/s de velocidad baja, 15 s de separación y 10 s de persistencia.
        const LOW_SPEED_KPH: f64 = 8.0 * 3.6;
        const MAX_TRAFFIC_GAP_SECONDS: f64 = 15.0;
        const HOLD_FRAMES: u16 = 200;
        let count = (snapshot.standings_count as usize).min(MAX_VEHICLES);
        let raw = &snapshot.standings[..count];
        let Some(player) = raw.iter().find(|entry| entry.is_player != 0) else {
            self.rejoin_hold_frames = 0;
            return super::RejoinWarning::default();
        };

        if player.in_pits != 0 {
            self.rejoin_reason = "pit_exit";
            self.rejoin_hold_frames = HOLD_FRAMES;
        } else if snapshot.player_offroad_wheels >= 4
            || (snapshot.speed_kph.is_finite() && snapshot.speed_kph < LOW_SPEED_KPH)
        {
            // Conserva el motivo de salida de boxes mientras continúa su ventana;
            // fuera de ella, la velocidad baja/off-road inicia un rejoin normal.
            if self.rejoin_hold_frames == 0 || self.rejoin_reason != "pit_exit" {
                self.rejoin_reason = "rejoin";
            }
            self.rejoin_hold_frames = HOLD_FRAMES;
        } else if self.rejoin_hold_frames > 0 {
            self.rejoin_hold_frames -= 1;
        }

        if self.rejoin_hold_frames == 0 {
            return super::RejoinWarning::default();
        }

        let rear_car = raw
            .iter()
            .filter(|entry| {
                entry.vehicle_id != player.vehicle_id
                    && entry.in_pits == 0
                    && entry.in_garage == 0
                    && entry.finish_status == 0
            })
            .filter_map(|entry| {
                let traffic_gap = Self::relative_gaps_seconds(player, entry).1;
                if !(0.05..MAX_TRAFFIC_GAP_SECONDS).contains(&traffic_gap) {
                    return None;
                }
                let distance = Self::track_distance(
                    entry.lap_distance,
                    player.lap_distance,
                    snapshot.track_length,
                );
                if distance <= 1.0 {
                    return None;
                }
                let closing_speed_ms = ((entry.speed_kph - player.speed_kph) / 3.6).max(0.0);
                let time_to_arrival = if closing_speed_ms > 0.5 {
                    distance / closing_speed_ms
                } else {
                    f64::INFINITY
                };
                Some((entry, distance, traffic_gap, time_to_arrival))
            })
            // Prioriza el coche que llegará antes. Si ninguno se acerca, usa la
            // separación temporal más corta, que es el criterio de TinyPedal.
            .min_by(|left, right| {
                left.3
                    .total_cmp(&right.3)
                    .then_with(|| left.2.total_cmp(&right.2))
            });

        let Some((rear, distance, _, time_to_arrival)) = rear_car else {
            return super::RejoinWarning::default();
        };

        let safety = Self::rejoin_safety(distance, time_to_arrival);
        let (car_position, vehicle_class) = Self::warning_car_details(snapshot, rear.vehicle_id);

        super::RejoinWarning {
            active: true,
            reason: self.rejoin_reason,
            safety,
            rear_car_available: true,
            distance_meters: distance,
            time_to_arrival_seconds: if time_to_arrival.is_finite() {
                time_to_arrival
            } else {
                0.0
            },
            car_position,
            vehicle_class,
        }
    }

    fn time_to_next_crossing(lap_time: f64, time_into_lap: f64) -> Option<f64> {
        if !lap_time.is_finite() || lap_time <= 0.0 {
            return None;
        }
        let elapsed = time_into_lap.clamp(0.0, lap_time);
        Some((lap_time - elapsed).max(0.001))
    }

    fn leader_finish_delay(snapshot: &LmuSnapshot) -> Option<f64> {
        let leader_lap = snapshot.leader_lap_time;
        let next_crossing = Self::time_to_next_crossing(leader_lap, snapshot.leader_time_into_lap)?;

        if snapshot.max_laps > 0 && snapshot.max_laps < 10_000 {
            let crossings = (snapshot.max_laps - snapshot.leader_total_laps).max(0);
            if crossings == 0 {
                return Some(0.0);
            }
            return Some(next_crossing + (crossings - 1) as f64 * leader_lap);
        }

        let clock = snapshot.session_time_remaining.max(0.0);
        if clock <= next_crossing {
            return Some(next_crossing);
        }

        let full_laps_after_next = ((clock - next_crossing) / leader_lap).ceil();
        Some(next_crossing + full_laps_after_next * leader_lap)
    }

    fn player_reference_lap(snapshot: &LmuSnapshot) -> f64 {
        if snapshot.estimated_lap_time > 0.0 {
            snapshot.estimated_lap_time
        } else if snapshot.last_lap_seconds > 0.0 {
            snapshot.last_lap_seconds
        } else {
            snapshot.best_lap_seconds
        }
    }

    fn initial_player_lap_pace(snapshot: &LmuSnapshot) -> Option<f64> {
        [
            snapshot.best_lap_seconds,
            snapshot.last_lap_seconds,
            snapshot.estimated_lap_time,
        ]
        .into_iter()
        .find(|value| value.is_finite() && *value > 0.0)
    }

    fn update_player_lap_pace(&mut self, snapshot: &LmuSnapshot, completed_is_clean: bool) {
        if self.lap_time_pace.is_none() {
            self.lap_time_pace = Self::initial_player_lap_pace(snapshot);
        }
        if !completed_is_clean || snapshot.last_lap_seconds <= 0.0 {
            return;
        }

        let completed = snapshot.last_lap_seconds;
        self.lap_time_pace = Some(match self.lap_time_pace {
            None => completed,
            Some(previous) if completed < previous => completed,
            // TinyPedal usa seis muestras (factor EMA 2/7) y limita a cinco
            // segundos el aumento producido por una sola vuelta válida.
            Some(previous) => (previous + (2.0 / 7.0) * (completed - previous)).min(previous + 5.0),
        });
    }

    fn estimated_laps_remaining(snapshot: &LmuSnapshot, lap_progress: f64, lap_pace: f64) -> f64 {
        if Self::player_finished(snapshot) {
            return 0.0;
        }

        let progress = lap_progress.clamp(0.0, 1.0);
        let finite_lap_target = snapshot.max_laps > 0 && snapshot.max_laps < 10_000;
        let target_finishes_first = finite_lap_target
            && (snapshot.session_time_remaining <= 0.0
                || Self::leader_finish_delay(snapshot)
                    .is_none_or(|delay| delay <= snapshot.session_time_remaining));
        if target_finishes_first {
            return (f64::from((snapshot.max_laps - snapshot.player_total_laps).max(0)) - progress)
                .max(0.0);
        }

        if !lap_pace.is_finite() || lap_pace <= 0.0 {
            return 0.0;
        }
        let laps_at_timer = if snapshot.session_time_remaining > 0.0 {
            snapshot.session_time_remaining / lap_pace + progress
        } else {
            progress
        };
        (laps_at_timer.ceil() - progress).max(0.0)
    }

    fn player_finished(snapshot: &LmuSnapshot) -> bool {
        let count = (snapshot.standings_count as usize).min(MAX_VEHICLES);
        snapshot.standings[..count]
            .iter()
            .any(|entry| entry.is_player != 0 && entry.finish_status != 0)
    }

    fn player_crossings_until_finish(snapshot: &LmuSnapshot, finish_delay: f64) -> Option<f64> {
        let player_lap = Self::player_reference_lap(snapshot);
        let next_crossing = Self::time_to_next_crossing(player_lap, snapshot.player_time_into_lap)?;

        if finish_delay <= next_crossing {
            return Some(1.0);
        }

        let crossings_after_next = ((finish_delay - next_crossing - 0.001) / player_lap)
            .ceil()
            .max(0.0);
        Some(1.0 + crossings_after_next)
    }

    fn player_class_leader(snapshot: &LmuSnapshot) -> Option<&LmuStandingEntry> {
        let count = (snapshot.standings_count as usize).min(MAX_VEHICLES);
        let standings = &snapshot.standings[..count];
        let player = standings.iter().find(|entry| entry.is_player != 0)?;
        standings
            .iter()
            .filter(|entry| entry.position > 0 && entry.vehicle_class == player.vehicle_class)
            .min_by_key(|entry| entry.position)
    }

    fn standing_reference_lap(entry: &LmuStandingEntry) -> Option<f64> {
        [
            entry.estimated_lap_time,
            entry.last_lap_seconds,
            entry.best_lap_seconds,
        ]
        .into_iter()
        .find(|value| value.is_finite() && *value > 0.0)
    }

    fn total_laps_estimated(snapshot: &LmuSnapshot) -> f64 {
        if snapshot.game_phase >= 8 {
            if let Some(class_leader) = Self::player_class_leader(snapshot) {
                return if class_leader.finish_status != 0 {
                    class_leader.total_laps.max(0) as f64
                } else {
                    class_leader.total_laps.saturating_add(1).max(0) as f64
                };
            }
            return if Self::player_finished(snapshot) {
                snapshot.player_total_laps.max(0) as f64
            } else {
                snapshot.player_total_laps.saturating_add(1).max(0) as f64
            };
        }
        if snapshot.max_laps > 0 && snapshot.max_laps < 10_000 {
            return snapshot.max_laps as f64;
        }

        let Some(finish_delay) = Self::leader_finish_delay(snapshot) else {
            return 0.0;
        };
        if finish_delay <= 0.0 {
            return 0.0;
        }

        let (completed_laps, lap_time, time_into_lap) =
            if let Some(class_leader) = Self::player_class_leader(snapshot) {
                if class_leader.finish_status != 0 {
                    return class_leader.total_laps.max(0) as f64;
                }
                let Some(lap_time) = Self::standing_reference_lap(class_leader) else {
                    return 0.0;
                };
                (
                    class_leader.total_laps,
                    lap_time,
                    class_leader.time_into_lap,
                )
            } else {
                (
                    snapshot.leader_total_laps,
                    snapshot.leader_lap_time,
                    snapshot.leader_time_into_lap,
                )
            };
        let Some(next_crossing) = Self::time_to_next_crossing(lap_time, time_into_lap) else {
            return 0.0;
        };
        let crossings = if finish_delay <= next_crossing {
            1.0
        } else {
            1.0 + ((finish_delay - next_crossing - 0.001) / lap_time)
                .ceil()
                .max(0.0)
        };

        completed_laps.max(0) as f64 + crossings
    }

    fn laps_remaining(snapshot: &LmuSnapshot) -> f64 {
        if snapshot.game_phase >= 8 {
            return if Self::player_finished(snapshot) {
                0.0
            } else {
                1.0
            };
        }
        if let Some(finish_delay) = Self::leader_finish_delay(snapshot) {
            if finish_delay <= 0.0 {
                return 0.0;
            }
            if let Some(crossings) = Self::player_crossings_until_finish(snapshot, finish_delay) {
                return crossings;
            }
        }

        // Fallback para sesiones sin datos válidos del líder.
        if snapshot.max_laps > 0 && snapshot.max_laps < 10_000 {
            return (snapshot.max_laps - snapshot.player_total_laps).max(0) as f64;
        }
        let reference_lap = Self::player_reference_lap(snapshot);
        if snapshot.session_time_remaining > 0.0 && reference_lap > 0.0 {
            return (snapshot.session_time_remaining / reference_lap).ceil() + 1.0;
        }
        0.0
    }
}

impl TelemetrySource for LmuTelemetrySource {
    fn next_frame(&mut self, include_standings: bool, include_track_map: bool) -> TelemetryFrame {
        const TRANSIENT_SNAPSHOT_HOLD: Duration = Duration::from_secs(2);
        const STANDINGS_STATE_INTERVAL: Duration = Duration::from_millis(200);

        let snapshot_started = Instant::now();
        let mut snapshot = LmuSnapshot::default();
        let result = unsafe { lmu_read_snapshot(&mut snapshot) };
        let live_snapshot = result > 0 && snapshot.connected != 0;
        if live_snapshot {
            self.last_valid_snapshot = Some(snapshot);
            self.last_valid_snapshot_at = Some(Instant::now());
        } else if self.last_valid_snapshot_at.is_some_and(|received| {
            received.elapsed() <= TRANSIENT_SNAPSHOT_HOLD && self.last_valid_snapshot.is_some()
        }) {
            snapshot = self.last_valid_snapshot.expect("snapshot comprobado");
        }
        let snapshot_us = snapshot_started.elapsed().as_micros();
        let rest_started = Instant::now();
        self.local_rest.refresh(
            snapshot.connected != 0,
            Self::weather_session_key(snapshot.session_type),
        );
        let rest_us = rest_started.elapsed().as_micros();

        if snapshot.connected == 0 {
            self.last_lap = -1;
            self.fuel_at_lap_start = None;
            self.fuel_previous_sample = None;
            self.fuel_added_this_lap = 0.0;
            self.energy_at_lap_start = None;
            self.energy_previous_sample = None;
            self.energy_added_this_lap = 0.0;
            self.consumption_profiler.reset_lap();
            self.tire_wear_tracker.reset();
            self.rejoin_hold_frames = 0;
            self.last_standings_state_update = None;
            self.last_valid_standings.clear();
            self.last_valid_standings_at = None;
            self.last_valid_snapshot = None;
            self.last_valid_snapshot_at = None;
            self.player_lap_distance.reset();
            return TelemetryFrame::waiting_for_lmu(false);
        }

        let session_started = Instant::now();
        self.update_session(snapshot.session_type);
        self.session_split.refresh();
        // Standings sigue el criterio preventivo de TinyPedal. El overlay de
        // banderas exige además una amarilla sectorial y proximidad.
        let standings_yellow_culprits = Self::slow_yellow_vehicles(&snapshot);
        let session_us = session_started.elapsed().as_micros();
        let standings_state_due = include_standings
            || self
                .last_standings_state_update
                .is_none_or(|updated| updated.elapsed() >= STANDINGS_STATE_INTERVAL);
        let standings_state_us = if standings_state_due {
            let started = Instant::now();
            self.update_standings_state(&snapshot);
            self.last_standings_state_update = Some(Instant::now());
            Some(started.elapsed().as_micros())
        } else {
            None
        };
        let standings_build_started = include_standings.then(Instant::now);
        let standings = if include_standings {
            let standings = self.standings(&snapshot, &standings_yellow_culprits);
            self.stable_standings(standings)
        } else {
            Vec::new()
        };
        let standings_build_us =
            standings_build_started.map(|started| started.elapsed().as_micros());

        if snapshot.player_active == 0 {
            self.last_lap = -1;
            self.fuel_at_lap_start = None;
            self.fuel_previous_sample = None;
            self.fuel_added_this_lap = 0.0;
            self.energy_at_lap_start = None;
            self.energy_previous_sample = None;
            self.energy_added_this_lap = 0.0;
            self.consumption_profiler.reset_lap();
            self.tire_wear_tracker.reset();
            self.player_lap_distance.reset();
            let mut frame = TelemetryFrame::waiting_for_lmu(true);
            frame.standings = standings;
            return frame;
        }

        let warnings_started = Instant::now();
        let flag_warning = Self::flag_warning(&snapshot, &standings_yellow_culprits);
        let rejoin_warning = self.update_rejoin_warning(&snapshot);
        let warnings_us = warnings_started.elapsed().as_micros();
        let frame_started = Instant::now();

        // Clasificamos la vuelta terminada con los estados observados durante toda
        // la vuelta. Solo una vuelta válida, sin boxes y completamente en verde
        // puede modificar el consumo base de carrera.
        let lap_changed = snapshot.lap_number != self.last_lap;
        let in_pits = Self::player_in_pits(&snapshot);
        let player_tire_flat_spot_percent = self.tire_wear_tracker.update(
            snapshot.player_tire_remaining_by_wheel_percent,
            snapshot.player_tire_slip_ratio,
            snapshot.player_tire_sliding_fraction,
            snapshot.brake,
            in_pits,
        );
        let formation = (10..=13).contains(&snapshot.session_type) && snapshot.game_phase == 3;
        let completed_is_clean = lap_changed
            && self.last_lap >= 0
            && self.lap_was_valid
            && !CarHistory::official_lap_is_invalid(snapshot.last_lap_seconds)
            && !self.lap_visited_pits
            && !self.lap_was_formation
            && self.lap_was_green;
        self.update_player_lap_pace(&snapshot, completed_is_clean);
        if lap_changed {
            self.fuel_last_lap = None;
            self.energy_last_lap = None;
        }
        let (virtual_energy_percent, virtual_energy_per_lap, estimated_virtual_energy_laps) =
            self.update_energy_estimate(&snapshot, lap_changed, completed_is_clean);
        let (fuel_per_lap, estimated_fuel_laps) =
            self.update_fuel_estimate(&snapshot, lap_changed, completed_is_clean);
        self.update_qualifying_reference(&snapshot, lap_changed, completed_is_clean);
        let raw_lap_progress = if snapshot.track_length > 1.0 {
            (snapshot.player_lap_distance / snapshot.track_length).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let player_validity_is_synchronized = snapshot.current_lap_seconds >= 2.0
            || raw_lap_progress * snapshot.track_length >= 300.0;
        let current_player_lap_valid =
            !player_validity_is_synchronized || snapshot.player_lap_valid != 0;
        let synchronized_progress =
            synchronized_lap_progress(raw_lap_progress, snapshot.current_lap_seconds, lap_changed);
        let lap_distance = self.player_lap_distance.update(
            synchronized_progress * snapshot.track_length,
            snapshot.current_lap_seconds,
            snapshot.speed_kph,
            snapshot.gear,
            snapshot.lap_number,
            snapshot.track_length,
            lap_changed,
        );
        let lap_progress = if snapshot.track_length > 1.0 {
            (lap_distance / snapshot.track_length).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let fuel_used_current_lap = self
            .fuel_at_lap_start
            .map(|start| start + self.fuel_added_this_lap - snapshot.fuel_liters)
            .unwrap_or(0.0)
            .max(0.0);
        let energy_used_current_lap = self
            .energy_at_lap_start
            .map(|start| start + self.energy_added_this_lap - virtual_energy_percent)
            .unwrap_or(0.0)
            .max(0.0);
        let vehicle_name = Self::string_from_chars(&snapshot.vehicle_name);
        let track_name = Self::string_from_chars(&snapshot.track_name);
        let (tc_active, abs_active) = Self::driver_assists(&snapshot);
        let (steering_angle_degrees, force_feedback) =
            Self::steering_and_force(&snapshot, self.local_rest.steering_range_degrees());
        let rest_pit_stop = self.local_rest.pit_stop().cloned();
        let rest_vehicle_damage = self.local_rest.vehicle_damage();
        let session_split = self.session_split.value().clone();
        let profile_estimate: ProfileEstimate = self.consumption_profiler.observe(
            &vehicle_name,
            &track_name,
            snapshot.lap_number,
            lap_progress,
            current_player_lap_valid && snapshot.game_phase == 5,
            in_pits,
            formation,
            fuel_used_current_lap,
            energy_used_current_lap,
            self.fuel_last_lap,
            self.energy_last_lap,
        );
        if lap_changed {
            self.last_lap = snapshot.lap_number;
            self.lap_visited_pits = in_pits;
            self.lap_was_formation = formation;
            self.lap_was_valid = true;
            self.lap_was_green = snapshot.game_phase == 5;
        } else {
            self.lap_visited_pits |= in_pits;
            self.lap_was_formation |= formation;
            self.lap_was_valid &= current_player_lap_valid;
            self.lap_was_green &= snapshot.game_phase == 5;
        }
        let session_laps_remaining = Self::laps_remaining(&snapshot);
        let session_laps_remaining_estimated = Self::estimated_laps_remaining(
            &snapshot,
            lap_progress,
            self.lap_time_pace.unwrap_or(0.0),
        );
        let session_lap_equivalents_remaining =
            (session_laps_remaining - profile_estimate.lap_progress).max(0.0);
        let session_total_laps_estimated = Self::total_laps_estimated(&snapshot);
        let planned_fuel_per_lap = [
            profile_estimate.fuel_projected,
            fuel_per_lap,
            self.fuel_last_lap.unwrap_or(0.0),
            profile_estimate.fuel_reference,
            self.fuel_qualifying_lap.unwrap_or(0.0),
        ]
        .into_iter()
        .find(|value| *value > 0.0)
        .unwrap_or(0.0);
        let virtual_energy_active = Self::uses_virtual_energy(&snapshot);
        let planned_energy_per_lap = [
            profile_estimate.energy_projected,
            virtual_energy_per_lap,
            self.energy_last_lap.unwrap_or(0.0),
            profile_estimate.energy_reference,
            self.energy_qualifying_lap.unwrap_or(0.0),
        ]
        .into_iter()
        .find(|value| *value > 0.0)
        .unwrap_or(0.0);
        let lap_seconds = [
            self.lap_time_pace.unwrap_or(0.0),
            snapshot.last_lap_seconds,
            snapshot.best_lap_seconds,
            snapshot.current_lap_seconds,
        ]
        .into_iter()
        .find(|value| value.is_finite() && *value > 0.0)
        .unwrap_or(0.0);
        let strategy_input =
            |current, capacity, consumption, pit_cycle, pit_out| ResourceStrategyInput {
                current,
                capacity,
                consumption,
                laps_remaining: session_lap_equivalents_remaining,
                lap_progress,
                completed_laps: snapshot.player_total_laps,
                pit_cycle_consumption: pit_cycle,
                pit_out_consumption: pit_out,
                pit_out_lap: profile_estimate.current_lap_started_in_pits,
            };
        let fuel_input = |consumption| {
            strategy_input(
                snapshot.fuel_liters,
                snapshot.fuel_capacity_liters,
                consumption,
                profile_estimate.fuel_pit_cycle_consumption,
                profile_estimate.fuel_pit_out_consumption,
            )
        };
        let energy_input = |consumption| {
            strategy_input(
                virtual_energy_percent,
                100.0,
                consumption,
                profile_estimate.energy_pit_cycle_consumption,
                profile_estimate.energy_pit_out_consumption,
            )
        };
        let fuel_strategy =
            calculate_resource_strategy(fuel_input(planned_fuel_per_lap), lap_seconds, 0);
        let active_input = if virtual_energy_active {
            energy_input(planned_energy_per_lap)
        } else {
            fuel_input(planned_fuel_per_lap)
        };
        let active_strategy = calculate_resource_strategy(
            active_input,
            lap_seconds,
            if virtual_energy_active {
                fuel_strategy.map_or(0, |strategy| strategy.stops)
            } else {
                0
            },
        );
        let active_minimum_stops = active_strategy.map_or(0, |strategy| strategy.stops);
        let active_scenario = |consumption| {
            let input = if virtual_energy_active {
                energy_input(consumption)
            } else {
                fuel_input(consumption)
            };
            calculate_resource_strategy(input, lap_seconds, active_minimum_stops)
        };
        let fuel_strategies = FuelStrategies {
            active: active_strategy,
            fuel: virtual_energy_active.then_some(fuel_strategy).flatten(),
            estimated: active_scenario(if virtual_energy_active {
                planned_energy_per_lap
            } else {
                planned_fuel_per_lap
            }),
            average: active_scenario(if virtual_energy_active {
                virtual_energy_per_lap
            } else {
                fuel_per_lap
            }),
            qualifying: active_scenario(if virtual_energy_active {
                self.energy_qualifying_lap.unwrap_or(0.0)
            } else {
                self.fuel_qualifying_lap.unwrap_or(0.0)
            }),
            last: active_scenario(if virtual_energy_active {
                self.energy_last_lap.unwrap_or(0.0)
            } else {
                self.fuel_last_lap.unwrap_or(0.0)
            }),
            ..FuelStrategies::default()
        }
        .with_qualifying_guidance();
        let fuel_needed_liters = fuel_strategy
            .map(|strategy| {
                snapshot.fuel_liters + strategy.total_additional - strategy.end_remaining
            })
            .unwrap_or(0.0);
        let (
            virtual_energy_needed_percent,
            virtual_energy_next_stint_percent,
            virtual_energy_stints_remaining,
        ) = if let Some(strategy) = if virtual_energy_active {
            active_strategy
        } else {
            None
        } {
            (
                virtual_energy_percent + strategy.total_additional - strategy.end_remaining,
                strategy.next_fill,
                strategy.stops,
            )
        } else {
            (0.0, 0.0, 0)
        };
        let track_map_vehicles = if include_track_map {
            snapshot.standings[..snapshot.standings_count.min(MAX_VEHICLES as u32) as usize]
                .iter()
                .filter(|entry| {
                    entry.vehicle_id > 0
                        && entry.world_x.is_finite()
                        && entry.world_y.is_finite()
                        && entry.in_garage == 0
                })
                .map(|entry| TrackMapVehicle {
                    vehicle_id: entry.vehicle_id,
                    overall_position: entry.position,
                    class_position: 0,
                    vehicle_class: Self::string_from_chars(&entry.vehicle_class),
                    world_x: entry.world_x,
                    world_y: entry.world_y,
                    lap_distance: entry.lap_distance.max(0.0),
                    total_laps: entry.total_laps,
                    in_pits: entry.in_pits != 0,
                    in_garage: entry.in_garage != 0,
                    is_player: entry.is_player != 0,
                })
                .collect()
        } else {
            Vec::new()
        };
        let weather_forecast = self.local_rest.weather_forecast().map(|session| {
            let mut nodes = session.forecast_nodes();
            let session_length = if snapshot.session_end_seconds > snapshot.session_elapsed_seconds
            {
                snapshot.session_end_seconds
            } else {
                self.local_rest.session_max_time_seconds()
            };
            let progress = if session_length > 0.0 {
                (snapshot.session_elapsed_seconds / session_length).clamp(0.0, 1.0)
            } else if snapshot.session_time_remaining > 0.0 {
                (snapshot.session_elapsed_seconds
                    / (snapshot.session_elapsed_seconds + snapshot.session_time_remaining))
                    .clamp(0.0, 1.0)
            } else {
                0.0
            };
            let current_index = if nodes.is_empty() {
                0
            } else {
                ((progress / 0.2).floor().min((nodes.len() - 1) as f64)) as i32
            };
            let next_index = (current_index + 1).min(nodes.len() as i32);
            if session_length > 0.0 {
                for (index, node) in nodes.iter_mut().enumerate() {
                    if index as i32 >= next_index {
                        let minutes = ((index as f64 * 0.2 * session_length
                            - snapshot.session_elapsed_seconds)
                            / 60.0)
                            .round();
                        node.minutes_from_now = Some(minutes.max(0.0) as i32);
                    }
                }
            }
            super::WeatherForecastModel {
                available: true,
                session: Self::weather_session_key(snapshot.session_type).to_owned(),
                current_index,
                next_index,
                nodes,
            }
        });
        let current_humidity_percent = weather_forecast
            .as_ref()
            .and_then(|model| {
                usize::try_from(model.current_index)
                    .ok()
                    .and_then(|index| model.nodes.get(index))
            })
            .map(|node| node.humidity_percent)
            .unwrap_or(0.0);
        let rest_wind = weather_forecast.as_ref().and_then(|model| {
            usize::try_from(model.current_index).ok().and_then(|index| {
                self.local_rest
                    .weather_forecast()
                    .and_then(|session| session.wind_at(index))
            })
        });
        let (wind_speed_ms, wind_direction_degrees) =
            Self::resolve_wind(snapshot.wind_x, snapshot.wind_z, rest_wind);
        let player_grip_percent = Self::track_grip_percent(snapshot.track_grip_level);
        let track_rubber_percent = Self::track_rubber_percent(&snapshot);
        let track_grip_state = Self::track_surface_state(snapshot.track_wetness_percent);
        let frame = TelemetryFrame {
            source: "lmu",
            performance_profile: "smooth",
            connected: true,
            player_active: true,
            game_in_foreground: snapshot.game_in_foreground != 0,
            game_in_realtime: snapshot.game_in_realtime != 0,
            player_in_garage: snapshot.player_in_garage != 0,
            session_type: snapshot.session_type,
            game_phase: snapshot.game_phase,
            session_max_laps: snapshot.max_laps,
            session_time_remaining: snapshot.session_time_remaining.max(0.0),
            session_elapsed_seconds: snapshot.session_elapsed_seconds.max(0.0),
            session_max_time_seconds: self.local_rest.session_max_time_seconds(),
            leader_total_laps: snapshot.leader_total_laps,
            session_split_number: session_split.number,
            session_split_count: session_split.count,
            track_name,
            player_vehicle_name: vehicle_name,
            rest_weather_available: snapshot.ambient_temperature_c.is_finite()
                && snapshot.track_temperature_c.is_finite(),
            ambient_temperature_c: snapshot.ambient_temperature_c,
            track_temperature_c: snapshot.track_temperature_c,
            rain_percent: snapshot.rain_percent.clamp(0.0, 100.0),
            track_wetness_percent: snapshot.track_wetness_percent.clamp(0.0, 100.0),
            track_wetness_min_percent: snapshot.track_wetness_min_percent.clamp(0.0, 100.0),
            track_wetness_max_percent: snapshot.track_wetness_max_percent.clamp(0.0, 100.0),
            weather_forecast: weather_forecast.unwrap_or_default(),
            current_humidity_percent,
            wind_speed_ms,
            wind_direction_degrees,
            player_grip_percent,
            track_rubber_percent,
            track_grip_state,
            cloud_coverage: Self::live_weather_icon(snapshot.cloud_coverage, snapshot.rain_percent),
            lap_number: snapshot.lap_number,
            player_sector: snapshot.player_sector,
            player_total_laps: snapshot.player_total_laps,
            player_lap_valid: current_player_lap_valid,
            player_in_pits: in_pits,
            speed_kph: snapshot.speed_kph.max(0.0),
            gear: snapshot.gear.clamp(-1, i8::MAX as i32) as i8,
            rpm: snapshot.rpm.max(0.0),
            max_rpm: snapshot.max_rpm.max(1.0),
            throttle: snapshot.throttle.clamp(0.0, 1.0),
            brake: snapshot.brake.clamp(0.0, 1.0),
            brake_bias_percent: snapshot.brake_bias_percent.clamp(0.0, 100.0),
            track_limits_steps: snapshot.track_limits_steps,
            track_limits_steps_per_penalty: snapshot.track_limits_steps_per_penalty,
            tc_active,
            abs_active,
            steering_angle_degrees,
            force_feedback,
            fuel_liters: snapshot.fuel_liters.max(0.0),
            fuel_added_this_lap: self.fuel_added_this_lap,
            fuel_capacity_liters: snapshot.fuel_capacity_liters.max(1.0),
            fuel_per_lap,
            fuel_last_lap: self.fuel_last_lap.unwrap_or(0.0),
            fuel_qualifying_lap: self.fuel_qualifying_lap.unwrap_or(0.0),
            fuel_reference_per_lap: profile_estimate.fuel_reference,
            fuel_projected_lap: profile_estimate.fuel_projected,
            fuel_pit_cycle_consumption: profile_estimate.fuel_pit_cycle_consumption,
            fuel_pit_out_consumption: profile_estimate.fuel_pit_out_consumption,
            fuel_ratio_assigned: if virtual_energy_active {
                self.local_rest.fuel_ratio_assigned()
            } else {
                0.0
            },
            fuel_ratio_average: virtual_energy_active
                .then(|| fuel_energy_ratio(fuel_per_lap, virtual_energy_per_lap))
                .unwrap_or(0.0),
            fuel_ratio_last: virtual_energy_active
                .then(|| {
                    fuel_energy_ratio(
                        self.fuel_last_lap.unwrap_or(0.0),
                        self.energy_last_lap.unwrap_or(0.0),
                    )
                })
                .unwrap_or(0.0),
            estimated_fuel_laps,
            session_laps_remaining,
            session_laps_remaining_estimated,
            session_lap_equivalents_remaining,
            session_total_laps_estimated,
            fuel_needed_liters,
            fuel_to_add_liters: (fuel_needed_liters - snapshot.fuel_liters).max(0.0),
            virtual_energy_active,
            virtual_energy_percent,
            virtual_energy_raw: snapshot.virtual_energy,
            virtual_energy_added_this_lap: self.energy_added_this_lap,
            virtual_energy_per_lap,
            virtual_energy_last_lap: self.energy_last_lap.unwrap_or(0.0),
            virtual_energy_qualifying_lap: self.energy_qualifying_lap.unwrap_or(0.0),
            virtual_energy_reference_per_lap: profile_estimate.energy_reference,
            virtual_energy_projected_lap: profile_estimate.energy_projected,
            virtual_energy_pit_cycle_consumption: profile_estimate.energy_pit_cycle_consumption,
            virtual_energy_pit_out_consumption: profile_estimate.energy_pit_out_consumption,
            player_pit_out_lap: profile_estimate.current_lap_started_in_pits,
            estimated_virtual_energy_laps,
            virtual_energy_needed_percent,
            virtual_energy_next_stint_percent,
            virtual_energy_stints_remaining,
            fuel_strategies,
            standings_model: Default::default(),
            relative_model: Default::default(),
            player_tire_remaining_percent: snapshot.player_tire_remaining_percent,
            player_damage_percent: snapshot.player_damage_percent.clamp(0.0, 100.0),
            player_aero_damage_percent: rest_vehicle_damage
                .map(|damage| damage.aero * 100.0)
                .unwrap_or(-1.0)
                .clamp(-1.0, 100.0),
            player_suspension_damage_percent: if snapshot.player_tire_detached.contains(&1) {
                100.0
            } else {
                rest_vehicle_damage
                    .map(|damage| damage.suspension.into_iter().fold(0.0_f64, f64::max) * 100.0)
                    .unwrap_or(-1.0)
                    .clamp(-1.0, 100.0)
            },
            player_suspension_damage_by_wheel_percent: suspension_damage_by_wheel_percent(
                rest_vehicle_damage,
                snapshot.player_tire_detached,
            ),
            player_body_damage_percent: snapshot
                .player_damage_severity
                .into_iter()
                .map(|severity| severity.min(2) as f64)
                .sum::<f64>()
                / 16.0
                * 100.0,
            player_damage_severity: snapshot.player_damage_severity,
            player_part_detached: snapshot.player_part_detached != 0,
            player_rear_wing_detached: rear_wing_detached(
                rest_vehicle_damage,
                snapshot.player_part_detached != 0,
                snapshot.player_damage_severity[4],
            ),
            player_tire_temperature_c: snapshot.player_tire_temperature_c,
            player_brake_temperature_c: snapshot.player_brake_temperature_c,
            player_tire_remaining_by_wheel_percent: snapshot.player_tire_remaining_by_wheel_percent,
            player_tire_flat_spot_percent,
            player_tire_compounds: Self::tire_compounds(&snapshot.player_tire_compounds),
            player_tire_flat: snapshot.player_tire_flat.map(|value| value != 0),
            player_tire_detached: snapshot.player_tire_detached.map(|value| value != 0),
            player_stint: snapshot
                .standings
                .iter()
                .find(|entry| entry.is_player != 0)
                .map(|entry| entry.pit_stops + 1)
                .unwrap_or(0),
            player_strategy_pit: in_pits,
            pit_stop_estimate_available: rest_pit_stop.is_some(),
            pit_stop_estimate_seconds: rest_pit_stop
                .as_ref()
                .map(|estimate| estimate.total.max(0.0))
                .unwrap_or(0.0),
            pit_stop_fuel_seconds: rest_pit_stop
                .as_ref()
                .map(|estimate| estimate.fuel.max(0.0))
                .unwrap_or(0.0),
            pit_stop_energy_seconds: rest_pit_stop
                .as_ref()
                .map(|estimate| estimate.ve.max(0.0))
                .unwrap_or(0.0),
            pit_stop_tire_seconds: rest_pit_stop
                .as_ref()
                .map(|estimate| estimate.tires.max(0.0))
                .unwrap_or(0.0),
            pit_stop_damage_seconds: rest_pit_stop
                .as_ref()
                .map(|estimate| (estimate.damage + estimate.brakes + estimate.brake_ducts).max(0.0))
                .unwrap_or(0.0),
            pit_stop_penalty_seconds: rest_pit_stop
                .as_ref()
                .map(|estimate| estimate.penalties.max(0.0))
                .unwrap_or(0.0),
            pit_stop_driver_swap_seconds: rest_pit_stop
                .as_ref()
                .map(|estimate| estimate.driver_swap.max(0.0))
                .unwrap_or(0.0),
            lap_progress: profile_estimate.lap_progress,
            track_length_meters: snapshot.track_length.max(0.0),
            track_map_vehicles,
            track_map_model: Default::default(),
            consumption_profile_samples: profile_estimate.samples,
            current_lap_seconds: snapshot.current_lap_seconds.max(0.0),
            current_sector1_seconds: snapshot.current_sector1_seconds.max(0.0),
            current_sector2_seconds: snapshot.current_sector2_seconds.max(0.0),
            player_best_sector_ends: snapshot.player_best_sector_ends,
            session_best_sector_ends: snapshot.session_best_sector_ends,
            last_lap_seconds: CarHistory::normalize_official_lap(snapshot.last_lap_seconds),
            last_lap_valid: !CarHistory::official_lap_is_invalid(snapshot.last_lap_seconds),
            best_lap_seconds: snapshot.best_lap_seconds.max(0.0),
            lap_delta_seconds: snapshot.lap_delta_seconds,
            delta_model: Default::default(),
            timing_model: Default::default(),
            flag_warning,
            rejoin_warning,
            standings,
        };
        self.source_stage_performance.record(SourceStageSample {
            snapshot_us,
            rest_us,
            session_us,
            standings_state_us,
            standings_build_us,
            warnings_us,
            frame_us: frame_started.elapsed().as_micros(),
        });
        frame
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

#[cfg(test)]
mod tests {
    use super::{
        fuel_energy_ratio, lmu_snapshot_size, rear_wing_detached,
        suspension_damage_by_wheel_percent, synchronized_lap_progress, CarHistory, LmuSnapshot,
        LmuStandingEntry, LmuTelemetrySource, PlayerLapDistanceEstimator, TireWearTracker,
    };
    use crate::telemetry::event_split::DriverRankSettings;
    use crate::telemetry::lmu_rest::{RestStanding, RestVehicleDamage};
    use crate::telemetry::StandingEntry;
    use std::collections::{HashMap, HashSet};

    #[test]
    fn fuel_energy_ratio_requires_both_valid_consumptions() {
        assert!((fuel_energy_ratio(12.0, 8.0) - 1.5).abs() < 1e-9);
        assert_eq!(fuel_energy_ratio(12.0, 0.0), 0.0);
        assert_eq!(fuel_energy_ratio(f64::NAN, 8.0), 0.0);
    }

    #[test]
    fn lap_progress_suppresses_stale_finish_distance_after_telemetry_boundary() {
        assert_eq!(synchronized_lap_progress(0.998, 0.02, true), 0.0);
        assert_eq!(synchronized_lap_progress(0.999, 0.12, false), 0.0);
        assert_eq!(synchronized_lap_progress(0.002, 0.22, false), 0.002);
        assert_eq!(synchronized_lap_progress(0.95, 96.0, false), 0.95);
    }

    #[test]
    fn player_lap_distance_advances_between_scoring_updates() {
        let mut estimator = PlayerLapDistanceEstimator::default();
        assert_eq!(estimator.update(0.0, 0.0, 180.0, 3, 4, 5_000.0, true), 0.0);
        assert_eq!(
            estimator.update(0.0, 0.02, 180.0, 3, 4, 5_000.0, false),
            1.0
        );
        assert_eq!(
            estimator.update(0.0, 0.04, 180.0, 3, 4, 5_000.0, false),
            2.0
        );

        let refreshed = estimator.update(10.0, 0.2, 180.0, 3, 4, 5_000.0, false);
        assert!((refreshed - 10.0).abs() < 0.001);
        let held = estimator.update(10.0, 0.22, 180.0, 3, 4, 5_000.0, false);
        assert!((held - 11.0).abs() < 0.001);
    }

    #[test]
    fn player_lap_distance_resets_at_the_lap_boundary() {
        let mut estimator = PlayerLapDistanceEstimator::default();
        estimator.update(4_990.0, 95.0, 180.0, 4, 3, 5_000.0, false);
        let reset = estimator.update(0.0, 0.02, 180.0, 3, 4, 5_000.0, true);
        assert_eq!(reset, 0.0);
    }

    #[test]
    fn live_weather_icon_combines_cloud_cover_and_rain_intensity() {
        assert_eq!(LmuTelemetrySource::live_weather_icon(3, 0.0), 3);
        assert_eq!(LmuTelemetrySource::live_weather_icon(9, 0.0), 4);
        assert_eq!(LmuTelemetrySource::live_weather_icon(2, 8.0), 5);
        assert_eq!(LmuTelemetrySource::live_weather_icon(2, 18.0), 7);
        assert_eq!(LmuTelemetrySource::live_weather_icon(2, 55.0), 9);
        assert_eq!(LmuTelemetrySource::live_weather_icon(2, 75.0), 10);
    }

    #[test]
    fn track_grip_level_maps_to_lmu_grip_fraction() {
        assert_eq!(LmuTelemetrySource::track_grip_percent(0), 0.0);
        assert_eq!(LmuTelemetrySource::track_grip_percent(1), 25.0);
        assert_eq!(LmuTelemetrySource::track_grip_percent(2), 50.0);
        assert_eq!(LmuTelemetrySource::track_grip_percent(3), 75.0);
        assert_eq!(LmuTelemetrySource::track_grip_percent(4), 90.0);
        assert_eq!(LmuTelemetrySource::track_grip_percent(5), 0.0);
    }

    #[test]
    fn track_rubber_estimate_uses_session_base_and_all_valid_completed_laps() {
        let mut snapshot = LmuSnapshot {
            session_type: 0,
            standings_count: 3,
            ..LmuSnapshot::default()
        };
        snapshot.standings[0] = LmuStandingEntry {
            vehicle_id: 1,
            position: 1,
            total_laps: 10,
            ..LmuStandingEntry::default()
        };
        snapshot.standings[1] = LmuStandingEntry {
            vehicle_id: 2,
            position: 2,
            total_laps: 17,
            ..LmuStandingEntry::default()
        };
        snapshot.standings[2] = LmuStandingEntry {
            vehicle_id: 3,
            position: 3,
            total_laps: 10_000,
            ..LmuStandingEntry::default()
        };

        assert_eq!(LmuTelemetrySource::track_rubber_percent(&snapshot), 26.0125);

        snapshot.session_type = 5;
        snapshot.standings_count = 0;
        assert_eq!(LmuTelemetrySource::track_rubber_percent(&snapshot), 50.0);
    }

    #[test]
    fn track_surface_state_uses_dry_and_lmu_wetness_bands() {
        assert_eq!(LmuTelemetrySource::track_surface_state(0.99), "dry");
        assert_eq!(LmuTelemetrySource::track_surface_state(1.0), "damp");
        assert_eq!(LmuTelemetrySource::track_surface_state(15.0), "wet");
        assert_eq!(LmuTelemetrySource::track_surface_state(40.0), "heavy");
        assert_eq!(LmuTelemetrySource::track_surface_state(70.0), "saturated");
    }

    #[test]
    fn wind_uses_official_rest_node_when_shared_memory_is_zero() {
        assert_eq!(
            LmuTelemetrySource::resolve_wind(0.0, 0.0, Some((7.0, 90.0))),
            (7.0, 90.0)
        );
        assert_eq!(
            LmuTelemetrySource::resolve_wind(3.0, 4.0, Some((7.0, 90.0))).0,
            5.0
        );
        assert_eq!(LmuTelemetrySource::resolve_wind(0.0, 0.0, None), (0.0, 0.0));
    }

    #[test]
    fn flat_spot_wear_only_accumulates_during_a_localized_slide() {
        let mut tracker = TireWearTracker::default();
        tracker.update([100.0; 4], [0.0; 4], [0.0; 4], 0.0, false);

        let normal_wear = tracker.update([99.9; 4], [-0.1; 4], [0.8; 4], 0.0, false);
        assert_eq!(normal_wear, [0.0; 4]);

        let low_grip_wear = tracker.update([99.8; 4], [-0.5; 4], [0.4; 4], 0.0, false);
        assert_eq!(low_grip_wear, [0.0; 4]);

        let slide_wear = tracker.update(
            [99.6, 99.7, 99.8, 99.8],
            [-0.5, -0.5, 0.0, 0.0],
            [0.8, 0.6, 0.8, 0.8],
            0.0,
            false,
        );
        assert!((slide_wear[0] - 0.2).abs() < 0.001);
        assert!((slide_wear[1] - 0.1).abs() < 0.001);
        assert_eq!(slide_wear[2], 0.0);
        assert_eq!(slide_wear[3], 0.0);
    }

    #[test]
    fn flat_spot_wear_uses_braking_as_a_fallback_when_sliding_fraction_is_missing() {
        let mut tracker = TireWearTracker::default();
        tracker.update([100.0; 4], [0.0; 4], [0.0; 4], 0.0, false);

        let lock_wear = tracker.update(
            [99.8, 99.9, 100.0, 100.0],
            [-0.5, -0.5, 0.0, 0.0],
            [0.0; 4],
            0.7,
            false,
        );

        assert!((lock_wear[0] - 0.2).abs() < 0.001);
        assert!((lock_wear[1] - 0.1).abs() < 0.001);
        assert_eq!(lock_wear[2], 0.0);
        assert_eq!(lock_wear[3], 0.0);
    }

    #[test]
    fn flat_spot_wear_resets_when_tyres_are_changed_in_pits() {
        let mut tracker = TireWearTracker::default();
        tracker.update([90.0; 4], [0.0; 4], [0.0; 4], 0.0, false);
        tracker.update([89.5; 4], [-0.5; 4], [0.8; 4], 0.0, false);

        let after_change = tracker.update([100.0; 4], [0.0; 4], [0.0; 4], 0.0, true);
        assert_eq!(after_change, [0.0; 4]);
    }

    #[test]
    fn suspension_damage_keeps_each_wheel_independent() {
        let damage = RestVehicleDamage {
            aero: 0.0,
            suspension: [0.02, 0.18, 0.51, 1.4],
        };

        assert_eq!(
            suspension_damage_by_wheel_percent(Some(damage), [0, 1, 0, 0]),
            [2.0, 100.0, 51.0, 100.0]
        );
        assert_eq!(
            suspension_damage_by_wheel_percent(None, [0, 0, 1, 0]),
            [-1.0, -1.0, 100.0, -1.0]
        );
    }

    #[test]
    fn rear_wing_loss_is_not_inferred_from_aggregate_damage() {
        let severe_aero_damage = RestVehicleDamage {
            aero: 2.016,
            suspension: [0.0; 4],
        };

        assert!(!rear_wing_detached(Some(severe_aero_damage), true, 2));
        assert!(!rear_wing_detached(None, true, 2));
    }
    use std::ffi::c_char;
    use std::time::{Duration, Instant};

    fn set_chars<const N: usize>(target: &mut [c_char; N], value: &str) {
        *target = [0; N];
        for (destination, source) in target.iter_mut().zip(value.as_bytes()) {
            *destination = *source as c_char;
        }
    }

    fn rejoin_snapshot() -> LmuSnapshot {
        let mut snapshot = LmuSnapshot {
            standings_count: 2,
            speed_kph: 100.0,
            track_length: 5_000.0,
            ..LmuSnapshot::default()
        };
        snapshot.standings[0] = LmuStandingEntry {
            vehicle_id: 10,
            position: 1,
            is_player: 1,
            speed_kph: 100.0,
            lap_distance: 1_000.0,
            time_into_lap: 20.0,
            estimated_lap_time: 100.0,
            ..LmuStandingEntry::default()
        };
        snapshot.standings[1] = LmuStandingEntry {
            vehicle_id: 20,
            position: 2,
            speed_kph: 200.0,
            lap_distance: 500.0,
            time_into_lap: 10.0,
            estimated_lap_time: 100.0,
            ..LmuStandingEntry::default()
        };
        snapshot
    }

    #[test]
    fn vehicle_identity_cache_updates_on_driver_swap_and_resets_with_session() {
        let mut source = LmuTelemetrySource::new();
        let mut entry = LmuStandingEntry {
            vehicle_id: 29,
            ..LmuStandingEntry::default()
        };
        set_chars(&mut entry.driver_name, "Primer Piloto");
        set_chars(&mut entry.vehicle_class, "LMGT3");
        set_chars(&mut entry.vehicle_name, "Ferrari 296 #29");

        source.ensure_vehicle_identity(&entry);
        assert_eq!(source.vehicle_identities[&29].driver_name, "Primer Piloto");
        assert_eq!(source.vehicle_identities[&29].fallback_car_number, "29");

        set_chars(&mut entry.driver_name, "Segundo Piloto");
        source.ensure_vehicle_identity(&entry);
        assert_eq!(source.vehicle_identities[&29].driver_name, "Segundo Piloto");

        source.scored_finish_positions.insert(29, 2);
        source.update_session(1);
        assert!(source.vehicle_identities.is_empty());
        assert!(source.scored_finish_positions.is_empty());
    }

    #[test]
    fn maps_each_wheel_compound_and_deduplicates_the_summary() {
        assert_eq!(
            LmuTelemetrySource::tire_compounds(&[0, 1, 2, 3]),
            ["S", "M", "H", "W"]
        );
        assert_eq!(LmuTelemetrySource::tire_compound(&[1, 2, 1, 2]), "M/H");
    }

    #[test]
    fn converts_rank_progress_to_a_continuous_rating_score() {
        assert_eq!(
            LmuTelemetrySource::driver_rank_score("B3", 78.0),
            Some(378.0)
        );
        assert_eq!(
            LmuTelemetrySource::driver_rank_score("S1", 52.0),
            Some(452.0)
        );
        assert_eq!(
            LmuTelemetrySource::driver_rank_score("P3", 100.0),
            Some(1300.0)
        );
        assert_eq!(LmuTelemetrySource::driver_rank_score("", 50.0), None);
    }

    #[test]
    fn scored_class_positions_require_complete_category_coverage() {
        let entries = vec![
            StandingEntry {
                vehicle_id: 1,
                vehicle_class: "GT3".to_owned(),
                position: 1,
                ..StandingEntry::default()
            },
            StandingEntry {
                vehicle_id: 2,
                vehicle_class: "GT3".to_owned(),
                position: 2,
                ..StandingEntry::default()
            },
            StandingEntry {
                vehicle_id: 3,
                vehicle_class: "GT3".to_owned(),
                position: 3,
                ..StandingEntry::default()
            },
        ];

        let partial =
            LmuTelemetrySource::scored_class_positions(&entries, &HashMap::from([(1, 1), (2, 3)]));
        assert!(partial.is_empty());

        let complete = LmuTelemetrySource::scored_class_positions(
            &entries,
            &HashMap::from([(1, 1), (2, 3), (3, 2)]),
        );
        assert_eq!(complete, HashMap::from([(1, 1), (2, 3), (3, 2)]));
    }

    #[test]
    fn driver_rank_estimate_prefers_complete_server_scored_finish_order() {
        let entries = vec![
            StandingEntry {
                vehicle_id: 1,
                position: 1,
                vehicle_class: "GT3".to_owned(),
                driver_rank: "S1".to_owned(),
                driver_rank_progress: 0.0,
                ..StandingEntry::default()
            },
            StandingEntry {
                vehicle_id: 2,
                position: 2,
                vehicle_class: "GT3".to_owned(),
                driver_rank: "S1".to_owned(),
                driver_rank_progress: 0.0,
                is_player: true,
                ..StandingEntry::default()
            },
            StandingEntry {
                vehicle_id: 3,
                position: 3,
                vehicle_class: "GT3".to_owned(),
                driver_rank: "S1".to_owned(),
                driver_rank_progress: 0.0,
                ..StandingEntry::default()
            },
        ];
        let rank_scores = HashMap::from([(1, 400.0), (2, 400.0), (3, 400.0)]);
        let qualifying_positions = HashMap::from([(1, 1), (2, 2), (3, 3)]);
        let settings = DriverRankSettings::default();

        let mut live_entries = entries.clone();
        let live = LmuTelemetrySource::update_driver_rank_estimates(
            &mut live_entries,
            &rank_scores,
            &qualifying_positions,
            &HashMap::new(),
            10,
            settings,
        )
        .unwrap();

        let mut scored_entries = entries;
        let scored = LmuTelemetrySource::update_driver_rank_estimates(
            &mut scored_entries,
            &rank_scores,
            &qualifying_positions,
            &HashMap::from([(1, 1), (2, 3), (3, 2)]),
            10,
            settings,
        )
        .unwrap();

        assert_eq!(live.race_position, 2);
        assert_eq!(scored.live_race_position, 2);
        assert_eq!(scored.race_position, 3);
        assert_eq!(scored.race_position_source, "rest_server_scored");
        assert!(scored.estimated_gain.unwrap() < live.estimated_gain.unwrap());
    }

    #[test]
    fn driver_rank_log_reports_current_rank_without_estimating_before_race() {
        for session_type in [1, 5] {
            let mut entries = vec![StandingEntry {
                vehicle_id: 7,
                position: 3,
                vehicle_class: "LMP2".to_owned(),
                driver_rank: "S1".to_owned(),
                driver_rank_progress: 90.0,
                is_player: true,
                ..StandingEntry::default()
            }];

            let diagnostic = LmuTelemetrySource::update_driver_rank_estimates(
                &mut entries,
                &HashMap::from([(7, 490.0)]),
                &HashMap::new(),
                &HashMap::new(),
                session_type,
                DriverRankSettings::default(),
            )
            .unwrap();

            assert_eq!(diagnostic.status, "current_rank");
            assert_eq!(diagnostic.visual_score, Some(490.0));
            assert_eq!(diagnostic.estimated_gain, None);
            assert!(!entries[0].estimated_driver_rank_gain_available);
        }
    }

    #[test]
    fn driver_rank_estimate_uses_the_observed_internal_scale_and_normalization() {
        let settings = DriverRankSettings::default();
        let expected = LmuTelemetrySource::driver_rank_expected(600.0, 600.0, settings);
        assert_eq!(expected, 0.5);

        let gain =
            LmuTelemetrySource::driver_rank_gain(1.0 - expected, 1.0 - expected, 1, settings);
        assert!((gain - 11.764_705_882_352_94).abs() < 1e-9);
    }

    #[test]
    fn maps_rest_finish_statuses_used_by_the_standings_counter() {
        assert_eq!(
            LmuTelemetrySource::rest_finish_status("FSTAT_FINISHED"),
            Some(1)
        );
        assert_eq!(LmuTelemetrySource::rest_finish_status("FSTAT_DNF"), Some(2));
        assert_eq!(LmuTelemetrySource::rest_finish_status("FSTAT_DQ"), Some(3));
        assert_eq!(LmuTelemetrySource::rest_finish_status("FSTAT_NONE"), None);
    }

    #[test]
    fn standings_history_averages_the_last_five_plausible_completed_laps() {
        let mut history = CarHistory::default();
        let mut entry = LmuStandingEntry {
            best_lap_seconds: 100.0,
            lap_start_elapsed_seconds: 1_000.0,
            elapsed_seconds: 1_002.0,
            ..LmuStandingEntry::default()
        };

        history.update(&entry, 100.0);
        for lap in 1..=6 {
            entry.total_laps = lap;
            entry.last_lap_seconds = 100.0 + f64::from(lap);
            entry.lap_start_elapsed_seconds += entry.last_lap_seconds;
            entry.elapsed_seconds = entry.lap_start_elapsed_seconds + 2.0;
            history.update(&entry, 100.0 - f64::from(lap) * 2.0);
        }

        assert_eq!(history.recent_lap_times.len(), 5);
        assert!((history.average_lap_time() - 104.0).abs() < 0.001);
        assert!((history.average_energy_usage() - 2.0).abs() < 0.001);
    }

    #[test]
    fn out_lap_stays_active_until_the_next_finish_line_crossing() {
        let mut history = CarHistory::default();
        let mut entry = LmuStandingEntry {
            total_laps: 3,
            ..LmuStandingEntry::default()
        };

        history.update(&entry, 50.0);
        entry.in_pits = 1;
        entry.pit_stops = 1;
        history.update(&entry, 50.0);
        entry.in_pits = 0;
        history.update(&entry, 49.0);
        assert!(history.is_out_lap());

        entry.total_laps = 4;
        history.update(&entry, 45.0);
        assert!(!history.is_out_lap());
    }

    #[test]
    fn pit_timer_freezes_on_exit_and_survives_until_out_lap_ends() {
        let started = Instant::now();
        let mut history = CarHistory::default();
        let mut entry = LmuStandingEntry {
            total_laps: 5,
            ..LmuStandingEntry::default()
        };

        history.update_at(&entry, 80.0, started);
        entry.in_pits = 1;
        history.update_at(&entry, 79.0, started + Duration::from_secs(2));
        history.update_at(&entry, 78.0, started + Duration::from_secs(14));
        assert_eq!(history.pit_stop_time_seconds(), Some(12.0));

        entry.pit_stops = 1;
        entry.in_pits = 0;
        history.update_at(&entry, 77.0, started + Duration::from_secs(20));
        assert!(history.is_out_lap());
        assert_eq!(history.pit_stop_time_seconds(), Some(18.0));

        history.update_at(&entry, 76.0, started + Duration::from_secs(30));
        assert_eq!(history.pit_stop_time_seconds(), Some(18.0));

        entry.total_laps = 6;
        history.update_at(&entry, 75.0, started + Duration::from_secs(31));
        assert!(!history.is_out_lap());
        assert_eq!(history.pit_stop_time_seconds(), None);
    }

    #[test]
    fn pit_timer_is_discarded_after_an_uncounted_pit_lane_passage() {
        let started = Instant::now();
        let mut history = CarHistory::default();
        let mut entry = LmuStandingEntry {
            total_laps: 5,
            ..LmuStandingEntry::default()
        };

        history.update_at(&entry, 80.0, started);
        entry.in_pits = 1;
        history.update_at(&entry, 79.0, started + Duration::from_secs(2));
        history.update_at(&entry, 78.0, started + Duration::from_secs(14));
        assert_eq!(history.pit_stop_time_seconds(), Some(12.0));

        entry.in_pits = 0;
        history.update_at(&entry, 77.0, started + Duration::from_secs(20));
        assert!(!history.is_out_lap());
        assert_eq!(history.pit_stop_time_seconds(), None);
    }

    #[test]
    fn pit_timer_accepts_a_counter_increase_after_pit_exit() {
        let started = Instant::now();
        let mut history = CarHistory::default();
        let mut entry = LmuStandingEntry {
            total_laps: 5,
            ..LmuStandingEntry::default()
        };

        history.update_at(&entry, 80.0, started);
        entry.in_pits = 1;
        history.update_at(&entry, 79.0, started + Duration::from_secs(2));
        entry.in_pits = 0;
        history.update_at(&entry, 78.0, started + Duration::from_secs(14));
        assert_eq!(history.pit_stop_time_seconds(), None);

        entry.pit_stops = 1;
        history.update_at(&entry, 77.0, started + Duration::from_secs(15));
        assert!(history.is_out_lap());
        assert_eq!(history.pit_stop_time_seconds(), Some(12.0));
    }

    #[test]
    fn pit_timer_does_not_invent_elapsed_time_when_started_mid_stop() {
        let mut history = CarHistory::default();
        let entry = LmuStandingEntry {
            total_laps: 5,
            in_pits: 1,
            ..LmuStandingEntry::default()
        };

        history.update_at(&entry, 80.0, Instant::now());

        assert_eq!(history.pit_stop_time_seconds(), None);
    }

    #[test]
    fn standings_history_uses_tinypedal_filters_for_invalid_and_pit_laps() {
        let mut history = CarHistory::default();
        let mut entry = LmuStandingEntry {
            best_lap_seconds: 100.0,
            lap_start_elapsed_seconds: 1_000.0,
            elapsed_seconds: 1_002.0,
            ..LmuStandingEntry::default()
        };

        history.update(&entry, 60.0);
        entry.lap_invalidated = 1;
        history.update(&entry, 59.5);
        entry.lap_invalidated = 0;
        entry.total_laps = 1;
        entry.last_lap_seconds = 105.0;
        entry.lap_start_elapsed_seconds = 1_105.0;
        entry.elapsed_seconds = 1_107.0;
        history.update(&entry, 59.0);

        entry.in_pits = 1;
        history.update(&entry, 58.5);
        entry.in_pits = 0;
        entry.total_laps = 2;
        entry.last_lap_seconds = 130.0;
        entry.lap_start_elapsed_seconds = 1_235.0;
        entry.elapsed_seconds = 1_237.0;
        history.update(&entry, 58.0);

        assert_eq!(history.recent_lap_times.len(), 2);
        assert!((history.average_lap_time() - 105.0).abs() < 0.001);
        assert!(history.recent_energy_usage.is_empty());
    }

    #[test]
    fn standings_history_reconstructs_and_keeps_an_invalid_last_lap_time() {
        let mut history = CarHistory::default();
        let mut entry = LmuStandingEntry {
            total_laps: 4,
            last_lap_seconds: 95.0,
            best_lap_seconds: 95.0,
            lap_start_elapsed_seconds: 400.0,
            elapsed_seconds: 402.0,
            ..LmuStandingEntry::default()
        };

        history.update(&entry, 60.0);
        assert!(history.is_last_lap_valid());
        assert_eq!(history.last_lap_seconds(entry.last_lap_seconds), 95.0);

        entry.lap_invalidated = 1;
        history.update(&entry, 59.0);
        entry.total_laps = 5;
        entry.lap_invalidated = 0;
        entry.last_lap_seconds = 0.0;
        entry.lap_start_elapsed_seconds = 418.58;
        entry.elapsed_seconds = 419.0;
        history.update(&entry, 58.0);

        assert!(history.is_last_lap_valid());
        assert_eq!(history.last_lap_seconds(entry.last_lap_seconds), 95.0);

        entry.lap_start_elapsed_seconds = 500.25;
        entry.elapsed_seconds = 500.8;
        history.update(&entry, 58.0);
        assert_eq!(history.last_lap_seconds(entry.last_lap_seconds), 95.0);

        entry.elapsed_seconds = 501.5;
        history.update(&entry, 58.0);

        assert!(!history.is_last_lap_valid());
        assert!((history.last_lap_seconds(entry.last_lap_seconds) - 100.25).abs() < 0.001);
        assert!((history.average_lap_time() - 100.25).abs() < 0.001);
    }

    #[test]
    fn standings_history_uses_negative_official_time_as_invalid_confirmation() {
        let mut history = CarHistory::default();
        let mut entry = LmuStandingEntry {
            total_laps: 4,
            last_lap_seconds: 95.0,
            best_lap_seconds: 95.0,
            lap_start_elapsed_seconds: 400.0,
            elapsed_seconds: 402.0,
            ..LmuStandingEntry::default()
        };

        history.update(&entry, 60.0);
        entry.total_laps = 5;
        entry.last_lap_seconds = -100.25;
        entry.lap_start_elapsed_seconds = 500.25;
        entry.elapsed_seconds = 500.8;
        history.update(&entry, 59.0);

        assert!(!history.is_last_lap_valid());
        assert!((history.last_lap_seconds(entry.last_lap_seconds) - 100.25).abs() < 0.001);
    }

    #[test]
    fn standings_history_ignores_residual_invalid_flag_after_lap_boundary() {
        let mut history = CarHistory::default();
        let mut entry = LmuStandingEntry {
            total_laps: 4,
            last_lap_seconds: 95.0,
            best_lap_seconds: 95.0,
            lap_start_elapsed_seconds: 400.0,
            elapsed_seconds: 402.0,
            ..LmuStandingEntry::default()
        };

        history.update(&entry, 60.0);
        entry.total_laps = 5;
        entry.last_lap_seconds = 100.0;
        entry.lap_start_elapsed_seconds = 500.0;
        entry.elapsed_seconds = 501.5;
        history.update(&entry, 59.0);
        assert!(history.is_last_lap_valid());

        entry.lap_invalidated = 1;
        entry.elapsed_seconds = 502.0;
        history.update(&entry, 58.5);
        entry.lap_invalidated = 0;
        entry.elapsed_seconds = 504.0;
        history.update(&entry, 58.0);

        entry.total_laps = 6;
        entry.last_lap_seconds = 100.0;
        entry.lap_start_elapsed_seconds = 600.0;
        entry.elapsed_seconds = 601.5;
        history.update(&entry, 57.0);

        assert!(history.is_last_lap_valid());
    }

    #[test]
    fn standings_history_rejects_an_impossible_stable_partial_lap() {
        let mut history = CarHistory::default();
        let mut entry = LmuStandingEntry {
            total_laps: 4,
            last_lap_seconds: 95.0,
            best_lap_seconds: 95.0,
            estimated_lap_time: 96.0,
            lap_start_elapsed_seconds: 400.0,
            elapsed_seconds: 402.0,
            ..LmuStandingEntry::default()
        };

        history.update(&entry, 60.0);
        entry.total_laps = 5;
        entry.last_lap_seconds = 0.0;
        entry.lap_start_elapsed_seconds = 409.7;
        entry.elapsed_seconds = 411.0;
        history.update(&entry, 59.0);

        assert!(history.is_last_lap_valid());
        assert_eq!(history.last_lap_seconds(entry.last_lap_seconds), 0.0);
        assert!(history.recent_lap_times.is_empty());
    }

    #[test]
    fn standings_extracts_the_car_number_from_lmu_names() {
        assert_eq!(
            LmuTelemetrySource::car_number("Porsche Penske #6", "Porsche_963_2024"),
            "6"
        );
        assert_eq!(
            LmuTelemetrySource::car_number("Porsche 963", "Porsche_963_006"),
            "006"
        );
    }

    #[test]
    fn standings_uses_rest_for_car_number_pit_state_and_energy() {
        let mut source = LmuTelemetrySource::new();
        source.local_rest.seed_standings(vec![RestStanding {
            slot_id: 20,
            car_number: "29".into(),
            pitstops: 3,
            pit_state: "REQUEST".into(),
            pitting: true,
            ve_fraction: 0.64,
            ..RestStanding::default()
        }]);
        let mut snapshot = LmuSnapshot {
            standings_count: 2,
            track_length: 5_000.0,
            ..LmuSnapshot::default()
        };
        snapshot.standings[0] = LmuStandingEntry {
            vehicle_id: 10,
            position: 1,
            vehicle_class_id: 0,
            total_laps: 4,
            lap_distance: 3_000.0,
            time_into_lap: 60.0,
            estimated_lap_time: 100.0,
            ..LmuStandingEntry::default()
        };
        snapshot.standings[1] = LmuStandingEntry {
            vehicle_id: 20,
            position: 2,
            vehicle_class_id: 0,
            track_limits_steps: 7,
            track_limits_available: 1,
            total_laps: 4,
            lap_distance: 2_500.0,
            time_into_lap: 50.0,
            ..LmuStandingEntry::default()
        };

        let standings = source.standings(&snapshot, &HashSet::new());
        let entry = standings
            .iter()
            .find(|entry| entry.vehicle_id == 20)
            .unwrap();
        assert_eq!(entry.car_number, "29");
        assert_eq!(entry.laps_behind_leader, 0);
        assert!((entry.time_behind_leader - 10.0).abs() < 0.1);
        assert_eq!(entry.laps_behind_next, 0);
        assert!((entry.interval - 10.0).abs() < 0.1);
        assert_eq!(entry.pit_stops, 3);
        assert!(entry.pit_stop_requested);
        assert!(entry.in_pits);
        assert!((entry.virtual_energy_percent - 64.0).abs() < f64::EPSILON);
        assert_eq!(entry.track_limits_steps, Some(7));
        assert_eq!(standings[0].track_limits_steps, None);

        let stable = source.stable_standings(standings);
        assert_eq!(stable.len(), 2);
        let held_during_transient_empty = source.stable_standings(Vec::new());
        assert_eq!(held_during_transient_empty.len(), 2);
    }

    #[test]
    fn rust_snapshot_matches_cpp_bridge_layout_size() {
        assert_eq!(std::mem::size_of::<LmuSnapshot>(), unsafe {
            lmu_snapshot_size()
        });
    }

    #[test]
    fn maps_explicit_tc_and_abs_activation_flags() {
        let mut snapshot = LmuSnapshot {
            tc_active: 1,
            abs_active: 0,
            ..LmuSnapshot::default()
        };
        assert_eq!(LmuTelemetrySource::driver_assists(&snapshot), (true, false));

        snapshot.tc_active = 0;
        snapshot.abs_active = 1;
        assert_eq!(LmuTelemetrySource::driver_assists(&snapshot), (false, true));
    }

    #[test]
    fn converts_steering_fraction_to_game_wheel_degrees_and_clamps_force() {
        let snapshot = LmuSnapshot {
            steering: -0.5,
            steering_range_degrees: 900.0,
            force_feedback: 1.4,
            ..LmuSnapshot::default()
        };
        assert_eq!(
            LmuTelemetrySource::steering_and_force(&snapshot, None),
            (-225.0, 1.0)
        );
        assert_eq!(
            LmuTelemetrySource::steering_and_force(&snapshot, Some(360.0)),
            (-90.0, 1.0)
        );

        let invalid = LmuSnapshot {
            steering: f64::NAN,
            steering_range_degrees: f64::INFINITY,
            force_feedback: f64::NAN,
            ..LmuSnapshot::default()
        };
        assert_eq!(
            LmuTelemetrySource::steering_and_force(&invalid, Some(f64::NAN)),
            (0.0, 0.0)
        );
    }

    #[test]
    fn standings_marks_slow_cars_without_waiting_for_a_sector_yellow() {
        let mut snapshot = LmuSnapshot {
            standings_count: 5,
            yellow_sectors: 0,
            ..LmuSnapshot::default()
        };
        snapshot.standings[0] = LmuStandingEntry {
            vehicle_id: 10,
            speed_kph: 28.79,
            ..LmuStandingEntry::default()
        };
        snapshot.standings[1] = LmuStandingEntry {
            vehicle_id: 20,
            speed_kph: 28.8,
            ..LmuStandingEntry::default()
        };
        snapshot.standings[2] = LmuStandingEntry {
            vehicle_id: 30,
            speed_kph: 0.0,
            in_pits: 1,
            ..LmuStandingEntry::default()
        };
        snapshot.standings[3] = LmuStandingEntry {
            vehicle_id: 40,
            speed_kph: 0.0,
            in_garage: 1,
            ..LmuStandingEntry::default()
        };
        snapshot.standings[4] = LmuStandingEntry {
            vehicle_id: 50,
            speed_kph: 0.0,
            pit_state: 2,
            ..LmuStandingEntry::default()
        };

        let slow = LmuTelemetrySource::slow_yellow_vehicles(&snapshot);
        assert_eq!(slow, HashSet::from([10]));
    }

    #[test]
    fn class_gap_uses_time_into_lap_on_the_same_lap() {
        let ahead = LmuStandingEntry {
            total_laps: 8,
            lap_distance: 3_000.0,
            time_into_lap: 60.0,
            estimated_lap_time: 100.0,
            ..LmuStandingEntry::default()
        };
        let behind = LmuStandingEntry {
            total_laps: 8,
            lap_distance: 2_750.0,
            time_into_lap: 55.0,
            ..LmuStandingEntry::default()
        };
        let (laps, seconds) = LmuTelemetrySource::class_relative_gap(&ahead, &behind, 5_000.0);
        assert_eq!(laps, 0);
        assert!((seconds - 5.0).abs() < 0.001);
    }

    #[test]
    fn class_gap_shows_laps_when_leader_fully_ahead() {
        let ahead = LmuStandingEntry {
            total_laps: 8,
            lap_distance: 4_000.0,
            ..LmuStandingEntry::default()
        };
        let behind = LmuStandingEntry {
            total_laps: 7,
            lap_distance: 4_000.0,
            ..LmuStandingEntry::default()
        };
        let (laps, seconds) = LmuTelemetrySource::class_relative_gap(&ahead, &behind, 5_000.0);
        assert_eq!(laps, 1);
        assert!((seconds - 0.0).abs() < 0.001);
    }

    #[test]
    fn class_gap_stays_seconds_during_finish_line_transition() {
        let ahead = LmuStandingEntry {
            total_laps: 8,
            lap_distance: 100.0,
            time_into_lap: 2.0,
            estimated_lap_time: 100.0,
            ..LmuStandingEntry::default()
        };
        let behind = LmuStandingEntry {
            total_laps: 7,
            lap_distance: 4_900.0,
            time_into_lap: 98.0,
            estimated_lap_time: 100.0,
            ..LmuStandingEntry::default()
        };
        let (laps, seconds) = LmuTelemetrySource::class_relative_gap(&ahead, &behind, 5_000.0);
        assert_eq!(laps, 0);
        assert!((seconds - 4.0).abs() < 0.001);
    }

    #[test]
    fn class_gap_corrects_negative_time_when_on_different_lap() {
        let ahead = LmuStandingEntry {
            total_laps: 8,
            lap_distance: 10.0,
            time_into_lap: 0.5,
            estimated_lap_time: 100.0,
            ..LmuStandingEntry::default()
        };
        let behind = LmuStandingEntry {
            total_laps: 7,
            lap_distance: 4_900.0,
            time_into_lap: 98.0,
            estimated_lap_time: 100.0,
            ..LmuStandingEntry::default()
        };
        let (laps, seconds) = LmuTelemetrySource::class_relative_gap(&ahead, &behind, 5_000.0);
        assert_eq!(laps, 0);
        assert!((seconds - 2.5).abs() < 0.1);
    }

    #[test]
    fn relative_gap_uses_circular_estimated_time_into_lap() {
        let player = LmuStandingEntry {
            vehicle_id: 1,
            time_into_lap: 98.0,
            estimated_lap_time: 100.0,
            ..LmuStandingEntry::default()
        };
        let just_ahead_after_finish = LmuStandingEntry {
            vehicle_id: 2,
            total_laps: 8,
            time_into_lap: 2.0,
            ..LmuStandingEntry::default()
        };
        let just_behind = LmuStandingEntry {
            vehicle_id: 3,
            total_laps: 7,
            time_into_lap: 94.0,
            ..LmuStandingEntry::default()
        };

        assert_eq!(
            LmuTelemetrySource::relative_gaps_seconds(&player, &just_ahead_after_finish),
            (-4.0, 96.0)
        );
        assert_eq!(
            LmuTelemetrySource::relative_gaps_seconds(&player, &just_behind),
            (-96.0, 4.0)
        );
    }

    #[test]
    fn relative_gap_ignores_completed_laps_for_lapped_traffic() {
        let player = LmuStandingEntry {
            vehicle_id: 1,
            total_laps: 10,
            time_into_lap: 50.0,
            estimated_lap_time: 100.0,
            ..LmuStandingEntry::default()
        };
        let lapped_car_ahead = LmuStandingEntry {
            vehicle_id: 2,
            total_laps: 9,
            time_into_lap: 55.0,
            ..LmuStandingEntry::default()
        };

        assert_eq!(
            LmuTelemetrySource::relative_gaps_seconds(&player, &lapped_car_ahead).0,
            -5.0
        );
    }

    #[test]
    fn lap_relation_uses_continuous_progress_across_the_timing_line() {
        let player = LmuStandingEntry {
            vehicle_id: 1,
            total_laps: 8,
            time_into_lap: 98.0,
            estimated_lap_time: 100.0,
            ..LmuStandingEntry::default()
        };
        let just_ahead_after_finish = LmuStandingEntry {
            vehicle_id: 2,
            total_laps: 9,
            time_into_lap: 2.0,
            ..LmuStandingEntry::default()
        };
        assert_eq!(
            LmuTelemetrySource::laps_relative_to_player(&player, &just_ahead_after_finish),
            0
        );

        let player = LmuStandingEntry {
            time_into_lap: 50.0,
            ..player
        };
        let lap_ahead = LmuStandingEntry {
            time_into_lap: 60.0,
            ..just_ahead_after_finish
        };
        assert_eq!(
            LmuTelemetrySource::laps_relative_to_player(&player, &lap_ahead),
            1
        );

        let lap_behind = LmuStandingEntry {
            total_laps: 7,
            time_into_lap: 45.0,
            ..lap_ahead
        };
        assert_eq!(
            LmuTelemetrySource::laps_relative_to_player(&player, &lap_behind),
            -1
        );
    }

    #[test]
    fn relative_omits_cars_in_the_garage() {
        let player = LmuStandingEntry {
            vehicle_id: 1,
            time_into_lap: 50.0,
            estimated_lap_time: 100.0,
            ..LmuStandingEntry::default()
        };
        let garage_car = LmuStandingEntry {
            vehicle_id: 2,
            in_garage: 1,
            time_into_lap: 55.0,
            ..LmuStandingEntry::default()
        };

        assert_eq!(
            LmuTelemetrySource::relative_gaps_seconds(&player, &garage_car),
            (0.0, 0.0)
        );
    }

    #[test]
    fn checkered_flag_has_priority_over_blue_and_yellow() {
        let mut snapshot = LmuSnapshot {
            standings_count: 1,
            game_phase: 5,
            ..LmuSnapshot::default()
        };
        snapshot.standings[0] = LmuStandingEntry {
            vehicle_id: 10,
            position: 1,
            is_player: 1,
            finish_status: 1,
            flag: 6,
            ..LmuStandingEntry::default()
        };
        let yellow_culprits = std::collections::HashSet::from([10]);

        let warning = LmuTelemetrySource::flag_warning(&snapshot, &yellow_culprits);
        assert!(warning.active);
        assert_eq!(warning.kind, "checkered");
    }

    #[test]
    fn blue_flag_selects_nearest_plausible_faster_car_behind() {
        let mut snapshot = LmuSnapshot {
            standings_count: 5,
            game_phase: 5,
            track_length: 5_000.0,
            ..LmuSnapshot::default()
        };
        snapshot.standings[0] = LmuStandingEntry {
            vehicle_id: 10,
            position: 8,
            is_player: 1,
            flag: 6,
            total_laps: 3,
            best_lap_seconds: 100.0,
            lap_distance: 1_000.0,
            ..LmuStandingEntry::default()
        };
        snapshot.standings[1] = LmuStandingEntry {
            vehicle_id: 20,
            position: 7,
            total_laps: 3,
            best_lap_seconds: 100.0,
            lap_distance: 950.0,
            ..LmuStandingEntry::default()
        };
        snapshot.standings[2] = LmuStandingEntry {
            vehicle_id: 30,
            position: 2,
            total_laps: 4,
            best_lap_seconds: 90.0,
            lap_distance: 900.0,
            ..LmuStandingEntry::default()
        };
        snapshot.standings[3] = LmuStandingEntry {
            vehicle_id: 40,
            position: 3,
            total_laps: 3,
            best_lap_seconds: 95.0,
            lap_distance: 800.0,
            ..LmuStandingEntry::default()
        };
        snapshot.standings[4] = LmuStandingEntry {
            vehicle_id: 50,
            position: 1,
            total_laps: 4,
            best_lap_seconds: 89.0,
            lap_distance: 700.0,
            in_pits: 1,
            ..LmuStandingEntry::default()
        };
        set_chars(&mut snapshot.standings[0].vehicle_class, "LMGT3");
        set_chars(&mut snapshot.standings[1].vehicle_class, "LMGT3");
        set_chars(&mut snapshot.standings[2].vehicle_class, "HYPERCAR");
        set_chars(&mut snapshot.standings[3].vehicle_class, "HYPERCAR");
        set_chars(&mut snapshot.standings[4].vehicle_class, "HYPERCAR");

        let warning = LmuTelemetrySource::flag_warning(&snapshot, &HashSet::new());

        assert!(warning.active);
        assert_eq!(warning.kind, "blue");
        assert_eq!(warning.distance_meters, 100.0);
        assert_eq!(warning.car_position, 2);
    }

    #[test]
    fn yellow_overlay_requires_sector_yellow_and_tinypedal_range() {
        let mut snapshot = LmuSnapshot {
            standings_count: 2,
            game_phase: 5,
            track_length: 5_000.0,
            ..LmuSnapshot::default()
        };
        snapshot.standings[0] = LmuStandingEntry {
            vehicle_id: 10,
            position: 1,
            is_player: 1,
            speed_kph: 200.0,
            lap_distance: 1_000.0,
            ..LmuStandingEntry::default()
        };
        snapshot.standings[1] = LmuStandingEntry {
            vehicle_id: 20,
            position: 2,
            speed_kph: 0.0,
            lap_distance: 1_500.0,
            ..LmuStandingEntry::default()
        };
        let slow = HashSet::from([20]);

        let no_sector_yellow = LmuTelemetrySource::flag_warning(&snapshot, &slow);
        assert!(!no_sector_yellow.active);

        snapshot.yellow_sectors = 1;
        snapshot.standings[1].lap_distance = 1_501.0;
        let too_far_ahead = LmuTelemetrySource::flag_warning(&snapshot, &slow);
        assert!(!too_far_ahead.active);

        snapshot.standings[1].lap_distance = 1_500.0;
        let at_ahead_limit = LmuTelemetrySource::flag_warning(&snapshot, &slow);
        assert!(at_ahead_limit.active);
        assert_eq!(at_ahead_limit.kind, "yellow");
        assert_eq!(at_ahead_limit.distance_meters, 500.0);

        snapshot.standings[1].lap_distance = 949.0;
        let too_far_behind = LmuTelemetrySource::flag_warning(&snapshot, &slow);
        assert!(!too_far_behind.active);

        snapshot.standings[1].lap_distance = 950.0;
        let at_behind_limit = LmuTelemetrySource::flag_warning(&snapshot, &slow);
        assert!(at_behind_limit.active);
        assert_eq!(at_behind_limit.kind, "yellow");
        assert_eq!(at_behind_limit.distance_meters, -50.0);
    }

    #[test]
    fn rejoin_risk_uses_both_distance_and_arrival_time() {
        assert_eq!(LmuTelemetrySource::rejoin_safety(80.0, 20.0), "danger");
        assert_eq!(LmuTelemetrySource::rejoin_safety(500.0, 5.0), "danger");
        assert_eq!(LmuTelemetrySource::rejoin_safety(180.0, 20.0), "caution");
        assert_eq!(LmuTelemetrySource::rejoin_safety(500.0, 9.0), "caution");
        assert_eq!(LmuTelemetrySource::rejoin_safety(500.0, 18.0), "safe");
        assert_eq!(
            LmuTelemetrySource::rejoin_safety(50.0, f64::INFINITY),
            "safe"
        );
    }

    #[test]
    fn rejoin_activates_in_pits_and_holds_for_ten_seconds_after_exit() {
        let mut source = LmuTelemetrySource::new();
        let mut snapshot = rejoin_snapshot();
        snapshot.standings[0].in_pits = 1;

        let warning = source.update_rejoin_warning(&snapshot);
        assert!(warning.active);
        assert_eq!(warning.reason, "pit_exit");

        snapshot.standings[0].in_pits = 0;
        for _ in 0..199 {
            assert!(source.update_rejoin_warning(&snapshot).active);
        }
        assert!(!source.update_rejoin_warning(&snapshot).active);
    }

    #[test]
    fn rejoin_activates_for_low_speed_or_four_offroad_wheels_without_a_yellow() {
        let mut low_speed_source = LmuTelemetrySource::new();
        let mut low_speed = rejoin_snapshot();
        low_speed.speed_kph = 20.0;
        assert!(low_speed_source.update_rejoin_warning(&low_speed).active);

        let mut offroad_source = LmuTelemetrySource::new();
        let mut offroad = rejoin_snapshot();
        offroad.player_offroad_wheels = 4;
        assert!(offroad_source.update_rejoin_warning(&offroad).active);
    }

    #[test]
    fn rejoin_hides_traffic_at_or_beyond_fifteen_seconds() {
        let mut source = LmuTelemetrySource::new();
        let mut snapshot = rejoin_snapshot();
        snapshot.speed_kph = 20.0;
        snapshot.standings[1].lap_distance = 100.0;
        snapshot.standings[1].time_into_lap = 5.0;

        assert!(!source.update_rejoin_warning(&snapshot).active);
    }

    #[test]
    fn rejoin_prioritizes_the_rear_car_that_arrives_first() {
        let mut source = LmuTelemetrySource::new();
        let mut snapshot = rejoin_snapshot();
        snapshot.speed_kph = 20.0;
        snapshot.standings_count = 3;
        snapshot.standings[2] = LmuStandingEntry {
            vehicle_id: 30,
            position: 3,
            speed_kph: 90.0,
            lap_distance: 750.0,
            time_into_lap: 15.0,
            estimated_lap_time: 100.0,
            ..LmuStandingEntry::default()
        };

        let warning = source.update_rejoin_warning(&snapshot);
        assert!(warning.active);
        assert_eq!(warning.distance_meters, 500.0);
    }

    #[test]
    fn normalizes_virtual_energy_fraction_and_percentage() {
        assert_eq!(LmuTelemetrySource::virtual_energy_percent(0.75), 75.0);
        assert_eq!(LmuTelemetrySource::virtual_energy_percent(75.0), 75.0);
        assert_eq!(LmuTelemetrySource::virtual_energy_percent(150.0), 100.0);
    }

    #[test]
    fn detects_regulated_classes_from_the_sdk() {
        let mut snapshot = LmuSnapshot {
            vehicle_class_id: 0,
            ..LmuSnapshot::default()
        };
        assert!(LmuTelemetrySource::uses_virtual_energy(&snapshot));
        snapshot.vehicle_class_id = 6;
        assert!(LmuTelemetrySource::uses_virtual_energy(&snapshot));
        snapshot.vehicle_class_id = 3;
        assert!(!LmuTelemetrySource::uses_virtual_energy(&snapshot));
    }

    #[test]
    fn uses_the_actual_initial_energy_when_the_race_starts_below_full() {
        let mut source = LmuTelemetrySource::new();
        let mut snapshot = LmuSnapshot {
            vehicle_class_id: 6,
            lap_number: 1,
            virtual_energy: 0.48,
            ..LmuSnapshot::default()
        };
        source.update_session(10);

        let (initial, _, _) = source.update_energy_estimate(&snapshot, true, false);
        source.update_fuel_estimate(&snapshot, true, false);
        assert!((initial - 48.0).abs() < 0.001);

        snapshot.lap_number = 2;
        snapshot.virtual_energy = 0.44;
        let (current, consumption, autonomy) = source.update_energy_estimate(&snapshot, true, true);

        assert!((current - 44.0).abs() < 0.001);
        assert!((consumption - 4.0).abs() < 0.001);
        assert!((autonomy - 11.0).abs() < 0.001);
    }

    #[test]
    fn qualifying_consumption_uses_the_official_fastest_lap_and_survives_the_race() {
        let mut source = LmuTelemetrySource::new();
        let mut snapshot = LmuSnapshot::default();

        source.update_session(5);
        source.fuel_last_lap = Some(10.0);
        source.energy_last_lap = Some(8.0);
        snapshot.session_type = 5;
        snapshot.last_lap_seconds = 105.0;
        snapshot.best_lap_seconds = 105.0;
        source.update_qualifying_reference(&snapshot, true, true);
        assert_eq!(source.fuel_qualifying_lap, Some(10.0));
        assert_eq!(source.energy_qualifying_lap, Some(8.0));

        // Una vuelta más rápida pero invalidada no cambia mBestLapTime.
        source.fuel_last_lap = Some(11.0);
        source.energy_last_lap = Some(8.5);
        snapshot.last_lap_seconds = 104.0;
        source.update_qualifying_reference(&snapshot, true, true);
        assert_eq!(source.fuel_qualifying_lap, Some(10.0));
        assert_eq!(source.energy_qualifying_lap, Some(8.0));

        // La nueva mejor vuelta oficial conserva su consumo asociado.
        source.fuel_last_lap = Some(12.0);
        source.energy_last_lap = Some(9.0);
        snapshot.last_lap_seconds = 103.0;
        snapshot.best_lap_seconds = 103.0;
        source.update_qualifying_reference(&snapshot, true, true);
        assert_eq!(source.fuel_qualifying_lap, Some(12.0));
        assert_eq!(source.energy_qualifying_lap, Some(9.0));

        source.fuel_per_lap = Some(11.0);
        source.energy_per_lap = Some(8.5);
        source.update_session(10);
        assert_eq!(source.fuel_qualifying_lap, Some(12.0));
        assert_eq!(source.energy_qualifying_lap, Some(9.0));
        assert_eq!(source.fuel_per_lap, None);
        assert_eq!(source.energy_per_lap, None);

        // Las vueltas de carrera nunca sustituyen la referencia de Qualy.
        source.fuel_last_lap = Some(20.0);
        source.energy_last_lap = Some(15.0);
        snapshot.session_type = 10;
        snapshot.last_lap_seconds = 100.0;
        snapshot.best_lap_seconds = 100.0;
        source.update_qualifying_reference(&snapshot, true, true);
        assert_eq!(source.fuel_qualifying_lap, Some(12.0));
        assert_eq!(source.energy_qualifying_lap, Some(9.0));
    }

    #[test]
    fn session_change_resets_average_and_last_but_keeps_qualifying_phases() {
        let mut source = LmuTelemetrySource::new();
        source.update_session(5);
        source.fuel_qualifying_lap = Some(12.0);
        source.energy_qualifying_lap = Some(9.0);
        source.fuel_per_lap = Some(11.0);
        source.fuel_last_lap = Some(11.5);
        source.energy_per_lap = Some(8.5);
        source.energy_last_lap = Some(8.8);

        source.update_session(6);
        assert_eq!(source.fuel_qualifying_lap, Some(12.0));
        assert_eq!(source.energy_qualifying_lap, Some(9.0));
        assert_eq!(source.fuel_per_lap, None);
        assert_eq!(source.fuel_last_lap, None);
        assert_eq!(source.energy_per_lap, None);
        assert_eq!(source.energy_last_lap, None);

        // Una nueva tanda de entrenamientos inicia un evento limpio.
        source.update_session(1);
        assert_eq!(source.fuel_qualifying_lap, None);
        assert_eq!(source.energy_qualifying_lap, None);
    }

    #[test]
    fn pit_lap_consumption_includes_the_resource_added_during_the_lap() {
        let mut source = LmuTelemetrySource::new();
        let mut snapshot = LmuSnapshot {
            vehicle_class_id: 0,
            lap_number: 1,
            fuel_liters: 50.0,
            virtual_energy: 0.60,
            standings_count: 1,
            ..LmuSnapshot::default()
        };
        source.update_session(10);

        snapshot.standings[0].is_player = 1;

        source.update_energy_estimate(&snapshot, true, false);
        source.update_fuel_estimate(&snapshot, true, false);

        // La recarga puede llegar en varios frames mientras el coche está en boxes.
        snapshot.standings[0].in_pits = 1;
        snapshot.fuel_liters = 65.0;
        snapshot.virtual_energy = 0.75;
        source.update_energy_estimate(&snapshot, false, false);
        source.update_fuel_estimate(&snapshot, false, false);

        snapshot.fuel_liters = 70.0;
        snapshot.virtual_energy = 0.90;
        source.update_energy_estimate(&snapshot, false, false);
        source.update_fuel_estimate(&snapshot, false, false);

        // Al completar la vuelta quedan 68 L y 87 %: se consumieron 2 L y 3 %
        // aunque el valor final sea mayor que al comenzar la vuelta.
        snapshot.standings[0].in_pits = 0;
        snapshot.lap_number = 2;
        snapshot.fuel_liters = 68.0;
        snapshot.virtual_energy = 0.87;
        source.update_energy_estimate(&snapshot, true, false);
        source.update_fuel_estimate(&snapshot, true, false);

        assert!((source.fuel_last_lap.unwrap() - 2.0).abs() < 0.001);
        assert!(source.fuel_per_lap.is_none());
        assert!((source.energy_last_lap.unwrap() - 3.0).abs() < 0.001);
        assert!(source.energy_per_lap.is_none());
    }

    #[test]
    fn fixed_lap_race_ends_when_the_leader_reaches_the_target() {
        let snapshot = LmuSnapshot {
            max_laps: 100,
            leader_total_laps: 99,
            leader_lap_time: 120.0,
            leader_time_into_lap: 100.0,
            player_total_laps: 95,
            estimated_lap_time: 180.0,
            player_time_into_lap: 20.0,
            ..LmuSnapshot::default()
        };

        // El líder termina en 20 s. El coche doblado recibe bandera en su próximo cruce.
        assert_eq!(LmuTelemetrySource::laps_remaining(&snapshot), 1.0);
        assert_eq!(LmuTelemetrySource::total_laps_estimated(&snapshot), 100.0);
    }

    #[test]
    fn timed_race_total_laps_falls_back_to_the_overall_leader_without_a_roster() {
        let snapshot = LmuSnapshot {
            max_laps: 10_000,
            session_time_remaining: 100.0,
            leader_total_laps: 42,
            leader_lap_time: 240.0,
            leader_time_into_lap: 230.0,
            estimated_lap_time: 130.0,
            player_time_into_lap: 30.0,
            ..LmuSnapshot::default()
        };

        // El reloj acaba en 100 s y el líder cruza después en t=250.
        // El jugador cruza en t=100, 230 y 360, recibiendo bandera en el tercero.
        assert_eq!(
            LmuTelemetrySource::leader_finish_delay(&snapshot),
            Some(250.0)
        );
        assert_eq!(LmuTelemetrySource::laps_remaining(&snapshot), 3.0);
        assert_eq!(LmuTelemetrySource::total_laps_estimated(&snapshot), 44.0);
    }

    #[test]
    fn multiclass_total_laps_uses_the_player_class_leader() {
        let mut snapshot = LmuSnapshot {
            max_laps: 10_000,
            session_time_remaining: 600.0,
            leader_total_laps: 25,
            leader_lap_time: 90.0,
            leader_time_into_lap: 45.0,
            standings_count: 3,
            ..LmuSnapshot::default()
        };
        snapshot.standings[0] = LmuStandingEntry {
            vehicle_id: 1,
            position: 1,
            total_laps: 25,
            estimated_lap_time: 90.0,
            time_into_lap: 45.0,
            ..LmuStandingEntry::default()
        };
        set_chars(&mut snapshot.standings[0].vehicle_class, "Hypercar");
        snapshot.standings[1] = LmuStandingEntry {
            vehicle_id: 2,
            position: 8,
            total_laps: 20,
            estimated_lap_time: 120.0,
            time_into_lap: 60.0,
            ..LmuStandingEntry::default()
        };
        set_chars(&mut snapshot.standings[1].vehicle_class, "LMGT3");
        snapshot.standings[2] = LmuStandingEntry {
            vehicle_id: 3,
            position: 10,
            total_laps: 20,
            is_player: 1,
            estimated_lap_time: 122.0,
            time_into_lap: 50.0,
            ..LmuStandingEntry::default()
        };
        set_chars(&mut snapshot.standings[2].vehicle_class, "LMGT3");

        // El Hypercar inicia la bandera en t=675, pero las vueltas máximas
        // mostradas pertenecen al líder de LMGT3.
        assert_eq!(
            LmuTelemetrySource::leader_finish_delay(&snapshot),
            Some(675.0)
        );
        assert_eq!(LmuTelemetrySource::total_laps_estimated(&snapshot), 27.0);
    }

    #[test]
    fn standings_remaining_laps_preserves_current_lap_progress() {
        let lap_race = LmuSnapshot {
            max_laps: 100,
            player_total_laps: 95,
            ..LmuSnapshot::default()
        };
        assert_eq!(
            LmuTelemetrySource::estimated_laps_remaining(&lap_race, 0.25, 120.0),
            4.75
        );

        let timed_race = LmuSnapshot {
            max_laps: 10_000,
            session_time_remaining: 300.0,
            ..LmuSnapshot::default()
        };
        assert_eq!(
            LmuTelemetrySource::estimated_laps_remaining(&timed_race, 0.25, 100.0),
            3.75
        );
    }

    #[test]
    fn standings_remaining_laps_uses_the_first_hybrid_finish_criterion() {
        let mut snapshot = LmuSnapshot {
            max_laps: 100,
            player_total_laps: 95,
            leader_total_laps: 99,
            leader_lap_time: 120.0,
            leader_time_into_lap: 100.0,
            session_time_remaining: 300.0,
            ..LmuSnapshot::default()
        };
        assert_eq!(
            LmuTelemetrySource::estimated_laps_remaining(&snapshot, 0.25, 120.0),
            4.75
        );

        snapshot.session_time_remaining = 10.0;
        assert_eq!(
            LmuTelemetrySource::estimated_laps_remaining(&snapshot, 0.25, 120.0),
            0.75
        );
    }

    #[test]
    fn leader_gets_exact_number_of_remaining_crossings_in_lap_race() {
        let snapshot = LmuSnapshot {
            max_laps: 100,
            leader_total_laps: 98,
            player_total_laps: 98,
            leader_lap_time: 120.0,
            leader_time_into_lap: 100.0,
            estimated_lap_time: 120.0,
            player_time_into_lap: 100.0,
            ..LmuSnapshot::default()
        };

        assert_eq!(LmuTelemetrySource::laps_remaining(&snapshot), 2.0);
        assert_eq!(LmuTelemetrySource::total_laps_estimated(&snapshot), 100.0);
    }

    #[test]
    fn session_over_keeps_one_crossing_until_the_player_finishes() {
        let mut snapshot = LmuSnapshot {
            game_phase: 8,
            player_total_laps: 36,
            standings_count: 1,
            ..LmuSnapshot::default()
        };
        snapshot.standings[0].is_player = 1;

        assert_eq!(LmuTelemetrySource::laps_remaining(&snapshot), 1.0);
        assert_eq!(LmuTelemetrySource::total_laps_estimated(&snapshot), 37.0);

        snapshot.player_total_laps = 37;
        snapshot.standings[0].finish_status = 1;
        assert_eq!(LmuTelemetrySource::laps_remaining(&snapshot), 0.0);
        assert_eq!(LmuTelemetrySource::total_laps_estimated(&snapshot), 37.0);
    }
}
