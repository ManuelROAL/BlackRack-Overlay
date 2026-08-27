use super::TelemetryFrame;
use serde::{Deserialize, Serialize};
use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

static ENABLED: AtomicBool = AtomicBool::new(false);
static DIRECTORY: OnceLock<PathBuf> = OnceLock::new();
static SETTINGS_PATH: OnceLock<PathBuf> = OnceLock::new();
static ACTIVE_FILE: Mutex<Option<PathBuf>> = Mutex::new(None);

#[derive(Clone, Deserialize, Serialize)]
struct Preferences {
    enabled: bool,
}

#[derive(Clone, Serialize)]
pub(crate) struct StrategyLoggingStatus {
    enabled: bool,
    directory: String,
    active_file: Option<String>,
}

pub(super) fn configure(app_data_directory: &Path) {
    let directory = app_data_directory.join("strategy-logs");
    let settings = app_data_directory.join("strategy-logging.json");
    let enabled = fs::read(&settings)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Preferences>(&bytes).ok())
        .map(|preferences| preferences.enabled)
        .unwrap_or(false);
    let _ = DIRECTORY.set(directory);
    let _ = SETTINGS_PATH.set(settings);
    ENABLED.store(enabled, Ordering::Relaxed);
}

pub(crate) fn status() -> StrategyLoggingStatus {
    let active_file = ACTIVE_FILE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .as_ref()
        .map(|path| path.display().to_string());
    StrategyLoggingStatus {
        enabled: ENABLED.load(Ordering::Relaxed),
        directory: DIRECTORY
            .get()
            .map(|path| path.display().to_string())
            .unwrap_or_default(),
        active_file,
    }
}

pub(crate) fn set_enabled(enabled: bool) -> Result<StrategyLoggingStatus, String> {
    ENABLED.store(enabled, Ordering::Relaxed);
    if let Some(path) = SETTINGS_PATH.get() {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        let bytes = serde_json::to_vec_pretty(&Preferences { enabled })
            .map_err(|error| error.to_string())?;
        fs::write(path, bytes).map_err(|error| error.to_string())?;
    }
    Ok(status())
}

const HEADER: &str = "timestamp_ms;track;vehicle;session_type;lap;stint;lap_time_s;valid;green;pit_lap;fuel_start_l;fuel_end_l;fuel_used_l;fuel_added_l;energy_start_pct;energy_end_pct;energy_used_pct;energy_added_pct;tire_change;compound_fl;compound_fr;compound_rl;compound_rr;tire_start_fl_pct;tire_start_fr_pct;tire_start_rl_pct;tire_start_rr_pct;tire_end_fl_pct;tire_end_fr_pct;tire_end_rl_pct;tire_end_rr_pct;tire_wear_fl_pct;tire_wear_fr_pct;tire_wear_rl_pct;tire_wear_rr_pct;tire_temp_avg_fl_c;tire_temp_avg_fr_c;tire_temp_avg_rl_c;tire_temp_avg_rr_c;tire_temp_max_fl_c;tire_temp_max_fr_c;tire_temp_max_rl_c;tire_temp_max_rr_c;ambient_avg_c;track_avg_c;rain_avg_pct;wetness_avg_pct;grip_avg_pct;damage_start_pct;damage_end_pct\n";

fn csv_text(value: &str) -> String {
    if value.contains([';', '"', '\n', '\r']) {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_owned()
    }
}

fn number(value: f64) -> String {
    value
        .is_finite()
        .then(|| format!("{value:.3}"))
        .unwrap_or_default()
}

#[derive(Clone)]
struct LapAccumulator {
    complete_from_start: bool,
    track: String,
    vehicle: String,
    session_type: i32,
    lap: i32,
    stint: u32,
    session_elapsed: f64,
    valid: bool,
    green: bool,
    pit_lap: bool,
    fuel_start: f64,
    fuel_added: f64,
    energy_start: f64,
    energy_added: f64,
    tire_start: [f64; 4],
    tire_end: [f64; 4],
    tire_min: [f64; 4],
    tire_change: bool,
    compounds: [String; 4],
    tire_temp_sum: [f64; 4],
    tire_temp_max: [f64; 4],
    ambient_sum: f64,
    track_temp_sum: f64,
    rain_sum: f64,
    wetness_sum: f64,
    grip_sum: f64,
    damage_start: f64,
    damage_end: f64,
    samples: u64,
}

impl LapAccumulator {
    fn new(frame: &TelemetryFrame, complete_from_start: bool) -> Self {
        let mut lap = Self {
            complete_from_start,
            track: frame.track_name.clone(),
            vehicle: frame.player_vehicle_name.clone(),
            session_type: frame.session_type,
            lap: frame.lap_number,
            stint: frame.player_stint,
            session_elapsed: frame.session_elapsed_seconds,
            valid: true,
            green: true,
            pit_lap: false,
            fuel_start: frame.fuel_liters,
            fuel_added: 0.0,
            energy_start: frame.virtual_energy_percent,
            energy_added: 0.0,
            tire_start: frame.player_tire_remaining_by_wheel_percent,
            tire_end: frame.player_tire_remaining_by_wheel_percent,
            tire_min: frame.player_tire_remaining_by_wheel_percent,
            tire_change: false,
            compounds: frame.player_tire_compounds.clone(),
            tire_temp_sum: [0.0; 4],
            tire_temp_max: [f64::NEG_INFINITY; 4],
            ambient_sum: 0.0,
            track_temp_sum: 0.0,
            rain_sum: 0.0,
            wetness_sum: 0.0,
            grip_sum: 0.0,
            damage_start: frame.player_damage_percent,
            damage_end: frame.player_damage_percent,
            samples: 0,
        };
        lap.observe(frame);
        lap
    }

    fn same_lap(&self, frame: &TelemetryFrame) -> bool {
        self.track == frame.track_name
            && self.vehicle == frame.player_vehicle_name
            && self.session_type == frame.session_type
            && self.lap == frame.lap_number
            && frame.session_elapsed_seconds + 1.0 >= self.session_elapsed
    }

    fn same_session(&self, frame: &TelemetryFrame) -> bool {
        self.track == frame.track_name
            && self.vehicle == frame.player_vehicle_name
            && self.session_type == frame.session_type
            && frame.session_elapsed_seconds + 1.0 >= self.session_elapsed
    }

    fn observe(&mut self, frame: &TelemetryFrame) {
        self.valid &= frame.player_lap_valid;
        self.green &= frame.game_phase == 5;
        self.pit_lap |= frame.player_in_pits || frame.player_in_garage;
        self.fuel_added = self.fuel_added.max(frame.fuel_added_this_lap);
        self.energy_added = self.energy_added.max(frame.virtual_energy_added_this_lap);
        for index in 0..4 {
            let remaining = frame.player_tire_remaining_by_wheel_percent[index];
            if remaining > self.tire_end[index] + 2.0 {
                self.tire_change = true;
            }
            self.tire_end[index] = remaining;
            self.tire_min[index] = self.tire_min[index].min(remaining);
            let temperature = frame.player_tire_temperature_c[index];
            if temperature.is_finite() {
                self.tire_temp_sum[index] += temperature;
                self.tire_temp_max[index] = self.tire_temp_max[index].max(temperature);
            }
        }
        self.ambient_sum += frame.ambient_temperature_c;
        self.track_temp_sum += frame.track_temperature_c;
        self.rain_sum += frame.rain_percent;
        self.wetness_sum += frame.track_wetness_percent;
        self.grip_sum += frame.player_grip_percent;
        self.damage_end = frame.player_damage_percent;
        self.samples += 1;
    }

    fn row(&self, completed: &TelemetryFrame) -> String {
        let samples = self.samples.max(1) as f64;
        let fuel_used = completed.fuel_last_lap;
        let energy_used = completed.virtual_energy_last_lap;
        let fuel_end = self.fuel_start + self.fuel_added - fuel_used;
        let energy_end = self.energy_start + self.energy_added - energy_used;
        let mut values = vec![
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis()
                .to_string(),
            csv_text(&self.track),
            csv_text(&self.vehicle),
            self.session_type.to_string(),
            self.lap.to_string(),
            self.stint.to_string(),
            number(completed.last_lap_seconds),
            (self.valid && completed.last_lap_valid).to_string(),
            self.green.to_string(),
            self.pit_lap.to_string(),
            number(self.fuel_start),
            number(fuel_end),
            number(fuel_used),
            number(self.fuel_added),
            number(self.energy_start),
            number(energy_end),
            number(energy_used),
            number(self.energy_added),
            self.tire_change.to_string(),
        ];
        values.extend(self.compounds.iter().map(|value| csv_text(value)));
        values.extend(self.tire_start.iter().map(|value| number(*value)));
        values.extend(self.tire_end.iter().map(|value| number(*value)));
        values.extend(
            (0..4).map(|index| number((self.tire_start[index] - self.tire_min[index]).max(0.0))),
        );
        values.extend(
            self.tire_temp_sum
                .iter()
                .map(|value| number(*value / samples)),
        );
        values.extend(self.tire_temp_max.iter().map(|value| number(*value)));
        values.extend([
            number(self.ambient_sum / samples),
            number(self.track_temp_sum / samples),
            number(self.rain_sum / samples),
            number(self.wetness_sum / samples),
            number(self.grip_sum / samples),
            number(self.damage_start),
            number(self.damage_end),
        ]);
        format!("{}\n", values.join(";"))
    }
}

pub(super) struct StrategyLogger {
    writer: Option<BufWriter<File>>,
    session: Option<RecordedSession>,
    lap: Option<LapAccumulator>,
}

struct RecordedSession {
    track: String,
    vehicle: String,
    session_type: i32,
    last_elapsed: f64,
}

impl RecordedSession {
    fn matches(&self, frame: &TelemetryFrame) -> bool {
        self.track == frame.track_name
            && self.vehicle == frame.player_vehicle_name
            && self.session_type == frame.session_type
            && frame.session_elapsed_seconds + 1.0 >= self.last_elapsed
    }
}

fn filename_component(value: &str) -> String {
    let mut result = String::with_capacity(value.len().min(48));
    for character in value.chars().take(48) {
        if character.is_ascii_alphanumeric() {
            result.push(character.to_ascii_lowercase());
        } else if matches!(character, ' ' | '-' | '_') && !result.ends_with('-') {
            result.push('-');
        }
    }
    result.trim_matches('-').to_owned()
}

impl StrategyLogger {
    pub(super) fn new() -> Self {
        Self {
            writer: None,
            session: None,
            lap: None,
        }
    }

    fn sync(&mut self) {
        if !ENABLED.load(Ordering::Relaxed) {
            self.close();
            self.lap = None;
            self.session = None;
        }
    }

    fn close(&mut self) {
        if let Some(mut writer) = self.writer.take() {
            let _ = writer.flush();
        }
        *ACTIVE_FILE
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = None;
    }

    fn open(&mut self, frame: &TelemetryFrame) {
        let Some(directory) = DIRECTORY.get() else {
            return;
        };
        if fs::create_dir_all(directory).is_err() {
            return;
        }
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        let track = filename_component(&frame.track_name);
        let path = directory.join(format!(
            "blackrack-strategy-{timestamp}-{}-s{}.csv",
            if track.is_empty() { "session" } else { &track },
            frame.session_type
        ));
        let Ok(file) = File::create(&path) else {
            return;
        };
        let mut writer = BufWriter::new(file);
        if writer.write_all(b"\xEF\xBB\xBF").is_err()
            || writer.write_all(HEADER.as_bytes()).is_err()
        {
            return;
        }
        self.writer = Some(writer);
        *ACTIVE_FILE
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(path);
    }

    fn ensure_session(&mut self, frame: &TelemetryFrame) {
        let same_session = self
            .session
            .as_ref()
            .map(|session| session.matches(frame))
            .unwrap_or(false);
        if !same_session {
            self.close();
            self.lap = None;
            self.open(frame);
            self.session = self.writer.is_some().then(|| RecordedSession {
                track: frame.track_name.clone(),
                vehicle: frame.player_vehicle_name.clone(),
                session_type: frame.session_type,
                last_elapsed: frame.session_elapsed_seconds,
            });
        } else if let Some(session) = self.session.as_mut() {
            session.last_elapsed = session.last_elapsed.max(frame.session_elapsed_seconds);
        }
    }

    pub(super) fn record(&mut self, frame: &TelemetryFrame) {
        self.sync();
        if !ENABLED.load(Ordering::Relaxed) {
            return;
        }
        if !frame.connected || !frame.player_active || frame.lap_number < 0 {
            self.lap = None;
            return;
        }
        self.ensure_session(frame);
        if self.writer.is_none() {
            return;
        }
        if let Some(mut lap) = self.lap.take() {
            if lap.same_lap(frame) {
                lap.observe(frame);
                self.lap = Some(lap);
                return;
            }
            let completed_lap = lap.same_session(frame) && frame.lap_number > lap.lap;
            if completed_lap && lap.complete_from_start {
                if let Some(writer) = self.writer.as_mut() {
                    let _ = writer.write_all(lap.row(frame).as_bytes());
                    let _ = writer.flush();
                }
            }
            self.lap = Some(LapAccumulator::new(frame, completed_lap));
            return;
        }
        self.lap = Some(LapAccumulator::new(frame, false));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn completed_lap_row_contains_consumption_and_wear() {
        let mut frame = TelemetryFrame::waiting_for_lmu(true);
        frame.connected = true;
        frame.player_active = true;
        frame.track_name = "Spa".into();
        frame.player_vehicle_name = "Hypercar".into();
        frame.session_type = 10;
        frame.game_phase = 5;
        frame.lap_number = 3;
        frame.player_lap_valid = true;
        frame.fuel_liters = 80.0;
        frame.virtual_energy_percent = 90.0;
        frame.player_tire_remaining_by_wheel_percent = [98.0; 4];
        frame.player_tire_temperature_c = [90.0, 91.0, 92.0, 93.0];
        let mut lap = LapAccumulator::new(&frame, true);
        frame.player_tire_remaining_by_wheel_percent = [97.5, 97.4, 97.3, 97.2];
        lap.observe(&frame);
        frame.lap_number = 4;
        frame.last_lap_seconds = 121.5;
        frame.fuel_last_lap = 4.2;
        frame.virtual_energy_last_lap = 8.1;
        let row = lap.row(&frame);
        assert!(row.contains(";3;0;121.500;true;true;false;80.000;75.800;4.200"));
        assert!(row.contains(";0.500;0.600;0.700;0.800;"));
    }

    #[test]
    fn completed_lap_row_applies_negative_official_time_confirmation() {
        let mut frame = TelemetryFrame::waiting_for_lmu(true);
        frame.track_name = "Spa".into();
        frame.player_vehicle_name = "Hypercar".into();
        frame.game_phase = 5;
        frame.lap_number = 3;
        frame.player_lap_valid = true;
        let lap = LapAccumulator::new(&frame, true);
        frame.last_lap_seconds = 121.5;
        frame.last_lap_valid = false;

        let row = lap.row(&frame);

        assert!(row.contains(";3;0;121.500;false;true;false;"));
    }

    #[test]
    fn session_rotation_detects_type_identity_and_time_reset() {
        let mut frame = TelemetryFrame::waiting_for_lmu(true);
        frame.track_name = "Le Mans".into();
        frame.player_vehicle_name = "LMGT3".into();
        frame.session_type = 10;
        frame.session_elapsed_seconds = 300.0;
        let session = RecordedSession {
            track: frame.track_name.clone(),
            vehicle: frame.player_vehicle_name.clone(),
            session_type: frame.session_type,
            last_elapsed: frame.session_elapsed_seconds,
        };
        frame.session_elapsed_seconds = 320.0;
        assert!(session.matches(&frame));
        frame.session_elapsed_seconds = 0.0;
        assert!(!session.matches(&frame));
        frame.session_elapsed_seconds = 320.0;
        frame.session_type = 11;
        assert!(!session.matches(&frame));
        assert_eq!(filename_component("Le Mans / 24h"), "le-mans-24h");
    }
}
