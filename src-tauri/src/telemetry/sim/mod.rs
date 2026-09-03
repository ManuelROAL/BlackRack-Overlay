//! Telemetry sources, one module per simulator.
//!
//! Everything a single simulator knows about — its shared-memory bridge, REST
//! clients, log readers and install discovery — lives under `sim::<id>`. The
//! modules outside `sim` stay simulator agnostic: they only see the frame, the
//! demand and the contract below.
//!
//! Adding a simulator means writing `sim::<id>` with a `try_new` probe and a
//! descriptor, then adding it to the candidate chain in `detect`. Nothing in
//! `telemetry` outside this module should need to change.

use serde::Serialize;
use std::path::Path;
use std::sync::OnceLock;

use super::track_geometry::OfficialTrackMapGeometry;
use super::{TelemetryDemand, TelemetryFrame};

pub(crate) mod lmu;
pub(crate) mod mock;

/// Reads the simulator's official track outline.
///
/// This is a plain function rather than a source method because the track map
/// is requested from the command and browser-source threads, while the source
/// itself is owned by the telemetry loop.
pub(crate) type OfficialGeometryFetcher = fn() -> Result<OfficialTrackMapGeometry, String>;

/// Checks whether the simulator's telemetry dependency — a plugin, an SDK or
/// a service — is in place. Runs on the command thread, so like the geometry
/// fetcher it is a function rather than a source method.
pub(crate) type DependencyProbe = fn() -> SourceDependency;

#[derive(Clone, Serialize)]
pub(crate) struct SourceDependency {
    pub(crate) available: bool,
    /// Where the dependency was found, or where it was expected.
    pub(crate) detail: Option<String>,
}

/// What the simulator behind a source can ever report.
///
/// This is not the same question as the per-session flags already carried by
/// the frame — `virtual_energy_active`, `rest_weather_available` and the `-1`
/// sentinels say what exists *right now*. Capabilities say what could ever
/// exist, so the control panel can retire an overlay a simulator will never
/// feed instead of offering one that stays empty.
#[derive(Clone, Copy, Serialize)]
pub struct SourceCapabilities {
    /// Energy budget alongside fuel, as LMU's hybrid classes use.
    pub(crate) virtual_energy: bool,
    /// Fuel and energy state for cars other than the player's.
    pub(crate) opponent_fuel: bool,
    /// Tyre compound and wear for cars other than the player's.
    pub(crate) opponent_tires: bool,
    /// Per-part damage percentages rather than a repair time.
    pub(crate) damage_detail: bool,
    pub(crate) tire_temperatures: bool,
    pub(crate) brake_temperatures: bool,
    pub(crate) weather_forecast: bool,
    /// Track grip and rubbering state.
    pub(crate) track_grip: bool,
    /// Driver and safety ratings for the roster.
    pub(crate) driver_ranks: bool,
    /// Track-limit counters and the penalty threshold.
    pub(crate) track_limits: bool,
    /// An authoritative track outline, as opposed to one learned from laps.
    pub(crate) official_track_map: bool,
    /// The simulator's own estimate for the next pit stop.
    pub(crate) pit_service_estimate: bool,
    /// Lift and coast guidance from the car.
    pub(crate) lift_and_coast: bool,
    /// Multi-split events, where the field is divided across sessions.
    pub(crate) session_splits: bool,
}

impl SourceCapabilities {
    /// Everything off. A new source turns on only what it can actually fill.
    pub(crate) const NONE: Self = Self {
        virtual_energy: false,
        opponent_fuel: false,
        opponent_tires: false,
        damage_detail: false,
        tire_temperatures: false,
        brake_temperatures: false,
        weather_forecast: false,
        track_grip: false,
        driver_ranks: false,
        track_limits: false,
        official_track_map: false,
        pit_service_estimate: false,
        lift_and_coast: false,
        session_splits: false,
    };
}

/// Identity and static abilities of the simulator behind a source.
#[derive(Clone, Copy)]
pub(crate) struct SourceDescriptor {
    pub(crate) id: &'static str,
    /// Shown to the user; the id is for logic, this is for copy.
    pub(crate) display_name: &'static str,
    pub(crate) capabilities: SourceCapabilities,
    pub(crate) official_geometry: Option<OfficialGeometryFetcher>,
    /// Absent when the simulator has nothing installable to check.
    pub(crate) dependency: Option<DependencyProbe>,
}

pub(crate) trait TelemetrySource: Send + 'static {
    fn descriptor(&self) -> SourceDescriptor;
    fn next_frame(&mut self, demand: TelemetryDemand) -> TelemetryFrame;
}

static ACTIVE: OnceLock<SourceDescriptor> = OnceLock::new();

/// Picks the first simulator whose telemetry can be read in this build. The
/// mock is the last resort so a build made without any SDK still renders.
pub(crate) fn detect(app_data: &Path) -> Box<dyn TelemetrySource> {
    let source = lmu::try_new(app_data)
        .unwrap_or_else(|| Box::new(mock::MockTelemetrySource::new()) as Box<dyn TelemetrySource>);
    let _ = ACTIVE.set(source.descriptor());
    source
}

pub(crate) fn active() -> Option<SourceDescriptor> {
    ACTIVE.get().copied()
}

/// What the control panel reports about the simulator behind the overlays.
#[derive(Serialize)]
pub(crate) struct SimulatorStatus {
    id: &'static str,
    display_name: &'static str,
    dependency: Option<SourceDependency>,
}

pub(crate) fn status() -> SimulatorStatus {
    let Some(descriptor) = active() else {
        return SimulatorStatus {
            id: "none",
            display_name: "",
            dependency: None,
        };
    };
    SimulatorStatus {
        id: descriptor.id,
        display_name: descriptor.display_name,
        dependency: descriptor.dependency.map(|probe| probe()),
    }
}

pub(super) fn active_official_geometry() -> Option<OfficialGeometryFetcher> {
    ACTIVE
        .get()
        .and_then(|descriptor| descriptor.official_geometry)
}
