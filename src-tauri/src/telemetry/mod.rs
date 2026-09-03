#[cfg(all(target_os = "windows", lmu_sdk))]
mod consumption_profile;
mod delta_records;
mod dr_estimate_log;
mod fuel_strategy;
mod sim;
mod standings_models;
mod strategy_log;
mod track_geometry;
mod track_map_model;

use serde::{Deserialize, Serialize};
use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicU8, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use std::{thread, time::Duration};
use tauri::{AppHandle, Emitter, Manager};

use fuel_strategy::FuelStrategies;

pub(crate) use delta_records::{
    cycle_mode as cycle_delta_mode, set_settings as set_delta_settings, set_timing_settings,
    DeltaSettings, TimingSettings,
};
pub(crate) use dr_estimate_log::{
    set_enabled as set_driver_rank_estimate_logging, status as driver_rank_estimate_logging_status,
    DriverRankEstimateLoggingStatus,
};
pub(crate) use sim::{
    set_preference as set_simulator_preference, status as simulator_status, SimulatorStatus,
};
pub(crate) use standings_models::{set_overlay_view_settings, OverlayViewSettings};
pub(crate) use strategy_log::{
    set_enabled as set_strategy_logging, status as strategy_logging_status, StrategyLoggingStatus,
};
pub(crate) use track_geometry::{track_map_geometry, TrackMapGeometry};
pub(crate) use track_map_model::{migrate_legacy_track_map_learning, LearnedTrackPoint};

static LOGGING_ENABLED: AtomicBool = AtomicBool::new(false);
static LOGGING_GENERATION: AtomicU64 = AtomicU64::new(0);
static LOGGING_DIRECTORY: OnceLock<PathBuf> = OnceLock::new();
static LOGGING_SETTINGS_PATH: OnceLock<PathBuf> = OnceLock::new();
static ACTIVE_LOG_FILE: Mutex<Option<PathBuf>> = Mutex::new(None);
static ANALYSIS_EVENTS: Mutex<Vec<serde_json::Value>> = Mutex::new(Vec::new());

#[derive(Clone, Deserialize, Serialize)]
struct LoggingPreferences {
    enabled: bool,
}

#[derive(Clone, Serialize)]
pub(crate) struct TelemetryLoggingStatus {
    enabled: bool,
    directory: String,
    active_file: Option<String>,
}

pub(crate) fn configure_logging(app_data_directory: &Path) {
    let directory = app_data_directory.join("telemetry-logs");
    let settings = app_data_directory.join("telemetry-logging.json");
    let enabled = fs::read(&settings)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<LoggingPreferences>(&bytes).ok())
        .map(|preferences| preferences.enabled)
        .unwrap_or(false);
    let _ = LOGGING_DIRECTORY.set(directory);
    let _ = LOGGING_SETTINGS_PATH.set(settings);
    LOGGING_ENABLED.store(enabled, Ordering::Relaxed);
    if enabled {
        LOGGING_GENERATION.fetch_add(1, Ordering::Relaxed);
    }
}

pub(crate) fn telemetry_logging_status() -> TelemetryLoggingStatus {
    let active_file = ACTIVE_LOG_FILE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .as_ref()
        .map(|path| path.display().to_string());
    TelemetryLoggingStatus {
        enabled: LOGGING_ENABLED.load(Ordering::Relaxed),
        directory: LOGGING_DIRECTORY
            .get()
            .map(|path| path.display().to_string())
            .unwrap_or_default(),
        active_file,
    }
}

pub(crate) fn set_telemetry_logging(enabled: bool) -> Result<TelemetryLoggingStatus, String> {
    let was_enabled = LOGGING_ENABLED.swap(enabled, Ordering::Relaxed);
    if enabled && !was_enabled {
        LOGGING_GENERATION.fetch_add(1, Ordering::Relaxed);
    } else if !enabled {
        ANALYSIS_EVENTS
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clear();
    }
    if let Some(path) = LOGGING_SETTINGS_PATH.get() {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        let bytes = serde_json::to_vec_pretty(&LoggingPreferences { enabled })
            .map_err(|error| error.to_string())?;
        fs::write(path, bytes).map_err(|error| error.to_string())?;
    }
    Ok(telemetry_logging_status())
}

fn analysis_logging_generation() -> Option<u64> {
    LOGGING_ENABLED
        .load(Ordering::Relaxed)
        .then(|| LOGGING_GENERATION.load(Ordering::Relaxed))
}

fn queue_analysis_event(event: serde_json::Value) {
    if !LOGGING_ENABLED.load(Ordering::Relaxed) {
        return;
    }
    ANALYSIS_EVENTS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .push(event);
}

pub(crate) fn queue_frontend_performance(mut sample: serde_json::Value) {
    if let Some(object) = sample.as_object_mut() {
        object.insert("event".into(), "frontend_performance_sample".into());
        queue_analysis_event(sample);
    }
}

fn take_analysis_events() -> Vec<serde_json::Value> {
    std::mem::take(
        &mut *ANALYSIS_EVENTS
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()),
    )
}

fn write_analysis_entry(writer: &mut BufWriter<File>, value: &serde_json::Value) -> bool {
    let timestamp_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;
    let mut entry = value.clone();
    if let Some(object) = entry.as_object_mut() {
        object.insert("timestamp_ms".into(), timestamp_ms.into());
    } else {
        entry = serde_json::json!({
            "timestamp_ms": timestamp_ms,
            "data": value,
        });
    }
    serde_json::to_writer(&mut *writer, &entry).is_ok() && writeln!(writer).is_ok()
}

struct AnalysisLogger {
    writer: Option<BufWriter<File>>,
    last_recorded_at: Instant,
    last_flushed_at: Instant,
    last_lap: i32,
    last_in_pits: bool,
    last_connected: bool,
}

impl AnalysisLogger {
    fn new() -> Self {
        let now = Instant::now();
        Self {
            writer: None,
            last_recorded_at: now.checked_sub(Duration::from_secs(1)).unwrap_or(now),
            last_flushed_at: now,
            last_lap: i32::MIN,
            last_in_pits: false,
            last_connected: false,
        }
    }

    fn sync(&mut self) {
        if LOGGING_ENABLED.load(Ordering::Relaxed) {
            if self.writer.is_none() {
                self.open();
            }
        } else if let Some(mut writer) = self.writer.take() {
            let _ = writer.flush();
            *ACTIVE_LOG_FILE
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner()) = None;
        }
    }

    fn open(&mut self) {
        let Some(directory) = LOGGING_DIRECTORY.get() else {
            return;
        };
        if fs::create_dir_all(directory).is_err() {
            return;
        }
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        let path = directory.join(format!("lmu-telemetry-{timestamp}.jsonl"));
        let Ok(file) = File::create(&path) else {
            return;
        };
        self.writer = Some(BufWriter::new(file));
        *ACTIVE_LOG_FILE
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(path);
    }

    fn record(&mut self, frame: &TelemetryFrame) {
        self.sync();
        let Some(writer) = self.writer.as_mut() else {
            return;
        };

        for event in take_analysis_events() {
            let _ = write_analysis_entry(writer, &event);
        }

        let now = Instant::now();
        let state_changed = frame.lap_number != self.last_lap
            || frame.player_in_pits != self.last_in_pits
            || frame.connected != self.last_connected;
        if !frame.player_active && !state_changed {
            return;
        }
        if !state_changed && now.duration_since(self.last_recorded_at) < Duration::from_millis(100)
        {
            return;
        }

        let mut frame_value = serde_json::to_value(frame).unwrap_or_default();
        if let Some(object) = frame_value.as_object_mut() {
            // La clasificación completa se emite al frontend, pero no aporta
            // información al análisis de consumo y multiplicaría el tamaño del log.
            object.remove("standings");
        }
        let _ = write_analysis_entry(writer, &serde_json::json!({ "frame": frame_value }));
        if now.duration_since(self.last_flushed_at) >= Duration::from_secs(1) {
            let _ = writer.flush();
            self.last_flushed_at = now;
        }
        self.last_recorded_at = now;
        self.last_lap = frame.lap_number;
        self.last_in_pits = frame.player_in_pits;
        self.last_connected = frame.connected;
    }
}

struct PerformanceMonitor {
    period_started_at: Instant,
    cycles: u64,
    source_micros: u128,
    source_with_standings_micros: u128,
    source_with_standings_cycles: u64,
    source_without_standings_micros: u128,
    source_without_standings_cycles: u64,
    standings_due_cycles: u64,
    relative_due_cycles: u64,
    standings_requested_cycles: u64,
    logging_micros: u128,
    visibility_micros: u128,
    emission_micros: u128,
    work_micros: u128,
    max_work_micros: u128,
    overruns: u64,
    emitted_delta: u64,
    emitted_timing: u64,
    emitted_stint_history: u64,
    emitted_driving: u64,
    emitted_liftcoast: u64,
    emitted_tires: u64,
    emitted_damage: u64,
    emitted_pitstop: u64,
    emitted_fuel: u64,
    emitted_standings: u64,
    emitted_relative: u64,
    emitted_flags: u64,
    emitted_rejoin: u64,
    emitted_forecast: u64,
    emitted_conditions: u64,
    emitted_dashboard: u64,
    max_standings_rows: usize,
}

impl PerformanceMonitor {
    fn new() -> Self {
        Self {
            period_started_at: Instant::now(),
            cycles: 0,
            source_micros: 0,
            source_with_standings_micros: 0,
            source_with_standings_cycles: 0,
            source_without_standings_micros: 0,
            source_without_standings_cycles: 0,
            standings_due_cycles: 0,
            relative_due_cycles: 0,
            standings_requested_cycles: 0,
            logging_micros: 0,
            visibility_micros: 0,
            emission_micros: 0,
            work_micros: 0,
            max_work_micros: 0,
            overruns: 0,
            emitted_delta: 0,
            emitted_timing: 0,
            emitted_stint_history: 0,
            emitted_driving: 0,
            emitted_liftcoast: 0,
            emitted_tires: 0,
            emitted_damage: 0,
            emitted_pitstop: 0,
            emitted_fuel: 0,
            emitted_standings: 0,
            emitted_relative: 0,
            emitted_flags: 0,
            emitted_rejoin: 0,
            emitted_forecast: 0,
            emitted_conditions: 0,
            emitted_dashboard: 0,
            max_standings_rows: 0,
        }
    }

    fn report_if_due(&mut self) {
        let elapsed = self.period_started_at.elapsed();
        if elapsed < Duration::from_secs(5) || self.cycles == 0 {
            return;
        }
        let cycles = self.cycles as u128;
        queue_analysis_event(serde_json::json!({
            "event": "performance_sample",
            "period_ms": elapsed.as_millis() as u64,
            "cycles": self.cycles,
            "cycle_hz": self.cycles as f64 / elapsed.as_secs_f64(),
            "average_source_us": (self.source_micros / cycles) as u64,
            "standings_source": {
                "due_cycles": self.standings_due_cycles,
                "relative_due_cycles": self.relative_due_cycles,
                "requested_cycles": self.standings_requested_cycles,
                "skipped_cycles": self.cycles.saturating_sub(self.standings_requested_cycles),
                "average_with_standings_us": (self.source_with_standings_micros
                    / self.source_with_standings_cycles.max(1) as u128) as u64,
                "average_without_standings_us": (self.source_without_standings_micros
                    / self.source_without_standings_cycles.max(1) as u128) as u64,
            },
            "average_logging_us": (self.logging_micros / cycles) as u64,
            "average_visibility_us": (self.visibility_micros / cycles) as u64,
            "average_emission_us": (self.emission_micros / cycles) as u64,
            "average_work_us": (self.work_micros / cycles) as u64,
            "max_work_us": self.max_work_micros as u64,
            "overruns": self.overruns,
            "emitted": {
                "delta": self.emitted_delta,
                "timing": self.emitted_timing,
                "stint_history": self.emitted_stint_history,
                "driving": self.emitted_driving,
                "liftcoast": self.emitted_liftcoast,
                "tires": self.emitted_tires,
                "damage": self.emitted_damage,
                "pitstop": self.emitted_pitstop,
                "fuel": self.emitted_fuel,
                "standings": self.emitted_standings,
                "relative": self.emitted_relative,
                "flags": self.emitted_flags,
                "rejoin": self.emitted_rejoin,
                "forecast": self.emitted_forecast,
                "conditions": self.emitted_conditions,
                "dashboard": self.emitted_dashboard,
            },
            "max_standings_rows": self.max_standings_rows,
        }));
        *self = Self::new();
    }
}

/// Every emission decision is evaluated once per source cycle, so a cadence is
/// a whole number of cycles. Deriving the cadences from the cycle counter, with
/// every period an exact multiple of the fastest one, keeps overlay updates on
/// the same compositor frame. Free-running timers drift into neighbouring cycles
/// instead, which makes WebView2 present the transparent host at close to the
/// monitor refresh rate even when each overlay changes far less often.
const fn cycle_due(cycle: u64, period: u64) -> bool {
    period == 0 || cycle % period == 0
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PerformanceProfile {
    Smooth = 0,
    Balanced = 1,
    Efficiency = 2,
}

/// Cadences are counted in source cycles of `SOURCE_INTERVAL` (20 ms). Every
/// period must be an exact multiple of `fast_overlay_cycles` so a cycle that
/// repaints a slow overlay always repaints the fast ones too.
#[derive(Clone, Copy)]
struct PerformanceTuning {
    profile: PerformanceProfile,
    fast_overlay_cycles: u64,
    standings_cycles: u64,
    relative_cycles: u64,
    track_map_cycles: u64,
    secondary_overlay_cycles: u64,
}

static PERFORMANCE_PROFILE: AtomicU8 = AtomicU8::new(PerformanceProfile::Smooth as u8);
static SPECTATOR_MODE: AtomicBool = AtomicBool::new(false);
static TEAM_MODE: AtomicBool = AtomicBool::new(false);

pub fn set_spectator_mode(enabled: bool) {
    SPECTATOR_MODE.store(enabled, Ordering::Relaxed);
    if enabled {
        TEAM_MODE.store(false, Ordering::Relaxed);
    }
}

pub fn set_team_mode(enabled: bool) {
    TEAM_MODE.store(enabled, Ordering::Relaxed);
    if enabled {
        SPECTATOR_MODE.store(false, Ordering::Relaxed);
    }
}

fn spectator_mode() -> bool {
    SPECTATOR_MODE.load(Ordering::Relaxed)
}

fn team_mode() -> bool {
    TEAM_MODE.load(Ordering::Relaxed)
}

fn observer_mode() -> bool {
    spectator_mode() || team_mode()
}

impl PerformanceProfile {
    fn current() -> Self {
        match PERFORMANCE_PROFILE.load(Ordering::Relaxed) {
            1 => Self::Balanced,
            2 => Self::Efficiency,
            _ => Self::Smooth,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Smooth => "smooth",
            Self::Balanced => "balanced",
            Self::Efficiency => "efficiency",
        }
    }

    fn tuning(self) -> PerformanceTuning {
        match self {
            // 20 / 100 / 40 / 40 / 40 ms.
            Self::Smooth => PerformanceTuning {
                profile: self,
                fast_overlay_cycles: 1,
                standings_cycles: 5,
                relative_cycles: 2,
                track_map_cycles: 2,
                secondary_overlay_cycles: 2,
            },
            // 40 / 160 / 80 / 80 / 80 ms.
            Self::Balanced => PerformanceTuning {
                profile: self,
                fast_overlay_cycles: 2,
                standings_cycles: 8,
                relative_cycles: 4,
                track_map_cycles: 4,
                secondary_overlay_cycles: 4,
            },
            // 60 / 240 / 120 / 120 / 120 ms.
            Self::Efficiency => PerformanceTuning {
                profile: self,
                fast_overlay_cycles: 3,
                standings_cycles: 12,
                relative_cycles: 6,
                track_map_cycles: 6,
                secondary_overlay_cycles: 6,
            },
        }
    }
}

pub fn set_performance_profile(profile: &str) -> Result<(), String> {
    let profile = match profile {
        "smooth" => PerformanceProfile::Smooth,
        "balanced" => PerformanceProfile::Balanced,
        "efficiency" => PerformanceProfile::Efficiency,
        _ => return Err("invalid_performance_profile".into()),
    };
    PERFORMANCE_PROFILE.store(profile as u8, Ordering::Relaxed);
    Ok(())
}

#[derive(Clone, Default, Serialize)]
pub struct StandingEntry {
    vehicle_id: i32,
    overall_position: i32,
    position: i32,
    position_change: i32,
    car_number: String,
    driver_name: String,
    driver_rank: String,
    driver_rank_progress: f64,
    estimated_driver_rank_gain: f64,
    estimated_driver_rank_gain_available: bool,
    safety_rank: String,
    safety_rank_progress: f64,
    nationality: String,
    driver_badge: String,
    team_name: String,
    vehicle_name: String,
    vehicle_class: String,
    #[serde(skip)]
    initial_class_count: usize,
    #[serde(skip)]
    laps_relative_to_player: i32,
    total_laps: i32,
    laps_behind_leader: i32,
    laps_behind_next: i32,
    time_behind_leader: f64,
    interval: f64,
    relative_gap_seconds: f64,
    relative_ahead_seconds: f64,
    relative_behind_seconds: f64,
    best_lap_seconds: f64,
    last_lap_seconds: f64,
    average_lap_seconds: f64,
    virtual_energy_active: bool,
    virtual_energy_percent: f64,
    virtual_energy_per_lap: f64,
    damage_percent: f64,
    track_limits_steps: Option<u32>,
    pit_stops: u32,
    pit_stop_requested: bool,
    pit_stop_lap: Option<i32>,
    pit_stop_time_seconds: Option<f64>,
    tire_compound: String,
    tire_compounds: [String; 4],
    flag: u32,
    causing_yellow: bool,
    has_fastest_lap: bool,
    in_pits: bool,
    in_garage: bool,
    is_out_lap: bool,
    last_lap_valid: bool,
    penalty_count: u32,
    finish_status: u32,
    is_player: bool,
}

#[derive(Clone, Serialize)]
pub struct TrackMapVehicle {
    vehicle_id: i32,
    overall_position: i32,
    vehicle_class: String,
    world_x: f64,
    world_y: f64,
    lap_distance: f64,
    total_laps: i32,
    in_pits: bool,
    in_garage: bool,
    causing_yellow: bool,
    #[serde(skip)]
    sector: i32,
    is_player: bool,
}

#[derive(Clone, Serialize)]
pub struct TelemetryFrame {
    source: &'static str,
    source_name: &'static str,
    capabilities: sim::SourceCapabilities,
    performance_profile: &'static str,
    connected: bool,
    #[serde(skip)]
    spectator_mode: bool,
    player_active: bool,
    game_in_foreground: bool,
    game_in_realtime: bool,
    player_in_garage: bool,
    session_type: i32,
    game_phase: u32,
    session_max_laps: i32,
    session_time_remaining: f64,
    session_elapsed_seconds: f64,
    game_time_of_day_seconds: f64,
    session_max_time_seconds: f64,
    leader_total_laps: i32,
    session_split_number: u32,
    session_split_count: u32,
    track_name: String,
    player_vehicle_name: String,
    #[serde(skip)]
    player_vehicle_livery_name: String,
    rest_weather_available: bool,
    ambient_temperature_c: f64,
    track_temperature_c: f64,
    rain_percent: f64,
    track_wetness_percent: f64,
    track_wetness_min_percent: f64,
    track_wetness_max_percent: f64,
    weather_forecast: WeatherForecastModel,
    current_humidity_percent: f64,
    wind_speed_ms: f64,
    wind_direction_degrees: f64,
    wind_relative_direction_degrees: f64,
    player_grip_percent: f64,
    track_rubber_percent: f64,
    track_grip_state: &'static str,
    cloud_coverage: i32,
    lap_number: i32,
    player_sector: i32,
    #[serde(skip)]
    yellow_sectors: u32,
    player_total_laps: i32,
    /// Overall place, and the place inside the player's own class alongside the
    /// size of that class. A multiclass grid is raced on the class one.
    player_position: i32,
    player_class_position: i32,
    player_class_size: i32,
    player_lap_valid: bool,
    player_in_pits: bool,
    speed_kph: f64,
    gear: i8,
    rpm: f64,
    max_rpm: f64,
    throttle: f64,
    brake: f64,
    brake_bias_percent: f64,
    track_limits_steps: u32,
    track_limits_steps_per_penalty: u32,
    tc_active: bool,
    abs_active: bool,
    /// Whether the source publishes the driver-selectable electronics below.
    /// Their neutral value is a real setting, so an unavailable source cannot
    /// be told apart from a car on map 0 without this.
    car_electronics_available: bool,
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
    speed_limiter_active: bool,
    headlights_on: bool,
    wiper_state: u8,
    /// Whether the car carries an electric boost system at all. Hypercars do;
    /// the GT and LMP2 classes sharing the grid do not.
    hybrid_available: bool,
    battery_charge_percent: f64,
    hybrid_regen_kw: f64,
    /// 0 unavailable, 1 inactive, 2 propulsion, 3 regeneration.
    hybrid_motor_state: u8,
    hybrid_motor_temperature_c: f64,
    hybrid_motor_rpm: f64,
    lift_and_coast_progress: u8,
    steering_angle_degrees: f64,
    force_feedback: f64,
    fuel_liters: f64,
    fuel_added_this_lap: f64,
    fuel_capacity_liters: f64,
    fuel_per_lap: f64,
    fuel_last_lap: f64,
    fuel_qualifying_lap: f64,
    fuel_reference_per_lap: f64,
    fuel_projected_lap: f64,
    fuel_pit_cycle_consumption: f64,
    fuel_pit_out_consumption: f64,
    fuel_ratio_assigned: f64,
    fuel_ratio_average: f64,
    fuel_ratio_last: f64,
    estimated_fuel_laps: f64,
    session_laps_remaining: f64,
    session_laps_remaining_estimated: f64,
    session_lap_equivalents_remaining: f64,
    session_total_laps_estimated: f64,
    fuel_needed_liters: f64,
    fuel_to_add_liters: f64,
    virtual_energy_active: bool,
    virtual_energy_percent: f64,
    virtual_energy_raw: f64,
    virtual_energy_added_this_lap: f64,
    virtual_energy_per_lap: f64,
    virtual_energy_last_lap: f64,
    virtual_energy_qualifying_lap: f64,
    virtual_energy_reference_per_lap: f64,
    virtual_energy_projected_lap: f64,
    virtual_energy_pit_cycle_consumption: f64,
    virtual_energy_pit_out_consumption: f64,
    player_pit_out_lap: bool,
    estimated_virtual_energy_laps: f64,
    virtual_energy_needed_percent: f64,
    virtual_energy_next_stint_percent: f64,
    virtual_energy_stints_remaining: u32,
    fuel_strategies: FuelStrategies,
    standings_model: standings_models::StandingsViewModel,
    relative_model: standings_models::RelativeViewModel,
    player_tire_remaining_percent: f64,
    player_damage_percent: f64,
    player_aero_damage_percent: f64,
    player_suspension_damage_percent: f64,
    player_suspension_damage_by_wheel_percent: [f64; 4],
    player_body_damage_percent: f64,
    player_damage_severity: [u8; 8],
    player_engine_overheating: bool,
    player_engine_oil_temperature_c: f64,
    player_engine_water_temperature_c: f64,
    player_part_detached: bool,
    player_rear_wing_detached: bool,
    player_tire_temperature_c: [f64; 4],
    player_tire_temperature_by_zone_c: [[f64; 3]; 4],
    player_brake_temperature_c: [f64; 4],
    player_tire_sliding_fraction: [f64; 4],
    player_tire_remaining_by_wheel_percent: [f64; 4],
    player_tire_pressure_kpa: [f64; 4],
    tire_life_model: Option<TireLifeModel>,
    player_tire_flat_spot_percent: [f64; 4],
    player_tire_compounds: [String; 4],
    /// Optimal tyre temperature the simulator publishes for the compound on
    /// each wheel, or -1 when it is unknown.
    player_tire_optimal_temperature_c: [f64; 4],
    player_tire_flat: [bool; 4],
    player_tire_detached: [bool; 4],
    player_stint: u32,
    player_strategy_pit: bool,
    pit_stop_estimate_available: bool,
    pit_stop_estimate_seconds: f64,
    pit_stop_fuel_seconds: f64,
    pit_stop_energy_seconds: f64,
    pit_stop_tire_seconds: f64,
    pit_stop_damage_seconds: f64,
    pit_stop_penalty_seconds: f64,
    pit_stop_driver_swap_seconds: f64,
    lap_progress: f64,
    track_length_meters: f64,
    track_map_vehicles: Vec<TrackMapVehicle>,
    track_map_model: track_map_model::TrackMapViewModel,
    consumption_profile_samples: u32,
    current_lap_seconds: f64,
    #[serde(skip)]
    current_sector1_seconds: f64,
    #[serde(skip)]
    current_sector2_seconds: f64,
    #[serde(skip)]
    player_best_sector_ends: [f64; 3],
    #[serde(skip)]
    session_best_sector_ends: [f64; 3],
    last_lap_seconds: f64,
    #[serde(skip)]
    last_lap_valid: bool,
    best_lap_seconds: f64,
    lap_delta_seconds: f64,
    delta_model: delta_records::DeltaViewModel,
    timing_model: delta_records::TimingViewModel,
    stint_history_model: delta_records::StintHistoryViewModel,
    flag_warning: FlagWarning,
    rejoin_warning: RejoinWarning,
    standings: Vec<StandingEntry>,
}

#[derive(Clone, Copy, Debug, Serialize)]
struct TireLifeModel {
    wear_per_lap_percent: [f64; 4],
    full_stint_laps: f64,
    remaining_laps: f64,
    remaining_stints: f64,
    projected_remaining_percent: [f64; 3],
}

#[derive(Clone, Serialize)]
pub struct FlagWarning {
    kind: &'static str,
    active: bool,
    distance_meters: f64,
    car_position: i32,
    vehicle_class: String,
}

impl Default for FlagWarning {
    fn default() -> Self {
        Self {
            kind: "green",
            active: false,
            distance_meters: 0.0,
            car_position: 0,
            vehicle_class: String::new(),
        }
    }
}

#[derive(Clone, Serialize)]
pub struct RejoinWarning {
    active: bool,
    reason: &'static str,
    safety: &'static str,
    rear_car_available: bool,
    distance_meters: f64,
    time_to_arrival_seconds: f64,
    car_position: i32,
    vehicle_class: String,
}

impl Default for RejoinWarning {
    fn default() -> Self {
        Self {
            active: false,
            reason: "rejoin",
            safety: "safe",
            rear_car_available: false,
            distance_meters: 0.0,
            time_to_arrival_seconds: 0.0,
            car_position: 0,
            vehicle_class: String::new(),
        }
    }
}

#[derive(Clone, Serialize)]
pub struct WeatherForecastNode {
    sky: i32,
    sky_label: String,
    temperature_c: f64,
    rain_chance_percent: f64,
    humidity_percent: f64,
    minutes_from_now: Option<i32>,
}

#[derive(Clone, Default, Serialize)]
pub struct WeatherForecastModel {
    available: bool,
    session: String,
    current_index: i32,
    next_index: i32,
    nodes: Vec<WeatherForecastNode>,
}

#[derive(Clone, Copy, Default)]
struct TelemetryDemand {
    include_standings: bool,
    include_track_map: bool,
    include_fuel_strategy: bool,
    include_tire_life: bool,
    include_flag_warning: bool,
    include_rejoin_warning: bool,
    include_rest_standings: bool,
    include_rest_supplement: bool,
    include_rest_weather: bool,
}

impl TelemetryFrame {
    pub(crate) fn should_hide_overlays(&self, app_has_focus: bool) -> bool {
        !self.connected
            || !self.player_active
            || self.player_in_garage
            || (!self.spectator_mode && !self.game_in_realtime)
            || self.game_phase == 9
            || (!self.game_in_foreground && !app_has_focus)
    }

    fn waiting_for_simulator(connected: bool) -> Self {
        Self {
            source: sim::active().map_or("none", |descriptor| descriptor.id),
            source_name: sim::active().map_or("", |descriptor| descriptor.display_name),
            capabilities: sim::active().map_or(sim::SourceCapabilities::NONE, |descriptor| {
                descriptor.capabilities
            }),
            performance_profile: "smooth",
            connected,
            spectator_mode: false,
            player_active: false,
            game_in_foreground: false,
            game_in_realtime: false,
            player_in_garage: false,
            session_type: 0,
            game_phase: 0,
            session_max_laps: 0,
            session_time_remaining: 0.0,
            session_elapsed_seconds: 0.0,
            game_time_of_day_seconds: 0.0,
            session_max_time_seconds: 0.0,
            leader_total_laps: 0,
            session_split_number: 0,
            session_split_count: 0,
            track_name: String::new(),
            player_vehicle_name: String::new(),
            player_vehicle_livery_name: String::new(),
            rest_weather_available: false,
            ambient_temperature_c: 0.0,
            track_temperature_c: 0.0,
            rain_percent: 0.0,
            track_wetness_percent: 0.0,
            track_wetness_min_percent: 0.0,
            track_wetness_max_percent: 0.0,
            weather_forecast: WeatherForecastModel::default(),
            current_humidity_percent: 0.0,
            wind_speed_ms: 0.0,
            wind_direction_degrees: 0.0,
            wind_relative_direction_degrees: 0.0,
            player_grip_percent: 0.0,
            track_rubber_percent: 0.0,
            track_grip_state: "dry",
            cloud_coverage: 0,
            lap_number: 0,
            player_sector: 0,
            yellow_sectors: 0,
            player_total_laps: 0,
            player_position: 0,
            player_class_position: 0,
            player_class_size: 0,
            player_lap_valid: false,
            player_in_pits: false,
            speed_kph: 0.0,
            gear: 0,
            rpm: 0.0,
            max_rpm: 1.0,
            throttle: 0.0,
            brake: 0.0,
            brake_bias_percent: 0.0,
            track_limits_steps: 0,
            track_limits_steps_per_penalty: 0,
            tc_active: false,
            abs_active: false,
            car_electronics_available: false,
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
            speed_limiter_active: false,
            headlights_on: false,
            wiper_state: 0,
            hybrid_available: false,
            battery_charge_percent: 0.0,
            hybrid_regen_kw: 0.0,
            hybrid_motor_state: 0,
            hybrid_motor_temperature_c: 0.0,
            hybrid_motor_rpm: 0.0,
            lift_and_coast_progress: 0,
            steering_angle_degrees: 0.0,
            force_feedback: 0.0,
            fuel_liters: 0.0,
            fuel_added_this_lap: 0.0,
            fuel_capacity_liters: 1.0,
            fuel_per_lap: 0.0,
            fuel_last_lap: 0.0,
            fuel_qualifying_lap: 0.0,
            fuel_reference_per_lap: 0.0,
            fuel_projected_lap: 0.0,
            fuel_pit_cycle_consumption: 0.0,
            fuel_pit_out_consumption: 0.0,
            fuel_ratio_assigned: 0.0,
            fuel_ratio_average: 0.0,
            fuel_ratio_last: 0.0,
            estimated_fuel_laps: 0.0,
            session_laps_remaining: 0.0,
            session_laps_remaining_estimated: 0.0,
            session_lap_equivalents_remaining: 0.0,
            session_total_laps_estimated: 0.0,
            fuel_needed_liters: 0.0,
            fuel_to_add_liters: 0.0,
            virtual_energy_active: false,
            virtual_energy_percent: 0.0,
            virtual_energy_raw: 0.0,
            virtual_energy_added_this_lap: 0.0,
            virtual_energy_per_lap: 0.0,
            virtual_energy_last_lap: 0.0,
            virtual_energy_qualifying_lap: 0.0,
            virtual_energy_reference_per_lap: 0.0,
            virtual_energy_projected_lap: 0.0,
            virtual_energy_pit_cycle_consumption: 0.0,
            virtual_energy_pit_out_consumption: 0.0,
            player_pit_out_lap: false,
            estimated_virtual_energy_laps: 0.0,
            virtual_energy_needed_percent: 0.0,
            virtual_energy_next_stint_percent: 0.0,
            virtual_energy_stints_remaining: 0,
            fuel_strategies: FuelStrategies::default(),
            standings_model: standings_models::StandingsViewModel::default(),
            relative_model: standings_models::RelativeViewModel::default(),
            player_tire_remaining_percent: -1.0,
            player_damage_percent: 0.0,
            player_aero_damage_percent: -1.0,
            player_suspension_damage_percent: -1.0,
            player_suspension_damage_by_wheel_percent: [-1.0; 4],
            player_body_damage_percent: 0.0,
            player_damage_severity: [0; 8],
            player_engine_overheating: false,
            player_engine_oil_temperature_c: -1.0,
            player_engine_water_temperature_c: -1.0,
            player_part_detached: false,
            player_rear_wing_detached: false,
            player_tire_temperature_c: [-1.0; 4],
            player_tire_temperature_by_zone_c: [[-1.0; 3]; 4],
            player_brake_temperature_c: [-1.0; 4],
            player_tire_sliding_fraction: [0.0; 4],
            player_tire_remaining_by_wheel_percent: [-1.0; 4],
            player_tire_pressure_kpa: [0.0; 4],
            tire_life_model: None,
            player_tire_flat_spot_percent: [0.0; 4],
            player_tire_compounds: std::array::from_fn(|_| String::new()),
            player_tire_optimal_temperature_c: [-1.0; 4],
            player_tire_flat: [false; 4],
            player_tire_detached: [false; 4],
            player_stint: 0,
            player_strategy_pit: false,
            pit_stop_estimate_available: false,
            pit_stop_estimate_seconds: 0.0,
            pit_stop_fuel_seconds: 0.0,
            pit_stop_energy_seconds: 0.0,
            pit_stop_tire_seconds: 0.0,
            pit_stop_damage_seconds: 0.0,
            pit_stop_penalty_seconds: 0.0,
            pit_stop_driver_swap_seconds: 0.0,
            lap_progress: 0.0,
            track_length_meters: 0.0,
            track_map_vehicles: Vec::new(),
            track_map_model: track_map_model::TrackMapViewModel::default(),
            consumption_profile_samples: 0,
            current_lap_seconds: 0.0,
            current_sector1_seconds: 0.0,
            current_sector2_seconds: 0.0,
            player_best_sector_ends: [0.0; 3],
            session_best_sector_ends: [0.0; 3],
            last_lap_seconds: 0.0,
            last_lap_valid: true,
            best_lap_seconds: 0.0,
            lap_delta_seconds: 0.0,
            delta_model: delta_records::DeltaViewModel::default(),
            timing_model: delta_records::TimingViewModel::default(),
            stint_history_model: delta_records::StintHistoryViewModel::default(),
            flag_warning: FlagWarning::default(),
            rejoin_warning: RejoinWarning::default(),
            standings: Vec::new(),
        }
    }
}

pub fn spawn_source(app: AppHandle) {
    let app_data_directory = crate::app_paths::data_directory();
    configure_logging(&app_data_directory);
    dr_estimate_log::configure(&app_data_directory);
    strategy_log::configure(&app_data_directory);
    track_map_model::configure_track_map_storage(&app_data_directory);
    let mut source = sim::detect(&app_data_directory);
    thread::spawn(move || {
        const SOURCE_INTERVAL: Duration = Duration::from_millis(20);
        // Fixed cadences in source cycles. Each one is a multiple of every
        // profile's `fast_overlay_cycles` (1, 2 and 3) so they land on cycles
        // that already repaint the fast overlays.
        const WEATHER_CYCLES: u64 = 24;
        const IDLE_WARNING_CYCLES: u64 = 12;
        const VISIBILITY_CYCLES: u64 = 12;
        const CONTROL_CYCLES: u64 = 24;

        let mut analysis_logger = AnalysisLogger::new();
        let mut driver_rank_estimate_logger = dr_estimate_log::DriverRankEstimateLogger::new();
        let mut strategy_logger = strategy_log::StrategyLogger::new();
        let mut performance = PerformanceMonitor::new();
        let mut track_map_model = track_map_model::TrackMapModelState::default();
        let mut delta_engine = delta_records::DeltaEngine::new(app_data_directory.clone());
        let mut cycle: u64 = 0;

        loop {
            if app.get_webview_window("control").is_none() {
                break;
            }
            let cycle_started = Instant::now();
            let tuning = PerformanceProfile::current().tuning();
            let standings_due = cycle_due(cycle, tuning.standings_cycles);
            let relative_due = cycle_due(cycle, tuning.relative_cycles);
            let track_map_due = cycle_due(cycle, tuning.track_map_cycles);
            let standings_visible = super::overlay_is_active(&app, "standings");
            let relative_visible = super::overlay_is_active(&app, "relative");
            let track_map_visible = super::overlay_is_active(&app, "trackmap");
            let browser_standings = crate::browser_source::overlay_has_clients("standings");
            let browser_relative = crate::browser_source::overlay_has_clients("relative");
            let browser_track_map = crate::browser_source::overlay_has_clients("trackmap");
            let standings_requested = (standings_due
                && (standings_visible
                    || browser_standings
                    || browser_relative
                    || dr_estimate_log::enabled()))
                || (relative_due && relative_visible);
            let source_started = Instant::now();
            let track_map_requested =
                (track_map_due && track_map_visible) || (standings_due && browser_track_map);
            let fuel_requested = super::overlay_is_active(&app, "fuel")
                || (standings_due && crate::browser_source::overlay_has_clients("fuel"));
            let tire_life_requested = super::overlay_is_active(&app, "tires")
                || (standings_due && crate::browser_source::overlay_has_clients("tires"));
            let flags_requested = super::overlay_is_active(&app, "flags")
                || (standings_due && crate::browser_source::overlay_has_clients("flags"));
            let rejoin_requested = super::overlay_is_active(&app, "rejoin")
                || (standings_due && crate::browser_source::overlay_has_clients("rejoin"));
            let overlay_requested = |label| {
                super::overlay_is_active(&app, label)
                    || crate::browser_source::overlay_has_clients(label)
            };
            let rest_supplement_requested = team_mode()
                || [
                    "standings",
                    "relative",
                    "fuel",
                    "tires",
                    "driving",
                    "damage",
                    "pitstop",
                    "trackmap",
                ]
                .into_iter()
                .any(overlay_requested);
            let rest_standings_requested = standings_visible
                || relative_visible
                || browser_standings
                || browser_relative
                || dr_estimate_log::enabled()
                || spectator_mode();
            let rest_weather_requested =
                overlay_requested("forecast") || overlay_requested("conditions");
            let mut frame = source.next_frame(TelemetryDemand {
                include_standings: standings_requested,
                include_track_map: track_map_requested,
                include_fuel_strategy: fuel_requested,
                include_tire_life: tire_life_requested,
                include_flag_warning: flags_requested,
                include_rejoin_warning: rejoin_requested,
                include_rest_standings: rest_standings_requested,
                include_rest_supplement: rest_supplement_requested,
                include_rest_weather: rest_weather_requested,
            });
            frame.spectator_mode = observer_mode();
            frame.performance_profile = tuning.profile.name();
            // The Dashboard draws the delta and the lap times, and both view
            // models are only built for the overlays that ask for them.
            let dashboard_requested = super::overlay_is_active(&app, "dashboard")
                || (standings_due && crate::browser_source::overlay_has_clients("dashboard"));
            let delta_requested = super::overlay_is_active(&app, "delta")
                || (standings_due && crate::browser_source::overlay_has_clients("delta"))
                || dashboard_requested;
            let timing_requested = super::overlay_is_active(&app, "timing")
                || (standings_due && crate::browser_source::overlay_has_clients("timing"))
                || dashboard_requested;
            let stint_history_requested = super::overlay_is_active(&app, "stinthistory")
                || crate::browser_source::overlay_has_clients("stinthistory");
            delta_engine.update(
                &mut frame,
                delta_requested,
                timing_requested,
                stint_history_requested,
            );
            standings_models::prepare_overlay_models(
                &mut frame,
                standings_visible || browser_standings,
                relative_visible || browser_relative,
            );
            if track_map_requested {
                track_map_model.update(&mut frame);
            }
            let source_elapsed = source_started.elapsed();
            let standings_rows = frame.standings.len();

            let visibility_started = Instant::now();
            if cycle_due(cycle, VISIBILITY_CYCLES) {
                super::update_overlay_auto_visibility(&app, &frame);
            }
            let visibility_elapsed = visibility_started.elapsed();

            let emission_started = Instant::now();
            let emit_standings = standings_due && standings_visible;
            let emit_relative = relative_due && relative_visible;
            let mut standings_targets = [""; 2];
            let mut standings_target_count = 0;
            if emit_standings {
                standings_targets[standings_target_count] = "standings";
                standings_target_count += 1;
            }
            if emit_relative {
                standings_targets[standings_target_count] = "relative";
                standings_target_count += 1;
            }
            if super::emit_overlay_frames(
                &app,
                &standings_targets[..standings_target_count],
                &frame,
            ) {
                if emit_standings {
                    performance.emitted_standings += 1;
                }
                if emit_relative {
                    performance.emitted_relative += 1;
                }
            }
            if standings_due {
                crate::browser_source::publish_frame(&frame);
            }
            frame.standings.clear();
            if track_map_due && track_map_visible {
                let _ = super::emit_overlay_frames(&app, &["trackmap"], &frame);
            }
            frame.track_map_vehicles.clear();

            let driving_due = cycle_due(cycle, tuning.fast_overlay_cycles);
            let emit_delta = driving_due && super::overlay_is_active(&app, "delta");
            let emit_timing = driving_due && super::overlay_is_active(&app, "timing");
            let emit_stint_history = cycle_due(cycle, IDLE_WARNING_CYCLES)
                && super::overlay_is_active(&app, "stinthistory");
            let emit_driving = driving_due && super::overlay_is_active(&app, "driving");
            let emit_liftcoast = driving_due && super::overlay_is_active(&app, "liftcoast");
            let emit_tires = driving_due && super::overlay_is_active(&app, "tires");
            let secondary_due = cycle_due(cycle, tuning.secondary_overlay_cycles);
            let emit_damage = secondary_due && super::overlay_is_active(&app, "damage");
            let emit_pitstop = secondary_due && super::overlay_is_active(&app, "pitstop");
            let weather_due = cycle_due(cycle, WEATHER_CYCLES);
            let emit_forecast = weather_due && super::overlay_is_active(&app, "forecast");
            let emit_conditions = secondary_due && super::overlay_is_active(&app, "conditions");
            let emit_dashboard = driving_due && super::overlay_is_active(&app, "dashboard");
            let emit_fuel = driving_due && super::overlay_is_active(&app, "fuel");
            // An active warning follows the fast cadence so its response never
            // depends on a separate timer landing between repaint cycles.
            let flag_cycles = if frame.flag_warning.active {
                tuning.fast_overlay_cycles
            } else {
                IDLE_WARNING_CYCLES
            };
            let emit_flags =
                cycle_due(cycle, flag_cycles) && super::overlay_is_active(&app, "flags");
            let rejoin_cycles = if frame.rejoin_warning.active {
                tuning.fast_overlay_cycles
            } else {
                IDLE_WARNING_CYCLES
            };
            let emit_rejoin =
                cycle_due(cycle, rejoin_cycles) && super::overlay_is_active(&app, "rejoin");

            let base_emissions = [
                ("delta", emit_delta),
                ("timing", emit_timing),
                ("stinthistory", emit_stint_history),
                ("driving", emit_driving),
                ("liftcoast", emit_liftcoast),
                ("tires", emit_tires),
                ("damage", emit_damage),
                ("pitstop", emit_pitstop),
                ("fuel", emit_fuel),
                ("flags", emit_flags),
                ("rejoin", emit_rejoin),
                ("forecast", emit_forecast),
                ("conditions", emit_conditions),
                ("dashboard", emit_dashboard),
            ];
            let mut base_targets = [""; 14];
            let mut base_target_count = 0;
            for (label, should_emit) in base_emissions {
                if should_emit {
                    base_targets[base_target_count] = label;
                    base_target_count += 1;
                }
            }
            if super::emit_overlay_frames(&app, &base_targets[..base_target_count], &frame) {
                performance.emitted_delta += u64::from(emit_delta);
                performance.emitted_timing += u64::from(emit_timing);
                performance.emitted_stint_history += u64::from(emit_stint_history);
                performance.emitted_driving += u64::from(emit_driving);
                performance.emitted_liftcoast += u64::from(emit_liftcoast);
                performance.emitted_tires += u64::from(emit_tires);
                performance.emitted_damage += u64::from(emit_damage);
                performance.emitted_pitstop += u64::from(emit_pitstop);
                performance.emitted_fuel += u64::from(emit_fuel);
                performance.emitted_flags += u64::from(emit_flags);
                performance.emitted_rejoin += u64::from(emit_rejoin);
                performance.emitted_forecast += u64::from(emit_forecast);
                performance.emitted_conditions += u64::from(emit_conditions);
                performance.emitted_dashboard += u64::from(emit_dashboard);
            }
            if cycle_due(cycle, CONTROL_CYCLES) {
                let _ = app.emit_to("control", "telemetry://frame", &frame);
            }
            let emission_elapsed = emission_started.elapsed();

            let logging_started = Instant::now();
            analysis_logger.record(&frame);
            driver_rank_estimate_logger.record();
            strategy_logger.record(&frame);
            let logging_elapsed = logging_started.elapsed();
            let work_elapsed = cycle_started.elapsed();

            performance.cycles += 1;
            performance.source_micros += source_elapsed.as_micros();
            if standings_due {
                performance.standings_due_cycles += 1;
            }
            if relative_due {
                performance.relative_due_cycles += 1;
            }
            if standings_requested {
                performance.standings_requested_cycles += 1;
                performance.source_with_standings_cycles += 1;
                performance.source_with_standings_micros += source_elapsed.as_micros();
            } else {
                performance.source_without_standings_cycles += 1;
                performance.source_without_standings_micros += source_elapsed.as_micros();
            }
            performance.logging_micros += logging_elapsed.as_micros();
            performance.visibility_micros += visibility_elapsed.as_micros();
            performance.emission_micros += emission_elapsed.as_micros();
            performance.work_micros += work_elapsed.as_micros();
            performance.max_work_micros = performance.max_work_micros.max(work_elapsed.as_micros());
            performance.max_standings_rows = performance.max_standings_rows.max(standings_rows);
            cycle = cycle.wrapping_add(1);
            if work_elapsed < SOURCE_INTERVAL {
                thread::sleep(SOURCE_INTERVAL - work_elapsed);
            } else {
                performance.overruns += 1;
            }
            performance.report_if_due();
        }
    });
}

#[cfg(test)]
mod tests {
    use super::{
        configure_logging, set_telemetry_logging, AnalysisLogger, PerformanceProfile,
        TelemetryFrame,
    };
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn overlay_visibility_tracks_focus_garage_and_session_state() {
        let mut frame = TelemetryFrame::waiting_for_simulator(true);
        assert!(frame.should_hide_overlays(false));

        frame.player_active = true;
        frame.game_in_foreground = true;
        frame.game_in_realtime = true;
        assert!(!frame.should_hide_overlays(false));

        frame.game_phase = 9;
        assert!(frame.should_hide_overlays(false));
        frame.game_phase = 5;

        frame.game_in_realtime = false;
        assert!(frame.should_hide_overlays(false));
        frame.spectator_mode = true;
        assert!(!frame.should_hide_overlays(false));
        frame.spectator_mode = false;
        frame.game_in_realtime = true;

        frame.game_in_foreground = false;
        assert!(frame.should_hide_overlays(false));
        assert!(!frame.should_hide_overlays(true));

        frame.player_in_garage = true;
        assert!(frame.should_hide_overlays(true));
    }

    #[test]
    fn performance_profiles_reduce_only_delivery_work() {
        let smooth = PerformanceProfile::Smooth.tuning();
        let balanced = PerformanceProfile::Balanced.tuning();
        let efficiency = PerformanceProfile::Efficiency.tuning();

        assert!(smooth.fast_overlay_cycles < balanced.fast_overlay_cycles);
        assert!(balanced.fast_overlay_cycles < efficiency.fast_overlay_cycles);
        assert!(smooth.standings_cycles < balanced.standings_cycles);
        assert!(balanced.standings_cycles < efficiency.standings_cycles);
    }

    /// A cadence that is not a multiple of the fast one repaints the composite
    /// host on a cycle where nothing else changes, which is what pushes WebView2
    /// towards presenting at the monitor refresh rate.
    #[test]
    fn every_profile_cadence_is_a_multiple_of_the_fast_one() {
        for profile in [
            PerformanceProfile::Smooth,
            PerformanceProfile::Balanced,
            PerformanceProfile::Efficiency,
        ] {
            let tuning = profile.tuning();
            assert!(tuning.fast_overlay_cycles > 0, "{}", profile.name());
            for cycles in [
                tuning.standings_cycles,
                tuning.relative_cycles,
                tuning.track_map_cycles,
                tuning.secondary_overlay_cycles,
            ] {
                assert_eq!(cycles % tuning.fast_overlay_cycles, 0, "{}", profile.name());
            }
        }
    }

    #[test]
    fn analysis_logging_can_be_enabled_written_and_disabled() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let app_data = std::env::temp_dir().join(format!(
            "blackrack-overlay-analysis-log-{}-{unique}",
            std::process::id()
        ));
        configure_logging(&app_data);
        set_telemetry_logging(true).unwrap();

        let mut logger = AnalysisLogger::new();
        let mut frame = TelemetryFrame::waiting_for_simulator(true);
        frame.player_active = true;
        frame.lap_number = 7;
        frame.virtual_energy_raw = 0.625;
        frame.lift_and_coast_progress = 4;
        logger.record(&frame);

        set_telemetry_logging(false).unwrap();
        logger.sync();

        let log_directory = app_data.join("telemetry-logs");
        let log_path = fs::read_dir(&log_directory)
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        let contents = fs::read_to_string(log_path).unwrap();
        let entry = contents
            .lines()
            .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
            .find(|entry| entry.get("frame").is_some())
            .expect("el log debe contener una entrada de telemetría");
        assert_eq!(entry["frame"]["lap_number"], 7);
        assert_eq!(entry["frame"]["virtual_energy_raw"], 0.625);
        assert_eq!(entry["frame"]["lift_and_coast_progress"], 4);
        assert!(entry["frame"].get("standings").is_none());

        let _ = fs::remove_dir_all(app_data);
    }
}
