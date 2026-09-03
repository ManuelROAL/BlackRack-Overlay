//! Everything specific to Le Mans Ultimate.
//!
//! `install` is always compiled because the control panel reports whether the
//! telemetry plugin is present even in builds made without the shared-memory
//! SDK. The rest needs the SDK header and the `bridge.cpp` symbols, so it is
//! gated the same way `build.rs` gates the native bridge.

use std::path::Path;

#[cfg(all(target_os = "windows", lmu_sdk))]
use super::SourceDescriptor;
use super::TelemetrySource;

pub(crate) mod install;

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
    official_geometry: Some(trackmap::official_geometry),
};

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
pub(super) fn try_new(_app_data: &Path) -> Option<Box<dyn TelemetrySource>> {
    None
}
