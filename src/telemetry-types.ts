export interface StandingEntry {
  vehicle_id: number;
  overall_position: number;
  position: number;
  position_change: number;
  car_number: string;
  driver_name: string;
    driver_rank: string;
    driver_rank_progress: number;
  estimated_driver_rank_gain: number;
  estimated_driver_rank_gain_available: boolean;
  safety_rank: string;
  safety_rank_progress: number;
  nationality: string;
  driver_badge: string;
  team_name: string;
  vehicle_name: string;
  vehicle_class: string;
  total_laps: number;
  laps_behind_leader: number;
  laps_behind_next: number;
  time_behind_leader: number;
  interval: number;
  relative_gap_seconds: number;
  relative_ahead_seconds: number;
  relative_behind_seconds: number;
  best_lap_seconds: number;
  last_lap_seconds: number;
  average_lap_seconds: number;
  virtual_energy_active: boolean;
  virtual_energy_percent: number;
  virtual_energy_per_lap: number;
  damage_percent: number;
  track_limits_steps: number | null;
  pit_stops: number;
  pit_stop_requested: boolean;
  pit_stop_time_seconds: number | null;
  tire_compound: string;
  tire_compounds: [string, string, string, string];
  flag: number;
  causing_yellow: boolean;
  has_fastest_lap: boolean;
  in_pits: boolean;
  in_garage: boolean;
    is_out_lap: boolean;
    penalty_count: number;
    last_lap_valid: boolean;
    finish_status: number;
  is_player: boolean;
}

export interface TrackMapVehicle {
  vehicle_id: number;
  overall_position: number;
  vehicle_class: string;
  world_x: number;
  world_y: number;
  lap_distance: number;
  total_laps: number;
  in_pits: boolean;
  in_garage: boolean;
  is_player: boolean;
}

export interface TrackMapViewModel {
  cache_key: string;
  geometry_revision: number;
  learned_geometry_available: boolean;
  pit_prediction_lap_distance: number | null;
}

export interface WeatherForecastNode {
  sky: number;
  sky_label: string;
  temperature_c: number;
  rain_chance_percent: number;
  humidity_percent: number;
  minutes_from_now: number | null;
}

export interface WeatherForecastModel {
  available: boolean;
  session: string;
  current_index: number;
  next_index: number;
  nodes: WeatherForecastNode[];
}

export interface DeltaViewModel {
  available: boolean;
  seconds: number;
  mode: import("./delta-settings").DeltaMode;
  reference_seconds: number;
  current_lap_valid: boolean;
  trend: "neutral" | "improving" | "worsening";
  sector_index: number;
  sector_count: number;
  reference_generation: number;
}

export interface TimingSectorView { seconds: number; state: "pending" | "neutral" | "personal" | "overall" | "invalid"; }
export interface TimingLapView { number: number; seconds: number; valid: boolean; state: "normal" | "best" | "invalid"; }
export interface TimingViewModel {
  available: boolean;
  lap_number: number;
  total_laps_estimated: number;
  current_seconds: number;
  last_seconds: number;
  session_personal_best_seconds: number;
  personal_best_seconds: number;
  average_seconds: number;
  optimal_seconds: number;
  estimated_seconds: number;
  active_sector: number;
  sectors: TimingSectorView[];
  history: TimingLapView[];
}

export interface TelemetryFrame {
  source: string;
  performance_profile: "smooth" | "balanced" | "efficiency";
  connected: boolean;
  player_active: boolean;
  game_in_foreground: boolean;
  game_in_realtime: boolean;
  player_in_garage: boolean;
  session_type: number;
  game_phase: number;
  session_max_laps: number;
  session_time_remaining: number;
  session_elapsed_seconds: number;
  session_max_time_seconds: number;
  leader_total_laps: number;
  session_split_number: number;
  session_split_count: number;
  track_name: string;
  player_vehicle_name: string;
  rest_weather_available: boolean;
  ambient_temperature_c: number;
  track_temperature_c: number;
  rain_percent: number;
  track_wetness_percent: number;
  track_wetness_min_percent: number;
  track_wetness_max_percent: number;
  weather_forecast: WeatherForecastModel;
  current_humidity_percent: number;
  wind_speed_ms: number;
  wind_direction_degrees: number;
  player_grip_percent: number;
  track_rubber_percent: number;
  track_grip_state: "dry" | "damp" | "wet" | "heavy" | "saturated";
  cloud_coverage: number;
  lap_number: number;
  player_sector: number;
  player_total_laps: number;
  player_lap_valid: boolean;
  player_in_pits: boolean;
  speed_kph: number;
  gear: number;
  rpm: number;
  max_rpm: number;
  throttle: number;
  brake: number;
  brake_bias_percent: number;
  track_limits_steps: number;
  track_limits_steps_per_penalty: number;
  tc_active: boolean;
  abs_active: boolean;
  steering_angle_degrees: number;
  force_feedback: number;
  fuel_liters: number;
  fuel_added_this_lap: number;
  fuel_capacity_liters: number;
  fuel_per_lap: number;
  fuel_last_lap: number;
  fuel_qualifying_lap: number;
  fuel_reference_per_lap: number;
  fuel_projected_lap: number;
  fuel_pit_cycle_consumption: number;
  fuel_pit_out_consumption: number;
  fuel_ratio_assigned: number;
  fuel_ratio_average: number;
  fuel_ratio_last: number;
  estimated_fuel_laps: number;
  session_laps_remaining: number;
  session_laps_remaining_estimated: number;
  session_lap_equivalents_remaining: number;
  session_total_laps_estimated: number;
  fuel_needed_liters: number;
  fuel_to_add_liters: number;
  virtual_energy_active: boolean;
  virtual_energy_percent: number;
  virtual_energy_raw: number;
  virtual_energy_added_this_lap: number;
  virtual_energy_per_lap: number;
  virtual_energy_last_lap: number;
  virtual_energy_qualifying_lap: number;
  virtual_energy_reference_per_lap: number;
  virtual_energy_projected_lap: number;
  virtual_energy_pit_cycle_consumption: number;
  virtual_energy_pit_out_consumption: number;
  player_pit_out_lap: boolean;
  estimated_virtual_energy_laps: number;
  virtual_energy_needed_percent: number;
  virtual_energy_next_stint_percent: number;
  virtual_energy_stints_remaining: number;
  fuel_strategies: FuelStrategies;
  standings_model: StandingsViewModel;
  relative_model: RelativeViewModel;
  player_tire_remaining_percent: number;
  player_damage_percent: number;
  player_aero_damage_percent: number;
  player_suspension_damage_percent: number;
  player_suspension_damage_by_wheel_percent: [number, number, number, number];
  player_body_damage_percent: number;
  player_damage_severity: [number, number, number, number, number, number, number, number];
  player_part_detached: boolean;
  player_rear_wing_detached: boolean;
  player_tire_temperature_c: [number, number, number, number];
  player_brake_temperature_c: [number, number, number, number];
  player_tire_remaining_by_wheel_percent: [number, number, number, number];
  player_tire_flat_spot_percent: [number, number, number, number];
  player_tire_compounds: [string, string, string, string];
  player_tire_flat: [boolean, boolean, boolean, boolean];
  player_tire_detached: [boolean, boolean, boolean, boolean];
  player_stint: number;
  player_strategy_pit: boolean;
  pit_stop_estimate_available: boolean;
  pit_stop_estimate_seconds: number;
  pit_stop_fuel_seconds: number;
  pit_stop_energy_seconds: number;
  pit_stop_tire_seconds: number;
  pit_stop_damage_seconds: number;
  pit_stop_penalty_seconds: number;
  pit_stop_driver_swap_seconds: number;
  lap_progress: number;
  track_length_meters: number;
  track_map_vehicles: TrackMapVehicle[];
  track_map_model: TrackMapViewModel;
  consumption_profile_samples: number;
  current_lap_seconds: number;
  last_lap_seconds: number;
  best_lap_seconds: number;
  lap_delta_seconds: number;
  delta_model: DeltaViewModel;
  timing_model: TimingViewModel;
  flag_warning: FlagWarning;
  rejoin_warning: RejoinWarning;
  standings: StandingEntry[];
}

export interface ResourceStrategy {
  stops: number;
  target_stops: number;
  target_consumption: number;
  saving_percent: number;
  autonomy: number;
  minutes: number;
  earliest_pit_lap: number;
  latest_pit_lap: number;
  next_fill: number;
  total_additional: number;
  end_remaining: number;
  autonomy_delta: number;
}

export interface FuelStrategies {
  active: ResourceStrategy | null;
  fuel: ResourceStrategy | null;
  estimated: ResourceStrategy | null;
  average: ResourceStrategy | null;
  qualifying: ResourceStrategy | null;
  last: ResourceStrategy | null;
  conservative_next_fill: number;
  conservative_fill_active: boolean;
}

export interface StrengthOfFieldModel {
  label: string;
  resolved_profiles: number;
  total_profiles: number;
}

export interface StandingsClassModel {
  vehicle_class: string;
  display_class: string;
  class_tone: string;
  current_count: number;
  initial_count: number;
  retired_count: number;
  strength_of_field: StrengthOfFieldModel | null;
  visible_vehicle_ids: number[];
}

export interface StandingsViewModel {
  groups: StandingsClassModel[];
}

export interface RelativeRowModel {
  vehicle_id: number;
  relative_gap_seconds: number;
  kind: "ahead" | "player" | "behind";
  lap_relation: "same_lap" | "player_ahead" | "opponent_ahead";
}

export interface RelativeViewModel {
  rows: RelativeRowModel[];
}

export interface FlagWarning {
  kind: "green" | "yellow" | "blue" | "checkered";
  active: boolean;
  distance_meters: number;
  car_position: number;
  vehicle_class: string;
}

export interface RejoinWarning {
  active: boolean;
  reason: "rejoin" | "pit_exit";
  safety: "safe" | "caution" | "danger";
  rear_car_available: boolean;
  distance_meters: number;
  time_to_arrival_seconds: number;
  car_position: number;
  vehicle_class: string;
}

export interface InteractionMode {
  click_through: boolean;
}
