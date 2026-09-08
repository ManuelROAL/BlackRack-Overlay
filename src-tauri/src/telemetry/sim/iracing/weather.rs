//! Current weather values published by iRacing's shared-memory telemetry.

use super::super::irsdk::Connection;

pub(super) struct WeatherValues {
    pub(super) available: bool,
    pub(super) ambient_temperature_c: f64,
    pub(super) track_temperature_c: f64,
    pub(super) rain_percent: f64,
    pub(super) track_wetness_percent: f64,
    pub(super) current_humidity_percent: f64,
    pub(super) wind_speed_ms: f64,
    pub(super) wind_direction_degrees: f64,
    pub(super) wind_relative_direction_degrees: f64,
    pub(super) track_grip_state: &'static str,
    pub(super) cloud_coverage: i32,
}

pub(super) fn read(sdk: &Connection) -> WeatherValues {
    let ambient_temperature_c = nonnegative(sdk.number("AirTemp"));
    let track_temperature_c = nonnegative(sdk.number("TrackTempCrew"));
    let rain_percent = sdk
        .number("Precipitation")
        .filter(|value| value.is_finite() && *value >= 0.0)
        .map_or(-1.0, |value| (value * 100.0).clamp(0.0, 100.0));
    let track_wetness_percent = wetness_percent(sdk.integer("TrackWetness"));
    let wind_speed_ms = nonnegative(sdk.number("WindVel"));
    let wind_direction_degrees = sdk
        .number("WindDir")
        .filter(|value| value.is_finite())
        .map_or(0.0, |value| value.to_degrees().rem_euclid(360.0));
    let yaw_degrees = sdk
        .number("YawNorth")
        .filter(|value| value.is_finite())
        .map_or(0.0, |value| value.to_degrees());
    let wind_relative_direction_degrees = (-wind_direction_degrees - yaw_degrees).rem_euclid(360.0);
    let skies = sdk.integer("Skies");
    let cloud_coverage = weather_icon(skies, rain_percent);
    let current_humidity_percent = percent(sdk.number("RelativeHumidity"));

    WeatherValues {
        available: [
            ambient_temperature_c,
            track_temperature_c,
            rain_percent,
            track_wetness_percent,
            current_humidity_percent,
            wind_speed_ms,
        ]
        .into_iter()
        .any(|value| value >= 0.0),
        ambient_temperature_c,
        track_temperature_c,
        rain_percent,
        track_wetness_percent,
        current_humidity_percent,
        wind_speed_ms,
        wind_direction_degrees,
        wind_relative_direction_degrees,
        track_grip_state: track_surface_state(track_wetness_percent),
        cloud_coverage,
    }
}

fn nonnegative(value: Option<f64>) -> f64 {
    value
        .filter(|value| value.is_finite() && *value >= 0.0)
        .unwrap_or(-1.0)
}

fn percent(value: Option<f64>) -> f64 {
    value
        .filter(|value| value.is_finite() && *value >= 0.0)
        .map_or(-1.0, |value| {
            if value <= 1.0 { value * 100.0 } else { value }.clamp(0.0, 100.0)
        })
}

fn wetness_percent(value: Option<i32>) -> f64 {
    match value {
        Some(0) => 0.0,
        Some(1) => 5.0,
        Some(2) => 15.0,
        Some(3) => 40.0,
        Some(4) => 70.0,
        Some(5) => 100.0,
        _ => -1.0,
    }
}

fn track_surface_state(track_wetness_percent: f64) -> &'static str {
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

fn weather_icon(skies: Option<i32>, rain_percent: f64) -> i32 {
    if rain_percent.is_finite() && rain_percent > 0.0 {
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
    } else {
        match skies.unwrap_or(0) {
            1 => 2,
            2 => 3,
            3 => 4,
            _ => 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{percent, track_surface_state, weather_icon, wetness_percent};

    #[test]
    fn maps_iracing_wetness_levels_to_shared_bands() {
        assert_eq!(wetness_percent(Some(0)), 0.0);
        assert_eq!(wetness_percent(Some(1)), 5.0);
        assert_eq!(wetness_percent(Some(3)), 40.0);
        assert_eq!(wetness_percent(Some(5)), 100.0);
        assert_eq!(wetness_percent(Some(9)), -1.0);
        assert_eq!(track_surface_state(0.0), "dry");
        assert_eq!(track_surface_state(5.0), "damp");
        assert_eq!(track_surface_state(15.0), "wet");
        assert_eq!(track_surface_state(40.0), "heavy");
        assert_eq!(track_surface_state(70.0), "saturated");
    }

    #[test]
    fn rain_intensity_overrides_sky_for_weather_icon() {
        assert_eq!(weather_icon(Some(3), 0.0), 4);
        assert_eq!(weather_icon(Some(1), 8.0), 5);
        assert_eq!(weather_icon(Some(2), 25.0), 8);
        assert_eq!(weather_icon(Some(2), 75.0), 10);
    }

    #[test]
    fn accepts_humidity_as_fraction_or_percent() {
        assert_eq!(percent(Some(0.8)), 80.0);
        assert_eq!(percent(Some(80.0)), 80.0);
        assert_eq!(percent(Some(-1.0)), -1.0);
    }
}
