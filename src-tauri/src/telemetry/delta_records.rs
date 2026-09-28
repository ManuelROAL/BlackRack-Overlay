use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use super::TelemetryFrame;

const SAMPLE_DISTANCE_METERS: f64 = 5.0;
const SECTOR_TARGET_METERS: f64 = 250.0;
const MIN_SECTORS: usize = 12;
const MAX_SECTORS: usize = 40;
const DELTA_SMOOTHING_SECONDS: f64 = 0.10;
const DELTA_DISPLAY_LIMIT_SECONDS: f64 = 9.9999;
const DELTA_TREND_INTERVAL: Duration = Duration::from_millis(500);
const MIN_SECTOR_DURATION_SECONDS: f64 = 5.0;
const TIMING_RESULT_FREEZE: Duration = Duration::from_secs(3);
const TIMING_COMPARISON_FREEZE: Duration = Duration::from_secs(15);
const STORE_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[repr(u8)]
pub(crate) enum DeltaMode {
    Off = 0,
    OverallBest = 1,
    OverallOptimalLap = 2,
    OverallOptimalSectors = 3,
    SessionBest = 4,
    SessionOptimalLap = 5,
    SessionOptimalSectors = 6,
    StintBest = 7,
    LastLap = 8,
}

impl DeltaMode {
    fn from_u8(value: u8) -> Self {
        match value {
            0 => Self::Off,
            1 => Self::OverallBest,
            2 => Self::OverallOptimalLap,
            3 => Self::OverallOptimalSectors,
            5 => Self::SessionOptimalLap,
            6 => Self::SessionOptimalSectors,
            7 => Self::StintBest,
            8 => Self::LastLap,
            _ => Self::SessionBest,
        }
    }

    fn is_sector_reset(self) -> bool {
        matches!(
            self,
            Self::OverallOptimalSectors | Self::SessionOptimalSectors
        )
    }

    fn next(self) -> Self {
        match self {
            Self::Off => Self::OverallBest,
            Self::OverallBest => Self::OverallOptimalLap,
            Self::OverallOptimalLap => Self::OverallOptimalSectors,
            Self::OverallOptimalSectors => Self::SessionBest,
            Self::SessionBest => Self::SessionOptimalLap,
            Self::SessionOptimalLap => Self::SessionOptimalSectors,
            Self::SessionOptimalSectors => Self::StintBest,
            Self::StintBest => Self::LastLap,
            Self::LastLap => Self::OverallBest,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DeltaSettings {
    mode: DeltaMode,
    display_range: f64,
}

impl Default for DeltaSettings {
    fn default() -> Self {
        Self {
            mode: DeltaMode::SessionBest,
            display_range: 2.0,
        }
    }
}

static DELTA_MODE: AtomicU8 = AtomicU8::new(DeltaMode::SessionBest as u8);

pub(crate) fn set_settings(settings: DeltaSettings) {
    DELTA_MODE.store(settings.mode as u8, Ordering::Relaxed);
}

pub(crate) fn cycle_mode() -> DeltaMode {
    let mut current = DELTA_MODE.load(Ordering::Relaxed);
    loop {
        let next = DeltaMode::from_u8(current).next() as u8;
        match DELTA_MODE.compare_exchange_weak(current, next, Ordering::Relaxed, Ordering::Relaxed)
        {
            Ok(_) => return DeltaMode::from_u8(next),
            Err(actual) => current = actual,
        }
    }
}

fn active_mode() -> DeltaMode {
    DeltaMode::from_u8(DELTA_MODE.load(Ordering::Relaxed))
}

const STORAGE_REPLY_TIMEOUT: Duration = Duration::from_secs(5);

/// The worker's queue, shared with the control panel so listing and deleting
/// records are ordered against the telemetry thread's own writes.
static STORAGE_SENDER: OnceLock<Sender<StorageCommand>> = OnceLock::new();
/// Identity keys the panel deleted and the engine has not yet forgotten.
static DELETED_KEYS: Mutex<Vec<String>> = Mutex::new(Vec::new());
static DELETIONS_PENDING: AtomicBool = AtomicBool::new(false);

/// One stored track and car combination as the control panel lists it.
#[derive(Clone, Debug, Serialize, PartialEq)]
pub(crate) struct LapRecordSummary {
    key: String,
    track: String,
    vehicle: String,
    track_length_meters: f64,
    best_lap_seconds: Option<f64>,
    optimal_lap_seconds: Option<f64>,
    best_sector_seconds: [Option<f64>; 3],
    updated_unix_ms: u64,
}

fn storage_request<T>(command: impl FnOnce(Sender<T>) -> StorageCommand) -> Result<T, String> {
    let sender = STORAGE_SENDER
        .get()
        .ok_or_else(|| "lap_records_unavailable".to_owned())?;
    let (reply, receiver) = mpsc::channel();
    sender
        .send(command(reply))
        .map_err(|_| "lap_records_unavailable".to_owned())?;
    receiver
        .recv_timeout(STORAGE_REPLY_TIMEOUT)
        .map_err(|_| "lap_records_unavailable".to_owned())
}

pub(crate) fn list_lap_records() -> Result<Vec<LapRecordSummary>, String> {
    storage_request(|reply| StorageCommand::List { reply })?
}

/// Forgets the stored references of one track and car combination. The
/// telemetry thread drops its in-memory copy too, so the next valid lap starts
/// a fresh record instead of writing the deleted one back.
pub(crate) fn delete_lap_record(key: String) -> Result<(), String> {
    storage_request(|reply| StorageCommand::Delete {
        key: key.clone(),
        reply,
    })??;
    if let Ok(mut deleted) = DELETED_KEYS.lock() {
        deleted.push(key);
        DELETIONS_PENDING.store(true, Ordering::Release);
    }
    Ok(())
}

fn take_deleted_keys() -> Vec<String> {
    if !DELETIONS_PENDING.swap(false, Ordering::Acquire) {
        return Vec::new();
    }
    DELETED_KEYS
        .lock()
        .map(|mut deleted| std::mem::take(&mut *deleted))
        .unwrap_or_default()
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[repr(u8)]
pub(crate) enum TimingSectorReference {
    Lmu = 0,
    Session = 1,
    Overall = 2,
}

impl TimingSectorReference {
    fn from_u8(value: u8) -> Self {
        match value {
            1 => Self::Session,
            2 => Self::Overall,
            _ => Self::Lmu,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TimingSettings {
    sector_reference: TimingSectorReference,
}

impl Default for TimingSettings {
    fn default() -> Self {
        Self {
            sector_reference: TimingSectorReference::Lmu,
        }
    }
}

static TIMING_SECTOR_REFERENCE: AtomicU8 = AtomicU8::new(TimingSectorReference::Lmu as u8);

pub(crate) fn set_timing_settings(settings: TimingSettings) {
    TIMING_SECTOR_REFERENCE.store(settings.sector_reference as u8, Ordering::Relaxed);
}

/// LMU/rFactor codifica 0=S3, 1=S1 y 2=S2.
fn active_timing_sector(frame: &TelemetryFrame) -> usize {
    match frame.player_sector {
        2 => 1,
        0 => 2,
        _ => 0,
    }
}

fn active_sector_reference() -> TimingSectorReference {
    TimingSectorReference::from_u8(TIMING_SECTOR_REFERENCE.load(Ordering::Relaxed))
}

fn sector_state(
    reference: TimingSectorReference,
    seconds: f64,
    official_end: f64,
    class_best_end: f64,
    player_best_end: f64,
    overall: Option<f64>,
    session: Option<f64>,
) -> &'static str {
    let matches_official = |current: f64, best: f64| {
        current.is_finite() && best.is_finite() && best > 0.0 && current <= best + 0.000_5
    };
    if matches_official(official_end, class_best_end) {
        return "overall";
    }
    match reference {
        TimingSectorReference::Lmu => {
            if matches_official(official_end, player_best_end) {
                "personal"
            } else {
                "neutral"
            }
        }
        TimingSectorReference::Session => {
            if session.is_none_or(|best| seconds + 0.000_5 < best) {
                "personal"
            } else {
                "neutral"
            }
        }
        TimingSectorReference::Overall => {
            if overall.is_none_or(|best| seconds + 0.000_5 < best) {
                "personal"
            } else {
                "neutral"
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum DeltaTrend {
    #[default]
    Neutral,
    Improving,
    Worsening,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct DeltaViewModel {
    available: bool,
    seconds: f64,
    mode: DeltaMode,
    reference_seconds: f64,
    current_lap_valid: bool,
    trend: DeltaTrend,
    sector_index: usize,
    sector_count: usize,
    reference_generation: u64,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct TimingSectorView {
    seconds: f64,
    state: &'static str,
}

impl Default for TimingSectorView {
    fn default() -> Self {
        Self {
            seconds: 0.0,
            state: "pending",
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct TimingLapView {
    number: i32,
    seconds: f64,
    valid: bool,
    state: &'static str,
}

#[derive(Clone, Debug, Default, Serialize)]
pub(crate) struct TimingComparisons {
    last_seconds: Option<f64>,
    session_personal_best_seconds: Option<f64>,
    personal_best_seconds: Option<f64>,
    average_seconds: Option<f64>,
    optimal_seconds: Option<f64>,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct TimingViewModel {
    available: bool,
    lap_number: i32,
    total_laps_estimated: f64,
    extra_laps_estimated: Option<i32>,
    extra_laps_approximate: bool,
    current_seconds: f64,
    last_seconds: f64,
    last_valid: bool,
    session_personal_best_seconds: f64,
    personal_best_seconds: f64,
    average_seconds: f64,
    optimal_seconds: f64,
    estimated_seconds: f64,
    comparisons: TimingComparisons,
    active_sector: usize,
    sectors: [TimingSectorView; 3],
    history: Vec<TimingLapView>,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct StintHistoryEntryView {
    number: u32,
    current: bool,
    laps: u32,
    time_seconds: f64,
    resource_used: f64,
    battery_start_percent: Option<f64>,
    battery_end_percent: Option<f64>,
    regeneration_kwh: Option<f64>,
    tire_wear_percent: f64,
    tire_compounds: [String; 4],
    delta_seconds: Option<f64>,
    consistency_percent: Option<f64>,
}

#[derive(Clone, Debug, Default, Serialize)]
pub(crate) struct StintHistoryViewModel {
    available: bool,
    uses_virtual_energy: bool,
    hybrid_available: bool,
    entries: Vec<StintHistoryEntryView>,
}

impl Default for TimingViewModel {
    fn default() -> Self {
        Self {
            available: false,
            lap_number: 0,
            total_laps_estimated: 0.0,
            extra_laps_estimated: None,
            extra_laps_approximate: false,
            current_seconds: 0.0,
            last_seconds: 0.0,
            last_valid: true,
            session_personal_best_seconds: 0.0,
            personal_best_seconds: 0.0,
            average_seconds: 0.0,
            optimal_seconds: 0.0,
            estimated_seconds: 0.0,
            comparisons: TimingComparisons::default(),
            active_sector: 0,
            sectors: std::array::from_fn(|_| TimingSectorView::default()),
            history: Vec::new(),
        }
    }
}

impl Default for DeltaViewModel {
    fn default() -> Self {
        Self {
            available: false,
            seconds: 0.0,
            mode: DeltaMode::SessionBest,
            reference_seconds: 0.0,
            current_lap_valid: false,
            trend: DeltaTrend::Neutral,
            sector_index: 0,
            sector_count: 0,
            reference_generation: 0,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct TracePoint {
    distance: f64,
    seconds: f64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct LapTrace {
    lap_time: f64,
    track_length: f64,
    points: Vec<TracePoint>,
    #[serde(default)]
    official_sector_ends: [Option<f64>; 2],
}

#[derive(Clone, Copy, Debug)]
struct DeltaTrendAnchor {
    lap_number: i32,
    mode: DeltaMode,
    reference_generation: u64,
    sector_index: usize,
    observed_at: Instant,
    seconds: f64,
}

impl LapTrace {
    fn time_at(&self, distance: f64) -> Option<f64> {
        interpolate(&self.points, distance.clamp(0.0, self.track_length))
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct SectorTrace {
    duration: f64,
    length: f64,
    points: Vec<TracePoint>,
}

impl SectorTrace {
    fn time_at(&self, distance: f64) -> Option<f64> {
        interpolate(&self.points, distance.clamp(0.0, self.length))
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
struct SectorBank {
    count: usize,
    sectors: Vec<Option<SectorTrace>>,
}

impl SectorBank {
    fn ensure_count(&mut self, count: usize) {
        if self.count != count || self.sectors.len() != count {
            self.count = count;
            self.sectors = vec![None; count];
        }
    }

    fn update(&mut self, traces: &[SectorTrace]) -> bool {
        self.ensure_count(traces.len());
        let mut changed = false;
        for (slot, candidate) in self.sectors.iter_mut().zip(traces) {
            if slot
                .as_ref()
                .is_none_or(|stored| candidate.duration + 0.000_5 < stored.duration)
            {
                *slot = Some(candidate.clone());
                changed = true;
            }
        }
        changed
    }

    fn merge(&mut self, other: &Self) -> bool {
        if other.count == 0 || other.sectors.len() != other.count {
            return false;
        }
        if self.count == 0 {
            *self = other.clone();
            return true;
        }
        if self.count != other.count || self.sectors.len() != self.count {
            return false;
        }
        let mut changed = false;
        for (slot, candidate) in self.sectors.iter_mut().zip(&other.sectors) {
            let Some(candidate) = candidate else {
                continue;
            };
            if slot
                .as_ref()
                .is_none_or(|stored| candidate.duration + 0.000_5 < stored.duration)
            {
                *slot = Some(candidate.clone());
                changed = true;
            }
        }
        changed
    }

    fn total(&self) -> Option<f64> {
        (self.count > 0 && self.sectors.iter().all(Option::is_some)).then(|| {
            self.sectors
                .iter()
                .filter_map(Option::as_ref)
                .map(|sector| sector.duration)
                .sum()
        })
    }

    fn reference_at(&self, distance: f64, track_length: f64, reset: bool) -> Option<(f64, f64)> {
        let total = self.total()?;
        if track_length <= 1.0 || self.count == 0 {
            return None;
        }
        let width = track_length / self.count as f64;
        let index = ((distance / width).floor() as usize).min(self.count - 1);
        let sector = self.sectors[index].as_ref()?;
        let within = distance - width * index as f64;
        let sector_time = sector.time_at(within)?;
        if reset {
            Some((sector_time, total))
        } else {
            let before: f64 = self.sectors[..index]
                .iter()
                .filter_map(Option::as_ref)
                .map(|item| item.duration)
                .sum();
            Some((before + sector_time, total))
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
struct ReferenceSet {
    best: Option<LapTrace>,
    optimal: SectorBank,
}

impl ReferenceSet {
    fn merge(&mut self, other: &Self) -> bool {
        let mut changed = false;
        if let Some(candidate) = &other.best {
            if self
                .best
                .as_ref()
                .is_none_or(|stored| candidate.lap_time + 0.000_5 < stored.lap_time)
            {
                self.best = Some(candidate.clone());
                changed = true;
            }
        }
        self.optimal.merge(&other.optimal) || changed
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
struct PersistentReferences {
    version: u32,
    overall: ReferenceSet,
    #[serde(default)]
    timing_sectors: [Option<f64>; 3],
}

#[derive(Clone, Debug)]
struct Identity {
    key: String,
    legacy_key: Option<String>,
    track: String,
    vehicle: String,
    track_length: f64,
}

impl Identity {
    fn from_frame(frame: &TelemetryFrame) -> Option<Self> {
        let track = frame.track_name.trim();
        let vehicle = frame.player_vehicle_name.trim();
        if track.is_empty() || vehicle.is_empty() || frame.track_length_meters <= 100.0 {
            return None;
        }
        let rounded_length = frame.track_length_meters.round() as i64;
        let legacy_vehicle = frame.player_vehicle_livery_name.trim();
        let legacy_key = (!legacy_vehicle.is_empty() && legacy_vehicle != vehicle)
            .then(|| format!("{track}\u{1f}{legacy_vehicle}\u{1f}{rounded_length}"));
        Some(Self {
            key: format!("{track}\u{1f}{vehicle}\u{1f}{rounded_length}"),
            legacy_key,
            track: track.to_owned(),
            vehicle: vehicle.to_owned(),
            track_length: frame.track_length_meters,
        })
    }
}

#[derive(Debug)]
struct CurrentLap {
    number: i32,
    scoring_laps_at_start: i32,
    started_at_line: bool,
    valid: bool,
    visited_pits: bool,
    formation: bool,
    points: Vec<TracePoint>,
    official_sector_ends: [Option<f64>; 2],
    previous_sector: i32,
    last_recorded_distance: f64,
    start_fuel: f64,
    start_energy: f64,
    start_tire: f64,
}

impl CurrentLap {
    fn new(frame: &TelemetryFrame) -> Self {
        let distance = frame.lap_progress * frame.track_length_meters;
        let started_at_line = frame.current_lap_seconds < 2.0 && distance < 300.0;
        Self {
            number: frame.lap_number,
            scoring_laps_at_start: frame.player_total_laps,
            started_at_line,
            valid: !frame.player_in_pits && frame.game_phase == 5,
            visited_pits: frame.player_in_pits,
            formation: frame.game_phase == 3,
            points: if started_at_line {
                vec![TracePoint {
                    distance: 0.0,
                    seconds: 0.0,
                }]
            } else {
                Vec::new()
            },
            last_recorded_distance: if started_at_line { 0.0 } else { distance },
            official_sector_ends: [None; 2],
            previous_sector: frame.player_sector,
            start_fuel: frame.fuel_liters,
            start_energy: frame.virtual_energy_percent,
            start_tire: frame.player_tire_remaining_percent,
        }
    }

    fn official_result_available(&self, frame: &TelemetryFrame) -> bool {
        frame.player_total_laps > self.scoring_laps_at_start
            && frame.last_lap_seconds.is_finite()
            && (20.0..900.0).contains(&frame.last_lap_seconds)
    }

    fn observe(&mut self, frame: &TelemetryFrame) {
        let distance = frame.lap_progress * frame.track_length_meters;
        self.scoring_laps_at_start = self.scoring_laps_at_start.max(frame.player_total_laps);
        let validity_is_synchronized = frame.current_lap_seconds >= 2.0 || distance >= 300.0;
        self.valid &= (!validity_is_synchronized || frame.player_lap_valid)
            && !frame.player_in_pits
            && frame.game_phase == 5;
        self.visited_pits |= frame.player_in_pits;
        self.formation |= frame.game_phase == 3;
        if frame.player_sector != self.previous_sector {
            let boundary = match frame.player_sector {
                2 => Some(0),
                0 => Some(1),
                _ => None,
            };
            if let Some(index) = boundary.filter(|_| self.started_at_line) {
                let official_end = match index {
                    0 => frame.current_sector1_seconds,
                    _ => frame.current_sector2_seconds,
                };
                let start_seconds = if index == 0 {
                    0.0
                } else {
                    self.official_sector_ends[index - 1].unwrap_or(0.0)
                };
                let minimum_end = start_seconds + MIN_SECTOR_DURATION_SECONDS;
                let end_seconds = if official_end.is_finite() && official_end > minimum_end {
                    official_end
                } else {
                    frame.current_lap_seconds
                };
                if self.official_sector_ends[index].is_none()
                    && end_seconds.is_finite()
                    && end_seconds > minimum_end
                {
                    self.official_sector_ends[index] = Some(end_seconds);
                }
            }
            self.previous_sector = frame.player_sector;
        }
        if distance + 200.0 < self.last_recorded_distance {
            self.valid = false;
        }
        if self.started_at_line
            && distance >= self.last_recorded_distance + SAMPLE_DISTANCE_METERS
            && frame.current_lap_seconds.is_finite()
        {
            self.points.push(TracePoint {
                distance,
                seconds: frame.current_lap_seconds,
            });
            self.last_recorded_distance = distance;
        }
    }

    fn finish(mut self, frame: &TelemetryFrame, track_length: f64) -> Option<CompletedLap> {
        let lap_time = frame.last_lap_seconds;
        if !self.started_at_line
            || !lap_time.is_finite()
            || !(20.0..900.0).contains(&lap_time)
            || self.points.len() < 10
        {
            return None;
        }
        let reconstructed = self.points.last().map_or(0.0, |point| point.seconds);
        if (reconstructed - lap_time).abs() > 0.5 {
            return None;
        }
        self.points
            .retain(|point| point.seconds < lap_time && point.distance < track_length);
        self.points.push(TracePoint {
            distance: track_length,
            seconds: lap_time,
        });
        self.valid &= frame.last_lap_valid;
        let eligible = self.valid && !self.visited_pits && !self.formation;
        let category = if self.formation {
            "formation"
        } else if self.visited_pits {
            "pit"
        } else if self.valid {
            "clean"
        } else {
            "invalid"
        };
        Some(CompletedLap {
            number: self.number,
            eligible,
            category,
            trace: LapTrace {
                lap_time,
                track_length,
                points: self.points,
                official_sector_ends: self.official_sector_ends,
            },
            fuel_used: if frame.fuel_last_lap.is_finite() && frame.fuel_last_lap > 0.0 {
                frame.fuel_last_lap
            } else {
                (self.start_fuel + frame.fuel_added_last_lap - frame.fuel_liters).max(0.0)
            },
            energy_used: if frame.virtual_energy_last_lap.is_finite()
                && frame.virtual_energy_last_lap > 0.0
            {
                frame.virtual_energy_last_lap
            } else {
                (self.start_energy + frame.virtual_energy_added_last_lap
                    - frame.virtual_energy_percent)
                    .max(0.0)
            },
            tire_used: if self.start_tire >= 0.0 && frame.player_tire_remaining_percent >= 0.0 {
                (self.start_tire - frame.player_tire_remaining_percent).max(0.0)
            } else {
                0.0
            },
        })
    }
}

#[derive(Clone, Debug)]
struct CompletedLap {
    number: i32,
    eligible: bool,
    category: &'static str,
    trace: LapTrace,
    fuel_used: f64,
    energy_used: f64,
    tire_used: f64,
}

#[derive(Clone, Debug)]
struct LapRecord {
    session_id: String,
    stint: u32,
    lap: CompletedLap,
}

#[derive(Clone, Debug)]
struct StintRecord {
    session_id: String,
    number: u32,
    started_lap: i32,
    ended_lap: i32,
    lap_count: u32,
    best_lap: f64,
    total_time: f64,
    fuel_used: f64,
    energy_used: f64,
    tire_used: f64,
}

#[derive(Debug)]
struct StintAccumulator {
    number: u32,
    started_lap: i32,
    ended_lap: i32,
    lap_count: u32,
    best_lap: f64,
    total_time: f64,
    fuel_used: f64,
    energy_used: f64,
    battery_start_percent: Option<f64>,
    battery_end_percent: Option<f64>,
    regeneration_kwh: f64,
    last_hybrid_sample_elapsed: Option<f64>,
    tire_used: f64,
    clean_lap_count: u32,
    clean_lap_time: f64,
    tire_start_average: Option<f64>,
    tire_current_average: Option<f64>,
    tire_compounds: [String; 4],
}

impl StintAccumulator {
    fn new(number: u32, frame: &TelemetryFrame) -> Self {
        let tire_average = average_tire_remaining(frame);
        let battery = battery_soc(frame);
        Self {
            number,
            started_lap: frame.lap_number,
            ended_lap: frame.lap_number,
            lap_count: 0,
            best_lap: 0.0,
            total_time: 0.0,
            fuel_used: 0.0,
            energy_used: 0.0,
            battery_start_percent: battery,
            battery_end_percent: battery,
            regeneration_kwh: 0.0,
            last_hybrid_sample_elapsed: battery
                .is_some()
                .then_some(frame.session_elapsed_seconds)
                .filter(|elapsed| elapsed.is_finite()),
            tire_used: 0.0,
            clean_lap_count: 0,
            clean_lap_time: 0.0,
            tire_start_average: tire_average,
            tire_current_average: tire_average,
            tire_compounds: frame.player_tire_compounds.clone(),
        }
    }

    fn observe(&mut self, frame: &TelemetryFrame) {
        if let Some(battery) = battery_soc(frame) {
            if let Some(previous) = self.last_hybrid_sample_elapsed {
                let elapsed = frame.session_elapsed_seconds - previous;
                if (0.0..=1.0).contains(&elapsed)
                    && frame.hybrid_motor_state == 3
                    && frame.hybrid_regen_kw.is_finite()
                    && frame.hybrid_regen_kw > 0.0
                {
                    self.regeneration_kwh += frame.hybrid_regen_kw * elapsed / 3_600.0;
                }
            }
            self.battery_start_percent.get_or_insert(battery);
            self.battery_end_percent = Some(battery);
            self.last_hybrid_sample_elapsed = frame
                .session_elapsed_seconds
                .is_finite()
                .then_some(frame.session_elapsed_seconds);
        } else {
            self.last_hybrid_sample_elapsed = None;
        }
        if let Some(average) = average_tire_remaining(frame) {
            self.tire_start_average.get_or_insert(average);
            self.tire_current_average = Some(average);
        }
        if frame
            .player_tire_compounds
            .iter()
            .any(|compound| !compound.trim().is_empty())
        {
            self.tire_compounds = frame.player_tire_compounds.clone();
        }
    }

    fn add(&mut self, lap: &CompletedLap) {
        self.ended_lap = lap.number;
        self.lap_count += 1;
        self.total_time += lap.trace.lap_time;
        self.fuel_used += lap.fuel_used;
        self.energy_used += lap.energy_used;
        self.tire_used += lap.tire_used;
        if lap.eligible {
            self.clean_lap_count += 1;
            self.clean_lap_time += lap.trace.lap_time;
            if self.best_lap <= 0.0 || lap.trace.lap_time < self.best_lap {
                self.best_lap = lap.trace.lap_time;
            }
        }
    }

    fn history_view(&self, current: bool, uses_virtual_energy: bool) -> StintHistoryEntryView {
        let average_non_best = (self.clean_lap_count > 1 && self.best_lap > 0.0)
            .then(|| (self.clean_lap_time - self.best_lap) / (self.clean_lap_count - 1) as f64)
            .filter(|average| average.is_finite() && *average > 0.0);
        let delta_seconds = average_non_best.map(|average| (average - self.best_lap).max(0.0));
        let consistency_percent =
            average_non_best.map(|average| (self.best_lap / average * 100.0).clamp(0.0, 100.0));
        let tire_wear_percent = self
            .tire_start_average
            .zip(self.tire_current_average)
            .map_or(self.tire_used, |(start, current)| {
                (start - current).max(0.0)
            });
        StintHistoryEntryView {
            number: self.number,
            current,
            laps: self.lap_count,
            time_seconds: self.total_time,
            resource_used: if uses_virtual_energy {
                self.energy_used
            } else {
                self.fuel_used
            },
            battery_start_percent: self.battery_start_percent,
            battery_end_percent: self.battery_end_percent,
            regeneration_kwh: self
                .battery_start_percent
                .is_some()
                .then_some(self.regeneration_kwh),
            tire_wear_percent,
            tire_compounds: self.tire_compounds.clone(),
            delta_seconds,
            consistency_percent,
        }
    }

    fn record(self, session_id: String) -> Option<StintRecord> {
        (self.lap_count > 0).then_some(StintRecord {
            session_id,
            number: self.number,
            started_lap: self.started_lap,
            ended_lap: self.ended_lap,
            lap_count: self.lap_count,
            best_lap: self.best_lap,
            total_time: self.total_time,
            fuel_used: self.fuel_used,
            energy_used: self.energy_used,
            tire_used: self.tire_used,
        })
    }
}

enum StorageCommand {
    Load {
        keys: Vec<String>,
        reply: Sender<(PersistentReferences, bool)>,
    },
    Save {
        identity: Identity,
        references: PersistentReferences,
    },
    StartSession {
        id: String,
        identity: Identity,
        session_type: i32,
    },
    RecordLap(LapRecord),
    RecordStint(StintRecord),
    List {
        reply: Sender<Result<Vec<LapRecordSummary>, String>>,
    },
    Delete {
        key: String,
        reply: Sender<Result<(), String>>,
    },
}

struct DeltaStorage {
    sender: Sender<StorageCommand>,
}

impl DeltaStorage {
    fn new(path: PathBuf) -> Self {
        let (sender, receiver) = mpsc::channel();
        let _ = thread::Builder::new()
            .name("lmu-lap-records".into())
            .spawn(move || storage_worker(path, receiver));
        let _ = STORAGE_SENDER.set(sender.clone());
        Self { sender }
    }

    fn load(&self, keys: Vec<String>) -> Receiver<(PersistentReferences, bool)> {
        let (reply, receiver) = mpsc::channel();
        let _ = self.sender.send(StorageCommand::Load { keys, reply });
        receiver
    }

    fn send(&self, command: StorageCommand) {
        let _ = self.sender.send(command);
    }
}

pub(crate) struct DeltaEngine {
    storage: DeltaStorage,
    identity: Option<Identity>,
    pending_load: Option<Receiver<(PersistentReferences, bool)>>,
    overall: ReferenceSet,
    session: ReferenceSet,
    stint_best: Option<LapTrace>,
    last_lap: Option<LapTrace>,
    current_lap: Option<CurrentLap>,
    pending_lap: Option<CurrentLap>,
    stint: Option<StintAccumulator>,
    stint_history: Vec<StintHistoryEntryView>,
    session_id: String,
    last_session_type: i32,
    last_session_elapsed: f64,
    last_lap_number: i32,
    generation: u64,
    timing_results_until: Option<Instant>,
    timing_comparisons_until: Option<Instant>,
    timing_sectors: [Option<f64>; 3],
    timing_sector_states: [&'static str; 3],
    overall_timing_sectors: [Option<f64>; 3],
    session_timing_sectors: [Option<f64>; 3],
    timing_history: Vec<TimingLapView>,
    timing_comparisons: TimingComparisons,
    smoothed_delta: f64,
    timing_smoothed_delta: f64,
    last_delta_update: Instant,
    last_timing_update: Instant,
    delta_trend: DeltaTrend,
    delta_trend_anchor: Option<DeltaTrendAnchor>,
}

impl DeltaEngine {
    pub(crate) fn new(app_data: PathBuf) -> Self {
        Self {
            storage: DeltaStorage::new(app_data.join("lap-records.sqlite3")),
            identity: None,
            pending_load: None,
            overall: ReferenceSet::default(),
            session: ReferenceSet::default(),
            stint_best: None,
            last_lap: None,
            current_lap: None,
            pending_lap: None,
            stint: None,
            stint_history: Vec::new(),
            session_id: String::new(),
            last_session_type: -1,
            last_session_elapsed: 0.0,
            last_lap_number: -1,
            generation: 0,
            timing_results_until: None,
            timing_comparisons_until: None,
            timing_sectors: [None; 3],
            timing_sector_states: ["pending"; 3],
            overall_timing_sectors: [None; 3],
            session_timing_sectors: [None; 3],
            timing_history: Vec::new(),
            timing_comparisons: TimingComparisons::default(),
            smoothed_delta: 0.0,
            timing_smoothed_delta: 0.0,
            last_delta_update: Instant::now(),
            last_timing_update: Instant::now(),
            delta_trend: DeltaTrend::Neutral,
            delta_trend_anchor: None,
        }
    }

    pub(crate) fn update(
        &mut self,
        frame: &mut TelemetryFrame,
        delta_requested: bool,
        timing_requested: bool,
        stint_history_requested: bool,
    ) {
        self.forget_deleted_records();
        self.poll_load();
        let mode = active_mode();
        if !frame.connected || !frame.player_active {
            frame.delta_model = DeltaViewModel {
                mode,
                ..DeltaViewModel::default()
            };
            self.current_lap = None;
            self.pending_lap = None;
            self.reset_delta_trend();
            frame.timing_model = TimingViewModel::default();
            frame.stint_history_model = StintHistoryViewModel::default();
            return;
        }

        let Some(identity) = Identity::from_frame(frame) else {
            frame.delta_model = DeltaViewModel {
                mode,
                ..DeltaViewModel::default()
            };
            self.reset_delta_trend();
            frame.timing_model = TimingViewModel::default();
            frame.stint_history_model = StintHistoryViewModel::default();
            return;
        };
        if self.identity.as_ref().map(|item| item.key.as_str()) != Some(identity.key.as_str()) {
            self.select_identity(identity);
        }

        let session_changed = self.last_session_type != frame.session_type
            || frame.session_elapsed_seconds + 5.0 < self.last_session_elapsed
            || (self.last_lap_number >= 0 && frame.lap_number + 1 < self.last_lap_number);
        if session_changed {
            self.begin_session(frame);
        }
        self.last_session_type = frame.session_type;
        self.last_session_elapsed = frame.session_elapsed_seconds;
        self.last_lap_number = frame.lap_number;

        self.update_stint(frame);
        let lap_changed = self
            .current_lap
            .as_ref()
            .is_some_and(|lap| lap.number != frame.lap_number);
        if lap_changed {
            self.pending_lap = self.current_lap.take();
            self.current_lap = Some(CurrentLap::new(frame));
            self.smoothed_delta = 0.0;
            self.timing_smoothed_delta = 0.0;
            self.last_delta_update = Instant::now();
            self.last_timing_update = Instant::now();
            self.reset_delta_trend();
            self.timing_results_until = None;
            self.timing_comparisons_until = None;
            self.timing_sectors = [None; 3];
            self.timing_sector_states = ["pending"; 3];
            self.timing_comparisons = TimingComparisons::default();
        } else if self.current_lap.is_none() {
            self.current_lap = Some(CurrentLap::new(frame));
        }
        self.finish_lap(frame);
        self.update_player_sectors(frame);
        if let Some(lap) = self.current_lap.as_mut() {
            lap.observe(frame);
        }

        if delta_requested {
            frame.delta_model = self.view_model(frame, mode);
        }
        if timing_requested {
            frame.timing_model = self.timing_view_model(frame);
        }
        if stint_history_requested {
            frame.stint_history_model = self.stint_history_view_model(frame);
        }
    }

    fn select_identity(&mut self, identity: Identity) {
        self.identity = Some(identity.clone());
        let mut keys = vec![identity.key.clone()];
        if let Some(legacy_key) = identity.legacy_key.clone() {
            keys.push(legacy_key);
        }
        self.pending_load = Some(self.storage.load(keys));
        self.overall = ReferenceSet::default();
        self.session = ReferenceSet::default();
        self.stint_best = None;
        self.last_lap = None;
        self.current_lap = None;
        self.pending_lap = None;
        self.stint_history.clear();
        self.timing_sectors = [None; 3];
        self.timing_sector_states = ["pending"; 3];
        self.overall_timing_sectors = [None; 3];
        self.generation = self.generation.wrapping_add(1);
        self.last_session_type = -1;
        self.last_session_elapsed = 0.0;
        self.last_lap_number = -1;
        self.reset_delta_trend();
        self.timing_smoothed_delta = 0.0;
        self.last_timing_update = Instant::now();
    }

    fn forget_deleted_records(&mut self) {
        let deleted = take_deleted_keys();
        let Some(identity) = self.identity.as_ref() else {
            return;
        };
        if !deleted.contains(&identity.key) {
            return;
        }
        // A load still in flight would bring the deleted record back.
        self.pending_load = None;
        self.overall = ReferenceSet::default();
        self.overall_timing_sectors = [None; 3];
        self.generation = self.generation.wrapping_add(1);
    }

    fn poll_load(&mut self) {
        let loaded = self
            .pending_load
            .as_ref()
            .and_then(|receiver| receiver.try_recv().ok());
        if let Some((loaded, needs_migration)) = loaded {
            if loaded.version == STORE_VERSION {
                self.overall_timing_sectors = loaded.timing_sectors;
                if self.overall.merge(&loaded.overall) {
                    self.generation = self.generation.wrapping_add(1);
                }
                if needs_migration {
                    if let Some(identity) = self.identity.clone() {
                        self.storage.send(StorageCommand::Save {
                            identity,
                            references: loaded,
                        });
                    }
                }
            }
            self.pending_load = None;
        }
    }

    fn begin_session(&mut self, frame: &TelemetryFrame) {
        if let Some(stint) = self
            .stint
            .take()
            .and_then(|item| item.record(self.session_id.clone()))
        {
            self.storage.send(StorageCommand::RecordStint(stint));
        }
        self.session = ReferenceSet::default();
        self.stint_best = None;
        self.last_lap = None;
        self.current_lap = None;
        self.pending_lap = None;
        self.stint = None;
        self.stint_history.clear();
        self.timing_results_until = None;
        self.timing_comparisons_until = None;
        self.timing_sectors = [None; 3];
        self.timing_sector_states = ["pending"; 3];
        self.session_timing_sectors = [None; 3];
        self.timing_history.clear();
        self.timing_comparisons = TimingComparisons::default();
        self.timing_smoothed_delta = 0.0;
        self.last_timing_update = Instant::now();
        self.smoothed_delta = 0.0;
        self.session_id = format!("{}-{}", unix_millis(), self.generation);
        if let Some(identity) = self.identity.clone() {
            self.storage.send(StorageCommand::StartSession {
                id: self.session_id.clone(),
                identity,
                session_type: frame.session_type,
            });
        }
        self.generation = self.generation.wrapping_add(1);
    }

    fn update_stint(&mut self, frame: &TelemetryFrame) {
        let number = frame.player_stint.max(1);
        let changed = self
            .stint
            .as_ref()
            .is_some_and(|stint| stint.number != number);
        if changed {
            if let Some(item) = self.stint.take() {
                if item.lap_count > 0 {
                    self.stint_history
                        .insert(0, item.history_view(false, frame.virtual_energy_active));
                    self.stint_history.truncate(4);
                }
                if let Some(record) = item.record(self.session_id.clone()) {
                    self.storage.send(StorageCommand::RecordStint(record));
                }
            }
            self.stint_best = None;
            self.generation = self.generation.wrapping_add(1);
        }
        if self.stint.is_none() {
            self.stint = Some(StintAccumulator::new(number, frame));
        }
        if let Some(stint) = self.stint.as_mut() {
            stint.observe(frame);
        }
    }

    fn stint_history_view_model(&self, frame: &TelemetryFrame) -> StintHistoryViewModel {
        let mut entries = Vec::with_capacity(5);
        if let Some(current) = self.stint.as_ref().filter(|stint| stint.lap_count > 0) {
            entries.push(current.history_view(true, frame.virtual_energy_active));
        }
        entries.extend(self.stint_history.iter().cloned());
        entries.truncate(5);
        StintHistoryViewModel {
            available: !entries.is_empty(),
            uses_virtual_energy: frame.virtual_energy_active,
            hybrid_available: frame.hybrid_available,
            entries,
        }
    }

    fn finish_lap(&mut self, frame: &TelemetryFrame) {
        if !self
            .pending_lap
            .as_ref()
            .is_some_and(|lap| lap.official_result_available(frame))
        {
            return;
        }
        let Some(identity) = self.identity.as_ref() else {
            return;
        };
        let Some(current) = self.pending_lap.take() else {
            return;
        };
        let Some(completed) = current.finish(frame, identity.track_length) else {
            return;
        };

        let previous_last = self.timing_history.first().map(|lap| lap.seconds);
        let previous_average = average_timing_laps(&self.timing_history);
        let previous_session_best = self.session.best.as_ref().map(|lap| lap.lap_time);
        let previous_personal_best = self.overall.best.as_ref().map(|lap| lap.lap_time);
        let previous_optimal = self.session.optimal.total();
        self.timing_comparisons = TimingComparisons::default();
        let coarse_sectors = three_sector_times(&completed.trace);
        self.timing_results_until = Some(Instant::now() + TIMING_RESULT_FREEZE);
        self.timing_comparisons_until = Some(Instant::now() + TIMING_COMPARISON_FREEZE);
        if let Some(sectors) = coarse_sectors {
            let reference = active_sector_reference();
            let sector_ends = [
                sectors[0],
                sectors[0] + sectors[1],
                completed.trace.lap_time,
            ];
            let mut learning_states = ["neutral"; 3];
            let mut display_states = ["neutral"; 3];
            for index in 0..3 {
                let seconds = sectors[index];
                learning_states[index] = if self.overall_timing_sectors[index]
                    .is_none_or(|best| seconds + 0.000_5 < best)
                {
                    "overall"
                } else if self.session_timing_sectors[index]
                    .is_none_or(|best| seconds + 0.000_5 < best)
                {
                    "personal"
                } else {
                    "neutral"
                };
                display_states[index] = sector_state(
                    reference,
                    seconds,
                    sector_ends[index],
                    frame.class_best_sector_ends[index],
                    frame.player_best_sector_ends[index],
                    self.overall_timing_sectors[index],
                    self.session_timing_sectors[index],
                );
                if completed.eligible {
                    if learning_states[index] == "overall" {
                        self.overall_timing_sectors[index] = Some(seconds);
                    }
                    if learning_states[index] != "neutral" {
                        self.session_timing_sectors[index] = Some(seconds);
                    }
                }
            }
            self.timing_sectors = sectors.map(Some);
            self.timing_sector_states = if completed.eligible {
                display_states
            } else {
                ["invalid"; 3]
            };
        }
        let history_state = if !completed.eligible {
            "invalid"
        } else if self
            .session
            .best
            .as_ref()
            .is_none_or(|best| completed.trace.lap_time < best.lap_time)
        {
            "best"
        } else {
            "normal"
        };
        self.timing_history.insert(
            0,
            TimingLapView {
                number: completed.number,
                seconds: completed.trace.lap_time,
                valid: completed.eligible,
                state: history_state,
            },
        );
        self.timing_history.truncate(5);

        self.timing_comparisons.last_seconds =
            timing_comparison(completed.trace.lap_time, previous_last);
        self.timing_comparisons.session_personal_best_seconds =
            timing_comparison(completed.trace.lap_time, previous_session_best);
        self.timing_comparisons.personal_best_seconds =
            timing_comparison(completed.trace.lap_time, previous_personal_best);
        if completed.eligible {
            self.timing_comparisons.average_seconds = timing_comparison(
                average_timing_laps(&self.timing_history),
                (previous_average > 0.0).then_some(previous_average),
            );
        }

        if let Some(stint) = self.stint.as_mut() {
            stint.add(&completed);
        }
        self.storage.send(StorageCommand::RecordLap(LapRecord {
            session_id: self.session_id.clone(),
            stint: self.stint.as_ref().map_or(1, |item| item.number),
            lap: completed.clone(),
        }));

        if !completed.eligible {
            return;
        }
        self.last_lap = Some(completed.trace.clone());
        if self
            .stint_best
            .as_ref()
            .is_none_or(|best| completed.trace.lap_time < best.lap_time)
        {
            self.stint_best = Some(completed.trace.clone());
        }
        let sectors = build_sectors(&completed.trace);
        let mut global_changed = false;
        if self
            .session
            .best
            .as_ref()
            .is_none_or(|best| completed.trace.lap_time < best.lap_time)
        {
            self.session.best = Some(completed.trace.clone());
        }
        self.session.optimal.update(&sectors);
        if self
            .overall
            .best
            .as_ref()
            .is_none_or(|best| completed.trace.lap_time < best.lap_time)
        {
            self.overall.best = Some(completed.trace.clone());
            global_changed = true;
        }
        global_changed |= self.overall.optimal.update(&sectors);
        self.timing_comparisons.optimal_seconds =
            timing_improvement(self.session.optimal.total(), previous_optimal);
        self.generation = self.generation.wrapping_add(1);
        if global_changed {
            self.storage.send(StorageCommand::Save {
                identity: identity.clone(),
                references: PersistentReferences {
                    version: STORE_VERSION,
                    overall: self.overall.clone(),
                    timing_sectors: self.overall_timing_sectors,
                },
            });
        }
    }

    /// Closes the sectors the player has already driven and publishes how each
    /// one compares. Every overlay that paints a sector reads this one result,
    /// so it runs whether or not the timing panel is on screen.
    fn update_player_sectors(&mut self, frame: &mut TelemetryFrame) {
        if self
            .timing_results_until
            .is_some_and(|until| Instant::now() >= until)
        {
            self.timing_results_until = None;
            self.timing_sectors = [None; 3];
            self.timing_sector_states = ["pending"; 3];
        }
        if self
            .timing_comparisons_until
            .is_some_and(|until| Instant::now() >= until)
        {
            self.timing_comparisons_until = None;
            self.timing_comparisons = TimingComparisons::default();
        }
        let reference = active_sector_reference();
        if let Some(lap) = self.current_lap.as_ref() {
            for index in 0..active_timing_sector(frame) {
                if self.timing_sectors[index].is_some() {
                    continue;
                }
                let end_time = lap.official_sector_ends[index];
                let start_time = if index == 0 {
                    Some(0.0)
                } else {
                    lap.official_sector_ends[index - 1]
                };
                if let (Some(end_time), Some(start_time)) = (end_time, start_time) {
                    let seconds = end_time - start_time;
                    let state = sector_state(
                        reference,
                        seconds,
                        end_time,
                        frame.class_best_sector_ends[index],
                        frame.player_best_sector_ends[index],
                        self.overall_timing_sectors[index],
                        self.session_timing_sectors[index],
                    );
                    self.timing_sectors[index] = Some(seconds);
                    self.timing_sector_states[index] = if lap.valid { state } else { "invalid" };
                }
            }
        }
        frame.player_sector_states = std::array::from_fn(|index| {
            if self.timing_sectors[index].is_some()
                && self.current_lap.as_ref().is_some_and(|lap| !lap.valid)
            {
                "invalid"
            } else {
                self.timing_sector_states[index]
            }
        });
    }

    fn timing_view_model(&mut self, frame: &TelemetryFrame) -> TimingViewModel {
        let active_sector = active_timing_sector(frame);
        let timed_lap_active = self
            .current_lap
            .as_ref()
            .is_some_and(|lap| lap.started_at_line);
        let estimated_seconds = self.timing_estimated_lap(frame, timed_lap_active);
        TimingViewModel {
            available: true,
            lap_number: frame.player_total_laps.saturating_add(1).max(1),
            total_laps_estimated: frame.session_total_laps_estimated,
            extra_laps_estimated: frame.session_extra_laps_estimated,
            extra_laps_approximate: frame.session_extra_laps_approximate,
            current_seconds: if timed_lap_active {
                frame.current_lap_seconds
            } else {
                0.0
            },
            last_seconds: frame.last_lap_seconds,
            last_valid: timing_last_lap_valid(frame, &self.timing_history),
            session_personal_best_seconds: frame.best_lap_seconds,
            personal_best_seconds: self.overall.best.as_ref().map_or(0.0, |lap| lap.lap_time),
            average_seconds: average_timing_laps(&self.timing_history),
            optimal_seconds: self.session.optimal.total().unwrap_or(0.0),
            estimated_seconds,
            comparisons: self.timing_comparisons.clone(),
            active_sector,
            sectors: std::array::from_fn(|index| TimingSectorView {
                seconds: self.timing_sectors[index].unwrap_or(0.0),
                state: if self.timing_sectors[index].is_some()
                    && self.current_lap.as_ref().is_some_and(|lap| !lap.valid)
                {
                    "invalid"
                } else {
                    self.timing_sector_states[index]
                },
            }),
            history: self.timing_history.clone(),
        }
    }

    fn timing_estimated_lap(&mut self, frame: &TelemetryFrame, timed_lap_active: bool) -> f64 {
        if !timed_lap_active {
            self.timing_smoothed_delta = 0.0;
            self.last_timing_update = Instant::now();
            return 0.0;
        }
        let distance = frame.lap_progress * frame.track_length_meters;
        let reference = [
            self.stint_best.as_ref(),
            self.session.best.as_ref(),
            self.overall.best.as_ref(),
        ]
        .into_iter()
        .find_map(|trace| trace_reference(trace, distance));
        let Some((reference_at, reference_total)) = reference else {
            self.timing_smoothed_delta = 0.0;
            return 0.0;
        };
        let raw_delta = frame.current_lap_seconds - reference_at;
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_timing_update).as_secs_f64();
        self.last_timing_update = now;
        let alpha = 1.0 - (-elapsed / DELTA_SMOOTHING_SECONDS).exp();
        if self.timing_smoothed_delta == 0.0 || !self.timing_smoothed_delta.is_finite() {
            self.timing_smoothed_delta = raw_delta;
        } else {
            self.timing_smoothed_delta +=
                (raw_delta - self.timing_smoothed_delta) * alpha.clamp(0.0, 1.0);
        }
        let estimated = reference_total + self.timing_smoothed_delta;
        if estimated.is_finite() && (20.0..900.0).contains(&estimated) {
            estimated
        } else {
            0.0
        }
    }

    fn view_model(&mut self, frame: &TelemetryFrame, mode: DeltaMode) -> DeltaViewModel {
        let count = sector_count(frame.track_length_meters);
        let distance = frame.lap_progress * frame.track_length_meters;
        let sector_index = if count > 0 {
            ((frame.lap_progress * count as f64).floor() as usize).min(count - 1)
        } else {
            0
        };
        if mode == DeltaMode::Off {
            let model = DeltaViewModel {
                mode,
                current_lap_valid: self.current_lap.as_ref().is_some_and(|lap| lap.valid),
                sector_index,
                sector_count: count,
                reference_generation: self.generation,
                ..DeltaViewModel::default()
            };
            return self.apply_delta_trend(frame, model);
        }
        if !can_show_live_delta(self.current_lap.as_ref()) {
            let model = DeltaViewModel {
                mode,
                current_lap_valid: self.current_lap.as_ref().is_some_and(|lap| lap.valid),
                sector_index,
                sector_count: count,
                reference_generation: self.generation,
                ..DeltaViewModel::default()
            };
            return self.apply_delta_trend(frame, model);
        }
        if should_reset_delta_at_lap_start(self.current_lap.as_ref(), distance) {
            let model = DeltaViewModel {
                available: true,
                seconds: 0.0,
                mode,
                reference_seconds: self.reference_total(mode).unwrap_or(0.0),
                current_lap_valid: self.current_lap.as_ref().is_some_and(|lap| lap.valid),
                trend: DeltaTrend::Neutral,
                sector_index,
                sector_count: count,
                reference_generation: self.generation,
            };
            return self.apply_delta_trend(frame, model);
        }

        if mode == DeltaMode::SessionBest {
            if let Some((seconds, reference_seconds)) = native_session_delta(frame) {
                let model = DeltaViewModel {
                    available: true,
                    seconds,
                    mode,
                    reference_seconds,
                    current_lap_valid: self.current_lap.as_ref().is_some_and(|lap| lap.valid),
                    trend: DeltaTrend::Neutral,
                    sector_index,
                    sector_count: count,
                    reference_generation: self.generation,
                };
                return self.apply_delta_trend(frame, model);
            }
        }

        let reference = self.reference_at(mode, distance, frame.track_length_meters);
        let Some((reference_at, reference_total)) = reference else {
            let model = DeltaViewModel {
                mode,
                current_lap_valid: self.current_lap.as_ref().is_some_and(|lap| lap.valid),
                sector_index,
                sector_count: count,
                reference_generation: self.generation,
                ..DeltaViewModel::default()
            };
            return self.apply_delta_trend(frame, model);
        };
        let current_at = if mode.is_sector_reset() {
            let width = frame.track_length_meters / count.max(1) as f64;
            let start = width * sector_index as f64;
            let started = self
                .current_lap
                .as_ref()
                .and_then(|lap| interpolate(&lap.points, start))
                .unwrap_or(0.0);
            frame.current_lap_seconds - started
        } else {
            frame.current_lap_seconds
        };
        let raw = current_at - reference_at;
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_delta_update).as_secs_f64();
        self.last_delta_update = now;
        let alpha = 1.0 - (-elapsed / DELTA_SMOOTHING_SECONDS).exp();
        if self.smoothed_delta == 0.0 || !self.smoothed_delta.is_finite() {
            self.smoothed_delta = raw;
        } else {
            self.smoothed_delta += (raw - self.smoothed_delta) * alpha.clamp(0.0, 1.0);
        }
        let model = DeltaViewModel {
            available: raw.is_finite(),
            seconds: self.smoothed_delta,
            mode,
            reference_seconds: reference_total,
            current_lap_valid: self.current_lap.as_ref().is_some_and(|lap| lap.valid),
            trend: DeltaTrend::Neutral,
            sector_index,
            sector_count: count,
            reference_generation: self.generation,
        };
        self.apply_delta_trend(frame, model)
    }

    fn apply_delta_trend(
        &mut self,
        frame: &TelemetryFrame,
        mut model: DeltaViewModel,
    ) -> DeltaViewModel {
        model.seconds = limit_delta_seconds(model.seconds);
        if !model.available || !model.current_lap_valid || !model.seconds.is_finite() {
            self.reset_delta_trend();
            return model;
        }

        let observed_at = Instant::now();
        let rounded_seconds = round_delta_for_trend(model.seconds);
        let reset_at_sector = model.mode.is_sector_reset();
        let anchor_matches = self.delta_trend_anchor.is_some_and(|anchor| {
            anchor.lap_number == frame.lap_number
                && anchor.mode == model.mode
                && anchor.reference_generation == model.reference_generation
                && (!reset_at_sector || anchor.sector_index == model.sector_index)
        });

        if !anchor_matches {
            self.delta_trend = DeltaTrend::Neutral;
            self.delta_trend_anchor = Some(DeltaTrendAnchor {
                lap_number: frame.lap_number,
                mode: model.mode,
                reference_generation: model.reference_generation,
                sector_index: model.sector_index,
                observed_at,
                seconds: rounded_seconds,
            });
        } else if let Some(anchor) = self.delta_trend_anchor {
            if observed_at.duration_since(anchor.observed_at) >= DELTA_TREND_INTERVAL {
                self.delta_trend = classify_delta_trend(anchor.seconds, rounded_seconds);
                self.delta_trend_anchor = Some(DeltaTrendAnchor {
                    lap_number: frame.lap_number,
                    mode: model.mode,
                    reference_generation: model.reference_generation,
                    sector_index: model.sector_index,
                    observed_at,
                    seconds: rounded_seconds,
                });
            }
        }

        model.trend = self.delta_trend;
        model
    }

    fn reset_delta_trend(&mut self) {
        self.delta_trend = DeltaTrend::Neutral;
        self.delta_trend_anchor = None;
    }

    fn reference_at(
        &self,
        mode: DeltaMode,
        distance: f64,
        track_length: f64,
    ) -> Option<(f64, f64)> {
        match mode {
            DeltaMode::OverallBest => trace_reference(self.overall.best.as_ref(), distance),
            DeltaMode::OverallOptimalLap | DeltaMode::OverallOptimalSectors => self
                .overall
                .optimal
                .reference_at(distance, track_length, mode.is_sector_reset()),
            DeltaMode::SessionBest => trace_reference(self.session.best.as_ref(), distance),
            DeltaMode::SessionOptimalLap | DeltaMode::SessionOptimalSectors => self
                .session
                .optimal
                .reference_at(distance, track_length, mode.is_sector_reset()),
            DeltaMode::StintBest => trace_reference(self.stint_best.as_ref(), distance),
            DeltaMode::LastLap => trace_reference(self.last_lap.as_ref(), distance),
            DeltaMode::Off => None,
        }
    }

    fn reference_total(&self, mode: DeltaMode) -> Option<f64> {
        match mode {
            DeltaMode::OverallBest => self.overall.best.as_ref().map(|trace| trace.lap_time),
            DeltaMode::OverallOptimalLap | DeltaMode::OverallOptimalSectors => {
                self.overall.optimal.total()
            }
            DeltaMode::SessionBest => self.session.best.as_ref().map(|trace| trace.lap_time),
            DeltaMode::SessionOptimalLap | DeltaMode::SessionOptimalSectors => {
                self.session.optimal.total()
            }
            DeltaMode::StintBest => self.stint_best.as_ref().map(|trace| trace.lap_time),
            DeltaMode::LastLap => self.last_lap.as_ref().map(|trace| trace.lap_time),
            DeltaMode::Off => None,
        }
    }
}

fn average_timing_laps(history: &[TimingLapView]) -> f64 {
    let valid = history
        .iter()
        .filter(|lap| lap.valid && lap.seconds.is_finite() && lap.seconds > 0.0);
    let (total, count) = valid.fold((0.0, 0_u32), |(total, count), lap| {
        (total + lap.seconds, count + 1)
    });
    if count == 0 {
        0.0
    } else {
        total / f64::from(count)
    }
}

fn average_tire_remaining(frame: &TelemetryFrame) -> Option<f64> {
    let values = frame
        .player_tire_remaining_by_wheel_percent
        .iter()
        .copied()
        .filter(|value| value.is_finite() && *value >= 0.0)
        .collect::<Vec<_>>();
    (!values.is_empty()).then(|| values.iter().sum::<f64>() / values.len() as f64)
}

fn battery_soc(frame: &TelemetryFrame) -> Option<f64> {
    (frame.hybrid_available && frame.battery_charge_percent.is_finite())
        .then_some(frame.battery_charge_percent.clamp(0.0, 100.0))
}

fn timing_comparison(current: f64, previous: Option<f64>) -> Option<f64> {
    previous
        .filter(|value| current.is_finite() && current > 0.0 && value.is_finite() && *value > 0.0)
        .map(|value| current - value)
        .filter(|delta| delta.abs() >= 0.000_5)
}

fn timing_improvement(current: Option<f64>, previous: Option<f64>) -> Option<f64> {
    match (current, previous) {
        (Some(current), Some(previous)) if current + 0.000_5 < previous => Some(current - previous),
        _ => None,
    }
}

fn timing_last_lap_valid(frame: &TelemetryFrame, history: &[TimingLapView]) -> bool {
    frame.last_lap_seconds <= 0.0
        || (frame.last_lap_valid && history.first().is_none_or(|lap| lap.valid))
}

fn limit_delta_seconds(seconds: f64) -> f64 {
    seconds.clamp(-DELTA_DISPLAY_LIMIT_SECONDS, DELTA_DISPLAY_LIMIT_SECONDS)
}

fn round_delta_for_trend(seconds: f64) -> f64 {
    (seconds * 100.0).round() / 100.0
}

fn classify_delta_trend(previous_seconds: f64, current_seconds: f64) -> DeltaTrend {
    if current_seconds < previous_seconds {
        DeltaTrend::Improving
    } else if current_seconds > previous_seconds {
        DeltaTrend::Worsening
    } else {
        DeltaTrend::Neutral
    }
}

fn native_session_delta(frame: &TelemetryFrame) -> Option<(f64, f64)> {
    (frame.lap_delta_available
        && frame.best_lap_seconds.is_finite()
        && (20.0..900.0).contains(&frame.best_lap_seconds)
        && frame.lap_delta_seconds.is_finite())
    .then_some((frame.lap_delta_seconds, frame.best_lap_seconds))
}

fn can_show_live_delta(current_lap: Option<&CurrentLap>) -> bool {
    current_lap.is_some_and(|lap| lap.started_at_line)
}

fn should_reset_delta_at_lap_start(current_lap: Option<&CurrentLap>, distance: f64) -> bool {
    current_lap.is_some_and(|lap| lap.started_at_line)
        && distance.is_finite()
        && distance < SAMPLE_DISTANCE_METERS
}

fn trace_reference(trace: Option<&LapTrace>, distance: f64) -> Option<(f64, f64)> {
    let trace = trace?;
    Some((trace.time_at(distance)?, trace.lap_time))
}

fn sector_count(track_length: f64) -> usize {
    if track_length <= 100.0 {
        return 0;
    }
    (track_length / SECTOR_TARGET_METERS)
        .round()
        .clamp(MIN_SECTORS as f64, MAX_SECTORS as f64) as usize
}

fn build_sectors(trace: &LapTrace) -> Vec<SectorTrace> {
    let count = sector_count(trace.track_length);
    if count == 0 {
        return Vec::new();
    }
    let width = trace.track_length / count as f64;
    (0..count)
        .filter_map(|index| {
            let start = width * index as f64;
            let end = if index + 1 == count {
                trace.track_length
            } else {
                width * (index + 1) as f64
            };
            let start_time = trace.time_at(start)?;
            let end_time = trace.time_at(end)?;
            let mut points = vec![TracePoint {
                distance: 0.0,
                seconds: 0.0,
            }];
            points.extend(
                trace
                    .points
                    .iter()
                    .filter(|point| point.distance > start && point.distance < end)
                    .map(|point| TracePoint {
                        distance: point.distance - start,
                        seconds: point.seconds - start_time,
                    }),
            );
            points.push(TracePoint {
                distance: end - start,
                seconds: end_time - start_time,
            });
            Some(SectorTrace {
                duration: end_time - start_time,
                length: end - start,
                points,
            })
        })
        .collect()
}

fn three_sector_times(trace: &LapTrace) -> Option<[f64; 3]> {
    let first =
        trace.official_sector_ends[0].or_else(|| trace.time_at(trace.track_length / 3.0))?;
    let second =
        trace.official_sector_ends[1].or_else(|| trace.time_at(trace.track_length * 2.0 / 3.0))?;
    Some([first, second - first, trace.lap_time - second])
}

fn interpolate(points: &[TracePoint], distance: f64) -> Option<f64> {
    if points.len() < 2 || distance < points.first()?.distance || distance > points.last()?.distance
    {
        return None;
    }
    let upper = points.partition_point(|point| point.distance < distance);
    if upper == 0 {
        return Some(points[0].seconds);
    }
    if upper >= points.len() {
        return Some(points.last()?.seconds);
    }
    let lower = &points[upper - 1];
    let higher = &points[upper];
    let span = higher.distance - lower.distance;
    if span <= f64::EPSILON {
        return Some(higher.seconds);
    }
    let factor = (distance - lower.distance) / span;
    Some(lower.seconds + (higher.seconds - lower.seconds) * factor)
}

fn unix_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn storage_worker(path: PathBuf, receiver: Receiver<StorageCommand>) {
    if let Some(parent) = path.parent() {
        if let Err(error) = std::fs::create_dir_all(parent) {
            crate::startup_log::record(format!(
                "delta storage could not create {}: {error}",
                parent.display()
            ));
            return;
        }
    }
    let connection = match Connection::open(&path) {
        Ok(connection) => connection,
        Err(error) => {
            crate::startup_log::record(format!(
                "delta storage could not open {}: {error}",
                path.display()
            ));
            return;
        }
    };
    if let Err(error) = initialize_database(&connection) {
        crate::startup_log::record(format!(
            "delta storage could not initialize {}: {error}",
            path.display()
        ));
        return;
    }
    while let Ok(command) = receiver.recv() {
        if let Err(error) = handle_storage_command(&connection, command) {
            crate::startup_log::record(format!("delta storage write failed: {error}"));
        }
    }
}

fn initialize_database(connection: &Connection) -> rusqlite::Result<()> {
    connection.execute_batch(
        "PRAGMA journal_mode=WAL;
         PRAGMA synchronous=NORMAL;
         PRAGMA foreign_keys=ON;
         CREATE TABLE IF NOT EXISTS delta_references (
           identity_key TEXT PRIMARY KEY,
           track_name TEXT NOT NULL,
           vehicle_name TEXT NOT NULL,
           track_length REAL NOT NULL,
           schema_version INTEGER NOT NULL,
           payload BLOB NOT NULL,
           updated_unix_ms INTEGER NOT NULL
         );
         CREATE TABLE IF NOT EXISTS sessions (
           id TEXT PRIMARY KEY,
           identity_key TEXT NOT NULL,
           track_name TEXT NOT NULL,
           vehicle_name TEXT NOT NULL,
           session_type INTEGER NOT NULL,
           started_unix_ms INTEGER NOT NULL
         );
         CREATE TABLE IF NOT EXISTS laps (
           id INTEGER PRIMARY KEY,
           session_id TEXT NOT NULL REFERENCES sessions(id),
           stint_number INTEGER NOT NULL,
           lap_number INTEGER NOT NULL,
           lap_time_seconds REAL NOT NULL,
           eligible INTEGER NOT NULL,
           category TEXT NOT NULL,
           fuel_used REAL NOT NULL,
           energy_used REAL NOT NULL,
           tire_used REAL NOT NULL,
           recorded_unix_ms INTEGER NOT NULL
         );
         CREATE INDEX IF NOT EXISTS laps_session_idx ON laps(session_id, lap_number);
         CREATE TABLE IF NOT EXISTS stints (
           id INTEGER PRIMARY KEY,
           session_id TEXT NOT NULL REFERENCES sessions(id),
           stint_number INTEGER NOT NULL,
           started_lap INTEGER NOT NULL,
           ended_lap INTEGER NOT NULL,
           lap_count INTEGER NOT NULL,
           best_lap_seconds REAL NOT NULL,
           total_time_seconds REAL NOT NULL,
           fuel_used REAL NOT NULL,
           energy_used REAL NOT NULL,
           tire_used REAL NOT NULL,
           recorded_unix_ms INTEGER NOT NULL
         );",
    )
}

fn handle_storage_command(
    connection: &Connection,
    command: StorageCommand,
) -> rusqlite::Result<()> {
    match command {
        StorageCommand::Load { keys, reply } => {
            let (loaded, needs_migration) = keys
                .into_iter()
                .enumerate()
                .find_map(|(index, key)| {
                    connection
                        .query_row(
                            "SELECT payload FROM delta_references WHERE identity_key = ?1",
                            params![key],
                            |row| row.get::<_, Vec<u8>>(0),
                        )
                        .ok()
                        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
                        .map(|references| (references, index > 0))
                })
                .unwrap_or_default();
            let _ = reply.send((loaded, needs_migration));
        }
        StorageCommand::Save {
            identity,
            references,
        } => {
            let payload = serde_json::to_vec(&references).unwrap_or_default();
            connection.execute(
                "INSERT INTO delta_references
                 (identity_key, track_name, vehicle_name, track_length, schema_version, payload, updated_unix_ms)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                 ON CONFLICT(identity_key) DO UPDATE SET
                   track_name=excluded.track_name, vehicle_name=excluded.vehicle_name,
                   track_length=excluded.track_length, schema_version=excluded.schema_version,
                   payload=excluded.payload, updated_unix_ms=excluded.updated_unix_ms",
                params![
                    identity.key,
                    identity.track,
                    identity.vehicle,
                    identity.track_length,
                    STORE_VERSION,
                    payload,
                    unix_millis()
                ],
            )?;
        }
        StorageCommand::StartSession {
            id,
            identity,
            session_type,
        } => {
            connection.execute(
                "INSERT OR IGNORE INTO sessions
                 (id, identity_key, track_name, vehicle_name, session_type, started_unix_ms)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    id,
                    identity.key,
                    identity.track,
                    identity.vehicle,
                    session_type,
                    unix_millis()
                ],
            )?;
        }
        StorageCommand::RecordLap(record) => {
            connection.execute(
                "INSERT INTO laps
                 (session_id, stint_number, lap_number, lap_time_seconds, eligible, category,
                  fuel_used, energy_used, tire_used, recorded_unix_ms)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                params![
                    record.session_id,
                    record.stint,
                    record.lap.number,
                    record.lap.trace.lap_time,
                    record.lap.eligible,
                    record.lap.category,
                    record.lap.fuel_used,
                    record.lap.energy_used,
                    record.lap.tire_used,
                    unix_millis()
                ],
            )?;
        }
        StorageCommand::RecordStint(record) => {
            connection.execute(
                "INSERT INTO stints
                 (session_id, stint_number, started_lap, ended_lap, lap_count, best_lap_seconds,
                  total_time_seconds, fuel_used, energy_used, tire_used, recorded_unix_ms)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                params![
                    record.session_id,
                    record.number,
                    record.started_lap,
                    record.ended_lap,
                    record.lap_count,
                    record.best_lap,
                    record.total_time,
                    record.fuel_used,
                    record.energy_used,
                    record.tire_used,
                    unix_millis()
                ],
            )?;
        }
        StorageCommand::List { reply } => {
            let _ = reply.send(list_stored_records(connection).map_err(|error| error.to_string()));
        }
        StorageCommand::Delete { key, reply } => {
            let result = connection
                .execute(
                    "DELETE FROM delta_references WHERE identity_key = ?1",
                    params![key],
                )
                .map(|_| ())
                .map_err(|error| error.to_string());
            let _ = reply.send(result);
        }
    }
    Ok(())
}

fn list_stored_records(connection: &Connection) -> rusqlite::Result<Vec<LapRecordSummary>> {
    let mut statement = connection.prepare(
        "SELECT identity_key, track_name, vehicle_name, track_length, payload, updated_unix_ms
         FROM delta_references
         ORDER BY track_name COLLATE NOCASE, vehicle_name COLLATE NOCASE",
    )?;
    let rows = statement.query_map([], |row| {
        let payload: Vec<u8> = row.get(4)?;
        // A record from another store version still lists, without times, so
        // it can be deleted.
        let references = serde_json::from_slice::<PersistentReferences>(&payload)
            .ok()
            .filter(|references| references.version == STORE_VERSION)
            .unwrap_or_default();
        Ok(LapRecordSummary {
            key: row.get(0)?,
            track: row.get(1)?,
            vehicle: row.get(2)?,
            track_length_meters: row.get(3)?,
            best_lap_seconds: references.overall.best.as_ref().map(|lap| lap.lap_time),
            optimal_lap_seconds: references.overall.optimal.total(),
            best_sector_seconds: references.timing_sectors,
            updated_unix_ms: row.get::<_, i64>(5)?.max(0) as u64,
        })
    })?;
    rows.collect()
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;

    use rusqlite::Connection;

    use super::{
        average_timing_laps, build_sectors, can_show_live_delta, classify_delta_trend,
        handle_storage_command, initialize_database, interpolate, limit_delta_seconds,
        native_session_delta, round_delta_for_trend, sector_count, sector_state,
        should_reset_delta_at_lap_start, three_sector_times, timing_comparison, timing_improvement,
        CompletedLap, CurrentLap, DeltaMode, DeltaTrend, Identity, LapTrace, PersistentReferences,
        ReferenceSet, SectorBank, StintAccumulator, StorageCommand, TimingLapView,
        TimingSectorReference, TracePoint, STORE_VERSION,
    };
    use crate::telemetry::TelemetryFrame;

    fn linear_lap(seconds: f64, length: f64) -> LapTrace {
        LapTrace {
            lap_time: seconds,
            track_length: length,
            points: (0..=100)
                .map(|index| TracePoint {
                    distance: length * index as f64 / 100.0,
                    seconds: seconds * index as f64 / 100.0,
                })
                .collect(),
            official_sector_ends: [None; 2],
        }
    }

    #[test]
    fn wheel_mode_cycle_enters_from_off_and_wraps_without_disabling_delta() {
        let mut mode = DeltaMode::Off;
        let expected = [
            DeltaMode::OverallBest,
            DeltaMode::OverallOptimalLap,
            DeltaMode::OverallOptimalSectors,
            DeltaMode::SessionBest,
            DeltaMode::SessionOptimalLap,
            DeltaMode::SessionOptimalSectors,
            DeltaMode::StintBest,
            DeltaMode::LastLap,
            DeltaMode::OverallBest,
        ];
        for next in expected {
            mode = mode.next();
            assert_eq!(mode, next);
        }
    }

    #[test]
    fn timing_average_uses_only_valid_recent_laps() {
        let history = [
            TimingLapView {
                number: 4,
                seconds: 100.0,
                valid: true,
                state: "normal",
            },
            TimingLapView {
                number: 3,
                seconds: 90.0,
                valid: false,
                state: "invalid",
            },
            TimingLapView {
                number: 2,
                seconds: 104.0,
                valid: true,
                state: "normal",
            },
        ];

        assert_eq!(average_timing_laps(&history), 102.0);
    }

    #[test]
    fn stint_history_excludes_non_clean_laps_from_delta_and_consistency() {
        let mut frame = active_frame();
        frame.player_stint = 2;
        frame.player_tire_remaining_by_wheel_percent = [90.0; 4];
        frame.player_tire_compounds = ["M".into(), "M".into(), "M".into(), "M".into()];
        let mut stint = StintAccumulator::new(2, &frame);
        let completed = |number, seconds, eligible| CompletedLap {
            number,
            eligible,
            category: if eligible { "clean" } else { "pit" },
            trace: linear_lap(seconds, 1_000.0),
            fuel_used: 2.5,
            energy_used: 4.0,
            tire_used: 1.0,
        };
        stint.add(&completed(3, 100.0, true));
        stint.add(&completed(4, 102.0, true));
        stint.add(&completed(5, 150.0, false));
        frame.player_tire_remaining_by_wheel_percent = [84.0; 4];
        stint.observe(&frame);

        let view = stint.history_view(true, false);

        assert_eq!(view.laps, 3);
        assert_eq!(view.resource_used, 7.5);
        assert_eq!(view.tire_wear_percent, 6.0);
        assert_eq!(view.delta_seconds, Some(2.0));
        assert!((view.consistency_percent.unwrap() - 98.039_215_686).abs() < 0.001);
        assert_eq!(view.tire_compounds, ["M", "M", "M", "M"]);
    }

    #[test]
    fn stint_history_accumulates_regeneration_between_live_samples() {
        let mut start = active_frame();
        start.hybrid_available = true;
        start.battery_charge_percent = 80.0;
        start.hybrid_regen_kw = 720.0;
        start.hybrid_motor_state = 3;
        start.session_elapsed_seconds = 100.0;
        let mut stint = StintAccumulator::new(1, &start);

        let mut sample = start;
        sample.session_elapsed_seconds = 100.5;
        sample.battery_charge_percent = 79.0;
        stint.observe(&sample);
        sample.session_elapsed_seconds = 101.0;
        sample.battery_charge_percent = 78.0;
        stint.observe(&sample);
        sample.session_elapsed_seconds = 103.0;
        stint.observe(&sample);

        let view = stint.history_view(true, false);

        assert_eq!(view.battery_start_percent, Some(80.0));
        assert_eq!(view.battery_end_percent, Some(78.0));
        assert!((view.regeneration_kwh.unwrap() - 0.2).abs() < 1e-9);
    }

    #[test]
    fn timing_comparisons_keep_both_signed_directions_and_only_mark_real_improvements() {
        assert!((timing_comparison(99.495, Some(100.0)).unwrap() + 0.505).abs() < 1e-9);
        assert_eq!(timing_comparison(100.5, Some(100.0)), Some(0.5));
        assert_eq!(timing_comparison(100.0, Some(100.0)), None);
        assert_eq!(timing_comparison(100.0, None), None);
        assert!((timing_improvement(Some(99.495), Some(100.0)).unwrap() + 0.505).abs() < 1e-9);
        assert_eq!(timing_improvement(Some(100.5), Some(100.0)), None);
        assert_eq!(timing_improvement(Some(100.0), None), None);
    }

    #[test]
    fn timing_model_keeps_reconstructed_invalid_last_lap_marked_invalid() {
        let history = [TimingLapView {
            number: 3,
            seconds: 90.0,
            valid: false,
            state: "invalid",
        }];
        let mut frame = active_frame();
        frame.last_lap_seconds = 90.0;
        frame.last_lap_valid = true;

        assert!(!super::timing_last_lap_valid(&frame, &history));
    }

    fn active_frame() -> TelemetryFrame {
        let mut frame = TelemetryFrame::waiting_for_simulator(true);
        frame.player_active = true;
        frame.game_phase = 5;
        frame.player_lap_valid = true;
        frame.lap_number = 3;
        frame.player_total_laps = 2;
        frame.track_length_meters = 1_000.0;
        frame
    }

    #[test]
    fn delta_trend_compares_values_at_dox_centisecond_precision() {
        assert_eq!(round_delta_for_trend(0.004), 0.0);
        assert_eq!(round_delta_for_trend(0.006), 0.01);
        assert_eq!(classify_delta_trend(0.01, 0.0), DeltaTrend::Improving);
        assert_eq!(classify_delta_trend(0.01, 0.01), DeltaTrend::Neutral);
        assert_eq!(classify_delta_trend(0.01, 0.02), DeltaTrend::Worsening);
    }

    #[test]
    fn displayed_delta_stops_at_the_symmetric_limit() {
        assert_eq!(limit_delta_seconds(10.5), 9.9999);
        assert_eq!(limit_delta_seconds(-10.5), -9.9999);
        assert_eq!(limit_delta_seconds(4.25), 4.25);
    }

    #[test]
    fn completed_lap_waits_for_matching_scoring_result() {
        let start = active_frame();
        let lap = CurrentLap::new(&start);
        let mut boundary = start;
        boundary.lap_number = 4;
        boundary.current_lap_seconds = 0.02;
        boundary.last_lap_seconds = 95.0;

        assert!(!lap.official_result_available(&boundary));
        boundary.player_total_laps = 3;
        boundary.last_lap_seconds = 0.0;
        assert!(!lap.official_result_available(&boundary));
        boundary.last_lap_seconds = 100.0;
        assert!(lap.official_result_available(&boundary));
    }

    #[test]
    fn negative_official_time_confirmation_invalidates_completed_lap() {
        let start = active_frame();
        let mut lap = CurrentLap::new(&start);
        lap.points = linear_lap(100.0, 1_000.0).points;
        let mut boundary = start;
        boundary.last_lap_seconds = 100.0;
        boundary.last_lap_valid = false;

        let completed = lap.finish(&boundary, 1_000.0).unwrap();

        assert!(!completed.eligible);
        assert_eq!(completed.category, "invalid");
    }

    #[test]
    fn completed_pit_lap_keeps_energy_consumed_before_a_second_stop() {
        let mut start = active_frame();
        start.virtual_energy_percent = 82.0;
        let mut lap = CurrentLap::new(&start);
        lap.points = linear_lap(100.0, 1_000.0).points;
        let mut boundary = start;
        boundary.last_lap_seconds = 100.0;
        boundary.virtual_energy_percent = 90.0;
        boundary.virtual_energy_last_lap = 7.5;

        let completed = lap.finish(&boundary, 1_000.0).unwrap();

        assert_eq!(completed.energy_used, 7.5);
    }

    #[test]
    fn completed_pit_lap_counts_energy_charged_during_the_lap_when_last_lap_is_missing() {
        let mut start = active_frame();
        start.virtual_energy_percent = 82.0;
        let mut lap = CurrentLap::new(&start);
        lap.points = linear_lap(100.0, 1_000.0).points;
        let mut boundary = start;
        boundary.last_lap_seconds = 100.0;
        boundary.virtual_energy_percent = 90.0;
        boundary.virtual_energy_last_lap = 0.0;
        boundary.virtual_energy_added_last_lap = 12.5;

        let completed = lap.finish(&boundary, 1_000.0).unwrap();

        // 82% at the start + 12.5% charged - 90% at the boundary = 4.5% used.
        assert_eq!(completed.energy_used, 4.5);
    }

    #[test]
    fn completed_pit_lap_counts_fuel_added_during_the_lap() {
        let mut start = active_frame();
        start.fuel_liters = 50.0;
        let mut lap = CurrentLap::new(&start);
        lap.points = linear_lap(100.0, 1_000.0).points;
        let mut boundary = start;
        boundary.last_lap_seconds = 100.0;
        boundary.fuel_liters = 67.5;
        boundary.fuel_last_lap = 0.0;
        boundary.fuel_added_last_lap = 20.0;

        let completed = lap.finish(&boundary, 1_000.0).unwrap();

        assert_eq!(completed.fuel_used, 2.5);
    }

    #[test]
    fn new_lap_synchronizes_scoring_before_latching_validity_and_result_counter() {
        let start = active_frame();
        let mut lap = CurrentLap::new(&start);
        let mut delayed_scoring = start;
        delayed_scoring.current_lap_seconds = 0.12;
        delayed_scoring.lap_progress = 0.001;
        delayed_scoring.player_total_laps = 3;
        delayed_scoring.player_lap_valid = false;
        lap.observe(&delayed_scoring);

        assert!(lap.valid);
        assert_eq!(lap.scoring_laps_at_start, 3);

        delayed_scoring.current_lap_seconds = 3.0;
        delayed_scoring.lap_progress = 0.05;
        lap.observe(&delayed_scoring);
        assert!(!lap.valid);
    }

    #[test]
    fn sector_crossings_prefer_official_scoring_partials() {
        let start = active_frame();
        let mut lap = CurrentLap::new(&start);
        let mut crossing = start;
        crossing.player_sector = 2;
        crossing.current_lap_seconds = 26.0;
        crossing.current_sector1_seconds = 25.9;
        lap.observe(&crossing);
        crossing.player_sector = 0;
        crossing.current_lap_seconds = 69.5;
        crossing.current_sector2_seconds = 69.358;
        lap.observe(&crossing);

        assert_eq!(lap.official_sector_ends, [Some(25.9), Some(69.358)]);
    }

    #[test]
    fn sector_crossings_ignore_unset_one_second_partials_and_outlaps() {
        let start = active_frame();
        let mut lap = CurrentLap::new(&start);
        let mut invalid_crossing = start.clone();
        invalid_crossing.player_sector = 2;
        invalid_crossing.current_lap_seconds = 25.9;
        invalid_crossing.current_sector1_seconds = 1.0;
        invalid_crossing.player_lap_valid = false;
        lap.observe(&invalid_crossing);
        assert_eq!(lap.official_sector_ends[0], Some(25.9));

        let mut outlap_start = start;
        outlap_start.current_lap_seconds = 60.0;
        outlap_start.lap_progress = 0.5;
        let mut outlap = CurrentLap::new(&outlap_start);
        let mut outlap_crossing = outlap_start;
        outlap_crossing.player_sector = 2;
        outlap_crossing.current_sector1_seconds = 1.0;
        outlap.observe(&outlap_crossing);
        assert_eq!(outlap.official_sector_ends, [None; 2]);
    }

    #[test]
    fn session_best_delta_uses_the_native_lmu_value_when_a_reference_exists() {
        let mut frame = active_frame();
        frame.best_lap_seconds = 95.033;
        frame.lap_delta_seconds = -0.382;
        frame.lap_delta_available = true;
        assert_eq!(native_session_delta(&frame), Some((-0.382, 95.033)));

        frame.best_lap_seconds = 0.0;
        assert_eq!(native_session_delta(&frame), None);
    }

    #[test]
    fn the_purple_sector_is_the_best_of_the_player_class() {
        // A quicker car of another class never reaches this comparison: the
        // frame carries the best ends of the player's own class.
        let class_best_end = 29.8;
        assert_eq!(
            sector_state(
                TimingSectorReference::Lmu,
                29.8,
                29.8,
                class_best_end,
                29.8,
                None,
                None,
            ),
            "overall"
        );
        assert_eq!(
            sector_state(
                TimingSectorReference::Lmu,
                30.0,
                30.0,
                class_best_end,
                30.0,
                None,
                None,
            ),
            "personal"
        );
    }

    #[test]
    fn sector_state_follows_the_selected_reference() {
        let overall = Some(30.0);
        let session = Some(31.0);
        assert_eq!(
            sector_state(
                TimingSectorReference::Session,
                29.8,
                29.8,
                29.8,
                30.2,
                overall,
                session,
            ),
            "overall"
        );
        assert_eq!(
            sector_state(
                TimingSectorReference::Session,
                30.5,
                30.5,
                29.8,
                30.2,
                overall,
                session,
            ),
            "personal"
        );
        assert_eq!(
            sector_state(
                TimingSectorReference::Session,
                31.5,
                31.5,
                29.8,
                30.2,
                overall,
                session,
            ),
            "neutral"
        );
        assert_eq!(
            sector_state(
                TimingSectorReference::Overall,
                29.9,
                30.5,
                29.8,
                30.2,
                overall,
                session,
            ),
            "personal"
        );
        assert_eq!(
            sector_state(
                TimingSectorReference::Lmu,
                30.2,
                30.2,
                29.8,
                30.2,
                overall,
                session,
            ),
            "personal"
        );
        assert_eq!(
            sector_state(
                TimingSectorReference::Lmu,
                30.5,
                30.5,
                29.8,
                30.2,
                overall,
                session,
            ),
            "neutral"
        );
    }

    #[test]
    fn delta_resets_at_the_line_until_the_new_lap_has_started_sampling() {
        let frame = active_frame();
        let lap = CurrentLap::new(&frame);

        assert!(can_show_live_delta(Some(&lap)));
        assert!(should_reset_delta_at_lap_start(Some(&lap), 0.0));
        assert!(!should_reset_delta_at_lap_start(Some(&lap), 5.0));

        let mut outlap_frame = frame;
        outlap_frame.current_lap_seconds = 60.0;
        outlap_frame.lap_progress = 0.5;
        let outlap = CurrentLap::new(&outlap_frame);
        assert!(!can_show_live_delta(Some(&outlap)));
    }

    #[test]
    fn interpolation_finds_reference_time_between_samples() {
        let points = vec![
            TracePoint {
                distance: 0.0,
                seconds: 0.0,
            },
            TracePoint {
                distance: 10.0,
                seconds: 2.0,
            },
        ];
        assert_eq!(interpolate(&points, 5.0), Some(1.0));
    }

    #[test]
    fn sector_count_scales_and_is_bounded() {
        assert_eq!(sector_count(1_000.0), 12);
        assert_eq!(sector_count(5_000.0), 20);
        assert_eq!(sector_count(14_000.0), 40);
    }

    #[test]
    fn three_timing_sectors_cover_the_complete_lap() {
        let sectors = three_sector_times(&linear_lap(90.0, 4_500.0)).unwrap();
        assert!(sectors.iter().all(|sector| (*sector - 30.0).abs() < 0.001));
        assert!((sectors.iter().sum::<f64>() - 90.0).abs() < 0.001);
    }

    #[test]
    fn three_timing_sectors_prefer_official_crossings() {
        let mut lap = linear_lap(90.0, 4_500.0);
        lap.official_sector_ends = [Some(28.0), Some(61.0)];
        assert_eq!(three_sector_times(&lap), Some([28.0, 33.0, 29.0]));
    }

    #[test]
    fn optimal_bank_combines_faster_sectors_from_different_laps() {
        let first = build_sectors(&linear_lap(100.0, 5_000.0));
        let mut second_lap = linear_lap(100.0, 5_000.0);
        for point in &mut second_lap.points {
            if point.distance <= 2_500.0 {
                point.seconds *= 0.98;
            } else {
                point.seconds = 49.0 + (point.seconds - 50.0) * 1.02;
            }
        }
        second_lap.lap_time = second_lap.points.last().unwrap().seconds;
        let second = build_sectors(&second_lap);
        let mut bank = SectorBank::default();
        assert!(bank.update(&first));
        assert!(bank.update(&second));
        assert!(bank.total().unwrap() < 100.0);
    }

    #[test]
    fn sqlite_round_trip_preserves_global_references() {
        let connection = Connection::open_in_memory().unwrap();
        initialize_database(&connection).unwrap();
        let identity = Identity {
            key: "track\u{1f}car\u{1f}5000".into(),
            legacy_key: None,
            track: "track".into(),
            vehicle: "car".into(),
            track_length: 5_000.0,
        };
        let lap = linear_lap(100.0, 5_000.0);
        let mut optimal = SectorBank::default();
        optimal.update(&build_sectors(&lap));
        handle_storage_command(
            &connection,
            StorageCommand::Save {
                identity: identity.clone(),
                references: PersistentReferences {
                    version: STORE_VERSION,
                    overall: ReferenceSet {
                        best: Some(lap),
                        optimal,
                    },
                    timing_sectors: [None; 3],
                },
            },
        )
        .unwrap();

        let (sender, receiver) = mpsc::channel();
        handle_storage_command(
            &connection,
            StorageCommand::Load {
                keys: vec![identity.key.clone()],
                reply: sender,
            },
        )
        .unwrap();
        let (loaded, needs_migration) = receiver.recv().unwrap();
        assert!(!needs_migration);
        assert_eq!(loaded.version, STORE_VERSION);
        assert_eq!(loaded.overall.best.unwrap().lap_time, 100.0);
        assert_eq!(loaded.overall.optimal.total(), Some(100.0));

        let (sender, receiver) = mpsc::channel();
        handle_storage_command(
            &connection,
            StorageCommand::Load {
                keys: vec!["new-model-key".into(), identity.key],
                reply: sender,
            },
        )
        .unwrap();
        let (loaded, needs_migration) = receiver.recv().unwrap();
        assert!(needs_migration);
        assert_eq!(loaded.version, STORE_VERSION);
    }

    #[test]
    fn stored_records_list_their_times_and_can_be_deleted() {
        let connection = Connection::open_in_memory().unwrap();
        initialize_database(&connection).unwrap();
        let save = |track: &str, vehicle: &str, seconds: f64| {
            let lap = linear_lap(seconds, 5_000.0);
            let mut optimal = SectorBank::default();
            optimal.update(&build_sectors(&lap));
            handle_storage_command(
                &connection,
                StorageCommand::Save {
                    identity: Identity {
                        key: format!("{track}\u{1f}{vehicle}\u{1f}5000"),
                        legacy_key: None,
                        track: track.into(),
                        vehicle: vehicle.into(),
                        track_length: 5_000.0,
                    },
                    references: PersistentReferences {
                        version: STORE_VERSION,
                        overall: ReferenceSet {
                            best: Some(lap),
                            optimal,
                        },
                        timing_sectors: [Some(30.0), Some(40.0), Some(29.5)],
                    },
                },
            )
            .unwrap();
        };
        save("Spa", "Porsche", 130.0);
        save("monza", "Ferrari", 105.0);
        let list = |connection: &Connection| {
            let (reply, receiver) = mpsc::channel();
            handle_storage_command(connection, StorageCommand::List { reply }).unwrap();
            receiver.recv().unwrap().unwrap()
        };

        let records = list(&connection);
        assert_eq!(
            records
                .iter()
                .map(|record| record.track.as_str())
                .collect::<Vec<_>>(),
            ["monza", "Spa"]
        );
        assert_eq!(records[0].best_lap_seconds, Some(105.0));
        assert_eq!(records[0].optimal_lap_seconds, Some(105.0));
        assert_eq!(
            records[0].best_sector_seconds,
            [Some(30.0), Some(40.0), Some(29.5)]
        );

        let (reply, receiver) = mpsc::channel();
        handle_storage_command(
            &connection,
            StorageCommand::Delete {
                key: records[0].key.clone(),
                reply,
            },
        )
        .unwrap();
        receiver.recv().unwrap().unwrap();
        let remaining = list(&connection);
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].vehicle, "Porsche");
    }
}
