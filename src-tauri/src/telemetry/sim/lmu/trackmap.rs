//! Official track outline, read from the local LMU REST endpoint.

use std::time::Duration;

use crate::telemetry::track_geometry::{
    decode_geometry, OfficialTrackMapGeometry, RawTrackMapPoint,
};

pub(super) fn official_geometry() -> Result<OfficialTrackMapGeometry, String> {
    let client = reqwest::blocking::Client::builder()
        .connect_timeout(Duration::from_millis(400))
        .timeout(Duration::from_millis(800))
        .build()
        .map_err(|error| {
            crate::startup_log::command_error("track_geometry_client_failed", error)
        })?;
    let points = client
        .get("http://127.0.0.1:6397/rest/watch/trackmap")
        .send()
        .and_then(reqwest::blocking::Response::error_for_status)
        .map_err(|error| crate::startup_log::command_error("track_geometry_request_failed", error))?
        .json::<Vec<RawTrackMapPoint>>()
        .map_err(|error| crate::startup_log::command_error("track_geometry_malformed", error))?;
    decode_geometry(points)
}
