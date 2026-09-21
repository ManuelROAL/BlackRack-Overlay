//! Everything specific to Le Mans Ultimate.
//!
//! All of it needs the SDK header and the `bridge.cpp` symbols, so the module
//! is gated the same way `build.rs` gates the native bridge. A build without
//! the SDK cannot read this simulator at all, so it has nothing to report
//! about the native shared-memory source either.

use std::path::Path;

use super::TelemetrySource;
#[cfg(all(target_os = "windows", lmu_sdk))]
use super::{SourceCapabilities, SourceDescriptor};

#[cfg(all(target_os = "windows", lmu_sdk))]
mod install;

#[cfg(all(target_os = "windows", lmu_sdk))]
mod driver_ranks;
#[cfg(all(target_os = "windows", lmu_sdk))]
mod event_split;
#[cfg(all(target_os = "windows", lmu_sdk))]
mod racecontrol;
#[cfg(all(target_os = "windows", lmu_sdk))]
mod rest;
#[cfg(all(target_os = "windows", lmu_sdk))]
mod source;
#[cfg(all(target_os = "windows", lmu_sdk))]
mod trackmap;

#[cfg(all(target_os = "windows", lmu_sdk))]
pub(super) const DESCRIPTOR: SourceDescriptor = SourceDescriptor {
    id: "lmu",
    display_name: "Le Mans Ultimate",
    capabilities: SourceCapabilities {
        virtual_energy: true,
        opponent_fuel: true,
        opponent_tires: true,
        damage_detail: true,
        tire_temperatures: true,
        brake_temperatures: true,
        weather_forecast: true,
        track_grip: true,
        driver_ranks: true,
        track_limits: true,
        official_track_map: true,
        pit_service_estimate: true,
        lift_and_coast: true,
        car_electronics: true,
        session_splits: true,
    },
    official_geometry: Some(trackmap::official_geometry),
    // LMU exposes LMU_Data natively. There is no separately installable runtime
    // dependency to probe; the source reports the live mapping state through
    // its telemetry frames.
    dependency: None,
};

/// The bridge symbols exist, so this simulator can be read whenever it is
/// running; the source itself reports whether it currently is.
#[cfg(all(target_os = "windows", lmu_sdk))]
pub(super) fn available() -> bool {
    true
}

#[cfg(all(target_os = "windows", lmu_sdk))]
pub(super) fn try_new(app_data: &Path) -> Option<Box<dyn TelemetrySource>> {
    Some(Box::new(
        source::LmuTelemetrySource::with_profile_directory(Some(
            app_data.join("consumption-profiles"),
        )),
    ))
}

/// Built without the shared-memory SDK, so the bridge symbols do not exist and
/// this simulator cannot be read at all.
#[cfg(not(all(target_os = "windows", lmu_sdk)))]
pub(super) fn available() -> bool {
    false
}

#[cfg(not(all(target_os = "windows", lmu_sdk)))]
pub(super) fn try_new(_app_data: &Path) -> Option<Box<dyn TelemetrySource>> {
    None
}
