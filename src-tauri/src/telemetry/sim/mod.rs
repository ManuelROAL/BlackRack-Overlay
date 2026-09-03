//! Telemetry sources, one module per simulator.
//!
//! Everything a single simulator knows about — its shared-memory bridge, REST
//! clients, log readers and install discovery — lives under `sim::<id>`. The
//! modules outside `sim` stay simulator agnostic: they only see the frame, the
//! demand and the contract below.
//!
//! Adding a simulator means writing `sim::<id>` with an `available` probe, a
//! `try_new` constructor and a descriptor, then adding it to `CANDIDATES`.
//! Nothing in `telemetry` outside this module should need to change.

use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::RwLock;
use std::time::{Duration, Instant};

use super::track_geometry::OfficialTrackMapGeometry;
use super::{TelemetryDemand, TelemetryFrame};

pub(crate) mod iracing;
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

/// One simulator the app can read, with the probe that says whether it can be
/// read right now. Order is priority.
struct Candidate {
    id: &'static str,
    available: fn() -> bool,
    try_new: fn(&Path) -> Option<Box<dyn TelemetrySource>>,
}

const CANDIDATES: [Candidate; 2] = [
    Candidate {
        id: "iracing",
        available: iracing::available,
        try_new: iracing::try_new,
    },
    Candidate {
        id: "lmu",
        available: lmu::available,
        try_new: lmu::try_new,
    },
];

/// How often a disconnected source looks for a simulator that is running.
const PROBE_INTERVAL: Duration = Duration::from_secs(2);

static ACTIVE: RwLock<Option<SourceDescriptor>> = RwLock::new(None);

fn set_active(descriptor: SourceDescriptor) {
    if let Ok(mut active) = ACTIVE.write() {
        *active = Some(descriptor);
    }
}

/// Follows whichever simulator is running instead of deciding once at startup:
/// the app is normally launched before the game, and closing one simulator to
/// open another must not need a restart.
struct SelectedSource {
    app_data: PathBuf,
    active: Box<dyn TelemetrySource>,
    on_mock: bool,
    probe_at: Instant,
}

impl SelectedSource {
    /// Adopts the highest-priority simulator that is available now. Keeping the
    /// current source when nothing is available is what leaves a waiting
    /// simulator reporting itself instead of falling back to the mock.
    fn reselect(&mut self) {
        let current = self.active.descriptor().id;
        let Some(candidate) = CANDIDATES.iter().find(|candidate| (candidate.available)()) else {
            return;
        };
        if candidate.id == current {
            return;
        }
        let Some(source) = (candidate.try_new)(&self.app_data) else {
            return;
        };
        set_active(source.descriptor());
        self.active = source;
        self.on_mock = false;
    }
}

impl TelemetrySource for SelectedSource {
    fn descriptor(&self) -> SourceDescriptor {
        self.active.descriptor()
    }

    fn next_frame(&mut self, demand: TelemetryDemand) -> TelemetryFrame {
        let frame = self.active.next_frame(demand);
        // A connected source is the right one by definition. The mock always
        // reports connected, so it is the one case that keeps probing.
        if (self.on_mock || !frame.connected) && Instant::now() >= self.probe_at {
            self.probe_at = Instant::now() + PROBE_INTERVAL;
            self.reselect();
        }
        frame
    }
}

/// Picks the first simulator whose telemetry can be read, and keeps looking
/// while none is connected. The mock is the last resort so a build made without
/// any simulator still renders.
pub(crate) fn detect(app_data: &Path) -> Box<dyn TelemetrySource> {
    let selected = CANDIDATES
        .iter()
        .filter(|candidate| (candidate.available)())
        .find_map(|candidate| (candidate.try_new)(app_data));
    let on_mock = selected.is_none();
    let active = selected
        .unwrap_or_else(|| Box::new(mock::MockTelemetrySource::new()) as Box<dyn TelemetrySource>);
    set_active(active.descriptor());
    Box::new(SelectedSource {
        app_data: app_data.to_path_buf(),
        active,
        on_mock,
        probe_at: Instant::now() + PROBE_INTERVAL,
    })
}

pub(crate) fn active() -> Option<SourceDescriptor> {
    ACTIVE.read().ok().and_then(|active| *active)
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
    active().and_then(|descriptor| descriptor.official_geometry)
}
