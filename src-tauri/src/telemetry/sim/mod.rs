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

/// Identity and static abilities of the simulator behind a source.
#[derive(Clone, Copy)]
pub(crate) struct SourceDescriptor {
    pub(crate) id: &'static str,
    pub(crate) official_geometry: Option<OfficialGeometryFetcher>,
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

pub(super) fn active_official_geometry() -> Option<OfficialGeometryFetcher> {
    ACTIVE
        .get()
        .and_then(|descriptor| descriptor.official_geometry)
}
