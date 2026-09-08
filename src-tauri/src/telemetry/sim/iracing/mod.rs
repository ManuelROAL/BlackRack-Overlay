//! Everything specific to iRacing.
//!
//! The simulator publishes its telemetry through a memory-mapped file that only
//! exists while it runs, and nothing here needs an SDK at build time, so the
//! module compiles on every Windows build and its probe answers whether the
//! simulator is running right now.

use std::path::Path;

use super::TelemetrySource;
#[cfg(target_os = "windows")]
use super::{SourceCapabilities, SourceDependency, SourceDescriptor};

#[cfg(target_os = "windows")]
mod foreground;
#[cfg(target_os = "windows")]
mod irsdk;
#[cfg(target_os = "windows")]
mod session;
#[cfg(target_os = "windows")]
mod source;
#[cfg(target_os = "windows")]
mod standings;
#[cfg(target_os = "windows")]
mod yaml;

/// What this source reports today. A capability is turned on when the source
/// actually fills the frame fields behind it, so the control panel never offers
/// an overlay that would stay empty. The simulator has no virtual energy, no
/// per-part damage, live brake temperatures or authoritative track outline, so
/// those stay off.
#[cfg(target_os = "windows")]
pub(super) const DESCRIPTOR: SourceDescriptor = SourceDescriptor {
    id: "iracing",
    display_name: "iRacing",
    capabilities: SourceCapabilities {
        driver_ranks: true,
        tire_temperatures: true,
        ..SourceCapabilities::NONE
    },
    official_geometry: None,
    dependency: Some(telemetry_dependency),
};

/// The telemetry interface is part of the simulator, so there is nothing to
/// install: the dependency is simply whether it is publishing right now.
#[cfg(target_os = "windows")]
fn telemetry_dependency() -> SourceDependency {
    SourceDependency {
        available: irsdk::available(),
        detail: None,
    }
}

/// Whether the simulator's mapping is open right now. Used only to rank this
/// candidate against the others in automatic selection — constructing the
/// source below never depends on it, so a source pinned to iRacing while it
/// is closed still gets its own "waiting" state instead of the mock's.
#[cfg(target_os = "windows")]
pub(super) fn available() -> bool {
    irsdk::available()
}

#[cfg(target_os = "windows")]
pub(super) fn try_new(_app_data: &Path) -> Option<Box<dyn TelemetrySource>> {
    Some(Box::new(source::IracingTelemetrySource::new()) as Box<dyn TelemetrySource>)
}

/// The shared memory is a Windows interface, so this simulator cannot be read
/// at all on another platform.
#[cfg(not(target_os = "windows"))]
pub(super) fn available() -> bool {
    false
}

#[cfg(not(target_os = "windows"))]
pub(super) fn try_new(_app_data: &Path) -> Option<Box<dyn TelemetrySource>> {
    None
}
