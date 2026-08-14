use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
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
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
struct LearnedTrack {
    track_name: String,
    track_length: f64,
    points: Vec<LearnedTrackPoint>,
    pit_traversal_samples: Vec<f64>,
    revision: u64,
}

#[derive(Default, Deserialize, Serialize)]
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

fn persist(store: &TrackStore) {
    if let Some(parent) = store.path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Ok(contents) = serde_json::to_vec_pretty(&store.data) {
        let temporary = store.path.with_extension("json.tmp");
        if fs::write(&temporary, contents).is_ok() {
            let _ = fs::remove_file(&store.path);
            let _ = fs::rename(temporary, &store.path);
        }
    }
}

pub(crate) fn learned_track_points(cache_key: &str) -> Option<(Vec<LearnedTrackPoint>, f64)> {
    with_store(|store| {
        store.data.tracks.get(cache_key).and_then(|track| {
            (track.points.len() >= 40).then(|| (track.points.clone(), track.track_length))
        })
    })
    .flatten()
}

pub(crate) fn migrate_legacy_track_map_learning(
    cache_key: &str,
    track_name: String,
    track_length: f64,
    points: Vec<LearnedTrackPoint>,
    pit_traversal_samples: Vec<f64>,
) -> bool {
    if cache_key.trim().is_empty() || !track_length.is_finite() || track_length <= 100.0 {
        return false;
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
            persist(store);
        }
        handled
    })
    .unwrap_or(false)
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
        "lmu-overlay.track-map.v1.{slug}.{}",
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
    max_pit_progress_delta: f64,
}

pub(crate) struct TrackMapModelState {
    cache_key: String,
    recording_lap: Option<i32>,
    recording_valid: bool,
    samples: Vec<LearnedTrackPoint>,
    last_sample_distance: f64,
    pit_vehicles: HashMap<i32, PitVehicleState>,
    previous_pit_sample_time: Option<f64>,
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
        }
        if frame.track_map_vehicles.is_empty() {
            frame.track_map_model = self.view_model(frame, None);
            return;
        }

        let player_index = frame
            .track_map_vehicles
            .iter()
            .position(|vehicle| vehicle.is_player);
        self.update_pit_traversal(frame);
        if let Some(index) = player_index {
            self.update_recorder(frame, index);
        }
        let player = player_index.and_then(|index| frame.track_map_vehicles.get(index));
        frame.track_map_model = self.view_model(frame, player);
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
                && frame.last_lap_seconds > 0.0
                && self.samples.len() >= 40
                && coverage >= frame.track_length_meters * 0.88
            {
                let points = self.samples.clone();
                let _ = with_store(|store| {
                    let track = store.data.tracks.entry(self.cache_key.clone()).or_default();
                    track.track_name.clone_from(&frame.track_name);
                    track.track_length = frame.track_length_meters;
                    track.points = points;
                    track.revision = track.revision.saturating_add(1).max(1);
                    persist(store);
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
                        max_pit_progress_delta: 0.0,
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
                previous.pit_start_progress = progress;
                previous.max_pit_progress_delta = 0.0;
            } else if previous.in_pits
                && vehicle.in_pits
                && previous.eligible
                && delta_seconds > 0.0
            {
                if let (Some(start), Some(current)) = (previous.pit_start_progress, progress) {
                    previous.max_pit_progress_delta =
                        previous.max_pit_progress_delta.max((current - start).abs());
                }
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
                if previous.eligible
                    && completed_official_pit_passage(*previous, progress, official.is_some())
                {
                    completed_samples
                        .push(previous.moving_seconds + previous.pending_seconds.min(0.5));
                }
                previous.eligible = true;
                previous.moving_seconds = 0.0;
                previous.pending_seconds = 0.0;
                previous.pit_start_progress = None;
                previous.max_pit_progress_delta = 0.0;
            }
            previous.in_pits = vehicle.in_pits;
            previous.last_distance = vehicle.lap_distance;
        }
        self.pit_vehicles
            .retain(|vehicle_id, _| active.contains(vehicle_id));
        for seconds in completed_samples {
            self.save_pit_sample(frame, seconds);
        }
    }

    fn save_pit_sample(&self, frame: &TelemetryFrame, seconds: f64) {
        if !seconds.is_finite() || !(5.0..=180.0).contains(&seconds) {
            return;
        }
        let _ = with_store(|store| {
            let track = store.data.tracks.entry(self.cache_key.clone()).or_default();
            track.track_name.clone_from(&frame.track_name);
            track.track_length = frame.track_length_meters;
            track.pit_traversal_samples.push(seconds);
            let overflow = track
                .pit_traversal_samples
                .len()
                .saturating_sub(MAX_PIT_SAMPLES);
            track.pit_traversal_samples.drain(..overflow);
            persist(store);
        });
    }

    fn view_model(
        &self,
        frame: &TelemetryFrame,
        player: Option<&TrackMapVehicle>,
    ) -> TrackMapViewModel {
        let (geometry_revision, learned_geometry_available, pit_samples) = self.track_metadata();
        let traversal_seconds = median(&pit_samples);
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
        TrackMapViewModel {
            cache_key: self.cache_key.clone(),
            geometry_revision,
            learned_geometry_available,
            pit_prediction_lap_distance,
        }
    }
}

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_key_matches_the_frontend_legacy_format() {
        assert_eq!(
            track_map_cache_key("Circuit de la Sarthe", 13_626.4),
            "lmu-overlay.track-map.v1.circuit-de-la-sarthe.13626"
        );
    }

    #[test]
    fn median_uses_the_middle_of_the_persisted_samples() {
        assert_eq!(median(&[31.0, 27.0, 29.0]), 29.0);
        assert_eq!(median(&[27.0, 29.0]), 28.0);
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
            max_pit_progress_delta: 0.8,
        };
        assert!(completed_official_pit_passage(state, Some(0.9), true));
        assert!(!completed_official_pit_passage(state, Some(0.3), true));
    }

    #[test]
    fn prediction_combines_service_and_moving_pit_time_once() {
        let distance = predicted_lap_distance(4_000.0, 20.0, 30.0, 100.0, 5_000.0);
        assert_eq!(distance, Some(1_500.0));
        assert_eq!(predicted_lap_distance(0.0, 20.0, 30.0, 0.0, 5_000.0), None);
    }
}
