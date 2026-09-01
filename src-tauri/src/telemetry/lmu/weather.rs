//! Track state and weather projections.

use super::*;

impl LmuTelemetrySource {
    pub(super) fn weather_session_key(session_type: i32) -> &'static str {
        if (0..=4).contains(&session_type) {
            "PRACTICE"
        } else if (5..=8).contains(&session_type) {
            "QUALIFY"
        } else {
            "RACE"
        }
    }

    pub(super) fn live_weather_icon(cloud_coverage: u8, rain_percent: f64) -> i32 {
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

    pub(super) fn track_grip_percent(track_grip_level: u8) -> f64 {
        match track_grip_level {
            1 => 25.0,
            2 => 50.0,
            3 => 75.0,
            4 => 90.0,
            _ => 0.0,
        }
    }

    pub(super) fn track_rubber_percent(snapshot: &LmuSnapshot) -> f64 {
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

    pub(super) fn track_surface_state(track_wetness_percent: f64) -> &'static str {
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

    pub(super) fn resolve_wind(rest_wind: Option<(f64, f64)>) -> (f64, f64) {
        // LMU's forecast direction is an explicit meteorological compass index.
        // The shared-memory mWind vector is expressed in track/world axes, so it
        // cannot be presented as a north-referenced bearing without extra track
        // orientation data.
        rest_wind.unwrap_or((0.0, 0.0))
    }

    pub(super) fn relative_wind_direction_degrees(
        wind_bearing_degrees: f64,
        orientation_right_z: f64,
        orientation_forward_z: f64,
    ) -> f64 {
        // Match SimHub's LMU OrientationYaw, which doX subtracts from the
        // meteorological direction in its wind dashboard.
        let vehicle_yaw_degrees = orientation_right_z
            .atan2(orientation_forward_z)
            .to_degrees();
        (-wind_bearing_degrees - vehicle_yaw_degrees).rem_euclid(360.0)
    }
}
