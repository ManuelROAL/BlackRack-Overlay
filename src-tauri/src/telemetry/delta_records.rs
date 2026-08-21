use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use super::TelemetryFrame;

const SAMPLE_DISTANCE_METERS: f64 = 5.0;
const SECTOR_TARGET_METERS: f64 = 250.0;
const MIN_SECTORS: usize = 12;
const MAX_SECTORS: usize = 40;
const DELTA_SMOOTHING_SECONDS: f64 = 0.10;
const DELTA_TREND_DISTANCE_METERS: f64 = 20.0;
const DELTA_TREND_DEADBAND_SECONDS: f64 = 0.008;
const MIN_SECTOR_DURATION_SECONDS: f64 = 5.0;
const TIMING_RESULT_FREEZE: Duration = Duration::from_secs(3);
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

fn active_mode() -> DeltaMode {
    DeltaMode::from_u8(DELTA_MODE.load(Ordering::Relaxed))
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

fn active_sector_reference() -> TimingSectorReference {
    TimingSectorReference::from_u8(TIMING_SECTOR_REFERENCE.load(Ordering::Relaxed))
}

fn sector_state(
    reference: TimingSectorReference,
    seconds: f64,
    overall: Option<f64>,
    session: Option<f64>,
) -> &'static str {
    match reference {
        TimingSectorReference::Lmu => "neutral",
        TimingSectorReference::Session => {
            if overall.is_none_or(|best| seconds + 0.000_5 < best) {
                "overall"
            } else if session.is_none_or(|best| seconds + 0.000_5 < best) {
                "personal"
            } else {
                "neutral"
            }
        }
        TimingSectorReference::Overall => {
            if overall.is_none_or(|best| seconds + 0.000_5 < best) {
                "overall"
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

#[derive(Clone, Debug, Serialize)]
pub(crate) struct TimingViewModel {
    available: bool,
    current_seconds: f64,
    last_seconds: f64,
    best_seconds: f64,
    active_sector: usize,
    sectors: [TimingSectorView; 3],
    history: Vec<TimingLapView>,
}

impl Default for TimingViewModel {
    fn default() -> Self {
        Self {
            available: false,
            current_seconds: 0.0,
            last_seconds: 0.0,
            best_seconds: 0.0,
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
    distance: f64,
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
        Some(Self {
            key: format!("{track}\u{1f}{vehicle}\u{1f}{rounded_length}"),
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
            fuel_used: (self.start_fuel - frame.fuel_liters).max(0.0),
            energy_used: (self.start_energy - frame.virtual_energy_percent).max(0.0),
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
    tire_used: f64,
}

impl StintAccumulator {
    fn new(number: u32, lap: i32) -> Self {
        Self {
            number,
            started_lap: lap,
            ended_lap: lap,
            lap_count: 0,
            best_lap: 0.0,
            total_time: 0.0,
            fuel_used: 0.0,
            energy_used: 0.0,
            tire_used: 0.0,
        }
    }

    fn add(&mut self, lap: &CompletedLap) {
        self.ended_lap = lap.number;
        self.lap_count += 1;
        self.total_time += lap.trace.lap_time;
        self.fuel_used += lap.fuel_used;
        self.energy_used += lap.energy_used;
        self.tire_used += lap.tire_used;
        if lap.eligible && (self.best_lap <= 0.0 || lap.trace.lap_time < self.best_lap) {
            self.best_lap = lap.trace.lap_time;
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
        key: String,
        reply: Sender<PersistentReferences>,
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
        Self { sender }
    }

    fn load(&self, key: String) -> Receiver<PersistentReferences> {
        let (reply, receiver) = mpsc::channel();
        let _ = self.sender.send(StorageCommand::Load { key, reply });
        receiver
    }

    fn send(&self, command: StorageCommand) {
        let _ = self.sender.send(command);
    }
}

pub(crate) struct DeltaEngine {
    storage: DeltaStorage,
    identity: Option<Identity>,
    pending_load: Option<Receiver<PersistentReferences>>,
    overall: ReferenceSet,
    session: ReferenceSet,
    stint_best: Option<LapTrace>,
    last_lap: Option<LapTrace>,
    current_lap: Option<CurrentLap>,
    pending_lap: Option<CurrentLap>,
    stint: Option<StintAccumulator>,
    session_id: String,
    last_session_type: i32,
    last_session_elapsed: f64,
    last_lap_number: i32,
    generation: u64,
    timing_results_until: Option<Instant>,
    timing_sectors: [Option<f64>; 3],
    timing_sector_states: [&'static str; 3],
    overall_timing_sectors: [Option<f64>; 3],
    session_timing_sectors: [Option<f64>; 3],
    timing_history: Vec<TimingLapView>,
    smoothed_delta: f64,
    last_delta_update: Instant,
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
            session_id: String::new(),
            last_session_type: -1,
            last_session_elapsed: 0.0,
            last_lap_number: -1,
            generation: 0,
            timing_results_until: None,
            timing_sectors: [None; 3],
            timing_sector_states: ["pending"; 3],
            overall_timing_sectors: [None; 3],
            session_timing_sectors: [None; 3],
            timing_history: Vec::new(),
            smoothed_delta: 0.0,
            last_delta_update: Instant::now(),
            delta_trend: DeltaTrend::Neutral,
            delta_trend_anchor: None,
        }
    }

    pub(crate) fn update(&mut self, frame: &mut TelemetryFrame) {
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
            return;
        }

        let Some(identity) = Identity::from_frame(frame) else {
            frame.delta_model = DeltaViewModel {
                mode,
                ..DeltaViewModel::default()
            };
            self.reset_delta_trend();
            frame.timing_model = TimingViewModel::default();
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
            self.last_delta_update = Instant::now();
            self.reset_delta_trend();
            self.timing_results_until = None;
            self.timing_sectors = [None; 3];
            self.timing_sector_states = ["pending"; 3];
        } else if self.current_lap.is_none() {
            self.current_lap = Some(CurrentLap::new(frame));
        }
        self.finish_lap(frame);
        if let Some(lap) = self.current_lap.as_mut() {
            lap.observe(frame);
        }

        frame.delta_model = self.view_model(frame, mode);
        frame.timing_model = self.timing_view_model(frame);
    }

    fn select_identity(&mut self, identity: Identity) {
        self.identity = Some(identity.clone());
        self.pending_load = Some(self.storage.load(identity.key.clone()));
        self.overall = ReferenceSet::default();
        self.session = ReferenceSet::default();
        self.stint_best = None;
        self.last_lap = None;
        self.current_lap = None;
        self.pending_lap = None;
        self.timing_sectors = [None; 3];
        self.timing_sector_states = ["pending"; 3];
        self.overall_timing_sectors = [None; 3];
        self.generation = self.generation.wrapping_add(1);
        self.last_session_type = -1;
        self.last_session_elapsed = 0.0;
        self.last_lap_number = -1;
        self.reset_delta_trend();
    }

    fn poll_load(&mut self) {
        let loaded = self
            .pending_load
            .as_ref()
            .and_then(|receiver| receiver.try_recv().ok());
        if let Some(loaded) = loaded {
            if loaded.version == STORE_VERSION {
                self.overall_timing_sectors = loaded.timing_sectors;
                if self.overall.merge(&loaded.overall) {
                    self.generation = self.generation.wrapping_add(1);
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
        self.timing_results_until = None;
        self.timing_sectors = [None; 3];
        self.timing_sector_states = ["pending"; 3];
        self.session_timing_sectors = [None; 3];
        self.timing_history.clear();
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
            if let Some(record) = self
                .stint
                .take()
                .and_then(|item| item.record(self.session_id.clone()))
            {
                self.storage.send(StorageCommand::RecordStint(record));
            }
            self.stint_best = None;
            self.generation = self.generation.wrapping_add(1);
        }
        if self.stint.is_none() {
            self.stint = Some(StintAccumulator::new(number, frame.lap_number));
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

        let coarse_sectors = three_sector_times(&completed.trace);
        self.timing_results_until = Some(Instant::now() + TIMING_RESULT_FREEZE);
        if let Some(sectors) = coarse_sectors {
            let reference = active_sector_reference();
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

    fn timing_view_model(&mut self, frame: &TelemetryFrame) -> TimingViewModel {
        if self
            .timing_results_until
            .is_some_and(|until| Instant::now() >= until)
        {
            self.timing_results_until = None;
            self.timing_sectors = [None; 3];
            self.timing_sector_states = ["pending"; 3];
        }
        // LMU/rFactor codifica 0=S3, 1=S1 y 2=S2.
        let active_sector = match frame.player_sector {
            2 => 1,
            0 => 2,
            _ => 0,
        };
        let reference = active_sector_reference();
        if let Some(lap) = self.current_lap.as_ref() {
            for index in 0..active_sector {
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
                        self.overall_timing_sectors[index],
                        self.session_timing_sectors[index],
                    );
                    self.timing_sectors[index] = Some(seconds);
                    self.timing_sector_states[index] = if lap.valid { state } else { "invalid" };
                }
            }
        }

        let timed_lap_active = self
            .current_lap
            .as_ref()
            .is_some_and(|lap| lap.started_at_line);
        TimingViewModel {
            available: true,
            current_seconds: if timed_lap_active {
                frame.current_lap_seconds
            } else {
                0.0
            },
            last_seconds: frame.last_lap_seconds,
            best_seconds: self
                .session
                .best
                .as_ref()
                .map_or(frame.best_lap_seconds, |lap| lap.lap_time),
            active_sector,
            sectors: std::array::from_fn(|index| TimingSectorView {
                seconds: self.timing_sectors[index].unwrap_or(0.0),
                state: if self.timing_sectors[index].is_some()
                    && self.current_lap.as_ref().is_some_and(|lap| !lap.valid)
                {
                    "invalid"
                } else if reference == TimingSectorReference::Lmu
                    && self.timing_sectors[index].is_some()
                {
                    if frame.lap_delta_seconds.is_finite() && frame.lap_delta_seconds < 0.0 {
                        "personal"
                    } else {
                        "neutral"
                    }
                } else {
                    self.timing_sector_states[index]
                },
            }),
            history: self.timing_history.clone(),
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
        if !model.available
            || !model.current_lap_valid
            || !model.seconds.is_finite()
            || !frame.track_length_meters.is_finite()
            || frame.track_length_meters <= 0.0
        {
            self.reset_delta_trend();
            return model;
        }

        let distance = frame.lap_progress * frame.track_length_meters;
        let reset_at_sector = model.mode.is_sector_reset();
        let anchor_matches = self.delta_trend_anchor.is_some_and(|anchor| {
            anchor.lap_number == frame.lap_number
                && anchor.mode == model.mode
                && anchor.reference_generation == model.reference_generation
                && (!reset_at_sector || anchor.sector_index == model.sector_index)
                && distance >= anchor.distance
        });

        if !anchor_matches {
            self.delta_trend = DeltaTrend::Neutral;
            self.delta_trend_anchor = Some(DeltaTrendAnchor {
                lap_number: frame.lap_number,
                mode: model.mode,
                reference_generation: model.reference_generation,
                sector_index: model.sector_index,
                distance,
                seconds: model.seconds,
            });
        } else if let Some(anchor) = self.delta_trend_anchor {
            if distance - anchor.distance >= DELTA_TREND_DISTANCE_METERS {
                self.delta_trend = classify_delta_trend(model.seconds - anchor.seconds);
                self.delta_trend_anchor = Some(DeltaTrendAnchor {
                    lap_number: frame.lap_number,
                    mode: model.mode,
                    reference_generation: model.reference_generation,
                    sector_index: model.sector_index,
                    distance,
                    seconds: model.seconds,
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

fn classify_delta_trend(change_seconds: f64) -> DeltaTrend {
    if change_seconds < -DELTA_TREND_DEADBAND_SECONDS {
        DeltaTrend::Improving
    } else if change_seconds > DELTA_TREND_DEADBAND_SECONDS {
        DeltaTrend::Worsening
    } else {
        DeltaTrend::Neutral
    }
}

fn native_session_delta(frame: &TelemetryFrame) -> Option<(f64, f64)> {
    (frame.best_lap_seconds.is_finite()
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
        StorageCommand::Load { key, reply } => {
            let loaded = connection
                .query_row(
                    "SELECT payload FROM delta_references WHERE identity_key = ?1",
                    params![key],
                    |row| row.get::<_, Vec<u8>>(0),
                )
                .ok()
                .and_then(|bytes| serde_json::from_slice(&bytes).ok())
                .unwrap_or_default();
            let _ = reply.send(loaded);
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
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;

    use rusqlite::Connection;

    use super::{
        build_sectors, can_show_live_delta, classify_delta_trend, handle_storage_command,
        initialize_database, interpolate, native_session_delta, sector_count, sector_state,
        should_reset_delta_at_lap_start, three_sector_times, CurrentLap, DeltaTrend, Identity,
        LapTrace, PersistentReferences, ReferenceSet, SectorBank, StorageCommand,
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

    fn active_frame() -> TelemetryFrame {
        let mut frame = TelemetryFrame::waiting_for_lmu(true);
        frame.player_active = true;
        frame.game_phase = 5;
        frame.player_lap_valid = true;
        frame.lap_number = 3;
        frame.player_total_laps = 2;
        frame.track_length_meters = 1_000.0;
        frame
    }

    #[test]
    fn delta_trend_uses_a_deadband_around_stable_segments() {
        assert_eq!(classify_delta_trend(-0.009), DeltaTrend::Improving);
        assert_eq!(classify_delta_trend(-0.008), DeltaTrend::Neutral);
        assert_eq!(classify_delta_trend(0.0), DeltaTrend::Neutral);
        assert_eq!(classify_delta_trend(0.008), DeltaTrend::Neutral);
        assert_eq!(classify_delta_trend(0.009), DeltaTrend::Worsening);
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
        assert_eq!(native_session_delta(&frame), Some((-0.382, 95.033)));

        frame.best_lap_seconds = 0.0;
        assert_eq!(native_session_delta(&frame), None);
    }

    #[test]
    fn sector_state_follows_the_selected_reference() {
        let overall = Some(30.0);
        let session = Some(31.0);
        assert_eq!(
            sector_state(TimingSectorReference::Session, 30.5, overall, session),
            "personal"
        );
        assert_eq!(
            sector_state(TimingSectorReference::Session, 29.9, overall, session),
            "overall"
        );
        assert_eq!(
            sector_state(TimingSectorReference::Session, 31.5, overall, session),
            "neutral"
        );
        assert_eq!(
            sector_state(TimingSectorReference::Overall, 30.5, overall, session),
            "neutral"
        );
        assert_eq!(
            sector_state(TimingSectorReference::Overall, 29.9, overall, session),
            "overall"
        );
        assert_eq!(
            sector_state(TimingSectorReference::Lmu, 30.5, overall, session),
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
                key: identity.key,
                reply: sender,
            },
        )
        .unwrap();
        let loaded = receiver.recv().unwrap();
        assert_eq!(loaded.version, STORE_VERSION);
        assert_eq!(loaded.overall.best.unwrap().lap_time, 100.0);
        assert_eq!(loaded.overall.optimal.total(), Some(100.0));
    }
}
