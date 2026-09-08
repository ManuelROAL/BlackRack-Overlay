//! Telemetry source backed by the simulator's shared memory.
//!
//! This source fills the session, car and standings state the overlay host
//! needs, plus the Driving values. Other areas remain at their documented
//! sentinel until they are validated against the simulator.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use super::super::{SourceDescriptor, TelemetrySource};
use super::irsdk::Connection;
use super::session::{self, Session};
use super::standings;
use super::{foreground, DESCRIPTOR};
use crate::telemetry::{TelemetryDemand, TelemetryFrame};

/// How long to wait before reopening a mapping that stopped answering, so a
/// simulator restart is picked up without probing on every cycle.
const RECONNECT_INTERVAL: Duration = Duration::from_millis(500);
/// The session string is large and is republished as results change. Parsing it
/// at most once a second keeps the 50 Hz loop away from that cost.
const SESSION_PARSE_INTERVAL: Duration = Duration::from_secs(1);
/// The foreground owner rarely changes, so it is sampled well below the loop
/// rate instead of once per frame.
const FOREGROUND_INTERVAL: Duration = Duration::from_millis(200);

const SURFACE_NOT_IN_WORLD: i32 = -1;
const SURFACE_OFF_TRACK: i32 = 0;

/// Full-course caution bits, which the frame reports as its own phase.
const FLAG_CAUTION: u32 = 0x0000_4000 | 0x0000_8000;

pub(super) struct IracingTelemetrySource {
    connection: Option<Connection>,
    minimum_tick_after_reconnect: Option<i32>,
    reconnect_at: Instant,
    session: Session,
    session_parsed_at: Option<Instant>,
    session_number: i32,
    last_laps_completed: i32,
    lap_valid: bool,
    observed_max_rpm: f64,
    foreground: bool,
    foreground_checked_at: Option<Instant>,
    starting_positions: HashMap<i32, i32>,
}

impl IracingTelemetrySource {
    pub(super) fn new() -> Self {
        Self {
            connection: None,
            minimum_tick_after_reconnect: None,
            reconnect_at: Instant::now(),
            session: Session::new(),
            session_parsed_at: None,
            session_number: i32::MIN,
            last_laps_completed: -1,
            lap_valid: true,
            observed_max_rpm: 0.0,
            foreground: false,
            foreground_checked_at: None,
            starting_positions: HashMap::new(),
        }
    }

    /// Drops everything learned inside a session. Called when the simulator
    /// disconnects and when it moves to another session of the event.
    fn reset(&mut self) {
        self.last_laps_completed = -1;
        self.lap_valid = true;
        self.observed_max_rpm = 0.0;
        self.starting_positions.clear();
    }

    /// A new mapping is a new producer, even if iRacing reuses its session
    /// generation counter. Invalidate the document cache before reading it
    /// again from the reopened connection.
    fn reset_connection(&mut self) {
        self.reset();
        self.session = Session::new();
        self.session_parsed_at = None;
        self.session_number = i32::MIN;
    }

    fn waiting(&self, connected: bool) -> TelemetryFrame {
        let mut frame = TelemetryFrame::waiting_for_simulator(connected);
        frame.source = DESCRIPTOR.id;
        frame.source_name = DESCRIPTOR.display_name;
        frame.capabilities = DESCRIPTOR.capabilities;
        frame
    }

    fn simulator_has_focus(&mut self) -> bool {
        let due = self
            .foreground_checked_at
            .is_none_or(|checked| checked.elapsed() >= FOREGROUND_INTERVAL);
        if due {
            self.foreground = foreground::simulator_has_focus();
            self.foreground_checked_at = Some(Instant::now());
        }
        self.foreground
    }
}

impl TelemetrySource for IracingTelemetrySource {
    fn descriptor(&self) -> SourceDescriptor {
        DESCRIPTOR
    }

    fn next_frame(&mut self, demand: TelemetryDemand) -> TelemetryFrame {
        if self.connection.is_none() {
            let now = Instant::now();
            if now < self.reconnect_at {
                return self.waiting(false);
            }
            self.reconnect_at = now + RECONNECT_INTERVAL;
            self.connection = Connection::open_after_tick(self.minimum_tick_after_reconnect);
            if self.connection.is_some() {
                self.minimum_tick_after_reconnect = None;
            }
        }
        let last_tick_before_refresh = self.connection.as_ref().and_then(Connection::tick);
        if !self.connection.as_mut().is_some_and(Connection::refresh) {
            // The mapping outlives a session but not the process, so a silent
            // one is closed and reopened instead of polled forever.
            self.minimum_tick_after_reconnect = last_tick_before_refresh;
            self.connection = None;
            self.reconnect_at = Instant::now() + RECONNECT_INTERVAL;
            self.reset_connection();
            return self.waiting(false);
        }
        // Held outside `self` while the frame is assembled so the readers can
        // take the source mutably; it goes straight back afterwards.
        let sdk = self.connection.take().expect("la conexión está abierta");
        let frame = self.build(&sdk, demand);
        self.connection = Some(sdk);
        frame
    }
}

impl IracingTelemetrySource {
    fn build(&mut self, sdk: &Connection, demand: TelemetryDemand) -> TelemetryFrame {
        if self
            .session_parsed_at
            .is_none_or(|parsed| parsed.elapsed() >= SESSION_PARSE_INTERVAL)
            && self
                .session
                .refresh(sdk.session_generation(), sdk.session_text())
        {
            self.session_parsed_at = Some(Instant::now());
        }

        let session_number = sdk.integer("SessionNum").unwrap_or(0);
        if session_number != self.session_number {
            self.reset();
            self.session_number = session_number;
        }
        let schedule = self.session.scheduled(session_number).cloned();
        let session_name = schedule.as_ref().map_or("", |entry| entry.name.as_str());
        let session_state = sdk.integer("SessionState").unwrap_or(0);

        let in_garage = sdk.flag("IsInGarage").unwrap_or(false);
        let on_track = sdk.flag("IsOnTrack").unwrap_or(false);
        let on_pit_road = sdk.flag("OnPitRoad").unwrap_or(false);
        let surface = sdk
            .integer("PlayerTrackSurface")
            .unwrap_or(SURFACE_NOT_IN_WORLD);
        let lap_progress = sdk.number("LapDistPct").unwrap_or(0.0).clamp(0.0, 1.0);
        let laps_completed = sdk.integer("LapCompleted").unwrap_or(0).max(0);

        // A lap is not marked invalid by the simulator, so it is tracked from
        // the surface the car has been on since the last crossing.
        if surface == SURFACE_OFF_TRACK || on_pit_road {
            self.lap_valid = false;
        }
        if laps_completed != self.last_laps_completed {
            self.last_laps_completed = laps_completed;
            self.lap_valid = !on_pit_road;
        }

        let has_focus = self.simulator_has_focus();
        let mut frame = self.waiting(true);

        frame.player_active = on_track && !in_garage;
        frame.game_in_foreground = has_focus;
        frame.game_in_realtime = !in_garage && session_state != 0;
        frame.player_in_garage = in_garage;
        frame.session_type = session::session_type_code(session_name);
        frame.game_phase = game_phase(session_state, sdk.bits("SessionFlags"));
        frame.session_max_laps = schedule.as_ref().map_or(0, |entry| entry.laps);
        frame.session_time_remaining = finite_duration(sdk.number("SessionTimeRemain"));
        frame.session_elapsed_seconds = sdk.number("SessionTime").unwrap_or(0.0).max(0.0);
        frame.game_time_of_day_seconds = sdk.number("SessionTimeOfDay").unwrap_or(0.0).max(0.0);
        frame.session_max_time_seconds = schedule.as_ref().map_or(0.0, |entry| entry.seconds);
        frame.track_name = self.session.track_name.clone();
        frame.player_vehicle_name = self.session.player_car_name.clone();
        frame.player_vehicle_livery_name = self.session.player_car_name.clone();
        frame.track_length_meters = self.session.track_length_meters;

        if demand.include_standings {
            frame.standings = standings::build(
                &self.session,
                sdk,
                session_number,
                frame.session_type,
                session_state,
                &mut self.starting_positions,
            );
            frame.leader_total_laps = frame
                .standings
                .iter()
                .map(|entry| entry.total_laps)
                .max()
                .unwrap_or(0);
            if let Some(player) = frame.standings.iter().find(|entry| entry.is_player) {
                frame.player_position = player.overall_position;
                frame.player_class_position = player.position;
                frame.player_class_size = frame
                    .standings
                    .iter()
                    .filter(|entry| entry.vehicle_class == player.vehicle_class)
                    .count() as i32;
            }
        }

        frame.lap_number = sdk.integer("Lap").unwrap_or(0).max(0);
        frame.player_total_laps = laps_completed;
        frame.player_sector = self.session.sector(lap_progress);
        frame.player_lap_valid = self.lap_valid;
        frame.player_in_pits = on_pit_road;
        frame.lap_progress = lap_progress;
        frame.current_lap_seconds = sdk.number("LapCurrentLapTime").unwrap_or(0.0).max(0.0);
        frame.last_lap_seconds = sdk.number("LapLastLapTime").unwrap_or(0.0).max(0.0);
        frame.best_lap_seconds = sdk.number("LapBestLapTime").unwrap_or(0.0).max(0.0);

        frame.speed_kph = sdk.number("Speed").unwrap_or(0.0).max(0.0) * 3.6;
        frame.gear = sdk.integer("Gear").unwrap_or(0).clamp(-1, 9) as i8;
        frame.rpm = sdk.number("RPM").unwrap_or(0.0).max(0.0);
        self.observed_max_rpm = self.observed_max_rpm.max(frame.rpm);
        // The published redline is the reliable ceiling; the highest reading of
        // the session stands in until the session string has been read.
        frame.max_rpm = [self.session.redline_rpm, self.observed_max_rpm, 1.0]
            .into_iter()
            .find(|value| *value > 1.0)
            .unwrap_or(1.0);
        frame.throttle = sdk.number("Throttle").unwrap_or(0.0).clamp(0.0, 1.0);
        frame.brake = sdk.number("Brake").unwrap_or(0.0).clamp(0.0, 1.0);
        frame.clutch = sdk.number("Clutch").unwrap_or(0.0).clamp(0.0, 1.0);
        frame.brake_bias_percent = sdk.number("dcBrakeBias").unwrap_or(0.0);
        frame.abs_active = sdk.flag("BrakeABSactive").unwrap_or(false);
        // Traction control has no published state, so the indicator stays off
        // rather than guessing one from throttle and slip.
        frame.tc_active = false;
        frame.steering_angle_degrees = sdk
            .number("SteeringWheelAngle")
            .map(f64::to_degrees)
            .unwrap_or(0.0);
        frame.force_feedback = sdk.number("SteeringWheelPctTorque").unwrap_or(0.0);
        frame
    }
}

/// The frame numbers session phases the way the domain models read them: three
/// is formation, five is green and six is a full-course caution.
fn game_phase(session_state: i32, flags: u32) -> u32 {
    if flags & FLAG_CAUTION != 0 {
        return 6;
    }
    match session_state {
        1 => 2,
        2 => 1,
        3 => 3,
        4 => 5,
        5 | 6 => 8,
        _ => 0,
    }
}

/// A session without a clock publishes a week of remaining time. Anything from
/// that sentinel upwards becomes the zero the frame expects for "no clock",
/// which still admits the longest real session of a day.
fn finite_duration(value: Option<f64>) -> f64 {
    const UNLIMITED_SECONDS: f64 = 7.0 * 24.0 * 3_600.0;
    value
        .filter(|seconds| seconds.is_finite() && (0.0..UNLIMITED_SECONDS).contains(seconds))
        .unwrap_or(0.0)
}

#[cfg(test)]
mod tests {
    use super::{finite_duration, game_phase};

    #[test]
    fn maps_session_state_and_flags_to_phases() {
        assert_eq!(game_phase(0, 0), 0);
        assert_eq!(game_phase(3, 0), 3);
        assert_eq!(game_phase(4, 0), 5);
        assert_eq!(game_phase(5, 0), 8);
        assert_eq!(game_phase(4, 0x0000_4000), 6);
        assert_eq!(game_phase(4, 0x0000_8000), 6);
    }

    #[test]
    fn treats_an_unlimited_clock_as_no_clock() {
        assert_eq!(finite_duration(Some(1_800.0)), 1_800.0);
        assert_eq!(finite_duration(Some(604_800.0)), 0.0);
        assert_eq!(finite_duration(Some(-1.0)), 0.0);
        assert_eq!(finite_duration(None), 0.0);
    }
}
