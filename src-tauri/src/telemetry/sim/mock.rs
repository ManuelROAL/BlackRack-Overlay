use std::time::Instant;

use super::{SourceCapabilities, SourceDescriptor, TelemetrySource};
use crate::telemetry::fuel_strategy::{
    calculate_resource_strategy, calculate_stint_targets, FuelStrategies, ResourceStrategyInput,
};
use crate::telemetry::{
    FlagWarning, RejoinWarning, StandingEntry, TelemetryDemand, TelemetryFrame, TireLifeModel,
    TrackMapVehicle, WeatherForecastModel, WeatherForecastNode,
};

/// Stands in for a simulator in builds made without any SDK, so the overlays
/// and the control panel stay developable without a game running.
const DESCRIPTOR: SourceDescriptor = SourceDescriptor {
    id: "mock",
    display_name: "Simulated telemetry",
    capabilities: SourceCapabilities {
        official_track_map: false,
        virtual_energy: true,
        opponent_fuel: true,
        opponent_tires: true,
        damage_detail: true,
        tire_temperatures: true,
        brake_temperatures: true,
        weather_forecast: true,
        track_grip: true,
        driver_ranks: true,
        track_limits: true,
        pit_service_estimate: true,
        lift_and_coast: true,
        car_electronics: true,
        session_splits: true,
    },
    official_geometry: None,
    dependency: None,
};

pub struct MockTelemetrySource {
    started_at: Instant,
}

impl MockTelemetrySource {
    pub fn new() -> Self {
        Self {
            started_at: Instant::now(),
        }
    }
}

impl TelemetrySource for MockTelemetrySource {
    fn descriptor(&self) -> SourceDescriptor {
        DESCRIPTOR
    }

    fn next_frame(&mut self, demand: TelemetryDemand) -> TelemetryFrame {
        let TelemetryDemand {
            include_standings,
            include_track_map,
            include_fuel_strategy,
            include_tire_life,
            include_flag_warning,
            include_rejoin_warning,
            ..
        } = demand;
        let elapsed = self.started_at.elapsed().as_secs_f64();
        let throttle = ((elapsed * 0.72).sin() * 0.48 + 0.52).clamp(0.0, 1.0);
        let brake = (((elapsed * 0.39).sin() - 0.58) * 2.1).clamp(0.0, 1.0);
        let speed_kph = (72.0 + throttle * 245.0 - brake * 105.0).clamp(0.0, 340.0);
        let gear = match speed_kph {
            speed if speed < 5.0 => 0,
            speed if speed < 75.0 => 1,
            speed if speed < 120.0 => 2,
            speed if speed < 165.0 => 3,
            speed if speed < 210.0 => 4,
            speed if speed < 258.0 => 5,
            speed if speed < 302.0 => 6,
            _ => 7,
        };
        let rpm = if gear == 0 {
            1_100.0
        } else {
            4_250.0 + (speed_kph % 62.0) * 86.0
        }
        .clamp(1_000.0, 9_200.0);
        let current_lap_seconds = elapsed % 215.0;
        let fuel_liters = (62.0 - elapsed / 145.0).max(0.0);
        let fuel_per_lap = 11.9;
        let virtual_energy_percent = (86.0 - (elapsed % 210.0) / 210.0 * 8.4).max(0.0);
        let virtual_energy_per_lap = 8.4;
        let session_laps_remaining = 5.0;
        let lap_progress = current_lap_seconds / 215.0;
        let laps_remaining = session_laps_remaining - lap_progress;
        let completed_laps = (elapsed / 215.0).floor() as i32;
        let fuel_strategies = if include_fuel_strategy {
            let strategy_input =
                |current, capacity, consumption, pit_cycle, pit_out| ResourceStrategyInput {
                    current,
                    capacity,
                    consumption,
                    laps_remaining,
                    lap_progress,
                    completed_laps,
                    pit_cycle_consumption: pit_cycle,
                    pit_out_consumption: pit_out,
                    pit_out_lap: false,
                    pit_requested: false,
                };
            let fuel_strategy = calculate_resource_strategy(
                strategy_input(fuel_liters, 90.0, 12.1, 20.6, 10.1),
                215.0,
                0,
            );
            let active_strategy = calculate_resource_strategy(
                strategy_input(virtual_energy_percent, 100.0, 8.5, 14.4, 7.0),
                215.0,
                fuel_strategy.map_or(0, |strategy| strategy.stops),
            );
            let minimum_stops = active_strategy.map_or(0, |strategy| strategy.stops);
            let energy_strategy = |consumption| {
                calculate_resource_strategy(
                    strategy_input(virtual_energy_percent, 100.0, consumption, 14.4, 7.0),
                    215.0,
                    minimum_stops,
                )
            };
            let stint_targets = calculate_stint_targets(
                strategy_input(
                    virtual_energy_percent,
                    100.0,
                    virtual_energy_per_lap,
                    14.4,
                    7.0,
                ),
                215.0,
                minimum_stops,
                virtual_energy_per_lap * 0.35,
                0.2,
                8.8,
                208.4,
                31.7,
                5.2,
                12.0,
                42.0,
            );
            FuelStrategies {
                active: active_strategy,
                fuel: fuel_strategy,
                estimated: energy_strategy(8.5),
                average: energy_strategy(virtual_energy_per_lap),
                qualifying: energy_strategy(8.8),
                last: energy_strategy(8.55),
                stint_targets,
                ..FuelStrategies::default()
            }
            .with_qualifying_guidance()
        } else {
            FuelStrategies::default()
        };

        let mut frame = TelemetryFrame {
            source: DESCRIPTOR.id,
            source_name: DESCRIPTOR.display_name,
            capabilities: DESCRIPTOR.capabilities,
            performance_profile: "smooth",
            connected: true,
            spectator_mode: false,
            player_active: true,
            game_in_foreground: true,
            game_in_realtime: true,
            player_in_garage: false,
            session_type: 10,
            game_phase: 5,
            session_max_laps: 0,
            session_time_remaining: (3_600.0 - elapsed).max(0.0),
            session_elapsed_seconds: elapsed,
            game_time_of_day_seconds: (63_000.0 + elapsed).rem_euclid(86_400.0),
            session_max_time_seconds: 3_600.0,
            leader_total_laps: 18,
            session_split_number: 2,
            session_split_count: 12,
            track_name: "Circuit de la Sarthe".into(),
            player_vehicle_name: "Mock Hypercar".into(),
            player_vehicle_livery_name: "Mock Hypercar #7".into(),
            rest_weather_available: true,
            ambient_temperature_c: 19.4,
            track_temperature_c: 27.8,
            rain_percent: 55.0,
            track_wetness_percent: 45.0,
            track_wetness_min_percent: 4.0,
            track_wetness_max_percent: 22.0,
            weather_forecast: WeatherForecastModel {
                available: true,
                session: "RACE".into(),
                current_index: 1,
                next_index: 2,
                nodes: vec![
                    WeatherForecastNode {
                        sky: 1,
                        sky_label: "Light Cloud".into(),
                        temperature_c: 19.4,
                        rain_chance_percent: 10.0,
                        humidity_percent: 62.0,
                        minutes_from_now: None,
                    },
                    WeatherForecastNode {
                        sky: 2,
                        sky_label: "Partially Cloudy".into(),
                        temperature_c: 19.1,
                        rain_chance_percent: 25.0,
                        humidity_percent: 66.0,
                        minutes_from_now: Some(24),
                    },
                    WeatherForecastNode {
                        sky: 3,
                        sky_label: "Mostly Cloudy".into(),
                        temperature_c: 18.6,
                        rain_chance_percent: 45.0,
                        humidity_percent: 72.0,
                        minutes_from_now: Some(46),
                    },
                    WeatherForecastNode {
                        sky: 4,
                        sky_label: "Overcast".into(),
                        temperature_c: 18.2,
                        rain_chance_percent: 70.0,
                        humidity_percent: 80.0,
                        minutes_from_now: Some(62),
                    },
                    WeatherForecastNode {
                        sky: 9,
                        sky_label: "Overcast and Heavy Rain".into(),
                        temperature_c: 17.9,
                        rain_chance_percent: 85.0,
                        humidity_percent: 88.0,
                        minutes_from_now: Some(78),
                    },
                ],
            },
            current_humidity_percent: 80.0,
            wind_speed_ms: 3.5,
            wind_direction_degrees: 290.0,
            wind_relative_direction_degrees: (elapsed * 18.0).rem_euclid(360.0),
            player_grip_percent: 75.0,
            track_rubber_percent: 56.0,
            track_grip_state: "heavy",
            cloud_coverage: 9,
            lap_number: (elapsed / 215.0).floor() as i32 + 1,
            player_sector: if lap_progress < 1.0 / 3.0 {
                1
            } else if lap_progress < 2.0 / 3.0 {
                2
            } else {
                0
            },
            yellow_sectors: if (elapsed as u64 / 8) % 2 == 0 {
                1 << 2
            } else {
                0
            },
            player_total_laps: completed_laps,
            player_position: 4,
            player_class_position: 2,
            player_class_size: 9,
            player_lap_valid: true,
            player_in_pits: false,
            speed_kph,
            gear,
            rpm,
            max_rpm: 9_200.0,
            throttle,
            brake,
            brake_bias_percent: 56.5,
            track_limits_steps: 4,
            track_limits_steps_per_penalty: 17,
            tc_active: throttle > 0.72 && (elapsed * 7.0).sin() > 0.35,
            abs_active: brake > 0.35 && (elapsed * 9.0).sin() > 0.2,
            lift_and_coast_progress: if (elapsed % 12.0) < 5.0 {
                5 - (elapsed % 6.0).floor() as u8
            } else {
                0
            },
            car_electronics_available: true,
            engine_map: 4 + (elapsed as u8 / 30) % 3,
            engine_map_max: 9,
            traction_control_level: 3,
            traction_control_max: 11,
            traction_control_slip: 5,
            traction_control_slip_max: 11,
            traction_control_cut: 4,
            traction_control_cut_max: 11,
            anti_lock_brakes_level: 2,
            anti_lock_brakes_max: 11,
            brake_migration: 3,
            brake_migration_max: 6,
            front_anti_roll_bar: 5,
            front_anti_roll_bar_max: 11,
            rear_anti_roll_bar: 4,
            rear_anti_roll_bar_max: 11,
            speed_limiter_active: false,
            headlights_on: true,
            wiper_state: 0,
            hybrid_available: true,
            battery_charge_percent: 50.0 + (elapsed * 0.9).sin() * 42.0,
            hybrid_regen_kw: if brake > 0.2 { brake * 145.0 } else { 0.0 },
            hybrid_motor_state: if brake > 0.2 {
                3
            } else if throttle > 0.5 {
                2
            } else {
                1
            },
            hybrid_motor_temperature_c: 62.0 + (elapsed * 0.4).sin() * 6.0,
            hybrid_motor_rpm: throttle * 21_000.0,
            player_tire_pressure_kpa: [158.0, 159.4, 162.1, 161.2],
            steering_angle_degrees: (elapsed * 1.35).sin() * 230.0,
            force_feedback: (elapsed * 4.2).sin() * 0.82,
            fuel_liters,
            fuel_added_this_lap: 0.0,
            fuel_capacity_liters: 90.0,
            fuel_per_lap,
            fuel_last_lap: 12.05,
            fuel_qualifying_lap: 12.3,
            fuel_reference_per_lap: 11.95,
            fuel_projected_lap: 12.1,
            fuel_pit_cycle_consumption: 20.6,
            fuel_pit_out_consumption: 10.1,
            fuel_ratio_assigned: 0.97,
            fuel_ratio_average: fuel_per_lap / virtual_energy_per_lap,
            fuel_ratio_last: 12.05 / 8.55,
            estimated_fuel_laps: fuel_liters / fuel_per_lap,
            session_laps_remaining,
            session_laps_remaining_estimated: session_laps_remaining - 0.35,
            session_lap_equivalents_remaining: laps_remaining,
            session_total_laps_estimated: 24.0,
            fuel_needed_liters: fuel_per_lap * session_laps_remaining,
            fuel_to_add_liters: (fuel_per_lap * session_laps_remaining - fuel_liters).max(0.0),
            virtual_energy_active: true,
            virtual_energy_percent,
            virtual_energy_raw: virtual_energy_percent / 100.0,
            virtual_energy_added_this_lap: 0.0,
            virtual_energy_per_lap,
            virtual_energy_last_lap: 8.55,
            virtual_energy_qualifying_lap: 8.8,
            virtual_energy_reference_per_lap: 8.4,
            virtual_energy_projected_lap: 8.5,
            virtual_energy_pit_cycle_consumption: 14.4,
            virtual_energy_pit_out_consumption: 7.0,
            player_pit_out_lap: false,
            estimated_virtual_energy_laps: virtual_energy_percent / virtual_energy_per_lap,
            virtual_energy_needed_percent: virtual_energy_per_lap * session_laps_remaining,
            virtual_energy_next_stint_percent: (virtual_energy_per_lap * session_laps_remaining
                - virtual_energy_percent)
                .clamp(0.0, 100.0),
            virtual_energy_stints_remaining: ((virtual_energy_per_lap * session_laps_remaining
                - virtual_energy_percent)
                .max(0.0)
                / 100.0)
                .ceil() as u32,
            fuel_strategies,
            standings_model: Default::default(),
            relative_model: Default::default(),
            player_tire_remaining_percent: 83.0,
            player_damage_percent: 0.0,
            player_aero_damage_percent: 4.0,
            player_suspension_damage_percent: 8.0,
            player_suspension_damage_by_wheel_percent: [2.0, 8.0, 4.0, 1.0],
            player_body_damage_percent: 6.0,
            player_damage_severity: [0; 8],
            player_engine_overheating: false,
            player_engine_oil_temperature_c: 101.0,
            player_engine_water_temperature_c: 86.0,
            player_part_detached: false,
            player_rear_wing_detached: false,
            player_tire_temperature_c: [76.2, 83.3, 76.7, 81.1],
            player_tire_temperature_by_zone_c: [
                [72.0, 76.0, 80.0],
                [79.0, 83.0, 87.0],
                [73.0, 77.0, 81.0],
                [77.0, 81.0, 85.0],
            ],
            player_brake_temperature_c: [540.0, 575.0, 420.0, 445.0],
            player_tire_sliding_fraction: [
                0.28 + (elapsed * 1.7).sin() * 0.18,
                0.31 + (elapsed * 1.9 + 0.7).sin() * 0.2,
                0.2 + (elapsed * 1.5 + 1.4).sin() * 0.14,
                0.24 + (elapsed * 1.6 + 2.1).sin() * 0.16,
            ],
            player_tire_remaining_by_wheel_percent: [94.0, 94.0, 95.0, 95.0],
            tire_life_model: include_tire_life.then_some(TireLifeModel {
                wear_per_lap_percent: [1.2, 1.2, 1.1, 1.1],
                full_stint_laps: 32.0,
                remaining_laps: 78.3,
                remaining_stints: 2.4,
                projected_remaining_percent: [55.6, 17.2, 0.0],
            }),
            player_tire_flat_spot_percent: [0.08, 0.0, 0.15, 0.03],
            player_tire_compounds: ["M".into(), "M".into(), "M".into(), "M".into()],
            player_tire_optimal_temperature_c: [89.0; 4],
            player_tire_flat: [false; 4],
            player_tire_detached: [false; 4],
            player_stint: 2,
            player_strategy_pit: false,
            pit_stop_estimate_available: true,
            pit_stop_estimate_seconds: 31.7,
            pit_stop_fuel_seconds: 8.4,
            pit_stop_energy_seconds: 5.2,
            pit_stop_tire_seconds: 12.0,
            pit_stop_damage_seconds: 0.0,
            pit_stop_penalty_seconds: 0.0,
            pit_stop_driver_swap_seconds: 6.1,
            lap_progress,
            track_length_meters: 13_626.0,
            track_map_vehicles: if include_track_map {
                (0..18)
                    .map(|index| {
                        let angle = elapsed * 0.045 + index as f64 / 18.0 * std::f64::consts::TAU;
                        let radius = 1_000.0 + (angle * 3.0).sin() * 230.0;
                        TrackMapVehicle {
                            vehicle_id: index + 1,
                            overall_position: index + 1,
                            vehicle_class: match index % 4 {
                                0 => "HYPERCAR",
                                1 => "LMP2",
                                2 => "LMP3",
                                _ => "LMGT3",
                            }
                            .into(),
                            world_x: angle.cos() * radius,
                            world_y: angle.sin() * radius * 0.62,
                            lap_distance: angle.rem_euclid(std::f64::consts::TAU)
                                / std::f64::consts::TAU
                                * 13_626.0,
                            total_laps: 18,
                            in_pits: false,
                            in_garage: false,
                            causing_yellow: index == 4,
                            sector: match angle.rem_euclid(std::f64::consts::TAU)
                                / std::f64::consts::TAU
                            {
                                progress if progress < 1.0 / 3.0 => 1,
                                progress if progress < 2.0 / 3.0 => 2,
                                _ => 0,
                            },
                            is_player: index == 7,
                        }
                    })
                    .collect()
            } else {
                Vec::new()
            },
            track_map_model: Default::default(),
            consumption_profile_samples: 5,
            current_lap_seconds,
            current_sector1_seconds: 0.0,
            current_sector2_seconds: 0.0,
            player_best_sector_ends: [0.0; 3],
            session_best_sector_ends: [0.0; 3],
            last_lap_seconds: 215.0,
            last_lap_valid: true,
            best_lap_seconds: 208.412,
            lap_delta_seconds: (elapsed * 0.31).sin() * 0.72,
            delta_model: Default::default(),
            timing_model: Default::default(),
            stint_history_model: Default::default(),
            flag_warning: if !include_flag_warning {
                FlagWarning::default()
            } else if (elapsed as u64 / 8) % 2 == 0 {
                FlagWarning {
                    kind: "yellow",
                    active: true,
                    distance_meters: 428.0,
                    car_position: 7,
                    vehicle_class: "LMGT3".into(),
                }
            } else {
                FlagWarning {
                    kind: "blue",
                    active: true,
                    distance_meters: 164.0,
                    car_position: 2,
                    vehicle_class: "HYPERCAR".into(),
                }
            },
            rejoin_warning: if include_rejoin_warning {
                RejoinWarning {
                    active: true,
                    reason: if (elapsed as u64 / 8) % 2 == 0 {
                        "rejoin"
                    } else {
                        "pit_exit"
                    },
                    safety: match (elapsed as u64 / 4) % 3 {
                        0 => "danger",
                        1 => "caution",
                        _ => "safe",
                    },
                    rear_car_available: true,
                    distance_meters: 184.0,
                    time_to_arrival_seconds: 7.4,
                    car_position: 2,
                    vehicle_class: "HYPERCAR".into(),
                }
            } else {
                RejoinWarning::default()
            },
            standings: vec![
                StandingEntry {
                    vehicle_id: 1,
                    overall_position: 1,
                    position: 1,
                    position_change: 1,
                    car_number: "6".into(),
                    driver_name: "A. Martin".into(),
                    driver_rank: "G1".into(),
                    driver_rank_progress: 64.0,
                    estimated_driver_rank_gain: 3.8,
                    estimated_driver_rank_gain_available: true,
                    safety_rank: "P1".into(),
                    safety_rank_progress: 91.0,
                    nationality: "fr".into(),
                    driver_badge: "sr-saint".into(),
                    team_name: "Porsche Penske".into(),
                    vehicle_name: "Porsche 963".into(),
                    vehicle_class: "HYPERCAR".into(),
                    initial_class_count: 3,
                    laps_relative_to_player: 0,
                    total_laps: 18,
                    laps_behind_leader: 0,
                    laps_behind_next: 0,
                    time_behind_leader: 0.0,
                    interval: 0.0,
                    relative_gap_seconds: -2.184,
                    relative_ahead_seconds: -2.184,
                    relative_behind_seconds: 207.816,
                    best_lap_seconds: 208.114,
                    last_lap_seconds: 209.021,
                    last_lap_valid: true,
                    average_lap_seconds: 209.334,
                    virtual_energy_active: true,
                    virtual_energy_percent: 68.2,
                    virtual_energy_per_lap: 8.31,
                    damage_percent: 0.0,
                    track_limits_steps: Some(2),
                    pit_stops: 1,
                    pit_stop_requested: false,
                    pit_stop_lap: Some(8),
                    pit_stop_time_seconds: None,
                    tire_compound: "M/S".into(),
                    tire_compounds: ["M".into(), "M".into(), "S".into(), "M".into()],
                    flag: 0,
                    causing_yellow: false,
                    has_fastest_lap: true,
                    in_pits: false,
                    in_garage: false,
                    is_out_lap: false,
                    penalty_count: 0,
                    finish_status: 0,
                    is_player: false,
                },
                StandingEntry {
                    vehicle_id: 2,
                    overall_position: 2,
                    position: 2,
                    position_change: -1,
                    car_number: "50".into(),
                    driver_name: "YOU".into(),
                    driver_rank: "S1".into(),
                    driver_rank_progress: 52.0,
                    estimated_driver_rank_gain: 6.2,
                    estimated_driver_rank_gain_available: true,
                    safety_rank: "G2".into(),
                    safety_rank_progress: 73.0,
                    nationality: "es".into(),
                    driver_badge: "sr-clean".into(),
                    team_name: "Ferrari AF Corse".into(),
                    vehicle_name: "Ferrari 499P".into(),
                    vehicle_class: "HYPERCAR".into(),
                    initial_class_count: 3,
                    laps_relative_to_player: 0,
                    total_laps: 18,
                    laps_behind_leader: 0,
                    laps_behind_next: 0,
                    time_behind_leader: 2.184,
                    interval: 2.184,
                    relative_gap_seconds: 0.0,
                    relative_ahead_seconds: 0.0,
                    relative_behind_seconds: 0.0,
                    best_lap_seconds: 208.662,
                    last_lap_seconds: 209.418,
                    last_lap_valid: true,
                    average_lap_seconds: 209.507,
                    virtual_energy_active: true,
                    virtual_energy_percent: virtual_energy_percent,
                    virtual_energy_per_lap: virtual_energy_per_lap,
                    damage_percent: 6.0,
                    track_limits_steps: Some(5),
                    pit_stops: 2,
                    pit_stop_requested: true,
                    pit_stop_lap: Some(11),
                    pit_stop_time_seconds: Some(28.7),
                    tire_compound: "M".into(),
                    tire_compounds: ["M".into(), "M".into(), "M".into(), "M".into()],
                    flag: 0,
                    causing_yellow: false,
                    has_fastest_lap: false,
                    in_pits: false,
                    in_garage: false,
                    is_out_lap: true,
                    penalty_count: 0,
                    finish_status: 0,
                    is_player: true,
                },
                StandingEntry {
                    vehicle_id: 3,
                    overall_position: 3,
                    position: 3,
                    position_change: 0,
                    car_number: "7".into(),
                    driver_name: "S. Laurent".into(),
                    driver_rank: "B3".into(),
                    driver_rank_progress: 78.0,
                    estimated_driver_rank_gain: -4.1,
                    estimated_driver_rank_gain_available: true,
                    safety_rank: "S2".into(),
                    safety_rank_progress: 46.0,
                    nationality: "jp".into(),
                    driver_badge: "sr-warning".into(),
                    team_name: "Toyota Gazoo Racing".into(),
                    vehicle_name: "Toyota GR010".into(),
                    vehicle_class: "HYPERCAR".into(),
                    initial_class_count: 3,
                    laps_relative_to_player: 0,
                    total_laps: 18,
                    laps_behind_leader: 0,
                    laps_behind_next: 0,
                    time_behind_leader: 5.731,
                    interval: 3.547,
                    relative_gap_seconds: 3.547,
                    relative_ahead_seconds: -206.453,
                    relative_behind_seconds: 3.547,
                    best_lap_seconds: 209.107,
                    last_lap_seconds: 211.844,
                    last_lap_valid: true,
                    average_lap_seconds: 210.202,
                    virtual_energy_active: true,
                    virtual_energy_percent: 43.1,
                    virtual_energy_per_lap: 8.47,
                    damage_percent: 18.0,
                    track_limits_steps: Some(11),
                    pit_stops: 1,
                    pit_stop_requested: false,
                    pit_stop_lap: Some(7),
                    pit_stop_time_seconds: Some(32.4),
                    tire_compound: "H".into(),
                    tire_compounds: ["H".into(), "H".into(), "H".into(), "H".into()],
                    flag: 6,
                    causing_yellow: false,
                    has_fastest_lap: false,
                    in_pits: true,
                    in_garage: false,
                    is_out_lap: false,
                    penalty_count: 1,
                    finish_status: 0,
                    is_player: false,
                },
            ],
        };
        if !include_standings {
            frame.standings.clear();
        }
        frame
    }
}
