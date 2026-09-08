use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use super::track_geometry::{cached_official_track_map_geometry, TrackGeometryPoint};
use super::{TelemetryFrame, TrackMapVehicle};

const MIN_SAMPLE_DISTANCE: f64 = 3.0;
const MAX_PIT_SAMPLES: usize = 7;

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
pub(crate) struct LearnedTrackPoint {
    pub(crate) x: f64,
    pub(crate) y: f64,
    pub(crate) distance: f64,
}

#[derive(Clone, Debug, Default, Serialize)]
pub(crate) struct TrackMapViewModel {
    cache_key: String,
    geometry_revision: u64,
    learned_geometry_available: bool,
    pit_prediction_lap_distance: Option<f64>,
    pit_prediction_approximate: bool,
    yellow_sectors: u32,
    class_best_sectors: u32,
    personal_best_sectors: u32,
    sector_boundaries: [Option<f64>; 2],
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
struct LearnedTrack {
    track_name: String,
    track_length: f64,
    points: Vec<LearnedTrackPoint>,
    pit_traversal_samples: Vec<f64>,
    #[serde(default)]
    pit_entry_distance: Option<f64>,
    #[serde(default)]
    pit_speed_ms: Option<f64>,
    revision: u64,
}

#[derive(Clone, Default, Deserialize, Serialize)]
struct StoredTracks {
    version: u32,
    tracks: HashMap<String, LearnedTrack>,
}

struct TrackStore {
    path: PathBuf,
    data: StoredTracks,
}

static STORE: OnceLock<Mutex<TrackStore>> = OnceLock::new();

pub(crate) fn configure_track_map_storage(app_data_directory: &Path) {
    let path = app_data_directory.join("track-map-learning.json");
    let _ = recover_persisted_backup(&path);
    let data = fs::read(&path)
        .ok()
        .and_then(|contents| serde_json::from_slice::<StoredTracks>(&contents).ok())
        .filter(|stored| stored.version == 1)
        .unwrap_or_else(|| StoredTracks {
            version: 1,
            tracks: HashMap::new(),
        });
    let _ = STORE.set(Mutex::new(TrackStore { path, data }));
}

fn with_store<R>(callback: impl FnOnce(&mut TrackStore) -> R) -> Option<R> {
    STORE.get().map(|store| {
        let mut store = store
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        callback(&mut store)
    })
}

fn persist(store: &TrackStore) -> Result<(), String> {
    let parent = store
        .path
        .parent()
        .ok_or_else(|| "track_map_storage_parent_missing".to_owned())?;
    fs::create_dir_all(parent)
        .map_err(|error| format!("track_map_storage_directory_failed: {error}"))?;

    let contents = serde_json::to_vec_pretty(&store.data)
        .map_err(|error| format!("track_map_storage_encode_failed: {error}"))?;
    let temporary = store.path.with_extension("json.tmp");
    let mut file = fs::File::create(&temporary)
        .map_err(|error| format!("track_map_storage_temp_create_failed: {error}"))?;
    file.write_all(&contents)
        .and_then(|_| file.sync_all())
        .map_err(|error| format!("track_map_storage_temp_sync_failed: {error}"))?;
    drop(file);

    if let Err(error) = replace_persisted_file(&temporary, &store.path) {
        let _ = fs::remove_file(&temporary);
        return Err(format!("track_map_storage_replace_failed: {error}"));
    }

    sync_persisted_file(&store.path)
        .map_err(|error| format!("track_map_storage_sync_failed: {error}"))?;
    sync_storage_directory(parent)
        .map_err(|error| format!("track_map_storage_directory_sync_failed: {error}"))
}

/// A process interrupted after moving the old file aside must recover that
/// file before the next launch. This is intentionally conservative: if the
/// primary file exists, it remains authoritative and the replacement path
/// below is responsible for its backup lifecycle.
fn recover_persisted_backup(destination: &Path) -> io::Result<bool> {
    if destination.exists() {
        return Ok(false);
    }
    let backup = destination.with_extension("json.bak");
    if !backup.is_file() {
        return Ok(false);
    }
    fs::rename(backup, destination)?;
    Ok(true)
}

fn replace_persisted_file(temporary: &Path, destination: &Path) -> io::Result<()> {
    #[cfg(not(windows))]
    {
        fs::rename(temporary, destination)
    }

    #[cfg(windows)]
    {
        if fs::rename(temporary, destination).is_ok() {
            return Ok(());
        }

        if !destination.is_file() {
            return fs::rename(temporary, destination);
        }

        let backup = destination.with_extension("json.bak");
        if backup.exists() {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "persistent track map backup already exists",
            ));
        }
        fs::rename(destination, &backup)?;
        match fs::rename(temporary, destination) {
            Ok(()) => {
                let _ = fs::remove_file(backup);
                Ok(())
            }
            Err(error) => match fs::rename(&backup, destination) {
                Ok(()) => Err(error),
                Err(restore_error) => Err(io::Error::new(
                    error.kind(),
                    format!("{error}; persistent track map backup restore failed: {restore_error}"),
                )),
            },
        }
    }
}

#[cfg(unix)]
fn sync_persisted_file(path: &Path) -> io::Result<()> {
    fs::File::open(path)?.sync_all()
}

#[cfg(not(unix))]
fn sync_persisted_file(_path: &Path) -> io::Result<()> {
    Ok(())
}

#[cfg(unix)]
fn sync_storage_directory(path: &Path) -> io::Result<()> {
    fs::File::open(path)?.sync_all()
}

#[cfg(not(unix))]
fn sync_storage_directory(_path: &Path) -> io::Result<()> {
    Ok(())
}

pub(crate) fn learned_track_points(cache_key: &str) -> Option<(Vec<LearnedTrackPoint>, f64)> {
    with_store(|store| {
        store.data.tracks.get(cache_key).and_then(|track| {
            (track.points.len() >= 40).then(|| (track.points.clone(), track.track_length))
        })
    })
    .flatten()
}

pub(crate) fn learned_pit_traversal_seconds(track_name: &str, track_length: f64) -> f64 {
    let cache_key = track_map_cache_key(track_name, track_length);
    with_store(|store| {
        store
            .data
            .tracks
            .get(&cache_key)
            .map_or(0.0, |track| median(&track.pit_traversal_samples))
    })
    .unwrap_or(0.0)
}

pub(crate) fn learned_pit_entry_bias(track_name: &str, track_length: f64) -> f64 {
    if !track_length.is_finite() || track_length <= 0.0 {
        return 0.0;
    }
    let cache_key = track_map_cache_key(track_name, track_length);
    with_store(|store| {
        store
            .data
            .tracks
            .get(&cache_key)
            .and_then(|track| track.pit_entry_distance)
            .filter(|distance| distance.is_finite() && *distance >= 0.0)
            .map_or(0.0, |distance| {
                1.0 - distance.rem_euclid(track_length) / track_length
            })
    })
    .unwrap_or(0.0)
    .clamp(0.0, 1.0)
}

pub(super) fn learned_pit_speed(track_name: &str, track_length: f64) -> f64 {
    let key = track_map_cache_key(track_name, track_length);
    with_store(|store| {
        store
            .data
            .tracks
            .get(&key)
            .and_then(|track| track.pit_speed_ms)
    })
    .flatten()
    .filter(|speed| speed.is_finite() && (5.0..=50.0).contains(speed))
    .unwrap_or(0.0)
}

pub(super) fn save_pit_speed(track_name: &str, track_length: f64, speed: f64) {
    let key = track_map_cache_key(track_name, track_length);
    let _ = with_store(|store| {
        let previous_data = store.data.clone();
        let track = store.data.tracks.entry(key).or_default();
        if track
            .pit_speed_ms
            .is_some_and(|old| (old - speed).abs() < 0.4)
        {
            return;
        }
        track.track_name = track_name.to_owned();
        track.track_length = track_length;
        track.pit_speed_ms = Some(speed);
        if persist(store).is_err() {
            store.data = previous_data;
        }
    });
}

pub(crate) fn migrate_legacy_track_map_learning(
    cache_key: &str,
    track_name: String,
    track_length: f64,
    points: Vec<LearnedTrackPoint>,
    pit_traversal_samples: Vec<f64>,
) -> Result<bool, String> {
    if cache_key.trim().is_empty() || !track_length.is_finite() || track_length <= 100.0 {
        return Ok(false);
    }
    let valid_points = points.len() >= 40
        && points
            .iter()
            .all(|point| point.x.is_finite() && point.y.is_finite() && point.distance.is_finite())
        && points
            .windows(2)
            .all(|pair| pair[1].distance >= pair[0].distance)
        && points
            .last()
            .zip(points.first())
            .is_some_and(|(last, first)| last.distance - first.distance >= track_length * 0.88);
    let samples = pit_traversal_samples
        .into_iter()
        .filter(|seconds| seconds.is_finite() && (5.0..=180.0).contains(seconds))
        .rev()
        .take(MAX_PIT_SAMPLES)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<Vec<_>>();
    let has_valid_samples = !samples.is_empty();
    with_store(|store| {
        let previous_data = store.data.clone();
        let track = store.data.tracks.entry(cache_key.to_owned()).or_default();
        let mut changed = false;
        if valid_points && track.points.len() < 40 {
            track.points = points;
            track.revision = track.revision.saturating_add(1).max(1);
            changed = true;
        }
        if !samples.is_empty() && track.pit_traversal_samples.is_empty() {
            track.pit_traversal_samples = samples;
            changed = true;
        }
        if changed {
            track.track_name = track_name;
            track.track_length = track_length;
        }
        let handled = changed
            || (valid_points && track.points.len() >= 40)
            || (has_valid_samples && !track.pit_traversal_samples.is_empty());
        if changed {
            if let Err(error) = persist(store) {
                store.data = previous_data;
                return Err(error);
            }
        }
        Ok(handled)
    })
    .unwrap_or_else(|| Err("track_map_storage_not_configured".to_owned()))
}

pub(crate) fn track_map_cache_key(track_name: &str, track_length: f64) -> String {
    let mut slug = String::new();
    let mut separator = false;
    for character in track_name.trim().to_ascii_lowercase().chars() {
        if character.is_ascii_alphanumeric() {
            slug.push(character);
            separator = false;
        } else if !separator && !slug.is_empty() {
            slug.push('-');
            separator = true;
        }
    }
    while slug.ends_with('-') {
        slug.pop();
    }
    format!(
        "blackrack-overlay.track-map.v1.{slug}.{}",
        track_length.round() as i64
    )
}

#[derive(Clone, Copy, Debug)]
struct PitVehicleState {
    in_pits: bool,
    eligible: bool,
    last_distance: f64,
    moving_seconds: f64,
    pending_seconds: f64,
    pit_start_progress: Option<f64>,
    last_pit_progress: Option<f64>,
    max_pit_progress_delta: f64,
    pit_entry_distance: Option<f64>,
}

pub(crate) struct TrackMapModelState {
    cache_key: String,
    recording_lap: Option<i32>,
    recording_valid: bool,
    samples: Vec<LearnedTrackPoint>,
    last_sample_distance: f64,
    pit_vehicles: HashMap<i32, PitVehicleState>,
    previous_pit_sample_time: Option<f64>,
    vehicle_sectors: HashMap<i32, (i32, f64)>,
    sector_boundaries: [Option<f64>; 2],
    sector_results: [&'static str; 3],
}

impl Default for TrackMapModelState {
    fn default() -> Self {
        Self {
            cache_key: String::new(),
            recording_lap: None,
            recording_valid: true,
            samples: Vec::new(),
            last_sample_distance: f64::NEG_INFINITY,
            pit_vehicles: HashMap::new(),
            previous_pit_sample_time: None,
            vehicle_sectors: HashMap::new(),
            sector_boundaries: [None; 2],
            sector_results: ["pending"; 3],
        }
    }
}

impl TrackMapModelState {
    pub(crate) fn update(&mut self, frame: &mut TelemetryFrame) {
        let key = track_map_cache_key(&frame.track_name, frame.track_length_meters);
        if key != self.cache_key {
            self.cache_key = key;
            self.recording_lap = None;
            self.recording_valid = true;
            self.samples.clear();
            self.last_sample_distance = f64::NEG_INFINITY;
            self.pit_vehicles.clear();
            self.previous_pit_sample_time = None;
            self.vehicle_sectors.clear();
            self.sector_boundaries = [None; 2];
            self.sector_results = ["pending"; 3];
        }
        if frame.track_map_vehicles.is_empty() {
            frame.track_map_model = self.view_model(frame, None);
            return;
        }

        let player_index = frame
            .track_map_vehicles
            .iter()
            .position(|vehicle| vehicle.is_player);
        self.update_sector_boundaries(frame);
        self.update_sector_results(frame);
        self.update_pit_traversal(frame);
        if let Some(index) = player_index {
            self.update_recorder(frame, index);
        }
        let player = player_index.and_then(|index| frame.track_map_vehicles.get(index));
        frame.track_map_model = self.view_model(frame, player);
    }

    fn update_sector_boundaries(&mut self, frame: &TelemetryFrame) {
        let track_length = frame.track_length_meters;
        if track_length <= 100.0 {
            return;
        }
        let mut active = HashSet::new();
        for vehicle in &frame.track_map_vehicles {
            active.insert(vehicle.vehicle_id);
            let distance = vehicle.lap_distance.rem_euclid(track_length);
            if let Some((previous_sector, previous_distance)) = self
                .vehicle_sectors
                .insert(vehicle.vehicle_id, (vehicle.sector, distance))
            {
                let boundary_index = match (previous_sector, vehicle.sector) {
                    (1, 2) => Some(0),
                    (2, 0) => Some(1),
                    _ => None,
                };
                let delta = (distance - previous_distance).rem_euclid(track_length);
                if let Some(index) = boundary_index.filter(|_| delta <= 250.0) {
                    self.sector_boundaries[index] =
                        Some((previous_distance + delta * 0.5).rem_euclid(track_length));
                }
            }
        }
        self.vehicle_sectors
            .retain(|vehicle_id, _| active.contains(vehicle_id));
    }

    fn track_metadata(&self) -> (u64, bool, Vec<f64>) {
        with_store(|store| {
            store
                .data
                .tracks
                .get(&self.cache_key)
                .map(|track| {
                    (
                        track.revision,
                        track.points.len() >= 40,
                        track.pit_traversal_samples.clone(),
                    )
                })
                .unwrap_or_default()
        })
        .unwrap_or_default()
    }

    fn update_recorder(&mut self, frame: &TelemetryFrame, player_index: usize) {
        let player = &frame.track_map_vehicles[player_index];
        if cached_official_track_map_geometry(&self.cache_key).is_some()
            || self.track_metadata().1
            || frame.track_length_meters <= 100.0
        {
            return;
        }
        if self.recording_lap.is_none() {
            self.recording_lap = Some(player.total_laps);
        }
        if self.recording_lap != Some(player.total_laps) {
            let coverage = self
                .samples
                .last()
                .zip(self.samples.first())
                .map_or(0.0, |(last, first)| last.distance - first.distance);
            if self.recording_valid
                && frame.last_lap_valid
                && frame.last_lap_seconds > 0.0
                && self.samples.len() >= 40
                && coverage >= frame.track_length_meters * 0.88
            {
                let points = self.samples.clone();
                let _ = with_store(|store| {
                    let previous_data = store.data.clone();
                    let track = store.data.tracks.entry(self.cache_key.clone()).or_default();
                    track.track_name.clone_from(&frame.track_name);
                    track.track_length = frame.track_length_meters;
                    track.points = points;
                    track.revision = track.revision.saturating_add(1).max(1);
                    if persist(store).is_err() {
                        store.data = previous_data;
                    }
                });
            }
            self.recording_lap = Some(player.total_laps);
            self.recording_valid = true;
            self.samples.clear();
            self.last_sample_distance = f64::NEG_INFINITY;
        }
        self.recording_valid &= frame.player_lap_valid && !player.in_pits;
        if player.lap_distance >= self.last_sample_distance + MIN_SAMPLE_DISTANCE {
            self.samples.push(LearnedTrackPoint {
                x: player.world_x,
                y: player.world_y,
                distance: player.lap_distance,
            });
            self.last_sample_distance = player.lap_distance;
        }
    }

    fn update_pit_traversal(&mut self, frame: &TelemetryFrame) {
        let now = frame.session_elapsed_seconds;
        if self
            .previous_pit_sample_time
            .is_some_and(|previous| now < previous)
        {
            self.pit_vehicles.clear();
        }
        let delta_seconds = self
            .previous_pit_sample_time
            .filter(|previous| now >= *previous)
            .map_or(0.0, |previous| (now - previous).min(0.25));
        self.previous_pit_sample_time = Some(now);
        let official = cached_official_track_map_geometry(&self.cache_key);
        let mut active = HashSet::new();
        let mut completed_samples = Vec::new();

        for vehicle in &frame.track_map_vehicles {
            active.insert(vehicle.vehicle_id);
            let Some(previous) = self.pit_vehicles.get_mut(&vehicle.vehicle_id) else {
                self.pit_vehicles.insert(
                    vehicle.vehicle_id,
                    PitVehicleState {
                        in_pits: vehicle.in_pits,
                        eligible: !vehicle.in_pits,
                        last_distance: vehicle.lap_distance,
                        moving_seconds: 0.0,
                        pending_seconds: 0.0,
                        pit_start_progress: None,
                        last_pit_progress: None,
                        max_pit_progress_delta: 0.0,
                        pit_entry_distance: None,
                    },
                );
                continue;
            };
            let progress = (vehicle.in_pits || previous.in_pits)
                .then(|| {
                    pit_progress(
                        vehicle,
                        official
                            .as_ref()
                            .map(|geometry| geometry.pit_path.as_slice()),
                    )
                })
                .flatten();
            if !previous.in_pits && vehicle.in_pits {
                previous.eligible = true;
                previous.moving_seconds = 0.0;
                previous.pending_seconds = 0.0;
                previous.pit_start_progress = None;
                previous.last_pit_progress = None;
                previous.max_pit_progress_delta = 0.0;
                previous.pit_entry_distance = Some(vehicle.lap_distance);
                observe_pit_progress(previous, progress);
            } else if previous.in_pits
                && vehicle.in_pits
                && previous.eligible
                && delta_seconds > 0.0
            {
                observe_pit_progress(previous, progress);
                previous.pending_seconds = (previous.pending_seconds + delta_seconds).min(0.5);
                let raw_delta = vehicle.lap_distance - previous.last_distance;
                let distance_delta = if frame.track_length_meters > 0.0 {
                    raw_delta.rem_euclid(frame.track_length_meters)
                } else {
                    raw_delta.max(0.0)
                };
                if (0.1..=8.0).contains(&distance_delta) {
                    previous.moving_seconds += previous.pending_seconds;
                    previous.pending_seconds = 0.0;
                }
            } else if previous.in_pits && !vehicle.in_pits {
                let exit_progress = progress
                    .and_then(pit_endpoint_progress)
                    .or_else(|| previous.last_pit_progress.and_then(pit_endpoint_progress));
                if previous.eligible
                    && completed_official_pit_passage(*previous, exit_progress, official.is_some())
                {
                    completed_samples.push((
                        previous.moving_seconds + previous.pending_seconds.min(0.5),
                        previous.pit_entry_distance,
                    ));
                }
                previous.eligible = true;
                previous.moving_seconds = 0.0;
                previous.pending_seconds = 0.0;
                previous.pit_start_progress = None;
                previous.last_pit_progress = None;
                previous.max_pit_progress_delta = 0.0;
                previous.pit_entry_distance = None;
            }
            previous.in_pits = vehicle.in_pits;
            previous.last_distance = vehicle.lap_distance;
        }
        self.pit_vehicles
            .retain(|vehicle_id, _| active.contains(vehicle_id));
        for (seconds, pit_entry_distance) in completed_samples {
            self.save_pit_sample(frame, seconds, pit_entry_distance);
        }
    }

    fn save_pit_sample(
        &self,
        frame: &TelemetryFrame,
        seconds: f64,
        pit_entry_distance: Option<f64>,
    ) {
        if !seconds.is_finite() || !(5.0..=180.0).contains(&seconds) {
            return;
        }
        let _ = with_store(|store| {
            let previous_data = store.data.clone();
            let track = store.data.tracks.entry(self.cache_key.clone()).or_default();
            track.track_name.clone_from(&frame.track_name);
            track.track_length = frame.track_length_meters;
            track.pit_traversal_samples.push(seconds);
            let overflow = track
                .pit_traversal_samples
                .len()
                .saturating_sub(MAX_PIT_SAMPLES);
            track.pit_traversal_samples.drain(..overflow);
            if let Some(distance) = pit_entry_distance.filter(|distance| {
                distance.is_finite()
                    && *distance >= 0.0
                    && frame.track_length_meters.is_finite()
                    && frame.track_length_meters > 0.0
            }) {
                track.pit_entry_distance = Some(distance.rem_euclid(frame.track_length_meters));
            }
            if persist(store).is_err() {
                store.data = previous_data;
            }
        });
    }

    /// Keeps the last painted result of each sector. The shared state falls back
    /// to `pending` between laps, and a segment that simply waits for its next
    /// visit should not blink back to the plain track meanwhile.
    ///
    /// Only qualifying paints them: that is the session where a lap is the whole
    /// point and the map can afford to talk about sector times.
    fn update_sector_results(&mut self, frame: &TelemetryFrame) {
        if !QUALIFYING_SESSIONS.contains(&frame.session_type) {
            self.sector_results = ["pending"; 3];
            return;
        }
        for index in 0..3 {
            let state = frame.player_sector_states[index];
            if state != "pending" {
                self.sector_results[index] = state;
            }
        }
    }

    /// The colours every live-timing screen uses, as two masks the renderer can
    /// paint directly: the best of the player's class, and a personal best.
    fn sector_highlights(&self) -> (u32, u32) {
        let (mut class_best, mut personal_best) = (0, 0);
        for (index, result) in self.sector_results.iter().enumerate() {
            match *result {
                "overall" => {
                    class_best |= 1 << SECTOR_BITS[index];
                    personal_best |= 1 << SECTOR_BITS[index];
                }
                "personal" => personal_best |= 1 << SECTOR_BITS[index],
                _ => {}
            }
        }
        (class_best, personal_best)
    }

    fn view_model(
        &self,
        frame: &TelemetryFrame,
        player: Option<&TrackMapVehicle>,
    ) -> TrackMapViewModel {
        let (geometry_revision, learned_geometry_available, pit_samples) = self.track_metadata();
        let measured = median(&pit_samples);
        let traversal = super::pit_traversal::choose_estimate(
            measured,
            cached_official_track_map_geometry(&self.cache_key)
                .map_or(0.0, |map| map.pit_length_meters),
            learned_pit_speed(&frame.track_name, frame.track_length_meters),
        );
        let traversal_seconds = traversal.seconds.unwrap_or(0.0);
        let pace = if frame.last_lap_seconds > 0.0 {
            frame.last_lap_seconds
        } else {
            frame.best_lap_seconds
        };
        let pit_prediction_lap_distance = player
            .filter(|_| {
                !frame.player_in_pits
                    && frame.pit_stop_estimate_available
                    && frame.pit_stop_estimate_seconds >= 0.0
                    && traversal_seconds > 0.0
                    && pace > 0.0
                    && frame.track_length_meters > 0.0
            })
            .and_then(|player| {
                predicted_lap_distance(
                    player.lap_distance,
                    frame.pit_stop_estimate_seconds,
                    traversal_seconds,
                    pace,
                    frame.track_length_meters,
                )
            });
        let (class_best_sectors, personal_best_sectors) = self.sector_highlights();
        TrackMapViewModel {
            cache_key: self.cache_key.clone(),
            geometry_revision,
            learned_geometry_available,
            pit_prediction_lap_distance,
            pit_prediction_approximate: pit_prediction_lap_distance.is_some()
                && traversal.approximate,
            yellow_sectors: frame.yellow_sectors,
            class_best_sectors,
            personal_best_sectors,
            sector_boundaries: self.sector_boundaries,
        }
    }
}

/// Sector bits follow the scoring numbering the yellow mask uses: 1, 2 and 0.
const SECTOR_BITS: [u32; 3] = [1, 2, 0];
/// The session numbering every overlay shares: 0-4 practice, 5-8 qualifying,
/// 9 warmup and 10-13 race.
const QUALIFYING_SESSIONS: std::ops::RangeInclusive<i32> = 5..=8;

fn predicted_lap_distance(
    player_distance: f64,
    service_seconds: f64,
    moving_pit_seconds: f64,
    pace_seconds: f64,
    track_length: f64,
) -> Option<f64> {
    (player_distance.is_finite()
        && service_seconds.is_finite()
        && moving_pit_seconds.is_finite()
        && pace_seconds > 0.0
        && track_length > 0.0)
        .then(|| {
            (player_distance - (service_seconds + moving_pit_seconds) / pace_seconds * track_length)
                .rem_euclid(track_length)
        })
}

fn median(values: &[f64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    let middle = sorted.len() / 2;
    if sorted.len() % 2 == 1 {
        sorted[middle]
    } else {
        (sorted[middle - 1] + sorted[middle]) / 2.0
    }
}

fn pit_progress(vehicle: &TrackMapVehicle, path: Option<&[TrackGeometryPoint]>) -> Option<f64> {
    let path = path.filter(|path| path.len() >= 2)?;
    let (best_index, best_squared) = path
        .iter()
        .enumerate()
        .map(|(index, point)| {
            let dx = vehicle.world_x - point.x;
            let dy = vehicle.world_y - point.y;
            (index, dx * dx + dy * dy)
        })
        .min_by(|left, right| left.1.total_cmp(&right.1))?;
    (best_squared <= 30.0 * 30.0).then(|| best_index as f64 / (path.len() - 1) as f64)
}

fn completed_official_pit_passage(
    state: PitVehicleState,
    exit_progress: Option<f64>,
    official_available: bool,
) -> bool {
    if !official_available {
        return true;
    }
    let (Some(start), Some(exit)) = (state.pit_start_progress, exit_progress) else {
        return false;
    };
    start.min(1.0 - start) <= 0.25
        && exit.min(1.0 - exit) <= 0.25
        && (exit - start).abs() >= 0.5
        && state.max_pit_progress_delta >= 0.5
}

fn pit_endpoint_progress(progress: f64) -> Option<f64> {
    (progress.min(1.0 - progress) <= 0.25).then_some(progress)
}

fn observe_pit_progress(state: &mut PitVehicleState, progress: Option<f64>) {
    let Some(current) = progress else {
        return;
    };
    if state.pit_start_progress.is_none() {
        state.pit_start_progress = pit_endpoint_progress(current);
    }
    state.last_pit_progress = Some(current);
    if let Some(start) = state.pit_start_progress {
        state.max_pit_progress_delta = state.max_pit_progress_delta.max((current - start).abs());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    static TEST_PATH_COUNTER: AtomicU64 = AtomicU64::new(0);

    fn test_storage_path() -> PathBuf {
        let suffix = TEST_PATH_COUNTER.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "blackrack-overlay-track-map-test-{}-{suffix}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    fn sector_vehicle(sector: i32, lap_distance: f64) -> TrackMapVehicle {
        TrackMapVehicle {
            vehicle_id: 7,
            overall_position: 1,
            vehicle_class: "HYPERCAR".into(),
            world_x: 0.0,
            world_y: 0.0,
            lap_distance,
            total_laps: 1,
            in_pits: false,
            in_garage: false,
            causing_yellow: false,
            sector,
            is_player: false,
        }
    }

    #[test]
    fn only_qualifying_paints_the_sector_results() {
        let mut state = TrackMapModelState::default();
        let mut frame = TelemetryFrame::waiting_for_simulator(true);
        frame.session_type = 5;
        frame.session_type = 5;
        frame.player_sector_states = ["overall", "personal", "neutral"];
        state.update_sector_results(&frame);
        assert_eq!(state.sector_highlights(), (1 << 1, (1 << 1) | (1 << 2)));

        frame.session_type = 10;
        state.update_sector_results(&frame);
        assert_eq!(state.sector_highlights(), (0, 0));
    }

    #[test]
    fn a_sector_waiting_for_its_next_visit_keeps_the_painted_result() {
        let mut state = TrackMapModelState::default();
        let mut frame = TelemetryFrame::waiting_for_simulator(true);
        frame.session_type = 5;
        frame.player_sector_states = ["overall", "pending", "pending"];
        state.update_sector_results(&frame);
        frame.player_sector_states = ["pending"; 3];
        state.update_sector_results(&frame);

        assert_eq!(state.sector_highlights(), (1 << 1, 1 << 1));

        frame.player_sector_states = ["invalid", "pending", "pending"];
        state.update_sector_results(&frame);

        assert_eq!(state.sector_highlights(), (0, 0));
    }

    #[test]
    fn cache_key_matches_the_frontend_legacy_format() {
        assert_eq!(
            track_map_cache_key("Circuit de la Sarthe", 13_626.4),
            "blackrack-overlay.track-map.v1.circuit-de-la-sarthe.13626"
        );
    }

    #[test]
    fn median_uses_the_middle_of_the_persisted_samples() {
        assert_eq!(median(&[31.0, 27.0, 29.0]), 29.0);
        assert_eq!(median(&[27.0, 29.0]), 28.0);
    }

    #[test]
    fn failed_replace_keeps_the_existing_persistent_path() {
        let root = test_storage_path();
        fs::create_dir_all(&root).unwrap();
        let destination = root.join("track-map-learning.json");
        fs::create_dir(&destination).unwrap();
        let store = TrackStore {
            path: destination.clone(),
            data: StoredTracks {
                version: 1,
                tracks: HashMap::new(),
            },
        };

        assert!(persist(&store).is_err());
        assert!(destination.is_dir());
        assert!(!destination.with_extension("json.tmp").exists());

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn successful_replace_keeps_the_new_persistent_contents() {
        let root = test_storage_path();
        fs::create_dir_all(&root).unwrap();
        let destination = root.join("track-map-learning.json");
        fs::write(&destination, br#"{"version":1,"tracks":{}}"#).unwrap();
        let store = TrackStore {
            path: destination.clone(),
            data: StoredTracks {
                version: 1,
                tracks: HashMap::new(),
            },
        };

        let result = persist(&store);
        assert!(result.is_ok(), "{result:?}");
        let stored =
            serde_json::from_slice::<StoredTracks>(&fs::read(destination).unwrap()).unwrap();
        assert_eq!(stored.version, 1);

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn recovers_a_backup_when_the_primary_file_is_missing() {
        let root = test_storage_path();
        fs::create_dir_all(&root).unwrap();
        let destination = root.join("track-map-learning.json");
        let backup = destination.with_extension("json.bak");
        fs::write(&backup, br#"{"version":1,"tracks":{}}"#).unwrap();

        assert!(recover_persisted_backup(&destination).unwrap());
        assert!(destination.is_file());
        assert!(!backup.exists());

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn previous_track_learning_remains_compatible_without_a_pit_entry() {
        let track: LearnedTrack = serde_json::from_str(
            r#"{"track_name":"Spa","track_length":7004.0,"points":[],"pit_traversal_samples":[31.0],"revision":1}"#,
        )
        .unwrap();

        assert_eq!(track.pit_entry_distance, None);
        assert_eq!(track.pit_traversal_samples, vec![31.0]);
    }

    #[test]
    fn official_pit_validation_requires_opposite_endpoints() {
        let state = PitVehicleState {
            in_pits: true,
            eligible: true,
            last_distance: 0.0,
            moving_seconds: 20.0,
            pending_seconds: 0.0,
            pit_start_progress: Some(0.1),
            last_pit_progress: Some(0.9),
            max_pit_progress_delta: 0.8,
            pit_entry_distance: Some(4_000.0),
        };
        assert!(completed_official_pit_passage(state, Some(0.9), true));
        assert!(!completed_official_pit_passage(state, Some(0.3), true));
    }

    #[test]
    fn pit_endpoints_are_latched_during_the_complete_traversal() {
        let mut state = PitVehicleState {
            in_pits: true,
            eligible: true,
            last_distance: 0.0,
            moving_seconds: 0.0,
            pending_seconds: 0.0,
            pit_start_progress: None,
            last_pit_progress: None,
            max_pit_progress_delta: 0.0,
            pit_entry_distance: Some(4_000.0),
        };
        observe_pit_progress(&mut state, Some(0.1));
        observe_pit_progress(&mut state, Some(0.6));
        observe_pit_progress(&mut state, Some(0.9));
        assert_eq!(state.pit_start_progress, Some(0.1));
        assert_eq!(state.last_pit_progress, Some(0.9));
        assert!((state.max_pit_progress_delta - 0.8).abs() < f64::EPSILON);
        assert!(completed_official_pit_passage(
            state,
            state.last_pit_progress.and_then(pit_endpoint_progress),
            true
        ));
    }

    #[test]
    fn prediction_combines_service_and_moving_pit_time_once() {
        let distance = predicted_lap_distance(4_000.0, 20.0, 30.0, 100.0, 5_000.0);
        assert_eq!(distance, Some(1_500.0));
        assert_eq!(predicted_lap_distance(0.0, 20.0, 30.0, 0.0, 5_000.0), None);
    }

    #[test]
    fn learns_scoring_sector_boundaries_from_vehicle_crossings() {
        let mut state = TrackMapModelState::default();
        let mut frame = TelemetryFrame::waiting_for_simulator(true);
        frame.track_length_meters = 5_000.0;
        frame.track_map_vehicles = vec![sector_vehicle(1, 990.0)];
        state.update_sector_boundaries(&frame);

        frame.track_map_vehicles[0] = sector_vehicle(2, 1_010.0);
        state.update_sector_boundaries(&frame);
        frame.track_map_vehicles[0] = sector_vehicle(2, 3_490.0);
        state.update_sector_boundaries(&frame);
        frame.track_map_vehicles[0] = sector_vehicle(0, 3_510.0);
        state.update_sector_boundaries(&frame);

        assert_eq!(state.sector_boundaries, [Some(1_000.0), Some(3_500.0)]);
    }
}
