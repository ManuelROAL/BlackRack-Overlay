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
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
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
    /// Driver-selectable electronics — engine map, traction control, ABS,
    /// brake migration, anti-roll bars — and the hybrid deployment state.
    pub(crate) car_electronics: bool,
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
        car_electronics: false,
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
/// read right now. Order is priority: a candidate whose `available` cannot
/// fail — LMU, once its SDK is compiled in — goes last, or nothing after it
/// would ever be reached in automatic selection.
struct Candidate {
    id: &'static str,
    /// Shown in the control panel's simulator picker.
    display_name: &'static str,
    available: fn() -> bool,
    try_new: fn(&Path) -> Option<Box<dyn TelemetrySource>>,
    /// Still filling the frame one overlay at a time. Such a candidate is
    /// compiled and tested in every build, but only a build with
    /// `experimental-simulators` lets the user reach it.
    experimental: bool,
}

const CANDIDATES: [Candidate; 2] = [
    Candidate {
        id: "iracing",
        display_name: "iRacing",
        available: iracing::available,
        try_new: iracing::try_new,
        experimental: true,
    },
    Candidate {
        id: "lmu",
        display_name: "Le Mans Ultimate",
        available: lmu::available,
        try_new: lmu::try_new,
        experimental: false,
    },
];

/// Whether this build offers the candidate to the user at all. Filtering here
/// rather than at the `CANDIDATES` declaration keeps every simulator compiled
/// and covered by `cargo test`, so one waiting to be finished cannot rot.
fn shipped(candidate: &Candidate) -> bool {
    cfg!(feature = "experimental-simulators") || !candidate.experimental
}

/// How often a disconnected source looks for a simulator that is running.
const PROBE_INTERVAL: Duration = Duration::from_secs(2);

static ACTIVE: RwLock<Option<SourceDescriptor>> = RwLock::new(None);

/// Adopts a source and records the change.
///
/// Only a transition is written, because the selection paths re-adopt the
/// simulator they already had. That leaves the log with one line per genuine
/// switch — which is what says whether the overlays were reading the simulator
/// the user believes they were, a question a report of "the overlays went
/// empty" cannot otherwise answer.
fn set_active(descriptor: SourceDescriptor) {
    let Ok(mut active) = ACTIVE.write() else {
        return;
    };
    let previous = active.map(|current| current.id);
    *active = Some(descriptor);
    drop(active);
    if previous != Some(descriptor.id) {
        crate::startup_log::record(format!(
            "telemetry source {} -> {}",
            previous.unwrap_or("none"),
            descriptor.id
        ));
    }
}

pub(crate) fn active() -> Option<SourceDescriptor> {
    ACTIVE.read().ok().and_then(|active| *active)
}

pub(super) fn active_official_geometry() -> Option<OfficialGeometryFetcher> {
    active().and_then(|descriptor| descriptor.official_geometry)
}

/// The user's simulator choice. `CANDIDATES.len()` stands for "auto" — follow
/// whichever candidate is running — so a plain index never needs an `Option`.
static PREFERENCE: AtomicUsize = AtomicUsize::new(CANDIDATES.len());
/// Bumped by every `set_preference` call so a running `SelectedSource` can
/// tell a genuine change from the value it already acted on, without
/// re-checking on every telemetry cycle.
static PREFERENCE_GENERATION: AtomicU64 = AtomicU64::new(0);

fn preference_index() -> Option<usize> {
    let stored = PREFERENCE.load(Ordering::Relaxed);
    (stored < CANDIDATES.len()).then_some(stored)
}

/// `"auto"` or the id of the pinned candidate, for the control panel to show
/// what is configured next to what is actually active.
pub(crate) fn preference() -> &'static str {
    preference_index().map_or("auto", |index| CANDIDATES[index].id)
}

/// Pins telemetry to one simulator, or `"auto"` to go back to following
/// whichever is running. Rejects an id that names no candidate rather than
/// silently falling back, so a stale or mistyped id is visible to the caller.
pub(crate) fn set_preference(id: &str) -> Result<(), String> {
    let index = if id == "auto" {
        CANDIDATES.len()
    } else {
        CANDIDATES
            .iter()
            .position(|candidate| candidate.id == id && shipped(candidate))
            .ok_or_else(|| format!("unknown_simulator_{id}"))?
    };
    PREFERENCE.store(index, Ordering::Relaxed);
    PREFERENCE_GENERATION.fetch_add(1, Ordering::Relaxed);
    Ok(())
}

/// A simulator the control panel can offer as a preference.
#[derive(Serialize)]
pub(crate) struct SimulatorOption {
    pub(crate) id: &'static str,
    pub(crate) display_name: &'static str,
}

pub(crate) fn options() -> Vec<SimulatorOption> {
    CANDIDATES
        .iter()
        .filter(|candidate| shipped(candidate))
        .map(|candidate| SimulatorOption {
            id: candidate.id,
            display_name: candidate.display_name,
        })
        .collect()
}

/// The candidates the current preference allows: every one of them under
/// "auto", or just the pinned one. `detect` and the gentle reselect below both
/// choose from this instead of `CANDIDATES` directly, so a pinned choice is
/// never quietly overridden by a higher-priority simulator starting up.
fn allowed_candidates() -> impl Iterator<Item = &'static Candidate> {
    let forced = preference_index();
    CANDIDATES
        .iter()
        .enumerate()
        .filter(|(_, candidate)| shipped(candidate))
        .filter(move |(index, _)| forced.is_none_or(|only| *index == only))
        .map(|(_, candidate)| candidate)
}

/// The simulator automatic selection adopts right now: the highest-priority
/// allowed candidate that is available.
///
/// Every automatic path shares this one function rather than walking the
/// candidates itself, because walking `CANDIDATES` directly is how a simulator
/// this build does not ship reaches the overlays through a path that never
/// shows it in the picker.
fn available_candidate() -> Option<&'static Candidate> {
    allowed_candidates().find(|candidate| (candidate.available)())
}

/// What the control panel reports about the simulator behind the overlays.
#[derive(Serialize)]
pub(crate) struct SimulatorStatus {
    id: &'static str,
    display_name: &'static str,
    dependency: Option<SourceDependency>,
    preference: &'static str,
    options: Vec<SimulatorOption>,
}

pub(crate) fn status() -> SimulatorStatus {
    let preference = preference();
    let options = options();
    let Some(descriptor) = active() else {
        return SimulatorStatus {
            id: "none",
            display_name: "",
            dependency: None,
            preference,
            options,
        };
    };
    SimulatorStatus {
        id: descriptor.id,
        display_name: descriptor.display_name,
        dependency: descriptor.dependency.map(|probe| probe()),
        preference,
        options,
    }
}

/// Follows whichever simulator is running, or the one the user pinned,
/// instead of deciding once at startup: the app is normally launched before
/// the game, and closing one simulator to open another must not need a
/// restart.
struct SelectedSource {
    app_data: PathBuf,
    active: Box<dyn TelemetrySource>,
    on_mock: bool,
    probe_at: Instant,
    /// The `PREFERENCE_GENERATION` this source has already reacted to.
    preference_generation: u64,
}

impl SelectedSource {
    /// Adopts the highest-priority allowed candidate that is available now.
    /// Keeping the current source when nothing is available is what leaves a
    /// waiting simulator reporting itself instead of falling back to mock.
    fn reselect(&mut self) {
        let current = self.active.descriptor().id;
        let Some(candidate) = available_candidate() else {
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

    /// Reacts to a preference change immediately rather than waiting for a
    /// disconnect: the user asked for something specific, so the source
    /// switches away from a now-disallowed one even while it is connected.
    fn force_reselect(&mut self) {
        let source = match preference_index() {
            // Pinned: build that candidate's own source even if it does not
            // look available yet, so it reports "waiting for it" instead of
            // falling through to fabricated mock data.
            Some(index) => (CANDIDATES[index].try_new)(&self.app_data),
            // Back to auto: let the normal priority order decide, through the
            // same `available_candidate` the gentle `reselect` uses. This
            // branch is not the rare one it reads as — the control panel sends
            // its stored preference on every launch, so auto is reselected each
            // time the app starts.
            None => available_candidate().and_then(|candidate| (candidate.try_new)(&self.app_data)),
        };
        let source = source.unwrap_or_else(|| {
            Box::new(mock::MockTelemetrySource::new()) as Box<dyn TelemetrySource>
        });
        self.on_mock = source.descriptor().id == "mock";
        set_active(source.descriptor());
        self.active = source;
    }
}

impl TelemetrySource for SelectedSource {
    fn descriptor(&self) -> SourceDescriptor {
        self.active.descriptor()
    }

    fn next_frame(&mut self, demand: TelemetryDemand) -> TelemetryFrame {
        let generation = PREFERENCE_GENERATION.load(Ordering::Relaxed);
        if generation != self.preference_generation {
            self.preference_generation = generation;
            self.force_reselect();
        }
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

/// Picks the first allowed simulator whose telemetry can be read, and keeps
/// looking while none is connected. The mock is the last resort so a build
/// made without any simulator still renders.
pub(crate) fn detect(app_data: &Path) -> Box<dyn TelemetrySource> {
    let selected = allowed_candidates()
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
        preference_generation: PREFERENCE_GENERATION.load(Ordering::Relaxed),
    })
}

#[cfg(test)]
mod tests {
    use super::{options, set_preference, CANDIDATES};

    /// A simulator still being filled in must be unreachable in the builds that
    /// go to users: absent from the picker, and refused when something tries to
    /// pin it from a preference saved by a development build.
    #[test]
    fn experimental_simulators_are_offered_only_when_the_build_asks_for_them() {
        let shipped = cfg!(feature = "experimental-simulators");
        let offered: Vec<&str> = options().iter().map(|option| option.id).collect();
        let experimental = CANDIDATES.iter().filter(|candidate| candidate.experimental);
        assert!(
            CANDIDATES.iter().any(|candidate| candidate.experimental),
            "the test proves nothing without an experimental candidate"
        );
        assert!(
            experimental.into_iter().all(|candidate| {
                offered.contains(&candidate.id) == shipped
                    // Pinning is only exercised where it must fail, so the
                    // shared preference is never disturbed.
                    && (shipped || set_preference(candidate.id).is_err())
            }),
            "offered {offered:?} with experimental-simulators = {shipped}"
        );
    }

    #[test]
    fn every_build_offers_at_least_one_simulator() {
        assert!(!options().is_empty());
    }

    /// The picker is not the only way to reach a simulator: automatic selection
    /// adopts one on its own, and it runs on every launch, as soon as the
    /// control panel sends its stored preference. So the pool it draws from has
    /// to hold exactly what the build offers, or a simulator deliberately kept
    /// out of the picker would still end up feeding the overlays.
    ///
    /// This pins the pool, not the walk over it. What keeps every automatic
    /// path on the pool is that they all go through `available_candidate`;
    /// proving that needs a candidate whose availability the test can force,
    /// which the fixed `CANDIDATES` array does not allow today.
    #[test]
    fn automatic_selection_draws_only_from_the_simulators_this_build_offers() {
        assert_eq!(super::preference(), "auto", "the default is auto selection");
        let offered: Vec<&str> = options().iter().map(|option| option.id).collect();
        let pool: Vec<&str> = super::allowed_candidates()
            .map(|candidate| candidate.id)
            .collect();
        assert_eq!(pool, offered);
    }
}
